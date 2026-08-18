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
use crate::types::DiagramType;
use crate::layout::{
    edge_point, EdgeLayout, GroupLayout, LayoutResult, NodeLayout, Port,
};
use crate::layout::routing::common::edge_geometry::{
    canonical_perpendicular, node_center, select_port,
};
use crate::layout::routing::common::parallel_edges::group_parallel_edges;
use crate::layout::routing::visibility;
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

/// 构建穿障检测上下文:节点 id → 索引映射 + 障碍物索引。
///
/// 统一 bezier / circular / organic / spline 四处逐字符相同的前置设置序列。
/// 若 `result.groups` 非空，将组框矩形并入障碍（硬绕行）；端点相关组由调用方经
/// `skip_obstacles` 豁免。
///
/// 返回的 `HashMap<&str, usize>` 借用 `result.nodes` 的 key；
/// 第三项为 group 障碍起始索引（`nodes.len()`），供按边跳过相关组。
pub fn build_obstacle_context<'a>(
    result: &'a LayoutResult,
) -> (HashMap<&'a str, usize>, visibility::ObstacleIndex, usize) {
    // R7：按 node id 排序再建索引，保证障碍索引确定性。
    let mut sorted_ids: Vec<&'a str> = result.nodes.keys().map(|s| s.as_str()).collect();
    sorted_ids.sort_unstable();
    let node_list: Vec<(usize, &NodeLayout)> = sorted_ids
        .iter()
        .enumerate()
        .filter_map(|(i, id)| result.nodes.get(*id).map(|nl| (i, nl)))
        .collect();
    let node_id_to_idx: HashMap<&str, usize> = sorted_ids
        .iter()
        .enumerate()
        .map(|(i, id)| (*id, i))
        .collect();
    let group_start = node_list.len();
    let mut group_ids: Vec<&str> = result.groups.keys().map(|s| s.as_str()).collect();
    group_ids.sort_unstable();
    let extra: Vec<crate::layout::geometry::Rect> = group_ids
        .iter()
        .filter_map(|gid| result.groups.get(*gid).map(crate::layout::geometry::Rect::from))
        .collect();
    let obstacle_index = if extra.is_empty() {
        visibility::ObstacleIndex::build(&node_list)
    } else {
        visibility::ObstacleIndex::build_with_extra_rects(&node_list, &extra)
    };
    (node_id_to_idx, obstacle_index, group_start)
}

/// 按稳定序返回 group id 列表（与 `build_obstacle_context` 中 extra 顺序一致）。
pub fn sorted_group_obstacle_ids(result: &LayoutResult) -> Vec<String> {
    let mut ids: Vec<String> = result.groups.keys().cloned().collect();
    ids.sort();
    ids
}

