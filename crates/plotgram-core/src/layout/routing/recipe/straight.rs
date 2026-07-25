//! 直线路由 Recipe（doc16 §5.1 / R3 Slice 3.2a）。
//!
//! 把 [`super::super::edge_routing::route_edges`] 拆分到 [`RoutingRecipe`] 接口后面：
//!
//! - `compile`：构造 [`RoutingContext`]（平行边偏移）并逐边 `resolve_endpoints`。
//! - `solve`：逐边产 family-neutral [`RoutePath`]（有中段偏移 → `Orthogonal` 折线，
//!   否则 `Straight`）+ 端口分配 + 标签计划。
//!
//! 几何物化 / 标签放置由 [`super::RecipeRouter`] 统一驱动，最终 SVG 与原
//! `StraightRouting` 字节一致。直线路由适用于 ER 等简单关系图。

use crate::ast::Diagram;
use crate::layout::geometry::Point;
use crate::layout::routing::common::edge_geometry::parse_label_t;
use crate::layout::routing::common::routing_skeleton::{
    resolve_endpoints, EdgeEndpoints, LabelOffset, RoutingContext,
};
use crate::layout::routing::model::{
    EmptyRouteReason, EndpointAssignment, OrthogonalPath, RoutePath, RouteSolution, StableEdgeId,
    StraightPath,
};
use crate::layout::types::{LayoutResult, Port};
use crate::types::DiagramType;

use super::{EdgeLabelPlan, RecipeSolution, RoutingRecipe};

/// 直线路由适用于 ER 等简单关系图。
const APPLICABLE_TYPES: &[DiagramType] = &[DiagramType::Er];

/// 直线边路由 Recipe（无状态）。
pub struct StraightRecipe;

/// 直线路由的已编译 Draft：逐边端点解析结果（`None` = 端点缺失）。
pub struct StraightDraft<'a> {
    diagram: &'a Diagram,
    resolved: Vec<Option<(EdgeEndpoints, LabelOffset)>>,
}

impl<'a> StraightDraft<'a> {
    /// 已解析端点的逐边只读视图（供 compile 独立单测）。
    pub fn resolved(&self) -> &[Option<(EdgeEndpoints, LabelOffset)>] {
        &self.resolved
    }
}

impl RoutingRecipe for StraightRecipe {
    type Draft<'a> = StraightDraft<'a>;

