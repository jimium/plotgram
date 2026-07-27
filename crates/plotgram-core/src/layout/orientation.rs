//! M5：规范空间 → 画布空间的一次性外壳变换（yFiles LayoutOrientation 模型）。
//!
//! Atlas Hier 度量/落笔全程在规范空间（rank=Y, order=X）；仅当有效方向为
//! `left-to-right` 时，在管线末端对节点/组框/边/label 做轴转置。

use crate::ast::Diagram;
use crate::layout::geometry::Point;
use crate::layout::types::{EdgeLayout, GroupLayout, LayoutResult, NodeLayout, PathGeometry, Port};
use crate::layout::resolve_effective_direction;

/// DSL / profile 有效方向是否要求轴转置（rank 轴映到画布 X）。
pub fn needs_axis_transpose(diagram: &Diagram) -> bool {
    resolve_effective_direction(diagram) == Some("left-to-right")
}

/// 规范空间 → 画布 LTR：节点、组框、边折线/曲线、端口与 label。
///
/// 与旧 BK L145–161 + Ink 出口转置同构。迭代按 id 排序（AGENTS §2）。
pub fn apply_layout_orientation(result: &mut LayoutResult) {
    let mut node_ids: Vec<_> = result.nodes.keys().cloned().collect();
    node_ids.sort();
    for id in node_ids {
        if let Some(n) = result.nodes.get_mut(&id) {
            transpose_node(n);
        }
    }

    let group_ids = result.groups.keys_sorted();
    for id in group_ids {
        if let Some(g) = result.groups.get_mut(&id) {
            transpose_group(g);
        }
    }

    for edge in &mut result.edges {
        transpose_edge(edge);
    }

    std::mem::swap(&mut result.total_width, &mut result.total_height);
}

/// 转置端口朝向（rank 轴 Y↔X）。
pub fn transpose_port(p: Port) -> Port {
    match p {
        Port::Top => Port::Left,
        Port::Bottom => Port::Right,
        Port::Left => Port::Top,
        Port::Right => Port::Bottom,
    }
}

fn transpose_node(n: &mut NodeLayout) {
    let (x, y, w, h) = (n.x, n.y, n.width, n.height);
    // 与 BK horizontal 出口同构：中心 (x+w/2, y+h/2) → (y+h/2, x+w/2)，尺寸互换
    let cx = x + w / 2.0;
    let cy = y + h / 2.0;
    n.x = cy - h / 2.0;
    n.y = cx - w / 2.0;
    n.width = h;
    n.height = w;
}

fn transpose_group(g: &mut GroupLayout) {
    let (x, y, w, h) = (g.x, g.y, g.width, g.height);
    let cx = x + w / 2.0;
    let cy = y + h / 2.0;
    g.x = cy - h / 2.0;
    g.y = cx - w / 2.0;
    g.width = h;
    g.height = w;
}

fn transpose_point(p: &mut Point) {
    std::mem::swap(&mut p.x, &mut p.y);
}

fn transpose_edge(e: &mut EdgeLayout) {
    match &mut e.geometry {
        PathGeometry::Polyline { points } => {
            for p in points.iter_mut() {
                transpose_point(p);
            }
        }
        PathGeometry::Straight { start, end } => {
            transpose_point(start);
            transpose_point(end);
        }
        PathGeometry::Bezier {
            start,
            end,
            controls,
        } => {
            transpose_point(start);
            transpose_point(end);
            transpose_point(&mut controls[0]);
            transpose_point(&mut controls[1]);
        }
    }
    for l in &mut e.labels {
        transpose_point(&mut l.center);
        if let Some(p) = &mut l.leader_to {
            transpose_point(p);
        }
        std::mem::swap(&mut l.size.0, &mut l.size.1);
    }
    e.from_port = transpose_port(e.from_port);
    e.to_port = transpose_port(e.to_port);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::types::EdgeLabelLayout;
    use std::collections::HashMap;

    #[test]
    fn apply_orientation_swaps_node_edge_port() {
        let mut nodes = HashMap::new();
        nodes.insert(
            "a".into(),
            NodeLayout {
                x: 10.0,
                y: 20.0,
                width: 40.0,
                height: 60.0,
                ..Default::default()
            },
        );
        let mut edge = EdgeLayout::empty();
        edge.geometry = PathGeometry::Polyline {
            points: vec![Point { x: 30.0, y: 80.0 }, Point { x: 30.0, y: 100.0 }],
        };
        edge.labels = vec![EdgeLabelLayout::with_size(
            "L",
            Point { x: 30.0, y: 90.0 },
            (20.0, 10.0),
        )];
        edge.from_port = Port::Bottom;
        edge.to_port = Port::Top;
        let edges = vec![edge];
        let mut result = LayoutResult {
            nodes,
            groups: Default::default(),
            edges,
            total_width: 200.0,
            total_height: 300.0,
            hints: Default::default(),
        };
        apply_layout_orientation(&mut result);

        let a = &result.nodes["a"];
        // center was (30, 50) → (50, 30); size 40x60 → 60x40
        assert!((a.x - 20.0).abs() < 1e-9);
        assert!((a.y - 10.0).abs() < 1e-9);
        assert!((a.width - 60.0).abs() < 1e-9);
        assert!((a.height - 40.0).abs() < 1e-9);

        let e = &result.edges[0];
        if let PathGeometry::Polyline { points } = &e.geometry {
            assert_eq!(points[0], Point { x: 80.0, y: 30.0 });
            assert_eq!(points[1], Point { x: 100.0, y: 30.0 });
        } else {
            panic!("expected polyline");
        }
        assert_eq!(e.from_port, Port::Right);
        assert_eq!(e.to_port, Port::Left);
        assert_eq!(e.labels[0].center, Point { x: 90.0, y: 30.0 });
        assert_eq!(e.labels[0].size, (10.0, 20.0));
        assert_eq!(result.total_width, 300.0);
        assert_eq!(result.total_height, 200.0);
    }
}
