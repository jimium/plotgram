//! 障碍避让多段样条路由 Recipe（doc16 §5.1 / R3 Slice 3.2c）。
//!
//! 把 [`super::super::edge_routing_spline::route_edges_spline`] 拆分到 [`RoutingRecipe`]
//! 接口后面。样条家族的两阶段算法（可见性图 + Dijkstra 求绕行折线 → Catmull-Rom 多段
//! 贝塞尔拟合）保持不变：
//!
//! - `compile`：构造 [`RoutingContext`]、懒构建避障索引、逐边解析端点 / 识别自环。
//! - `solve`：
//!   - 无障碍 → 简单贝塞尔（`Cubic`，控制点按端口方向 + 平行中段偏移）。
//!   - 有障碍 → 可见性图绕行折线经 [`fit_multi_segment_spline`] 拟合采样为 `Spline`。
//!   - 自环 → 复用共享 [`solve_self_loop`]（`Cubic` topology + anchor 标签计划）。
//!
//! ## byte-identical 要点
//!
//! 样条家族的标签放置与 straight/bezier 不同：**始终**沿采样折线按弧长
//! （[`point_at_path_t`](crate::layout::routing::common::edge_geometry::point_at_path_t)）放置，
//! 即使几何是 `Bezier`（无障碍时）也用 `sample_bezier` 的采样折线取点，而非 `cubic_bezier_point`。
//! 故 solve 把采样折线放进 [`EdgeLabelPlan::sample_path`]，由 LabelSolver 显式采用。
//! 中段 t 用 [`label_t_for_diagram`]（ER 有专属默认）。

use std::collections::HashMap;

use crate::ast::Diagram;
use crate::layout::algorithm_config::AlgorithmOptionSpec;
use crate::layout::geometry::Point;
use crate::layout::pipeline::plan::ResolvedAlgoOptions;
use crate::layout::routing::common::edge_geometry::{compute_bezier_controls, label_t_for_diagram};
use crate::layout::routing::common::routing_skeleton::{
    build_obstacle_context, quick_check_need_obstacle_index, resolve_endpoints,
    sorted_group_obstacle_ids, EdgeEndpoints, LabelOffset, RoutingContext,
};
use crate::layout::routing::common::self_loop::{
    self_loop_indices, solve_self_loop, SelfLoopStyle,
};
use crate::layout::routing::edge_routing_bezier::{BezierConfig, BEZIER_OPTIONS};
use crate::layout::routing::edge_routing_spline::{
    build_full_path, fit_multi_segment_spline, sample_bezier,
};
use crate::layout::routing::model::{
    CubicPath, EmptyRouteReason, EndpointAssignment, RoutePath, RouteSolution, SplinePath,
    StableEdgeId,
};
use crate::layout::routing::visibility::ObstacleIndex;
use crate::layout::types::{LayoutResult, Port};
use crate::layout::{EdgeLayout, NodeLayout};
use crate::types::DiagramType;

use super::{EdgeLabelPlan, RecipeSolution, RoutingRecipe};

/// 样条路由适用的内置图类型（与原 `SplineRouting` 一致，无 Mindmap）。
const APPLICABLE_TYPES: &[DiagramType] = &[
    DiagramType::State,
    DiagramType::Er,
];

/// 贝塞尔路径每段的采样点数（与原 `edge_routing_spline` 一致）。
const BEZIER_SAMPLES_PER_SEGMENT: usize = 12;

/// 障碍避让多段样条路由 Recipe（构造时注入已解析的 tension）。
pub struct SplineRecipe {
    config: BezierConfig,
}

impl Default for SplineRecipe {
    fn default() -> Self {
        Self {
            config: BezierConfig::default(),
        }
    }
}

impl SplineRecipe {
    /// 从 DSL 已解析 option 构造（与 `SplineRouting::from_options` 同源）。
    pub fn from_options(options: &ResolvedAlgoOptions) -> Self {
        Self {
            config: BezierConfig {
                tension: options.get_or_default(&BEZIER_OPTIONS[0]),
            },
        }
    }

    /// 直接注入配置（供单测）。
    pub fn new(config: BezierConfig) -> Self {
        Self { config }
    }
}