    fn name(&self) -> &'static str {
        "straight"
    }

    fn applicable_diagram_types(&self) -> &'static [DiagramType] {
        APPLICABLE_TYPES
    }

    fn compile<'a>(&self, diagram: &'a Diagram, result: &'a LayoutResult) -> StraightDraft<'a> {
        let ctx = RoutingContext::new(diagram, result);
        let resolved = diagram
            .relations
            .iter()
            .enumerate()
            .map(|(i, rel)| resolve_endpoints(&ctx, rel, i))
            .collect();
        StraightDraft { diagram, resolved }
    }

    fn solve(&self, draft: &StraightDraft<'_>) -> RecipeSolution {
        let mut solution = RouteSolution::default();
        let mut label_plans: Vec<Option<EdgeLabelPlan>> = Vec::with_capacity(draft.resolved.len());

        for (i, resolved) in draft.resolved.iter().enumerate() {
            match resolved {
                // 端点缺失：声明性空路径（materialize → 空 Polyline，auditor 放行），无标签。
                None => {
                    solution.paths.push(RoutePath::Empty(EmptyRouteReason::MissingEndpoint));
                    solution.ports.push(EndpointAssignment::minimal(
                        StableEdgeId(i), Port::Bottom, Port::Top,
                    ));
                    label_plans.push(None);
                }
                Some((ep, label_off)) => {
                    // 有平行中段偏移 → 折线分离；否则保持直线（与原 route_edges 一致）。
                    let path = if ep.mid_ox.abs() > 0.1 || ep.mid_oy.abs() > 0.1 {
                        let mid = Point::new(
                            (ep.start.x + ep.end.x) * 0.5 + ep.mid_ox,
                            (ep.start.y + ep.end.y) * 0.5 + ep.mid_oy,
                        );
                        RoutePath::Orthogonal(OrthogonalPath {
                            points: vec![ep.start, mid, ep.end],
                        })
                    } else {
                        RoutePath::Straight(StraightPath {
                            start: ep.start,
                            end: ep.end,
                        })
                    };
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
    use crate::layout::routing::recipe::RecipeRouter;

    #[test]
    fn compile_resolves_each_edge() {
        let (diagram, result) = make_diagram_with_layout(
            vec![("a", 40.0, 40.0), ("b", 40.0, 170.0)],
            vec![("a", "b", None)],
        );
        let draft = StraightRecipe.compile(&diagram, &result);
        assert_eq!(draft.resolved().len(), 1);
        assert!(draft.resolved()[0].is_some(), "端点应解析成功");
    }

    #[test]
    fn compile_marks_missing_node_as_unresolved() {
        let (diagram, result) = make_diagram_with_layout(
            vec![("a", 40.0, 40.0)],
            vec![("a", "missing", None)],
        );
        let draft = StraightRecipe.compile(&diagram, &result);
        assert_eq!(draft.resolved().len(), 1);
        assert!(draft.resolved()[0].is_none(), "缺失节点应标记为未解析");
    }

    #[test]
    fn solve_produces_straight_path_and_ports() {
        let (diagram, result) = make_diagram_with_layout(
            vec![("a", 40.0, 40.0), ("b", 40.0, 170.0)],
            vec![("a", "b", None)],
        );
        let draft = StraightRecipe.compile(&diagram, &result);
        let sol = StraightRecipe.solve(&draft);
        assert_eq!(sol.solution.paths.len(), 1);
        assert!(matches!(sol.solution.paths[0], RoutePath::Straight(_)));
        // 垂直布局：起点 Bottom，终点 Top。
        assert_eq!(sol.solution.ports[0].from_port, Port::Bottom);
        assert_eq!(sol.solution.ports[0].to_port, Port::Top);
    }

    #[test]
    fn solve_suppresses_labels_for_missing_node() {
        let (diagram, result) = make_diagram_with_layout(
            vec![("a", 40.0, 40.0)],
            vec![("a", "missing", Some("lbl"))],
        );
        let draft = StraightRecipe.compile(&diagram, &result);
        let sol = StraightRecipe.solve(&draft);
        assert!(matches!(sol.solution.paths[0], RoutePath::Empty(_)));
        assert!(sol.label_plans[0].is_none(), "空边不产标签");
    }

    /// PathGeometry 未派生 PartialEq，逐变体逐字段比较（Point: PartialEq）。
    fn geom_eq(a: &crate::layout::types::PathGeometry, b: &crate::layout::types::PathGeometry) -> bool {
        use crate::layout::types::PathGeometry as G;
        match (a, b) {
            (G::Straight { start: s1, end: e1 }, G::Straight { start: s2, end: e2 }) => {
                s1 == s2 && e1 == e2
            }
            (
                G::Bezier { start: s1, end: e1, controls: c1 },
                G::Bezier { start: s2, end: e2, controls: c2 },
            ) => s1 == s2 && e1 == e2 && c1 == c2,
            (G::Polyline { points: p1 }, G::Polyline { points: p2 }) => p1 == p2,
            _ => false,
        }
    }

    /// RecipeRouter<StraightRecipe> 与原 route_edges 输出等价（同一 EdgeLayout 序列）。
    #[test]
    fn recipe_router_matches_legacy_route_edges() {
        let cases: Vec<(Vec<(&str, f64, f64)>, Vec<(&str, &str, Option<&str>)>)> = vec![
            (
                vec![("a", 40.0, 40.0), ("b", 40.0, 170.0)],
                vec![("a", "b", Some("lbl"))],
            ),
            (
                vec![("a", 40.0, 40.0), ("b", 260.0, 40.0)],
                vec![("a", "b", None)],
            ),
            // 双向边（平行偏移 → 折线）。
            (
                vec![("a", 40.0, 40.0), ("b", 40.0, 170.0)],
                vec![("a", "b", None), ("b", "a", None)],
            ),
            // 缺失节点。
            (vec![("a", 40.0, 40.0)], vec![("a", "missing", None)]),
        ];

        for (entities, relations) in cases {
            let (diagram, result) = make_diagram_with_layout(entities, relations);
            let legacy = crate::layout::routing::edge_routing::route_edges(&diagram, result.clone());
            let router = RecipeRouter::new(StraightRecipe);
            let recipe = route_via_prepared(&router, &diagram, &result);

            assert_eq!(legacy.edges.len(), recipe.edges.len());
            for (l, r) in legacy.edges.iter().zip(recipe.edges.iter()) {
                assert!(geom_eq(&l.geometry, &r.geometry), "几何应字节一致");
                assert_eq!(l.from_port, r.from_port);
                assert_eq!(l.to_port, r.to_port);
                assert_eq!(l.labels.len(), r.labels.len(), "标签数应一致");
                // R9a：标签位置由 LabelSolver::solve() 统一消解（有意改善），
                // 此处仅验证标签文本一致，不再要求 center/rotation 字节一致。
                for (ll, rl) in l.labels.iter().zip(r.labels.iter()) {
                    assert_eq!(ll.text, rl.text);
                }
            }
        }
    }
}
