//! 自环边统一几何：矩形折线（orthogonal）与小贝塞尔环（curved）。

use crate::ast::Relation;
use crate::layout::geometry::Point;
use crate::layout::{EdgeLayout, NodeLayout, PathGeometry, Port};
use crate::layout::edge::common::edge_geometry::{build_edge_labels, node_center, parse_label_t, point_at_path_t};

/// 自环绘制风格
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelfLoopStyle {
    /// 右上角矩形折线环（orthogonal / straight 默认）
    Orthogonal,
    /// 小贝塞尔环（circular / curved 默认）
    Curved,
}

/// 为每条自环边分配同节点内的序号（0, 1, 2…），用于角落轮转。
pub fn self_loop_indices(relations: &[Relation]) -> std::collections::HashMap<usize, usize> {
    let mut per_node: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut indices = std::collections::HashMap::new();
    for (i, rel) in relations.iter().enumerate() {
        if rel.from.as_str() != rel.to.as_str() {
            continue;
        }
        let node = rel.from.as_str();
        let idx = per_node.get(node).copied().unwrap_or(0);
        indices.insert(i, idx);
        per_node.insert(node.to_string(), idx + 1);
    }
    indices
}

/// 生成标准化自环几何；`loop_index` 决定角落轮转（0=右上，1=左上，2=右下，3=左下）。
pub fn route_self_loop(
    rel: &Relation,
    node: &NodeLayout,
    loop_index: usize,
    style: SelfLoopStyle,
) -> EdgeLayout {
    match style {
        SelfLoopStyle::Orthogonal => route_orthogonal_self_loop(rel, node, loop_index),
        SelfLoopStyle::Curved => route_curved_self_loop(rel, node, loop_index),
    }
}

fn route_orthogonal_self_loop(rel: &Relation, node: &NodeLayout, loop_index: usize) -> EdgeLayout {
    let corner = corner_for_index(loop_index);
    let loop_r = (node.width.min(node.height) * 0.28).max(16.0) + (loop_index as f64) * 6.0;
    let (path, label_point, from_port, to_port) =
        orthogonal_loop_geometry(node, corner, loop_r);

    let label_offset = label_outward_offset(corner, loop_r);
    let middle_t = parse_label_t(rel);
    let labels = build_edge_labels(
        rel,
        middle_t,
        label_offset,
        |t| point_at_path_t(&path, t),
    );

    let mut edge = EdgeLayout {
        geometry: PathGeometry::Polyline { points: Vec::new() },
        labels,
        from_port,
        to_port,
    };
    edge.set_polyline_points(path);
    edge.labels.iter_mut().for_each(|lbl| {
        lbl.center = Point::new(
            label_point.x + label_offset.x,
            label_point.y + label_offset.y,
        );
    });
    edge
}

fn route_curved_self_loop(rel: &Relation, node: &NodeLayout, loop_index: usize) -> EdgeLayout {
    let corner = corner_for_index(loop_index);
    let loop_r = (node.width.min(node.height) * 0.30).max(18.0) + (loop_index as f64) * 5.0;
    let (sx, sy, ex, ey, apex, from_port, to_port) =
        curved_loop_endpoints(node, corner, loop_r);

    let cp1 = Point::new(sx + loop_r * corner.dx, sy + loop_r * corner.dy);
    let cp2 = Point::new(
        apex.x - loop_r * corner.perp_x * 0.35,
        apex.y - loop_r * corner.perp_y * 0.35,
    );

    let label_offset = label_outward_offset(corner, loop_r);
    let labels = build_edge_labels(rel, 0.5, label_offset, |_| apex);

    EdgeLayout {
        geometry: PathGeometry::Bezier {
            start: Point::new(sx, sy),
            end: Point::new(ex, ey),
            controls: [cp1, cp2],
        },
        labels,
        from_port,
        to_port,
    }
}

#[derive(Clone, Copy)]
struct Corner {
    dx: f64,
    dy: f64,
    perp_x: f64,
    perp_y: f64,
    from_port: Port,
    to_port: Port,
}

fn corner_for_index(loop_index: usize) -> Corner {
    match loop_index % 4 {
        0 => Corner {
            dx: 1.0,
            dy: -1.0,
            perp_x: 0.0,
            perp_y: -1.0,
            from_port: Port::Right,
            to_port: Port::Top,
        },
        1 => Corner {
            dx: -1.0,
            dy: -1.0,
            perp_x: 0.0,
            perp_y: -1.0,
            from_port: Port::Left,
            to_port: Port::Top,
        },
        2 => Corner {
            dx: 1.0,
            dy: 1.0,
            perp_x: 0.0,
            perp_y: 1.0,
            from_port: Port::Right,
            to_port: Port::Bottom,
        },
        _ => Corner {
            dx: -1.0,
            dy: 1.0,
            perp_x: 0.0,
            perp_y: 1.0,
            from_port: Port::Left,
            to_port: Port::Bottom,
        },
    }
}