/// 一条边的已编译 Spline 计划。
enum SplineEdgePlan {
    /// 端点缺失：物化为空几何（== `EdgeLayout::empty()`），无标签。
    Missing,
    /// 自环边：共享 helper 产 topology（`Cubic`）+ anchor 标签计划。
    SelfLoop { loop_index: usize },
    /// 普通边：已解析端点 + 标签偏移。
    Normal {
        ep: EdgeEndpoints,
        label_off: LabelOffset,
    },
}

/// 样条路由的已编译 Draft。
pub struct SplineDraft<'a> {
    diagram: &'a Diagram,
    nodes: &'a HashMap<String, NodeLayout>,
    edges: Vec<SplineEdgePlan>,
    node_id_to_idx: HashMap<&'a str, usize>,
    obstacle_index: Option<ObstacleIndex>,
    /// group 障碍在 ObstacleIndex 中的起始下标；`group_ids[i]` ↔ `group_start + i`。
    group_start: usize,
    group_ids: Vec<String>,
    tension: f64,
}

impl<'a> SplineDraft<'a> {
    /// 逐边计划数量（供 compile 独立单测）。
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// 是否已构建避障索引（供懒构建单测）。
    pub fn has_obstacle_index(&self) -> bool {
        self.obstacle_index.is_some()
    }
}

impl RoutingRecipe for SplineRecipe {
    type Draft<'a> = SplineDraft<'a>;

