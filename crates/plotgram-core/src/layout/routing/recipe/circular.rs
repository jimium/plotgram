//! 圆形布局弧形边路由 Recipe（doc16 §5.1 / R3 Slice 3.4）。
//!
//! 把 [`super::super::edge_routing_circular::route_edges_circular`] 拆分到 [`RoutingRecipe`]
//! 接口后面：
//!
//! - `compile`：解析圆簇（无圆簇 / 无边 → `should_skip`）、构造节点圆位与 lane 偏移、
//!   懒构建避障索引，逐边编译几何：同圆 [`intra_circle_bezier`]、跨圆
//!   [`inter_circle_bezier`]、自环走共享 [`solve_self_loop`]。
//! - `solve`：逐边产 family-neutral [`RoutePath`]：弧形边 → `Radial`；穿障退化 →
//!   `Spline`（可见性图绕行 / outer 折线兜底）；自环无退化 → `Cubic` + anchor 标签计划。
//! - `finalize`：覆盖默认收尾，用 circular 专属径向推开 [`RadialPlacer`]（与 legacy 一致）。
//!
//! ## byte-identical 要点
//!
//! - 弧形 `Radial` → materialize `Bezier` → LabelSolver 用 `cubic_bezier_point` 取点；
//!   `label_t` / `label_offset` 由 [`CircularBezier`] 提供（与 legacy 逐边 `build_edge_labels` 同源）。
//! - 穿障退化 `Spline` → materialize `Polyline` → LabelSolver 用 `point_at_path_t` 取点，
//!   偏移固定 `(0, -6)`、`middle_t = parse_label_t`，与 legacy 折线重建逐字一致。
//! - 自环标签锚定环 apex（[`EdgeLabelPlan::anchor`]），与遗留 `|_| apex` 取点字节一致。
//! - 收尾 `RadialPlacer` 与 legacy 同一实例、同一 `LabelContext`，标签推开字节一致。

use std::collections::HashMap;

use crate::ast::Diagram;
use crate::layout::geometry::Point;
use crate::layout::routing::common::circular_support::{resolve_circle_groups, APPLICABLE_TYPES};
use crate::layout::routing::common::edge_geometry::parse_label_t;
use crate::layout::routing::common::label_placement::{LabelContext, LabelPlacer, RadialPlacer};
use crate::layout::routing::common::obstacle_check::curve_intersects_obstacles;
use crate::layout::routing::common::routing_skeleton::{
    build_obstacle_context, quick_check_need_obstacle_index,
};
use crate::layout::routing::common::self_loop::{self_loop_indices, solve_self_loop, SelfLoopStyle};
use crate::layout::routing::edge_routing_circular::{
    build_node_placement, compute_lane_offsets, inter_circle_bezier, intra_circle_bezier,
    outer_polyline_detour, CircularBezier,
};
use crate::layout::routing::model::{
    DegradedReason, EmptyRouteReason, EndpointAssignment, GeometryFamily, RadialPath, RoutePath,
    RouteSolution, SplinePath, StableEdgeId,
};
use crate::layout::routing::visibility::ObstacleIndex;
use crate::layout::types::{EdgeLayout, LayoutResult, NodeLayout, PathGeometry, Port};
use crate::types::DiagramType;

use super::{EdgeLabelPlan, RecipeSolution, RoutingRecipe};

/// 圆形弧形边路由 Recipe（无状态纯映射）。
#[derive(Default)]
pub struct CircularRecipe;

/// 一条边的已编译 circular 计划。
enum CircularEdgePlan<'a> {
    /// 端点缺失：物化为空几何（== `EdgeLayout::empty()`），无标签。
    Missing,
    /// 自环边：共享 helper 整体产出（几何 + 标签一体）；仍参与穿障退化决策。
    SelfLoop {
        loop_index: usize,
        node_id: &'a str,
        /// 避障 skip：`[from_idx, to_idx]`（自环两端同一节点）。
        skip: [usize; 2],
    },
    /// 弧形边（同圆 / 跨圆）：已编译弧形贝塞尔几何 + 标签参数。
    Curve {
        base: CircularBezier,
        from_id: &'a str,
        to_id: &'a str,
        skip: [usize; 2],
    },
}

