//! 贝塞尔路由 Recipe（doc16 §5.1 / R3 Slice 3.2b）。
//!
//! 把 [`super::super::edge_routing_bezier::route_edges_bezier`] 拆分到 [`RoutingRecipe`]
//! 接口后面：
//!
//! - `compile`：构造 [`RoutingContext`]（平行边偏移）、懒构建避障索引、逐边解析端点 /
//!   识别自环，产出逐边 [`BezierEdgePlan`]。
//! - `solve`：逐边产 family-neutral [`RoutePath`]：普通边 → `Cubic`（控制点由端口方向自
//!   适应延伸 + 平行中段偏移）；穿障边 → 退化 `Spline`（可见性图绕行折线，记 `DegradedReason`）；
//!   自环 → 复用共享 [`solve_self_loop`] 产出的 `Cubic` topology + anchor 标签计划。
//!
//! 几何物化 / 标签放置由 [`super::RecipeRouter`] 统一驱动，最终 SVG 与原 `BezierRouting`
//! 字节一致。
//!
//! ## byte-identical 要点
//!
//! - 普通边 `Cubic` → materialize `Bezier` → LabelSolver 用 `cubic_bezier_point` 取点；
//!   穿障退化 `Spline` → materialize `Polyline` → LabelSolver 用 `point_at_path_t` 取点。
//!   与原 router「先定最终几何再采样建标签」逐字一致。
//! - 自环标签锚定环 apex（[`EdgeLabelPlan::anchor`]），与遗留 `|_| apex` 取点字节一致。

use std::collections::HashMap;

use crate::ast::Diagram;
use crate::layout::algorithm_config::AlgorithmOptionSpec;
use crate::layout::geometry::Point;
use crate::layout::pipeline::plan::ResolvedAlgoOptions;
use crate::layout::routing::common::edge_geometry::{compute_bezier_controls, parse_label_t};
use crate::layout::routing::common::obstacle_check::curve_intersects_obstacles;
use crate::layout::routing::common::routing_skeleton::{
    build_obstacle_context, quick_check_need_obstacle_index, resolve_endpoints, EdgeEndpoints,
    LabelOffset, RoutingContext,
};
use crate::layout::routing::common::self_loop::{
    self_loop_indices, solve_self_loop, SelfLoopStyle,
};
use crate::layout::routing::edge_routing_bezier::{BezierConfig, BEZIER_OPTIONS};
use crate::layout::routing::model::{
    CubicPath, DegradedReason, EmptyRouteReason, EndpointAssignment, GeometryFamily, RoutePath,
    RouteSolution, SplinePath, StableEdgeId,
};
use crate::layout::routing::visibility::ObstacleIndex;
use crate::layout::types::{LayoutResult, Port};
use crate::layout::{EdgeLayout, NodeLayout, PathGeometry};
use crate::types::DiagramType;

use super::{EdgeLabelPlan, RecipeSolution, RoutingRecipe};

/// 贝塞尔路由适用的内置图类型（与原 `BezierRouting` 一致）。
const APPLICABLE_TYPES: &[DiagramType] = &[
    DiagramType::Flowchart,
    DiagramType::Architecture,
    DiagramType::State,
    DiagramType::Er,
    DiagramType::Mindmap,
];

/// 贝塞尔边路由 Recipe（构造时注入已解析的 tension）。
pub struct BezierRecipe {
    config: BezierConfig,
}

impl Default for BezierRecipe {
    fn default() -> Self {
        Self {
            config: BezierConfig::default(),
        }
    }
}

impl BezierRecipe {
    /// 从 DSL 已解析 option 构造（与 `BezierRouting::from_options` 同源）。
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

/// 一条边的已编译 Bezier 计划。
enum BezierEdgePlan {
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

/// 贝塞尔路由的已编译 Draft。
pub struct BezierDraft<'a> {
    diagram: &'a Diagram,
    nodes: &'a HashMap<String, NodeLayout>,
    edges: Vec<BezierEdgePlan>,
    /// node id → 障碍索引下标（借用 `result.nodes` 的 key），懒构建时为空。
    node_id_to_idx: HashMap<&'a str, usize>,
    /// 避障索引（懒构建：无边可能穿障时为 `None`）。
    obstacle_index: Option<ObstacleIndex>,
    tension: f64,
}

impl<'a> BezierDraft<'a> {
    /// 逐边计划数量（供 compile 独立单测）。
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// 是否已构建避障索引（供懒构建单测）。
    pub fn has_obstacle_index(&self) -> bool {
        self.obstacle_index.is_some()
    }
}

impl RoutingRecipe for BezierRecipe {
    type Draft<'a> = BezierDraft<'a>;

    fn name(&self) -> &'static str {
        "bezier"
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

    fn compile<'a>(&self, diagram: &'a Diagram, result: &'a LayoutResult) -> BezierDraft<'a> {
        let ctx = RoutingContext::new(diagram, result);
        let relations = &diagram.relations;

        // 懒构建避障索引：快速预检无边可能穿障时跳过 O(n²) 构建。
        let (node_id_to_idx, obstacle_index): (HashMap<&str, usize>, Option<ObstacleIndex>) =
            if quick_check_need_obstacle_index(result, relations) {
                let (idx, obs, _) = build_obstacle_context(result);
                (idx, Some(obs))
            } else {
                (HashMap::new(), None)
            };

        let self_loop_idx = self_loop_indices(relations);
        let mut edges = Vec::with_capacity(relations.len());
        for (i, rel) in relations.iter().enumerate() {
            if rel.from.as_str() == rel.to.as_str() {
                if ctx.nodes.contains_key(rel.from.as_str()) {
                    let loop_index = self_loop_idx.get(&i).copied().unwrap_or(0);
                    edges.push(BezierEdgePlan::SelfLoop { loop_index });
                } else {
                    edges.push(BezierEdgePlan::Missing);
                }
                continue;
            }
            match resolve_endpoints(&ctx, rel, i) {
                Some((ep, label_off)) => edges.push(BezierEdgePlan::Normal { ep, label_off }),
                None => edges.push(BezierEdgePlan::Missing),
            }
        }

        BezierDraft {
            diagram,
            nodes: &result.nodes,
            edges,
            node_id_to_idx,
            obstacle_index,
            tension: self.config.tension,
        }
    }