    fn name(&self) -> &'static str {
        "spline"
    }

    fn applicable_diagram_types(&self) -> &'static [DiagramType] {
        APPLICABLE_TYPES
    }

    fn supports_custom(&self) -> bool {
        true
    }


    fn option_specs(&self) -> &'static [AlgorithmOptionSpec] {
        BEZIER_OPTIONS
    }

    fn compile<'a>(&self, diagram: &'a Diagram, result: &'a LayoutResult) -> SplineDraft<'a> {
        let ctx = RoutingContext::new(diagram, result);
        let relations = &diagram.relations;

        let (node_id_to_idx, obstacle_index, group_start): (
            HashMap<&str, usize>,
            Option<ObstacleIndex>,
            usize,
        ) = if quick_check_need_obstacle_index(result, relations) || !result.groups.is_empty() {
            let (idx, obs, gs) = build_obstacle_context(result);
            (idx, Some(obs), gs)
        } else {
            (HashMap::new(), None, 0)
        };
        let group_ids = sorted_group_obstacle_ids(result);

        let self_loop_idx = self_loop_indices(relations);
        let mut edges = Vec::with_capacity(relations.len());
        for (i, rel) in relations.iter().enumerate() {
            if rel.from.as_str() == rel.to.as_str() {
                if ctx.nodes.contains_key(rel.from.as_str()) {
                    let loop_index = self_loop_idx.get(&i).copied().unwrap_or(0);
                    edges.push(SplineEdgePlan::SelfLoop { loop_index });
                } else {
                    edges.push(SplineEdgePlan::Missing);
                }
                continue;
            }
            match resolve_endpoints(&ctx, rel, i) {
                Some((ep, label_off)) => edges.push(SplineEdgePlan::Normal { ep, label_off }),
                None => edges.push(SplineEdgePlan::Missing),
            }
        }

        SplineDraft {
            diagram,
            nodes: &result.nodes,
            edges,
            node_id_to_idx,
            obstacle_index,
            group_start,
            group_ids,
            tension: self.config.tension,
        }
    }

    fn solve(&self, draft: &SplineDraft<'_>) -> RecipeSolution {
        let mut solution = RouteSolution::default();
        let mut label_plans: Vec<Option<EdgeLabelPlan>> = Vec::with_capacity(draft.edges.len());

        for (i, plan) in draft.edges.iter().enumerate() {
            match plan {
                SplineEdgePlan::Missing => {
                    solution
                        .paths
                        .push(RoutePath::Empty(EmptyRouteReason::MissingEndpoint));
                    solution.ports.push(EndpointAssignment::minimal(
                        StableEdgeId(i), Port::Bottom, Port::Top,
                    ));
                    label_plans.push(None);
                }
                SplineEdgePlan::SelfLoop { loop_index } => {
                    let rel = &draft.diagram.relations[i];
                    let node = draft
                        .nodes
                        .get(rel.from.as_str())
                        .expect("compile 已确认自环节点存在");
                    // Slice D1：自环只产 topology + anchor 标签计划，几何经 materializer 物化。
                    let sol = solve_self_loop(node, *loop_index, SelfLoopStyle::Curved);
                    solution.paths.push(sol.path);
                    solution.ports.push(EndpointAssignment::minimal(
                        StableEdgeId(i), sol.from_port, sol.to_port,
                    ));
                    label_plans.push(Some(EdgeLabelPlan {
                        middle_t: 0.5,
                        offset: sol.label_offset,
                        sample_path: None,
                        anchor: Some(sol.label_anchor),
                    }));
                }
                SplineEdgePlan::Normal { ep, label_off } => {
                    let from_idx = draft
                        .node_id_to_idx
                        .get(ep.from_id.as_str())
                        .copied()
                        .unwrap_or(usize::MAX);
                    let to_idx = draft
                        .node_id_to_idx
                        .get(ep.to_id.as_str())
                        .copied()
                        .unwrap_or(usize::MAX);

                    let mut skip = vec![from_idx, to_idx];
                    if !draft.group_ids.is_empty() {
                        let maps =
                            crate::layout::quality::lint::GroupInteriorMaps::new(draft.diagram);
                        let from_rel = maps.related_groups(ep.from_id.as_str());
                        let to_rel = maps.related_groups(ep.to_id.as_str());
                        for (gi, gid) in draft.group_ids.iter().enumerate() {
                            if from_rel.contains(gid.as_str()) || to_rel.contains(gid.as_str()) {
                                skip.push(draft.group_start + gi);
                            }
                        }
                    }

                    let detour_path = if let Some(ref obstacle_index) = draft.obstacle_index {
                        obstacle_index.shortest_path(ep.start, ep.end, &skip)
                    } else {
                        Vec::new()
                    };

                    // 先定最终几何与用于标签的采样折线（与原 router 完全一致）。
                    let (path, sampled) = if detour_path.is_empty() {
                        // 无障碍：简单贝塞尔，控制点 + 平行中段偏移。
                        let mut cp = compute_bezier_controls(
                            ep.start.x,
                            ep.start.y,
                            ep.end.x,
                            ep.end.y,
                            ep.from_port,
                            ep.to_port,
                            draft.tension,
                        );
                        cp[0].x += ep.mid_ox;
                        cp[0].y += ep.mid_oy;
                        cp[1].x += ep.mid_ox;
                        cp[1].y += ep.mid_oy;
                        let sampled =
                            sample_bezier(ep.start, cp[0], cp[1], ep.end, BEZIER_SAMPLES_PER_SEGMENT);
                        let path = RoutePath::Cubic(CubicPath {
                            start: ep.start,
                            end: ep.end,
                            controls: cp,
                        });
                        (path, sampled)
                    } else {
                        // 有障碍：绕行折线 + 平行中段偏移 → 多段样条拟合采样。
                        let mut full_path = build_full_path(ep.start, &detour_path, ep.end);
                        if ep.mid_ox.abs() > 0.1 || ep.mid_oy.abs() > 0.1 {
                            let n = full_path.len();
                            if n > 2 {
                                for p in &mut full_path[1..n - 1] {
                                    p.x += ep.mid_ox;
                                    p.y += ep.mid_oy;
                                }
                            }
                        }
                        let sampled = fit_multi_segment_spline(&full_path, BEZIER_SAMPLES_PER_SEGMENT);
                        let path = RoutePath::Spline(SplinePath {
                            points: sampled.clone(),
                        });
                        (path, sampled)
                    };

                    solution.paths.push(path);
                    solution.ports.push(EndpointAssignment::minimal(
                        StableEdgeId(i), ep.from_port, ep.to_port,
                    ));
                    let rel = &draft.diagram.relations[i];
                    label_plans.push(Some(EdgeLabelPlan {
                        middle_t: label_t_for_diagram(draft.diagram, rel),
                        offset: Point::new(label_off.ox, label_off.oy),
                        // 样条标签始终沿采样折线放置（含无障碍的 Bezier 情形）。
                        sample_path: Some(sampled),
                        anchor: None,
                    }));
                }
            }
        }

        RecipeSolution {
            solution,
            label_plans,
            orthogonal_debug: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::routing::common::test_fixtures::{make_diagram_with_layout, route_via_prepared};
    use crate::layout::routing::edge_routing_spline::route_edges_spline;
    use crate::layout::routing::recipe::RecipeRouter;
    use crate::layout::types::PathGeometry;

    #[test]
    fn compile_produces_one_plan_per_edge() {
        let (diagram, result) = make_diagram_with_layout(
            vec![("a", 40.0, 40.0), ("b", 40.0, 200.0)],
            vec![("a", "b", None), ("a", "a", None)],
        );
        let draft = SplineRecipe::default().compile(&diagram, &result);
        assert_eq!(draft.edge_count(), 2);
    }

    #[test]
    fn compile_skips_obstacle_index_when_no_crossing_possible() {
        let (diagram, result) = make_diagram_with_layout(
            vec![("a", 40.0, 40.0), ("b", 260.0, 40.0)],
            vec![("a", "b", None)],
        );
        let draft = SplineRecipe::default().compile(&diagram, &result);
        assert!(!draft.has_obstacle_index(), "两节点应跳过避障索引构建");
    }

    /// PathGeometry 未派生 PartialEq，逐变体逐字段比较（Point: PartialEq）。
    fn geom_eq(a: &PathGeometry, b: &PathGeometry) -> bool {
        use PathGeometry as G;
        match (a, b) {
            (G::Straight { start: s1, end: e1 }, G::Straight { start: s2, end: e2 }) => {
                s1 == s2 && e1 == e2
            }
            (
                G::Bezier {
                    start: s1,
                    end: e1,
                    controls: c1,
                },
                G::Bezier {
                    start: s2,
                    end: e2,
                    controls: c2,
                },
            ) => s1 == s2 && e1 == e2 && c1 == c2,
            (G::Polyline { points: p1 }, G::Polyline { points: p2 }) => p1 == p2,
            _ => false,
        }
    }

    /// RecipeRouter<SplineRecipe> 与原 route_edges_spline 输出等价（同一 EdgeLayout 序列）。
    #[test]
    fn recipe_router_matches_legacy_spline() {
        let cases: Vec<(Vec<(&str, f64, f64)>, Vec<(&str, &str, Option<&str>)>)> = vec![
            // 无障碍：简单贝塞尔。
            (
                vec![("a", 40.0, 40.0), ("b", 260.0, 40.0)],
                vec![("a", "b", Some("lbl"))],
            ),
            // 双向边（平行偏移）。
            (
                vec![("a", 40.0, 40.0), ("b", 40.0, 200.0)],
                vec![("a", "b", None), ("b", "a", None)],
            ),
            // 有障碍：绕行多段样条（三垂直节点，a→c 穿过 b）。
            (
                vec![("a", 120.0, 40.0), ("b", 120.0, 150.0), ("c", 120.0, 300.0)],
                vec![("a", "c", Some("through"))],
            ),
            // 自环边。
            (
                vec![("a", 100.0, 100.0), ("b", 300.0, 100.0)],
                vec![("a", "a", Some("retry")), ("a", "b", None)],
            ),
            // 缺失节点。
            (vec![("a", 40.0, 40.0)], vec![("a", "missing", None)]),
        ];

        for (entities, relations) in cases {
            let (diagram, result) = make_diagram_with_layout(entities, relations);
            let legacy = route_edges_spline(&diagram, result.clone(), BezierConfig::default());
            let router = RecipeRouter::new(SplineRecipe::default());
            let recipe = route_via_prepared(&router, &diagram, &result);

            assert_eq!(legacy.edges.len(), recipe.edges.len(), "边数应一致");
            for (idx, (l, r)) in legacy.edges.iter().zip(recipe.edges.iter()).enumerate() {
                assert!(
                    geom_eq(&l.geometry, &r.geometry),
                    "边 {idx} 几何应字节一致: {:?} vs {:?}",
                    l.geometry,
                    r.geometry
                );
                assert_eq!(l.from_port, r.from_port, "边 {idx} from_port");
                assert_eq!(l.to_port, r.to_port, "边 {idx} to_port");
                assert_eq!(l.labels.len(), r.labels.len(), "边 {idx} 标签数");
                // R9a：标签位置由 LabelSolver::solve() 统一消解（有意改善）。
                for (ll, rl) in l.labels.iter().zip(r.labels.iter()) {
                    assert_eq!(ll.text, rl.text);
                }
            }
        }
    }
}
