//! 有机曲线（MindMap 绽放）边路由 Recipe（doc16 §5.1 / R3 Slice 3.4）。
//!
//! 把 [`super::super::edge_routing_organic::route_edges_organic`] 拆分到 [`RoutingRecipe`]
//! 接口后面：
//!
//! - `compile`：曲线风格预设 → 基础张力/肩长，读取 mindmap 深度作层级衰减，两轮解析端点
//!   （第一轮 `resolve_endpoints`，第二轮连接点均匀分布），逐边编译绽放贝塞尔控制点
//!   （共享 [`organic_control_points`]）+ 避障 skip（含同父兄弟）。
//! - `solve`：逐边产 family-neutral [`RoutePath`]：普通边 → `Cubic`；穿障时 mindmap 沿绕行
//!   中点拉弓保持 `Cubic`（标签走 24 段采样折线），非 mindmap 退化 `Spline`（可见性图绕行）。
//! - `finalize`：走默认 [`finalize_edges`](crate::layout::routing::common::routing_skeleton::finalize_edges)
//!   （mindmap 清标签 + `resolve_label_overlaps`），与 legacy 一致。
//!
//! ## byte-identical 要点
//!
//! - 普通 / mindmap 拉弓 `Cubic` → materialize `Bezier` → LabelSolver 用 `cubic_bezier_point`；
//!   mindmap 拉弓标签历史上沿 `sampled_path(24)` 放置，故携带 `sample_path`。
//! - 非 mindmap 退化 `Spline` → materialize `Polyline` → LabelSolver 用 `point_at_path_t`，
//!   偏移沿用端点 `label_off`（**非** circular 的 `(0,-6)`）。

use std::collections::HashMap;

use crate::ast::Diagram;
use crate::layout::algorithm_config::AlgorithmOptionSpec;
use crate::layout::geometry::Point;
use crate::layout::pipeline::plan::ResolvedAlgoOptions;
use crate::layout::routing::common::edge_geometry::{
    parse_label_t, DEFAULT_BEZIER_TENSION, DEFAULT_SHOULDER_RATIO,
};
use crate::layout::routing::common::obstacle_check::curve_intersects_obstacles;
use crate::layout::routing::common::routing_skeleton::{
    build_obstacle_context, quick_check_need_obstacle_index, resolve_endpoints, EdgeEndpoints,
    LabelOffset, RoutingContext,
};
use crate::layout::routing::edge_routing_organic::{
    bow_bezier_around_obstacles, coerce_mindmap_start_to_horizontal_port,
    compute_distributed_port_points, organic_control_points, snap_mindmap_end, OrganicConfig,
    ORGANIC_OPTIONS,
};
use crate::layout::routing::model::{
    CubicPath, DegradedReason, EmptyRouteReason, EndpointAssignment, GeometryFamily, RoutePath,
    RouteSolution, SplinePath, StableEdgeId,
};
use crate::layout::routing::visibility::ObstacleIndex;
use crate::layout::types::{EdgeLayout, LayoutResult, PathGeometry, Port};
use crate::types::DiagramType;

use super::{EdgeLabelPlan, RecipeSolution, RoutingRecipe};

/// 有机曲线边路由适用的内置图类型（与原 `OrganicRouting` 一致）。
const APPLICABLE_TYPES: &[DiagramType] = &[
    DiagramType::Flowchart,
    DiagramType::Architecture,
    DiagramType::State,
    DiagramType::Er,
    DiagramType::Mindmap,
];

/// 有机曲线边路由 Recipe（构造时注入已解析的曲线参数）。
pub struct OrganicRecipe {
    config: OrganicConfig,
}

impl Default for OrganicRecipe {
    fn default() -> Self {
        Self::from_options(&ResolvedAlgoOptions::from_spec_defaults(ORGANIC_OPTIONS))
    }
}

impl OrganicRecipe {
    /// 从 DSL 已解析 option 构造（与 `OrganicRouting::from_options` 同源）。
    pub fn from_options(options: &ResolvedAlgoOptions) -> Self {
        Self {
            config: OrganicConfig {
                tension: options.get_or_default(&ORGANIC_OPTIONS[0]),
                shoulder_ratio: options.get_or_default(&ORGANIC_OPTIONS[1]),
                depth_decay: options.get_or_default(&ORGANIC_OPTIONS[2]),
                curve_style: options.get_or_default(&ORGANIC_OPTIONS[3]),
                port_distribution: options.get_or_default(&ORGANIC_OPTIONS[4]),
            },
        }
    }

    /// 直接注入配置（供单测）。
    pub fn new(config: OrganicConfig) -> Self {
        Self { config }
    }
}

