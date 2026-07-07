//! 线段与矩形相交几何工具。

use crate::layout::geometry::{Point, Rect};

/// 线段 vs AABB 相交检测。
pub(crate) fn segment_intersects_aabb(p1: Point, p2: Point, rect: Rect) -> bool {
    rect.intersects_segment(p1, p2, 0.0)
}

/// 线段是否与节点矩形相交（含 0.5px 容差）。
pub fn segment_intersects_node(
    a: Point,
    b: Point,
    nl: &crate::layout::NodeLayout,
) -> bool {
    Rect::from(nl).segment_crosses_interior(a, b, 0.5)
}