/// circular 路由的已编译 Draft。
pub struct CircularDraft<'a> {
    diagram: &'a Diagram,
    nodes: &'a HashMap<String, NodeLayout>,
    edges: Vec<CircularEdgePlan<'a>>,
    /// 避障索引（懒构建：无边可能穿障时为 `None`）。
    obstacle_index: Option<ObstacleIndex>,
    /// 无圆簇 / 无边：直接短路返回原 `result`（不改 `edges`）。
    skip: bool,
}

impl<'a> CircularDraft<'a> {
    /// 逐边计划数量（供 compile 独立单测）。
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// 是否短路（供 compile 单测）。
    pub fn is_skipped(&self) -> bool {
        self.skip
    }
}

/// 穿障退化决策：复刻 legacy `route_edges_circular` 的逐边 detour 分支。
///
/// 返回 `Some(points)` 表示应退化为折线绕行；`None` 表示保持原几何（未穿障 / 兜底失败）。
fn resolve_detour(
    probe: &EdgeLayout,
    obstacle_index: &ObstacleIndex,
    skip: &[usize; 2],
    nodes: &HashMap<String, NodeLayout>,
    from_id: &str,
    to_id: &str,
) -> Option<Vec<Point>> {
    if !curve_intersects_obstacles(probe, obstacle_index, skip) {
        return None;
    }
    let (Some(start), Some(end)) = (probe.path_start(), probe.path_end()) else {
        return None;
    };
    let detour = obstacle_index.shortest_path(start, end, skip);
    if !detour.is_empty() {
        return Some(detour);
    }
    // 空 detour → outer 折线兜底，避免静默保留穿障 Bezier。
    let outer = outer_polyline_detour(start, end, nodes, from_id, to_id);
    let outer_probe = EdgeLayout {
        geometry: PathGeometry::Polyline {
            points: outer.clone(),
        },
        labels: Vec::new(),
        from_port: probe.from_port,
        to_port: probe.to_port,
    };
    if !curve_intersects_obstacles(&outer_probe, obstacle_index, skip) || outer.len() >= 3 {
        return Some(outer);
    }
    None
}

/// 穿障退化后的折线标签计划（固定偏移 `(0, -6)`，按弧长取点）。
fn detour_label_plan(rel: &crate::ast::Relation) -> EdgeLabelPlan {
    EdgeLabelPlan {
        middle_t: parse_label_t(rel),
        offset: Point::new(0.0, -6.0),
        sample_path: None,
        anchor: None,
    }
}

impl RoutingRecipe for CircularRecipe {
    type Draft<'a> = CircularDraft<'a>;

