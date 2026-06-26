//! 边路由共享骨架
//!
//! 收敛 straight / bezier / spline 三套路由器共用的前置步骤：
//! 节点解析、平行边偏移、端口选择、标签偏移量计算。
//!
//! 路由器只需关注"路径生成"这一步，骨架统一处理：
//! 1. 平行边分组与偏移
//! 2. 节点中心 / 法线 / 边界交点 / 端口选择
//! 3. 标签沿法线的偏移量
//! 4. 收尾的标签避让 + `result.edges` 赋值

use crate::ast::{Diagram, Relation};
use crate::layout::geometry::Point;
use crate::layout::{
    edge_point, EdgeLayout, GroupLayout, LayoutResult, NodeLayout, Port,
};
use crate::layout::edge::common::edge_geometry::{
    canonical_perpendicular, node_center, select_port,
};
use crate::layout::edge::common::label_avoidance::resolve_label_overlaps;
use crate::layout::edge::common::parallel_edges::group_parallel_edges;
use crate::layout::constants;
use std::collections::HashMap;

/// 路由上下文：全图共享的只读引用 + 平行边偏移表
pub struct RoutingContext<'a> {
    pub nodes: &'a HashMap<String, NodeLayout>,
    pub groups: &'a HashMap<String, GroupLayout>,
    pub parallel_offsets: Vec<f64>,
}

impl<'a> RoutingContext<'a> {
    /// 从 Diagram + LayoutResult 构建路由上下文（含平行边偏移计算）
    pub fn new(diagram: &'a Diagram, result: &'a LayoutResult) -> Self {
        let pg = group_parallel_edges(&diagram.relations, constants::DEFAULT_EDGE_OFFSET);
        Self {
            nodes: &result.nodes,
            groups: &result.groups,
            parallel_offsets: pg.offsets,
        }
    }
}

/// 一条边的端点解析结果
#[derive(Clone)]
pub struct EdgeEndpoints {
    pub start: Point,
    pub end: Point,
    pub from_port: Port,
    pub to_port: Port,
    pub from_id: String,
    pub to_id: String,
}

/// 标签沿法线方向的偏移量（调用方需将其加到路径中点上）
#[derive(Clone, Copy)]
pub struct LabelOffset {
    pub ox: f64,
    pub oy: f64,
}

/// 解析一对节点的端点 + 端口 + 平行边偏移 + 标签偏移量
///
/// 返回 `None` 表示起止节点缺失，调用方应推入 `EdgeLayout::empty()`。
///
/// 统一了 straight / bezier / spline 三处重复的 10 步前置逻辑：
/// 节点查找 → 中心 → 法线 → 偏移 → 边界交点 → 端口 → 标签偏移。
pub fn resolve_endpoints(
    ctx: &RoutingContext,
    rel: &Relation,
    edge_index: usize,
) -> Option<(EdgeEndpoints, LabelOffset)> {
    let from_id = rel.from.as_str();
    let to_id = rel.to.as_str();

    let (from_nl, to_nl) = match (ctx.nodes.get(from_id), ctx.nodes.get(to_id)) {
        (Some(f), Some(t)) => (f, t),
        _ => return None,
    };

    let c1 = node_center(from_nl);
    let c2 = node_center(to_nl);

    let perp = canonical_perpendicular(from_id, to_id, c1.x, c1.y, c2.x, c2.y);
    let offset_scalar = ctx.parallel_offsets[edge_index];
    let ox = perp.x * offset_scalar;
    let oy = perp.y * offset_scalar;

    // 先计算无偏移的边界交点，再直接平移
    // （避免 edge_point 射线截断导致偏移量被压缩）
    let (sx, sy) = edge_point(from_nl, c2.x, c2.y);
    let (ex, ey) = edge_point(to_nl, c1.x, c1.y);
    let (sx, sy) = (sx + ox, sy + oy);
    let (ex, ey) = (ex + ox, ey + oy);

    let from_port = select_port(sx, sy, from_nl);
    let to_port = select_port(ex, ey, to_nl);

    // 标签沿法线额外偏移（避免标签贴在箭头上），无偏移时向上微调 6px
    let has_offset = offset_scalar.abs() > 0.1;
    let (label_ox, label_oy) = if has_offset {
        (
            perp.x * offset_scalar.signum() * constants::DEFAULT_LABEL_PERP_OFFSET,
            perp.y * offset_scalar.signum() * constants::DEFAULT_LABEL_PERP_OFFSET,
        )
    } else {
        (0.0, -6.0)
    };

    Some((
        EdgeEndpoints {
            start: Point::new(sx, sy),
            end: Point::new(ex, ey),
            from_port,
            to_port,
            from_id: from_id.to_string(),
            to_id: to_id.to_string(),
        },
        LabelOffset {
            ox: label_ox,
            oy: label_oy,
        },
    ))
}

