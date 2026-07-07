//! 布局 lint 共用的几何判定。

use crate::layout::geometry::Point;
use crate::layout::{GroupLayout, NodeLayout};

pub const OVERLAP_EPS: f64 = 0.5;
pub const BORDER_EPS: f64 = 2.0;
pub const GROUP_INTERIOR_INSET: f64 = 2.0;

/// 两矩形 AABB 重叠面积（无重叠返回 0）。
pub fn rect_overlap_area(
    ax: f64,
    ay: f64,
    aw: f64,
    ah: f64,
    bx: f64,
    by: f64,
    bw: f64,
    bh: f64,
) -> f64 {
    let ox = (ax + aw).min(bx + bw) - ax.max(bx);
    let oy = (ay + ah).min(by + bh) - ay.max(by);
    if ox > OVERLAP_EPS && oy > OVERLAP_EPS {
        ox * oy
    } else {
        0.0
    }
}

pub fn node_overlap_area(a: &NodeLayout, b: &NodeLayout) -> f64 {
    rect_overlap_area(a.x, a.y, a.width, a.height, b.x, b.y, b.width, b.height)
}

pub fn group_overlap_area(a: &GroupLayout, b: &GroupLayout) -> f64 {
    rect_overlap_area(a.x, a.y, a.width, a.height, b.x, b.y, b.width, b.height)
}

pub fn point_in_rect_interior(px: f64, py: f64, gl: &GroupLayout) -> bool {
    px > gl.x + GROUP_INTERIOR_INSET
        && px < gl.x + gl.width - GROUP_INTERIOR_INSET
        && py > gl.y + GROUP_INTERIOR_INSET
        && py < gl.y + gl.height - GROUP_INTERIOR_INSET
}

/// 线段中点。
pub fn segment_midpoint(a: Point, b: Point) -> Point {
    Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)
}

/// 水平线段是否贴在矩形上/下边框上（长度与边框有实质重合）。
pub fn horizontal_segment_on_border(
    y: f64,
    x1: f64,
    x2: f64,
    border_y: f64,
    rect_left: f64,
    rect_right: f64,
) -> bool {
    if (y - border_y).abs() > BORDER_EPS {
        return false;
    }
    let seg_left = x1.min(x2);
    let seg_right = x1.max(x2);
    seg_right > rect_left + BORDER_EPS && seg_left < rect_right - BORDER_EPS
}

/// 垂直线段是否贴在矩形左/右边框上。
pub fn vertical_segment_on_border(
    x: f64,
    y1: f64,
    y2: f64,
    border_x: f64,
    rect_top: f64,
    rect_bottom: f64,
) -> bool {
    if (x - border_x).abs() > BORDER_EPS {
        return false;
    }
    let seg_top = y1.min(y2);
    let seg_bottom = y1.max(y2);
    seg_bottom > rect_top + BORDER_EPS && seg_top < rect_bottom - BORDER_EPS
}

/// 线段是否与分组四条边之一重合。
pub fn segment_on_group_border(
    a: Point,
    b: Point,
    gl: &GroupLayout,
) -> bool {
    let left = gl.x;
    let top = gl.y;
    let right = gl.x + gl.width;
    let bottom = gl.y + gl.height;

    let dx = (a.x - b.x).abs();
    let dy = (a.y - b.y).abs();

    if dy <= BORDER_EPS && dx > BORDER_EPS {
        return horizontal_segment_on_border(a.y, a.x, b.x, top, left, right)
            || horizontal_segment_on_border(a.y, a.x, b.x, bottom, left, right);
    }
    if dx <= BORDER_EPS && dy > BORDER_EPS {
        return vertical_segment_on_border(a.x, a.y, b.y, left, top, bottom)
            || vertical_segment_on_border(a.x, a.y, b.y, right, top, bottom);
    }
    false
}

/// 两线段是否真正交叉（不含共线重叠）。
pub fn segments_cross(a1: Point, a2: Point, b1: Point, b2: Point) -> bool {
    let d1 = cross(b1, b2, a1);
    let d2 = cross(b1, b2, a2);
    let d3 = cross(a1, a2, b1);
    let d4 = cross(a1, a2, b2);
    if d1 * d2 < -OVERLAP_EPS && d3 * d4 < -OVERLAP_EPS {
        return true;
    }
    false
}

fn cross(o: Point, a: Point, b: Point) -> f64 {
    (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlap_area_positive() {
        let area = rect_overlap_area(0.0, 0.0, 100.0, 50.0, 50.0, 10.0, 100.0, 50.0);
        assert!(area > 0.0);
    }

    #[test]
    fn segment_on_top_border() {
        let gl = GroupLayout {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 100.0,
        };
        assert!(segment_on_group_border(Point::new(10.0, 0.0), Point::new(150.0, 0.0), &gl));
        assert!(!segment_on_group_border(Point::new(10.0, 5.0), Point::new(150.0, 5.0), &gl));
    }
}