/// 一条边的已编译 organic 计划。
enum OrganicEdgePlan {
    /// 端点缺失：物化为空几何（== `EdgeLayout::empty()`），无标签。
    Missing,
    /// 普通边：已编译绽放贝塞尔几何 + 标签偏移 + 避障 skip / 拉弓参数。
    Normal {
        start: Point,
        end: Point,
        controls: [Point; 2],
        from_port: Port,
        to_port: Port,
        label_ox: f64,
        label_oy: f64,
        /// 避障 skip（端点 + 同父兄弟）；`obstacle_index` 为 `None` 时为空。
        skip: Vec<usize>,
        /// mindmap 拉弓所需的有效张力 / 自适应肩长。
        tension: f64,
        shoulder: f64,
    },
}

/// organic 路由的已编译 Draft。
pub struct OrganicDraft<'a> {
    diagram: &'a Diagram,
    edges: Vec<OrganicEdgePlan>,
    /// 避障索引（懒构建：mindmap 树 / 无边可能穿障时为 `None`）。
    obstacle_index: Option<ObstacleIndex>,
    /// 是否 mindmap（决定穿障退化走拉弓保曲线还是折线）。
    is_mindmap: bool,
}

impl<'a> OrganicDraft<'a> {
    /// 逐边计划数量（供 compile 独立单测）。
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// 是否已构建避障索引（供懒构建单测）。
    pub fn has_obstacle_index(&self) -> bool {
        self.obstacle_index.is_some()
    }
}

impl RoutingRecipe for OrganicRecipe {
    type Draft<'a> = OrganicDraft<'a>;