/// 收尾：标签避让 + 赋值 `result.edges`
///
/// 统一所有路由器的收尾行为，确保 bezier 也走标签避让
/// （修复原 bezier 路由器遗漏 `resolve_label_overlaps` 的不一致）。
pub fn finalize_edges(
    mut result: LayoutResult,
    mut edges: Vec<EdgeLayout>,
    _diagram: &Diagram,
) -> LayoutResult {
    resolve_label_overlaps(&mut edges, &result.nodes, &result.groups);
    result.edges = edges;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        ArrowType, AttributeMap, Diagram, Entity, Identifier, Relation, SourceInfo, Span,
    };
    use crate::types::DiagramType;
    use crate::layout::{LayoutResult, NodeLayout, Port};

    fn make_setup(
        entities: Vec<(&str, f64, f64)>,
        relations: Vec<(&str, &str, Option<&str>)>,
    ) -> (Diagram, LayoutResult) {
        let span = Span::dummy();
        let nodes: HashMap<String, NodeLayout> = entities
            .iter()
            .map(|(id, x, y)| {
                (
                    id.to_string(),
                    NodeLayout {
                        x: *x,
                        y: *y,
                        width: 160.0,
                        height: 50.0,
                        ..Default::default()
                    },
                )
            })
            .collect();

        let diagram = Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: Vec::new(),
            entities: entities
                .into_iter()
                .map(|(id, _x, _y)| Entity {
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
            total_width: 500.0,
            total_height: 500.0,
            hints: Default::default(),
        };

        (diagram, result)
    }

    #[test]
    fn resolve_endpoints_missing_node_returns_none() {
        let (diagram, result) = make_setup(
            vec![("a", 40.0, 40.0)],
            vec![("a", "missing", None)],
        );
        let ctx = RoutingContext::new(&diagram, &result);
        // to 节点 missing 不存在 → None
        assert!(resolve_endpoints(&ctx, &diagram.relations[0], 0).is_none());
    }

    #[test]
    fn resolve_endpoints_missing_from_node_returns_none() {
        let (diagram, result) = make_setup(
            vec![("b", 40.0, 40.0)],
            vec![("missing", "b", None)],
        );
        let ctx = RoutingContext::new(&diagram, &result);
        assert!(resolve_endpoints(&ctx, &diagram.relations[0], 0).is_none());
    }

    #[test]
    fn resolve_endpoints_both_missing_returns_none() {
        let (diagram, result) = make_setup(vec![], vec![("a", "b", None)]);
        let ctx = RoutingContext::new(&diagram, &result);
        assert!(resolve_endpoints(&ctx, &diagram.relations[0], 0).is_none());
    }

    #[test]
    fn resolve_endpoints_vertical_edge_ports() {
        let (diagram, result) = make_setup(
            vec![("a", 40.0, 40.0), ("b", 40.0, 170.0)],
            vec![("a", "b", None)],
        );
        let ctx = RoutingContext::new(&diagram, &result);
        let (ep, _label) = resolve_endpoints(&ctx, &diagram.relations[0], 0).unwrap();
        assert_eq!(ep.from_port, Port::Bottom);
        assert_eq!(ep.to_port, Port::Top);
        // 起点 y 应小于终点 y
        assert!(ep.start.y < ep.end.y);
    }

    #[test]
    fn resolve_endpoints_horizontal_edge_ports() {
        let (diagram, result) = make_setup(
            vec![("a", 40.0, 40.0), ("b", 260.0, 40.0)],
            vec![("a", "b", None)],
        );
        let ctx = RoutingContext::new(&diagram, &result);
        let (ep, _label) = resolve_endpoints(&ctx, &diagram.relations[0], 0).unwrap();
        assert_eq!(ep.from_port, Port::Right);
        assert_eq!(ep.to_port, Port::Left);
    }

    #[test]
    fn resolve_endpoints_parallel_bidirectional_offset() {
        let (diagram, result) = make_setup(
            vec![("a", 40.0, 40.0), ("b", 40.0, 170.0)],
            vec![("a", "b", None), ("b", "a", None)],
        );
        let ctx = RoutingContext::new(&diagram, &result);
        let (ep1, _) = resolve_endpoints(&ctx, &diagram.relations[0], 0).unwrap();
        let (ep2, _) = resolve_endpoints(&ctx, &diagram.relations[1], 1).unwrap();
        // 两条边应有不同的偏移（避免重叠）
        assert!(
            (ep1.start.x - ep2.start.x).abs() > 0.1 || (ep1.start.y - ep2.start.y).abs() > 0.1,
            "双向边应有不同偏移"
        );
    }

    #[test]
    fn resolve_endpoints_label_offset_no_offset() {
        let (diagram, result) = make_setup(
            vec![("a", 40.0, 40.0), ("b", 260.0, 40.0)],
            vec![("a", "b", None)],
        );
        let ctx = RoutingContext::new(&diagram, &result);
        let (_, label) = resolve_endpoints(&ctx, &diagram.relations[0], 0).unwrap();
        // 单条边无偏移 → label_oy = -6.0
        assert!((label.oy - (-6.0)).abs() < 1e-6);
        assert!(label.ox.abs() < 1e-6);
    }

    #[test]
    fn routing_context_parallel_offsets_length() {
        let (diagram, result) = make_setup(
            vec![("a", 40.0, 40.0), ("b", 40.0, 170.0)],
            vec![("a", "b", None), ("b", "a", None), ("a", "b", None)],
        );
        let ctx = RoutingContext::new(&diagram, &result);
        assert_eq!(ctx.parallel_offsets.len(), 3);
    }

    #[test]
    fn finalize_edges_runs_label_avoidance() {
        use crate::layout::{EdgeLayout, PathGeometry};
        let (diagram, mut result) = make_setup(
            vec![("a", 40.0, 40.0), ("b", 40.0, 170.0)],
            vec![("a", "b", Some("label"))],
        );
        let edges = vec![EdgeLayout {
            geometry: PathGeometry::Straight {
                start: Point::new(120.0, 40.0),
                end: Point::new(120.0, 170.0),
            },
            labels: vec![crate::layout::EdgeLabelLayout::new("label", Point::new(120.0, 100.0))],
            from_port: Port::Bottom,
            to_port: Port::Top,
        }];
        result = finalize_edges(result, edges, &diagram);
        assert_eq!(result.edges.len(), 1);
    }
}