    fn solve(&self, draft: &BezierDraft<'_>) -> RecipeSolution {
        let mut solution = RouteSolution::default();
        let mut label_plans: Vec<Option<EdgeLabelPlan>> = Vec::with_capacity(draft.edges.len());

        for (i, plan) in draft.edges.iter().enumerate() {
            match plan {
                BezierEdgePlan::Missing => {
                    solution
                        .paths
                        .push(RoutePath::Empty(EmptyRouteReason::MissingEndpoint));
                    solution.ports.push(EndpointAssignment::minimal(
                        StableEdgeId(i), Port::Bottom, Port::Top,
                    ));
                    label_plans.push(None);
                }
                BezierEdgePlan::SelfLoop { loop_index } => {
                    let rel = &draft.diagram.relations[i];
                    // compile 已保证节点存在。
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
                BezierEdgePlan::Normal { ep, label_off } => {
                    let base = compute_bezier_controls(
                        ep.start.x,
                        ep.start.y,
                        ep.end.x,
                        ep.end.y,
                        ep.from_port,
                        ep.to_port,
                        draft.tension,
                    );
                    // 平行边法向偏移只作用于控制点，端点保持贴边。
                    let controls = [
                        Point::new(base[0].x + ep.mid_ox, base[0].y + ep.mid_oy),
                        Point::new(base[1].x + ep.mid_ox, base[1].y + ep.mid_oy),
                    ];

                    let mut path = RoutePath::Cubic(CubicPath {
                        start: ep.start,
                        end: ep.end,
                        controls,
                    });

                    // 穿障检测：采样曲线，若穿过非端点节点则退化到可见性图绕行折线。
                    if let Some(ref obstacle_index) = draft.obstacle_index {
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
                        let skip = [from_idx, to_idx];
                        let probe = EdgeLayout {
                            geometry: PathGeometry::Bezier {
                                start: ep.start,
                                end: ep.end,
                                controls,
                            },
                            labels: Vec::new(),
                            from_port: ep.from_port,
                            to_port: ep.to_port,
                        };
                        if curve_intersects_obstacles(&probe, obstacle_index, &skip) {
                            let detour = obstacle_index.shortest_path(ep.start, ep.end, &skip);
                            if !detour.is_empty() {
                                path = RoutePath::Spline(SplinePath { points: detour });
                                // §4.5：geometry family 改变必须记录退化原因。
                                solution.diagnostics.degraded.push((
                                    StableEdgeId(i),
                                    DegradedReason::ObstacleFallback {
                                        requested: GeometryFamily::Cubic,
                                        actual: GeometryFamily::Spline,
                                    },
                                ));
                            }
                        }
                    }

                    solution.paths.push(path);
                    solution.ports.push(EndpointAssignment::minimal(
                        StableEdgeId(i), ep.from_port, ep.to_port,
                    ));
                    let rel = &draft.diagram.relations[i];
                    label_plans.push(Some(EdgeLabelPlan {
                        middle_t: parse_label_t(rel),
                        offset: Point::new(label_off.ox, label_off.oy),
                        sample_path: None,
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
    use crate::layout::routing::edge_routing_bezier::route_edges_bezier;
    use crate::layout::routing::recipe::RecipeRouter;

    #[test]
    fn compile_produces_one_plan_per_edge() {
        let (diagram, result) = make_diagram_with_layout(
            vec![("a", 40.0, 40.0), ("b", 40.0, 200.0)],
            vec![("a", "b", None), ("a", "a", None)],
        );
        let draft = BezierRecipe::default().compile(&diagram, &result);
        assert_eq!(draft.edge_count(), 2);
    }

    #[test]
    fn compile_skips_obstacle_index_when_no_crossing_possible() {
        // 仅两个节点 → 不可能穿障 → 懒构建跳过。
        let (diagram, result) = make_diagram_with_layout(
            vec![("a", 40.0, 40.0), ("b", 260.0, 40.0)],
            vec![("a", "b", None)],
        );
        let draft = BezierRecipe::default().compile(&diagram, &result);
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

    /// RecipeRouter<BezierRecipe> 与原 route_edges_bezier 输出等价（同一 EdgeLayout 序列）。
    #[test]
    fn recipe_router_matches_legacy_bezier() {
        let cases: Vec<(Vec<(&str, f64, f64)>, Vec<(&str, &str, Option<&str>)>)> = vec![
            // 单条水平边（保持 bezier）。
            (
                vec![("a", 40.0, 40.0), ("b", 260.0, 40.0)],
                vec![("a", "b", Some("lbl"))],
            ),
            // 双向边（平行偏移 → 控制点分离）。
            (
                vec![("a", 40.0, 40.0), ("b", 40.0, 200.0)],
                vec![("a", "b", None), ("b", "a", None)],
            ),
            // 穿障退化为绕行折线：三个垂直对齐节点，a→c 穿过 b。
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
            let legacy = route_edges_bezier(&diagram, result.clone(), BezierConfig::default());
            let router = RecipeRouter::new(BezierRecipe::default());
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