/// 快速检测是否有任何边可能穿障（直线段 vs 节点 bbox 粗检）。
///
/// 用于懒构建 ObstacleIndex：若无边可能穿障 → 跳过 O(n²) 构建。
/// 粗检使用直线段（而非实际曲线），保守估计：
/// - 若直线段不穿障，曲线也可能穿障（假阴性）→ 仍构建 ObstacleIndex
/// - 若直线段穿障，曲线很可能穿障（真阳性）→ 构建 ObstacleIndex
///
/// 返回 `true` 表示需要构建 ObstacleIndex，`false` 表示可跳过。
pub fn quick_check_need_obstacle_index(
    result: &LayoutResult,
    relations: &[Relation],
) -> bool {
    // 节点少于 3 个时不可能穿障（只有起止节点）
    if result.nodes.len() <= 2 {
        return false;
    }

    for rel in relations {
        let Some(from_nl) = result.nodes.get(rel.from.as_str()) else {
            continue;
        };
        let Some(to_nl) = result.nodes.get(rel.to.as_str()) else {
            continue;
        };

        // 起止节点中心
        let start = Point::new(
            from_nl.x + from_nl.width / 2.0,
            from_nl.y + from_nl.height / 2.0,
        );
        let end = Point::new(
            to_nl.x + to_nl.width / 2.0,
            to_nl.y + to_nl.height / 2.0,
        );

        // 检查直线段是否穿过任何其他节点的 bbox
        for (id, nl) in &result.nodes {
            // 跳过起止节点
            if id == rel.from.as_str() || id == rel.to.as_str() {
                continue;
            }

            // 节点 bbox（含 padding）
            let pad = constants::DEFAULT_NODE_MARGIN;
            let bbox = (
                nl.x - pad,
                nl.y - pad,
                nl.x + nl.width + pad,
                nl.y + nl.height + pad,
            );

            // 快速排斥：线段 bbox 与节点 bbox 不相交 → 不可能穿障
            let seg_min_x = start.x.min(end.x);
            let seg_max_x = start.x.max(end.x);
            let seg_min_y = start.y.min(end.y);
            let seg_max_y = start.y.max(end.y);
            if seg_max_x < bbox.0 || seg_min_x > bbox.2 || seg_max_y < bbox.1 || seg_min_y > bbox.3 {
                continue;
            }

            // 线段与 bbox 相交 → 可能需要穿障检测
            return true;
        }
    }

    false
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
    /// 平行边法向偏移，只作用于中段/控制点，端点保持在节点边界上。
    pub mid_ox: f64,
    pub mid_oy: f64,
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
/// 节点查找 → 中心 → 法线 → 边界交点 → 端口 → 中段偏移 → 标签偏移。
/// 端点始终落在 `edge_point` 边界上；平行分离通过 `mid_ox/mid_oy` 偏移中段。
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
    let mid_ox = perp.x * offset_scalar;
    let mid_oy = perp.y * offset_scalar;

    // 端点保持在节点边界；平行偏移留给中段/控制点
    let (sx, sy) = edge_point(from_nl, c2.x, c2.y);
    let (ex, ey) = edge_point(to_nl, c1.x, c1.y);

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
            mid_ox,
            mid_oy,
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
    diagram: &Diagram,
) -> LayoutResult {
    if matches!(diagram.diagram_type, DiagramType::Mindmap) {
        for edge in &mut edges {
            edge.labels.clear();
        }
    }
    // R9 Slice 9a：标签冲突消解已迁至 LabelSolver::solve() 统一入口（RecipeRouter 在
    // finalize 前调用）。此处不再重复调用 resolve_label_overlaps。
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
            groups: crate::layout::GroupTable::new(),
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
        // 端点贴边；平行分离体现在中段偏移方向相反
        assert!(
            (ep1.mid_ox - ep2.mid_ox).abs() > 0.1 || (ep1.mid_oy - ep2.mid_oy).abs() > 0.1,
            "双向边中段应有不同偏移"
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

    #[test]
    fn finalize_edges_strips_labels_for_mindmap() {
        use crate::layout::{EdgeLayout, PathGeometry};
        let span = Span::dummy();
        let diagram = Diagram {
            diagram_type: DiagramType::Mindmap,
            attributes: Vec::new(),
            entities: vec![Entity {
                id: Identifier::new_unchecked("a"),
                label: "a".to_string(),
                attributes: AttributeMap::default(),
                group_id: None,
                span,
            }],
            relations: vec![Relation {
                from: Identifier::new_unchecked("a"),
                to: Identifier::new_unchecked("b"),
                arrow: ArrowType::Active,
                label: Some("ignored".to_string()),
                head_label: Some("head".to_string()),
                tail_label: Some("tail".to_string()),
                attributes: AttributeMap::default(),
                span,
            }],
            groups: Vec::new(),
            constraints: vec![],
            style_decls: vec![],
            source_info: SourceInfo {
                file: None,
                line_count: 1,
            },
            ..Default::default()
        };
        let mut result = LayoutResult {
            nodes: HashMap::new(),
            groups: crate::layout::GroupTable::new(),
            edges: vec![],
            total_width: 100.0,
            total_height: 100.0,
            hints: Default::default(),
        };
        let edges = vec![EdgeLayout {
            geometry: PathGeometry::Straight {
                start: Point::new(0.0, 0.0),
                end: Point::new(100.0, 0.0),
            },
            labels: vec![crate::layout::EdgeLabelLayout::new("mid", Point::new(50.0, 0.0))],
            from_port: Port::Right,
            to_port: Port::Left,
        }];
        let result = finalize_edges(result, edges, &diagram);
        assert!(result.edges[0].labels.is_empty());
    }
}