    fn name(&self) -> &'static str {
        "organic"
    }

    fn applicable_diagram_types(&self) -> &'static [DiagramType] {
        APPLICABLE_TYPES
    }

    fn supports_custom(&self) -> bool {
        true
    }

    
    fn option_specs(&self) -> &'static [AlgorithmOptionSpec] {
        ORGANIC_OPTIONS
    }

    fn compile<'a>(&self, diagram: &'a Diagram, result: &'a LayoutResult) -> OrganicDraft<'a> {
        let relations = &diagram.relations;
        let depth_decay = self.config.depth_decay;
        let curve_style = self.config.curve_style;
        let port_distribution = self.config.port_distribution;
        let ctx = RoutingContext::new(diagram, result);
        let node_depths = result.hints.mindmap_depths.as_ref();

        // ── 曲线风格预设（用户显式配置覆盖预设）──
        let (style_tension, style_shoulder): (f64, f64) = match curve_style as i32 {
            1 => (0.9, 0.55),
            2 => (0.4, 0.25),
            _ => (self.config.tension, self.config.shoulder_ratio),
        };
        let base_tension = if (self.config.tension - DEFAULT_BEZIER_TENSION).abs() < 0.001 {
            style_tension
        } else {
            self.config.tension
        };
        let base_shoulder_ratio =
            if (self.config.shoulder_ratio - DEFAULT_SHOULDER_RATIO).abs() < 0.001 {
                style_shoulder
            } else {
                self.config.shoulder_ratio
            };

        let is_mindmap = matches!(diagram.diagram_type, DiagramType::Mindmap);
        // mindmap 树父子直连不穿障 → 跳过 O(n²) 构建；非 mindmap 走快速粗检。
        let need_obstacle_index = if is_mindmap && node_depths.is_some() {
            false
        } else {
            quick_check_need_obstacle_index(result, relations)
        };
        let (node_id_to_idx, obstacle_index): (HashMap<&str, usize>, Option<ObstacleIndex>) =
            if need_obstacle_index {
                let (idx, obs, _) = build_obstacle_context(result);
                (idx, Some(obs))
            } else {
                (HashMap::new(), None)
            };

        // 父子映射：穿障检测跳过同父兄弟，避免扇出曲线误判。
        let mut children_of: HashMap<&str, Vec<&str>> = HashMap::new();
        for rel in relations {
            children_of
                .entry(rel.from.as_str())
                .or_default()
                .push(rel.to.as_str());
        }

        // ── 第一轮：解析所有边端点 ──
        let mut endpoints: Vec<Option<(EdgeEndpoints, LabelOffset)>> =
            Vec::with_capacity(relations.len());
        for (i, rel) in relations.iter().enumerate() {
            endpoints.push(resolve_endpoints(&ctx, rel, i));
        }

        // ── 连接点均匀分布 ──
        let distributed_starts: HashMap<usize, (f64, f64)> = if port_distribution > 0.01 {
            compute_distributed_port_points(result, relations, &endpoints, port_distribution)
        } else {
            HashMap::new()
        };

        let mut edges = Vec::with_capacity(relations.len());
        for (i, _rel) in relations.iter().enumerate() {
            let Some((ep, label_off)) = endpoints[i].clone() else {
                edges.push(OrganicEdgePlan::Missing);
                continue;
            };

            // ── 层级感知参数 ──
            let (effective_tension, effective_shoulder) = if let Some(depths) = node_depths {
                let from_depth = depths.get(ep.from_id.as_str()).copied().unwrap_or(0);
                let decay = depth_decay.powi(from_depth as i32);
                let t = (base_tension * decay).max(base_tension * 0.48);
                let s = (base_shoulder_ratio * decay).max(base_shoulder_ratio * 0.55);
                (t, s)
            } else {
                (base_tension, base_shoulder_ratio)
            };

            // ── 起点 / 起点端口 ──
            let (start_pt, from_port) = if let Some((sx, sy)) = distributed_starts.get(&i) {
                let port = if let Some(nl) = result.nodes.get(ep.from_id.as_str()) {
                    let cx = nl.x + nl.width / 2.0;
                    if *sx >= cx {
                        Port::Right
                    } else {
                        Port::Left
                    }
                } else {
                    ep.from_port
                };
                (Point::new(*sx, *sy), port)
            } else if is_mindmap {
                coerce_mindmap_start_to_horizontal_port(result, &ep)
            } else {
                (ep.start, ep.from_port)
            };

            // ── 终点 / 终点端口 ──
            let (end_pt, to_port) = if is_mindmap {
                snap_mindmap_end(result, &ep, &start_pt)
            } else {
                (ep.end, ep.to_port)
            };

            let (controls, adaptive_shoulder) = organic_control_points(
                result,
                &ep,
                start_pt,
                from_port,
                end_pt,
                to_port,
                effective_tension,
                effective_shoulder,
            );

            // ── 避障 skip：端点 + 同父兄弟（仅在有避障索引时构建）──
            let mut skip = Vec::new();
            if obstacle_index.is_some() {
                let from_idx = node_id_to_idx
                    .get(ep.from_id.as_str())
                    .copied()
                    .unwrap_or(usize::MAX);
                let to_idx = node_id_to_idx
                    .get(ep.to_id.as_str())
                    .copied()
                    .unwrap_or(usize::MAX);
                skip.push(from_idx);
                skip.push(to_idx);
                if let Some(siblings) = children_of.get(ep.from_id.as_str()) {
                    for sib in siblings {
                        if *sib != ep.to_id.as_str() {
                            if let Some(&idx) = node_id_to_idx.get(sib) {
                                skip.push(idx);
                            }
                        }
                    }
                }
            }

            edges.push(OrganicEdgePlan::Normal {
                start: start_pt,
                end: end_pt,
                controls,
                from_port,
                to_port,
                label_ox: label_off.ox,
                label_oy: label_off.oy,
                skip,
                tension: effective_tension,
                shoulder: adaptive_shoulder,
            });
        }

        OrganicDraft {
            diagram,
            edges,
            obstacle_index,
            is_mindmap,
        }
    }

    fn solve(&self, draft: &OrganicDraft<'_>) -> RecipeSolution {
        let mut solution = RouteSolution::default();
        let mut label_plans: Vec<Option<EdgeLabelPlan>> = Vec::with_capacity(draft.edges.len());

        for (i, plan) in draft.edges.iter().enumerate() {
            match plan {
                OrganicEdgePlan::Missing => {
                    solution
                        .paths
                        .push(RoutePath::Empty(EmptyRouteReason::MissingEndpoint));
                    solution.ports.push(EndpointAssignment::minimal(
                        StableEdgeId(i), Port::Bottom, Port::Top,
                    ));
                    label_plans.push(None);
                }
                OrganicEdgePlan::Normal {
                    start,
                    end,
                    controls,
                    from_port,
                    to_port,
                    label_ox,
                    label_oy,
                    skip,
                    tension,
                    shoulder,
                } => {
                    let rel = &draft.diagram.relations[i];
                    let middle_t = parse_label_t(rel);
                    let mut path = RoutePath::Cubic(CubicPath {
                        start: *start,
                        end: *end,
                        controls: *controls,
                    });
                    let mut label_plan = EdgeLabelPlan {
                        middle_t,
                        offset: Point::new(*label_ox, *label_oy),
                        sample_path: None,
                        anchor: None,
                    };

                    // 穿障检测：采样曲线，若穿过非端点节点则退化。
                    if let Some(obstacle_index) = draft.obstacle_index.as_ref() {
                        let probe = EdgeLayout {
                            geometry: PathGeometry::Bezier {
                                start: *start,
                                end: *end,
                                controls: *controls,
                            },
                            labels: Vec::new(),
                            from_port: *from_port,
                            to_port: *to_port,
                        };
                        if curve_intersects_obstacles(&probe, obstacle_index, skip) {
                            if draft.is_mindmap {
                                // 思维导图保持平滑贝塞尔：绕行中点拉弓。
                                if let Some(PathGeometry::Bezier {
                                    start: bs,
                                    end: be,
                                    controls: bc,
                                }) = bow_bezier_around_obstacles(
                                    &probe,
                                    obstacle_index,
                                    skip,
                                    *from_port,
                                    *to_port,
                                    *tension,
                                    *shoulder,
                                ) {
                                    path = RoutePath::Cubic(CubicPath {
                                        start: bs,
                                        end: be,
                                        controls: bc,
                                    });
                                    // 标签沿拉弓后曲线的 24 段采样折线放置。
                                    let sampled = EdgeLayout {
                                        geometry: PathGeometry::Bezier {
                                            start: bs,
                                            end: be,
                                            controls: bc,
                                        },
                                        labels: Vec::new(),
                                        from_port: *from_port,
                                        to_port: *to_port,
                                    }
                                    .sampled_path(24);
                                    label_plan.sample_path = Some(sampled);
                                }
                            } else {
                                let detour = obstacle_index.shortest_path(*start, *end, skip);
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
                    }

                    solution.paths.push(path);
                    solution.ports.push(EndpointAssignment::minimal(
                        StableEdgeId(i), *from_port, *to_port,
                    ));
                    label_plans.push(Some(label_plan));
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
    use crate::layout::routing::common::test_fixtures::{make_diagram_grid, make_diagram_with_layout, route_via_prepared};
    use crate::layout::routing::edge_routing_organic::route_edges_organic;
    use crate::layout::routing::recipe::RecipeRouter;

    #[test]
    fn compile_produces_one_plan_per_edge() {
        let (diagram, result) = make_diagram_with_layout(
            vec![("a", 40.0, 40.0), ("b", 40.0, 200.0)],
            vec![("a", "b", None), ("a", "missing", None)],
        );
        let draft = OrganicRecipe::default().compile(&diagram, &result);
        assert_eq!(draft.edge_count(), 2);
    }

    #[test]
    fn compile_skips_obstacle_index_when_no_crossing_possible() {
        let (diagram, result) = make_diagram_with_layout(
            vec![("a", 40.0, 40.0), ("b", 260.0, 40.0)],
            vec![("a", "b", None)],
        );
        let draft = OrganicRecipe::default().compile(&diagram, &result);
        assert!(!draft.has_obstacle_index(), "两节点应跳过避障索引构建");
    }

    /// PathGeometry 未派生 PartialEq，逐变体逐字段比较。
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

    fn assert_edges_eq(legacy: &LayoutResult, recipe: &crate::layout::RoutingProduct) {
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
                assert_eq!(ll.text, rl.text, "边 {idx} 标签文本");
            }
        }
    }

    /// RecipeRouter<OrganicRecipe> 与原 route_edges_organic 输出等价。
    #[test]
    fn recipe_router_matches_legacy_organic() {
        let cases: Vec<(Vec<(&str, f64, f64)>, Vec<(&str, &str, Option<&str>)>)> = vec![
            // 普通弯边（保持 Cubic）。
            (
                vec![("a", 40.0, 40.0), ("b", 300.0, 200.0)],
                vec![("a", "b", Some("lbl"))],
            ),
            // 一父两子扇出（连接点均匀分布）。
            (
                vec![("a", 40.0, 120.0), ("b", 320.0, 40.0), ("c", 320.0, 240.0)],
                vec![("a", "b", None), ("a", "c", Some("branch"))],
            ),
            // 缺失节点。
            (vec![("a", 40.0, 40.0)], vec![("a", "missing", None)]),
        ];

        let config = OrganicConfig::default();
        for (entities, relations) in cases {
            let (diagram, result) = make_diagram_with_layout(entities, relations);
            let legacy = route_edges_organic(&diagram, result.clone(), config);
            let router = RecipeRouter::new(OrganicRecipe::new(config));
            let recipe = route_via_prepared(&router, &diagram, &result);
            assert_edges_eq(&legacy, &recipe);
        }
    }

    /// 穿障退化场景：网格中间节点作障碍，a→c 退化为 Polyline 绕行。
    #[test]
    fn recipe_router_matches_legacy_organic_with_obstacle() {
        let config = OrganicConfig::default();
        let (diagram, result) = make_diagram_grid(3, 3);
        let legacy = route_edges_organic(&diagram, result.clone(), config);
        let router = RecipeRouter::new(OrganicRecipe::new(config));
        let recipe = route_via_prepared(&router, &diagram, &result);
        assert_edges_eq(&legacy, &recipe);
    }
}