fn orthogonal_loop_geometry(
    node: &NodeLayout,
    corner: Corner,
    loop_r: f64,
) -> (Vec<Point>, Point, Port, Port) {
    let cx = node.x + node.width / 2.0;
    let cy = node.y + node.height / 2.0;
    let hw = node.width / 2.0;
    let hh = node.height / 2.0;
    // Iteration 3：最小段长，避免小节点上 p4→end 退化成近零长度
    const MIN_SEG: f64 = 8.0;
    let inset_start = hh.max(MIN_SEG) * 0.15;
    let inset_near = (hw * 0.15).max(MIN_SEG * 0.5).min(hw.max(MIN_SEG));
    let inset_far = (hw * 0.35).max(MIN_SEG).min(hw.max(MIN_SEG));
    let loop_out = loop_r.max(MIN_SEG);

    let (start, p2, p3, p4, end, apex) = if corner.dx > 0.0 && corner.dy < 0.0 {
        let start = Point::new(node.x + node.width, cy - inset_start);
        let p2 = Point::new(start.x + loop_out, start.y);
        let p3 = Point::new(p2.x, node.y - loop_out);
        let p4 = Point::new(cx + inset_near, p3.y);
        let end = Point::new(cx + inset_far, node.y);
        let apex = Point::new(p3.x, p3.y - loop_out * 0.35);
        (start, p2, p3, p4, end, apex)
    } else if corner.dx < 0.0 && corner.dy < 0.0 {
        let start = Point::new(node.x, cy - inset_start);
        let p2 = Point::new(start.x - loop_out, start.y);
        let p3 = Point::new(p2.x, node.y - loop_out);
        let p4 = Point::new(cx - inset_near, p3.y);
        let end = Point::new(cx - inset_far, node.y);
        let apex = Point::new(p3.x, p3.y - loop_out * 0.35);
        (start, p2, p3, p4, end, apex)
    } else if corner.dx > 0.0 && corner.dy > 0.0 {
        let start = Point::new(node.x + node.width, cy + inset_start);
        let p2 = Point::new(start.x + loop_out, start.y);
        let p3 = Point::new(p2.x, node.y + node.height + loop_out);
        let p4 = Point::new(cx + inset_near, p3.y);
        let end = Point::new(cx + inset_far, node.y + node.height);
        let apex = Point::new(p3.x, p3.y + loop_out * 0.35);
        (start, p2, p3, p4, end, apex)
    } else {
        let start = Point::new(node.x, cy + inset_start);
        let p2 = Point::new(start.x - loop_out, start.y);
        let p3 = Point::new(p2.x, node.y + node.height + loop_out);
        let p4 = Point::new(cx - inset_near, p3.y);
        let end = Point::new(cx - inset_far, node.y + node.height);
        let apex = Point::new(p3.x, p3.y + loop_out * 0.35);
        (start, p2, p3, p4, end, apex)
    };

    (
        vec![start, p2, p3, p4, end],
        apex,
        corner.from_port,
        corner.to_port,
    )
}

fn curved_loop_endpoints(
    node: &NodeLayout,
    corner: Corner,
    loop_r: f64,
) -> (f64, f64, f64, f64, Point, Port, Port) {
    let center = node_center(node);
    let ncx = center.x;
    let ncy = center.y;
    let scale = node.width.min(node.height) * 0.35;

    let sx = ncx + corner.dx * scale;
    let sy = ncy + corner.dy * scale * 0.5;
    let apex = Point::new(
        sx + corner.dx * loop_r * 1.5,
        sy + corner.dy * loop_r * 1.5,
    );
    let ex = ncx + corner.dx * scale * 0.55 - corner.perp_x * loop_r * 0.4;
    let ey = ncy + corner.dy * scale * 0.55 - corner.perp_y * loop_r * 0.4;

    (sx, sy, ex, ey, apex, corner.from_port, corner.to_port)
}

fn label_outward_offset(corner: Corner, loop_r: f64) -> Point {
    Point::new(
        corner.dx * (loop_r * 0.25),
        corner.dy * (loop_r * 0.25),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        ArrowType, AttributeMap, Identifier, Relation, SourceInfo, Span,
    };

    fn node_at(x: f64, y: f64) -> NodeLayout {
        NodeLayout {
            x,
            y,
            width: 120.0,
            height: 50.0,
            ..Default::default()
        }
    }

    fn self_relation() -> Relation {
        Relation {
            from: Identifier::new_unchecked("a"),
            to: Identifier::new_unchecked("a"),
            arrow: ArrowType::Active,
            label: Some("retry".to_string()),
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span: Span::dummy(),
        }
    }

    #[test]
    fn self_loop_indices_counts_per_node() {
        let span = Span::dummy();
        let mk = |from: &str, to: &str| Relation {
            from: Identifier::new_unchecked(from),
            to: Identifier::new_unchecked(to),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span,
        };
        let relations = vec![mk("a", "a"), mk("a", "b"), mk("a", "a"), mk("b", "b")];
        let idx = self_loop_indices(&relations);
        assert_eq!(idx.get(&0), Some(&0));
        assert_eq!(idx.get(&2), Some(&1));
        assert_eq!(idx.get(&3), Some(&0));
        assert!(!idx.contains_key(&1));
    }

    #[test]
    fn orthogonal_self_loop_has_polyline() {
        let edge = route_self_loop(&self_relation(), &node_at(10.0, 20.0), 0, SelfLoopStyle::Orthogonal);
        assert!(edge.path_len() >= 4);
        assert!(edge.has_label());
    }

    #[test]
    fn curved_self_loop_is_bezier() {
        let edge = route_self_loop(&self_relation(), &node_at(10.0, 20.0), 1, SelfLoopStyle::Curved);
        assert!(matches!(edge.geometry, PathGeometry::Bezier { .. }));
    }

    #[test]
    fn multiple_loops_use_different_corners() {
        let rel = self_relation();
        let node = node_at(0.0, 0.0);
        let e0 = route_self_loop(&rel, &node, 0, SelfLoopStyle::Orthogonal);
        let e1 = route_self_loop(&rel, &node, 1, SelfLoopStyle::Orthogonal);
        assert_ne!(e0.from_port, e1.from_port);
    }
}