    fn name(&self) -> &'static str {
        "circular"
    }

    fn applicable_diagram_types(&self) -> &'static [DiagramType] {
        APPLICABLE_TYPES
    }

    
    fn compile<'a>(&self, diagram: &'a Diagram, result: &'a LayoutResult) -> CircularDraft<'a> {
        let circles = resolve_circle_groups(diagram, &result.nodes, &result.hints);
        if circles.is_empty() || diagram.relations.is_empty() {
            return CircularDraft {
                diagram,
                nodes: &result.nodes,
                edges: Vec::new(),
                obstacle_index: None,
                skip: true,
            };
        }

        let node_placement = build_node_placement(diagram, &circles);
        let (lane_offsets, arc_sides) = compute_lane_offsets(diagram, &node_placement);

        // 懒构建避障索引：快速预检无边可能穿障时跳过 O(n²) 构建。
        let (node_id_to_idx, obstacle_index): (HashMap<&str, usize>, Option<ObstacleIndex>) =
            if quick_check_need_obstacle_index(result, &diagram.relations) {
                let (idx, obs) = build_obstacle_context(result);
                (idx, Some(obs))
            } else {
                (HashMap::new(), None)
            };

        let self_loop_idx = self_loop_indices(&diagram.relations);
        let mut edges = Vec::with_capacity(diagram.relations.len());

        for (i, rel) in diagram.relations.iter().enumerate() {
            let from_id = rel.from.as_str();
            let to_id = rel.to.as_str();
            let from_idx = node_id_to_idx.get(from_id).copied().unwrap_or(usize::MAX);
            let to_idx = node_id_to_idx.get(to_id).copied().unwrap_or(usize::MAX);
            let skip = [from_idx, to_idx];

            if from_id == to_id {
                if result.nodes.contains_key(from_id) {
                    let loop_index = self_loop_idx.get(&i).copied().unwrap_or(0);
                    edges.push(CircularEdgePlan::SelfLoop {
                        loop_index,
                        node_id: from_id,
                        skip,
                    });
                } else {
                    edges.push(CircularEdgePlan::Missing);
                }
                continue;
            }

            let (from_nl, to_nl) = match (result.nodes.get(from_id), result.nodes.get(to_id)) {
                (Some(f), Some(t)) => (f, t),
                _ => {
                    edges.push(CircularEdgePlan::Missing);
                    continue;
                }
            };

            let base = match (node_placement.get(from_id), node_placement.get(to_id)) {
                (Some(from_pos), Some(to_pos)) if from_pos.circle_idx == to_pos.circle_idx => {
                    intra_circle_bezier(
                        from_nl,
                        to_nl,
                        &circles[from_pos.circle_idx],
                        from_pos.pos_idx,
                        to_pos.pos_idx,
                        lane_offsets[i],
                        arc_sides[i],
                    )
                }
                _ => inter_circle_bezier(from_nl, to_nl, lane_offsets[i], arc_sides[i]),
            };
            edges.push(CircularEdgePlan::Curve {
                base,
                from_id,
                to_id,
                skip,
            });
        }

        CircularDraft {
            diagram,
            nodes: &result.nodes,
            edges,
            obstacle_index,
            skip: false,
        }
    }

    fn solve(&self, draft: &CircularDraft<'_>) -> RecipeSolution {
        let mut solution = RouteSolution::default();
        let mut label_plans: Vec<Option<EdgeLabelPlan>> = Vec::with_capacity(draft.edges.len());

        for (i, plan) in draft.edges.iter().enumerate() {
            match plan {
                CircularEdgePlan::Missing => {
                    solution
                        .paths
                        .push(RoutePath::Empty(EmptyRouteReason::MissingEndpoint));
                    solution.ports.push(EndpointAssignment::minimal(
                        StableEdgeId(i), Port::Bottom, Port::Top,
                    ));
                    label_plans.push(None);
                }
                CircularEdgePlan::SelfLoop {
                    loop_index,
                    node_id,
                    skip,
                } => {
                    let rel = &draft.diagram.relations[i];
                    // compile 已保证节点存在。
                    let node = draft
                        .nodes
                        .get(*node_id)
                        .expect("compile 已确认自环节点存在");
                    // Slice D1：自环只产 topology + anchor 标签计划，几何经 materializer 物化。
                    let sol = solve_self_loop(node, *loop_index, SelfLoopStyle::Curved);
                    let RoutePath::Cubic(cubic) = sol.path else {
                        unreachable!("Curved 自环必产 Cubic");
                    };
                    // 自环几何同样参与穿障退化决策（与 legacy 对所有边一致）。
                    let probe = EdgeLayout {
                        geometry: PathGeometry::Bezier {
                            start: cubic.start,
                            end: cubic.end,
                            controls: cubic.controls,
                        },
                        labels: Vec::new(),
                        from_port: sol.from_port,
                        to_port: sol.to_port,
                    };
                    let detour = draft
                        .obstacle_index
                        .as_ref()
                        .and_then(|obs| resolve_detour(&probe, obs, skip, draft.nodes, node_id, node_id));
                    solution.ports.push(EndpointAssignment::minimal(
                        StableEdgeId(i), sol.from_port, sol.to_port,
                    ));
                    match detour {
                        Some(points) => {
                            solution.paths.push(RoutePath::Spline(SplinePath { points }));
                            label_plans.push(Some(detour_label_plan(rel)));
                        }
                        None => {
                            solution.paths.push(RoutePath::Cubic(cubic));
                            label_plans.push(Some(EdgeLabelPlan {
                                middle_t: 0.5,
                                offset: sol.label_offset,
                                sample_path: None,
                                anchor: Some(sol.label_anchor),
                            }));
                        }
                    }
                }
                CircularEdgePlan::Curve {
                    base,
                    from_id,
                    to_id,
                    skip,
                } => {
                    let rel = &draft.diagram.relations[i];
                    let probe = EdgeLayout {
                        geometry: PathGeometry::Bezier {
                            start: base.start,
                            end: base.end,
                            controls: base.controls,
                        },
                        labels: Vec::new(),
                        from_port: base.from_port,
                        to_port: base.to_port,
                    };
                    let detour = draft
                        .obstacle_index
                        .as_ref()
                        .and_then(|obs| resolve_detour(&probe, obs, skip, draft.nodes, from_id, to_id));
                    match detour {
                        Some(points) => {
                            solution.paths.push(RoutePath::Spline(SplinePath { points }));
                            // §4.5：geometry family 改变必须记录退化原因。
                            solution.diagnostics.degraded.push((
                                StableEdgeId(i),
                                DegradedReason::ObstacleFallback {
                                    requested: GeometryFamily::Radial,
                                    actual: GeometryFamily::Spline,
                                },
                            ));
                            label_plans.push(Some(detour_label_plan(rel)));
                        }
                        None => {
                            solution.paths.push(RoutePath::Radial(RadialPath {
                                start: base.start,
                                end: base.end,
                                controls: base.controls,
                            }));
                            label_plans.push(Some(EdgeLabelPlan {
                                middle_t: base.label_t,
                                offset: base.label_offset,
                                sample_path: None,
                                anchor: None,
                            }));
                        }
                    }
                    solution.ports.push(EndpointAssignment::minimal(
                        StableEdgeId(i), base.from_port, base.to_port,
                    ));
                }
            }
        }

        RecipeSolution {
            solution,
            label_plans,
        }
    }

    fn should_skip(&self, draft: &CircularDraft<'_>) -> bool {
        draft.skip
    }

    /// circular 收尾：径向推开标签（`RadialPlacer`），与 legacy `route_edges_circular` 一致。
    fn finalize(
        &self,
        mut result: LayoutResult,
        mut edges: Vec<EdgeLayout>,
        _diagram: &Diagram,
    ) -> LayoutResult {
        let label_ctx = LabelContext::new(&result.nodes, &result.groups);
        RadialPlacer::default().place(&mut edges, &label_ctx);
        result.edges = edges;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        ArrowType, AttributeMap, Entity, Identifier, Relation, SourceInfo, Span,
    };
    use crate::layout::recipes::circular::CircularLayoutHints;
    use crate::layout::routing::common::circular_support::CircleGroup;
    use crate::layout::routing::common::test_fixtures::route_via_prepared;
    use crate::layout::routing::edge_routing_circular::route_edges_circular;
    use crate::layout::routing::recipe::RecipeRouter;
    use crate::layout::NodeLayout;

    /// 构造 State 图 + 圆形布局（含圆簇提示），供 compile / router 对拍。
    fn make_circular(
        entities: Vec<(&str, f64, f64, f64, f64)>,
        relations: Vec<(&str, &str, Option<&str>)>,
        circles: Vec<CircleGroup>,
    ) -> (Diagram, LayoutResult) {
        let span = Span::dummy();
        let nodes: HashMap<String, NodeLayout> = entities
            .iter()
            .map(|(id, x, y, w, h)| {
                (
                    id.to_string(),
                    NodeLayout {
                        x: *x,
                        y: *y,
                        width: *w,
                        height: *h,
                        ..Default::default()
                    },
                )
            })
            .collect();

        let diagram = Diagram {
            diagram_type: DiagramType::State,
            attributes: Vec::new(),
            entities: entities
                .iter()
                .map(|(id, _, _, _, _)| Entity {
                    id: Identifier::new_unchecked(id),
                    label: id.to_string(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span,
                })
                .collect(),
            relations: relations
                .into_iter()
                .map(|(from, to, label)| Relation {
                    from: Identifier::new_unchecked(from),
                    to: Identifier::new_unchecked(to),
                    arrow: ArrowType::Active,
                    label: label.map(|s| s.to_string()),
                    head_label: None,
                    tail_label: None,
                    attributes: AttributeMap::default(),
                    span,
                })
                .collect(),
            groups: Vec::new(),
            constraints: vec![],
            style_decls: vec![],
            source_info: SourceInfo {
                file: None,
                line_count: 1,
            },
            ..Default::default()
        };

        let result = LayoutResult {
            nodes,
            groups: HashMap::new(),
            edges: vec![],
            total_width: 600.0,
            total_height: 400.0,
            hints: CircularLayoutHints { circles }.into(),
        };

        (diagram, result)
    }

    #[test]
    fn compile_produces_one_plan_per_edge() {
        let (diagram, result) = make_circular(
            vec![("a", 100.0, 100.0, 80.0, 44.0), ("b", 260.0, 100.0, 80.0, 44.0)],
            vec![("a", "b", Some("go")), ("a", "a", None)],
            vec![CircleGroup {
                center: (180.0, 122.0),
                radius: 120.0,
                entity_indices: vec![0, 1],
            }],
        );
        let draft = CircularRecipe.compile(&diagram, &result);
        assert!(!draft.is_skipped());
        assert_eq!(draft.edge_count(), 2);
    }

    #[test]
    fn compile_skips_when_no_relations() {
        // 无边 → 短路（circles 可能从节点坐标反推，但无边直接返回原 result）。
        let (diagram, result) = make_circular(
            vec![("a", 100.0, 100.0, 80.0, 44.0), ("b", 260.0, 100.0, 80.0, 44.0)],
            vec![],
            vec![CircleGroup {
                center: (180.0, 122.0),
                radius: 120.0,
                entity_indices: vec![0, 1],
            }],
        );
        let draft = CircularRecipe.compile(&diagram, &result);
        assert!(draft.is_skipped());
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

    /// RecipeRouter<CircularRecipe> 与原 route_edges_circular 输出等价。
    #[test]
    fn recipe_router_matches_legacy_circular() {
        let cases: Vec<(
            Vec<(&str, f64, f64, f64, f64)>,
            Vec<(&str, &str, Option<&str>)>,
            Vec<CircleGroup>,
        )> = vec![
            // 同圆两节点弧形边（保持 Radial → Bezier）。
            (
                vec![
                    ("a", 100.0, 100.0, 80.0, 44.0),
                    ("b", 300.0, 100.0, 80.0, 44.0),
                ],
                vec![("a", "b", Some("lbl"))],
                vec![CircleGroup {
                    center: (240.0, 122.0),
                    radius: 140.0,
                    entity_indices: vec![0, 1],
                }],
            ),
            // 正反双向边（弦两侧分离）。
            (
                vec![
                    ("a", 100.0, 100.0, 80.0, 44.0),
                    ("b", 300.0, 100.0, 80.0, 44.0),
                ],
                vec![("a", "b", None), ("b", "a", None)],
                vec![CircleGroup {
                    center: (240.0, 122.0),
                    radius: 140.0,
                    entity_indices: vec![0, 1],
                }],
            ),
            // 跨圆边 a→c 穿过 b → 退化 Polyline。
            (
                vec![
                    ("a", 50.0, 100.0, 80.0, 44.0),
                    ("b", 200.0, 100.0, 80.0, 44.0),
                    ("c", 350.0, 100.0, 80.0, 44.0),
                ],
                vec![("a", "c", Some("through"))],
                vec![
                    CircleGroup {
                        center: (170.0, 122.0),
                        radius: 130.0,
                        entity_indices: vec![0, 1],
                    },
                    CircleGroup {
                        center: (390.0, 122.0),
                        radius: 50.0,
                        entity_indices: vec![2],
                    },
                ],
            ),
            // 自环 + 普通边。
            (
                vec![
                    ("a", 100.0, 100.0, 80.0, 44.0),
                    ("b", 300.0, 100.0, 80.0, 44.0),
                ],
                vec![("a", "a", Some("retry")), ("a", "b", None)],
                vec![CircleGroup {
                    center: (240.0, 122.0),
                    radius: 140.0,
                    entity_indices: vec![0, 1],
                }],
            ),
        ];

        for (entities, relations, circles) in cases {
            let (diagram, result) = make_circular(entities, relations, circles);
            let legacy = route_edges_circular(&diagram, result.clone());
            let router = RecipeRouter::new(CircularRecipe);
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
                    assert_eq!(ll.text, rl.text, "边 {idx} 标签文本");
                }
            }
        }
    }
}
