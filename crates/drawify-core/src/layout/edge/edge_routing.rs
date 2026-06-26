//! 边路由模块
//!
//! 在节点布局完成后计算每条边的几何信息：
//! - 平行边分离：同一对节点间的双向边/多边做垂直偏移，避免重叠
//! - 端口选择：根据节点相对位置选择连接侧（上/下/左/右）
//! - 标签定位：沿路径中点计算标签坐标

use crate::types::DiagramType;
use crate::ast::{Diagram};
use crate::layout::geometry::Point;
use crate::layout::{EdgeLayout, EdgeRoutingStrategy, LayoutResult, PathGeometry};
use crate::layout::edge::common::edge_geometry::{build_edge_labels, parse_label_t, point_at_path_t};
use crate::layout::edge::common::routing_skeleton::{
    finalize_edges, resolve_endpoints, RoutingContext,
};

/// 直线路由适用于 ER 等简单关系图；时序图边几何由 `sequence` 布局内置，不支持本路由。
const APPLICABLE_TYPES: &[DiagramType] = &[DiagramType::Er];

/// 直线边路由策略
pub struct StraightRouting;

impl EdgeRoutingStrategy for StraightRouting {
    fn name(&self) -> &'static str {
        "straight"
    }

    fn applicable_diagram_types(&self) -> &'static [DiagramType] {
        APPLICABLE_TYPES
    }

    fn route(&self, diagram: &Diagram, result: LayoutResult) -> LayoutResult {
        route_edges(diagram, result)
    }
}

/// 在节点布局完成后，为所有边计算几何路径与标签位置
///
/// 边的索引与 `diagram.relations` 一一对应。
pub fn route_edges(diagram: &Diagram, result: LayoutResult) -> LayoutResult {
    let relations = &diagram.relations;
    let ctx = RoutingContext::new(diagram, &result);

    let mut edges: Vec<EdgeLayout> = Vec::with_capacity(relations.len());

    for (i, rel) in relations.iter().enumerate() {
        let Some((ep, label_off)) = resolve_endpoints(&ctx, rel, i) else {
            edges.push(EdgeLayout::empty());
            continue;
        };

        // 标签位置：根据 label_position 锚点沿路径取点
        let path_pts = [ep.start, ep.end];
        let middle_t = parse_label_t(rel);
        let labels = build_edge_labels(rel, middle_t, Point::new(label_off.ox, label_off.oy), |t| {
            point_at_path_t(&path_pts, t)
        });

        edges.push(EdgeLayout {
            geometry: PathGeometry::Straight {
                start: ep.start,
                end: ep.end,
            },
            labels,
            from_port: ep.from_port,
            to_port: ep.to_port,
        });
    }

    finalize_edges(result, edges, diagram)
}

// ═══════════════════════════════════════════════════════════
//  单元测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::edge::common::edge_geometry::{undirected_pair_key, canonical_pair, select_port};
    use crate::ast::{Diagram, SourceInfo};
    use crate::layout::{NodeLayout, LayoutResult, Port, EdgeLabelLayout};
    use crate::layout::geometry::Point;
    use crate::layout::edge::common::label_avoidance::{
        aabb_overlap, estimate_label_width,
    };
    use crate::layout::edge::common::test_fixtures::make_diagram_with_layout;
    use crate::layout::constants;
    use std::collections::HashMap;

    #[test]
    fn test_route_edges_empty() {
        let diagram = Diagram::new(DiagramType::Flowchart, SourceInfo {
            file: None,
            line_count: 1,
        });
        let result = LayoutResult {
            nodes: HashMap::new(),
            groups: HashMap::new(),
            edges: vec![],
            total_width: 100.0,
            total_height: 100.0,
            hints: Default::default(),
        };

        let routed = route_edges(&diagram, result);
        assert!(routed.edges.is_empty());
    }

    #[test]
    fn test_route_edges_single_edge() {
        let (diagram, result) = make_diagram_with_layout(
            vec![("a", 40.0, 40.0), ("b", 40.0, 170.0)],
            vec![("a", "b", None)],
        );

        let routed = route_edges(&diagram, result);

        assert_eq!(routed.edges.len(), 1);
        let edge = &routed.edges[0];

        // 路径应有起点和终点
        assert_eq!(edge.path_len(), 2);

        // 起点 y 应小于终点 y（垂直布局）
        let start = edge.path_start().unwrap();
        let end = edge.path_end().unwrap();
        assert!(start.y < end.y);

        // 端口选择：垂直布局时，起点应为 Bottom，终点应为 Top
        assert_eq!(edge.from_port, Port::Bottom);
        assert_eq!(edge.to_port, Port::Top);
    }

    #[test]
    fn test_route_edges_horizontal() {
        let (diagram, result) = make_diagram_with_layout(
            vec![("a", 40.0, 40.0), ("b", 260.0, 40.0)],
            vec![("a", "b", None)],
        );

        let routed = route_edges(&diagram, result);

        assert_eq!(routed.edges.len(), 1);
        let edge = &routed.edges[0];

        // 水平布局时，起点应为 Right，终点应为 Left
        assert_eq!(edge.from_port, Port::Right);
        assert_eq!(edge.to_port, Port::Left);
    }

    #[test]
    fn test_route_edges_with_label() {
        let (diagram, result) = make_diagram_with_layout(
            vec![("a", 40.0, 40.0), ("b", 40.0, 170.0)],
            vec![("a", "b", Some("test label"))],
        );

        let routed = route_edges(&diagram, result);

        assert_eq!(routed.edges.len(), 1);
        let edge = &routed.edges[0];

        // 标签位置应在路径中点附近
        let start = edge.path_start().unwrap();
        let end = edge.path_end().unwrap();
        let mid_y = (start.y + end.y) / 2.0;
        assert!((edge.label_pos().y - mid_y).abs() < 20.0);
    }

    #[test]
    fn test_route_edges_bidirectional() {
        // 双向边：a -> b 和 b -> a
        let (diagram, result) = make_diagram_with_layout(
            vec![("a", 40.0, 40.0), ("b", 40.0, 170.0)],
            vec![("a", "b", None), ("b", "a", None)],
        );

        let routed = route_edges(&diagram, result);

        assert_eq!(routed.edges.len(), 2);

        // 两条边应有不同的偏移（避免重叠）：路径起点和终点应不同
        let s1 = routed.edges[0].path_start().unwrap();
        let s2 = routed.edges[1].path_start().unwrap();
        assert!(s1.x != s2.x || s1.y != s2.y);
    }

    #[test]
    fn test_route_edges_multiple_parallel() {
        // 多条同向边：a -> b (两次)
        let (diagram, result) = make_diagram_with_layout(
            vec![("a", 40.0, 40.0), ("b", 40.0, 170.0)],
            vec![("a", "b", None), ("a", "b", None)],
        );

        let routed = route_edges(&diagram, result);

        assert_eq!(routed.edges.len(), 2);

        // 两条边应有不同的偏移
        let s1 = routed.edges[0].path_start().unwrap();
        let s2 = routed.edges[1].path_start().unwrap();
        assert!(s1.x != s2.x || s1.y != s2.y);
    }

    #[test]
    fn test_route_edges_missing_node() {
        // 边指向不存在的节点
        let (diagram, result) = make_diagram_with_layout(
            vec![("a", 40.0, 40.0)],
            vec![("a", "missing", None)],
        );

        let routed = route_edges(&diagram, result);

        assert_eq!(routed.edges.len(), 1);
        let edge = &routed.edges[0];

        // 缺失节点时，路径应为空
        assert!(edge.path_is_empty());
    }

    #[test]
    fn test_undirected_pair_key() {
        let key1 = undirected_pair_key("a", "b");
        let key2 = undirected_pair_key("b", "a");

        // 无论顺序如何，键应相同
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_canonical_pair() {
        let (a, b) = canonical_pair("a", "b");
        assert_eq!(a, "a");
        assert_eq!(b, "b");

        let (a, b) = canonical_pair("b", "a");
        assert_eq!(a, "a");
        assert_eq!(b, "b");
    }

    #[test]
    fn test_select_port() {
        let nl = NodeLayout {
            x: 100.0,
            y: 100.0,
            width: 160.0,
            height: 50.0,
            ..Default::default()
        };

        // 测试各方向的端口选择
        assert_eq!(select_port(260.0, 125.0, &nl), Port::Right); // 右侧
        assert_eq!(select_port(20.0, 125.0, &nl), Port::Left);   // 左侧
        assert_eq!(select_port(180.0, 200.0, &nl), Port::Bottom); // 下侧
        assert_eq!(select_port(180.0, 50.0, &nl), Port::Top);    // 上侧
    }

    #[test]
    fn test_estimate_label_width() {
        // ASCII 文本
        let ascii_width = estimate_label_width("hello");
        assert_eq!(ascii_width, 5.0 * constants::DEFAULT_ASCII_CHAR_WIDTH);

        // CJK 文本
        let cjk_width = estimate_label_width("你好");
        assert_eq!(cjk_width, 2.0 * constants::DEFAULT_CJK_CHAR_WIDTH);

        // 混合文本
        let mixed_width = estimate_label_width("hello你好");
        assert_eq!(mixed_width, 5.0 * constants::DEFAULT_ASCII_CHAR_WIDTH + 2.0 * constants::DEFAULT_CJK_CHAR_WIDTH);
    }

    #[test]
    fn test_aabb_overlap_no_overlap() {
        let a = (0.0, 0.0, 10.0, 10.0);
        let b = (20.0, 20.0, 30.0, 30.0);

        assert!(aabb_overlap(&a, &b).is_none());
    }

    #[test]
    fn test_aabb_overlap_partial_overlap() {
        let a = (0.0, 0.0, 10.0, 10.0);
        let b = (5.0, 5.0, 15.0, 15.0);

        let overlap = aabb_overlap(&a, &b);
        assert!(overlap.is_some());

        let (dx, dy) = overlap.unwrap();
        assert_eq!(dx, 5.0); // x 方向重叠 5
        assert_eq!(dy, 5.0); // y 方向重叠 5
    }

    #[test]
    fn test_aabb_overlap_full_overlap() {
        let a = (0.0, 0.0, 10.0, 10.0);
        let b = (2.0, 2.0, 8.0, 8.0);

        let overlap = aabb_overlap(&a, &b);
        assert!(overlap.is_some());

        let (dx, dy) = overlap.unwrap();
        assert_eq!(dx, 6.0);
        assert_eq!(dy, 6.0);
    }

    #[test]
    fn test_label_bbox() {
        let edge = EdgeLayout {
            geometry: PathGeometry::Straight {
                start: Point::new(0.0, 0.0),
                end: Point::new(100.0, 100.0),
            },
            labels: vec![EdgeLabelLayout::new("test", Point::new(50.0, 50.0))],
            from_port: Port::Bottom,
            to_port: Port::Top,
        };

        let bbox = edge.label_bbox();

        // 标签宽度 = 文本宽度 + padding
        let expected_width = estimate_label_width("test") + constants::DEFAULT_LABEL_PADDING * 2.0;
        let expected_height = constants::DEFAULT_LABEL_FONT_SIZE + constants::DEFAULT_LABEL_PADDING * 2.0;

        assert_eq!(bbox.2 - bbox.0, expected_width);
        assert_eq!(bbox.3 - bbox.1, expected_height);
    }
}
