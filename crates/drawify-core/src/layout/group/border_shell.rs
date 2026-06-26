//! 路径段 vs 分组边框壳层（Border Shell）几何判定。

use crate::layout::geometry::Point;
use crate::layout::GroupLayout;

use super::constants::{COLLINEAR_EPS, EPS};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SegmentGroupRelation {
    Free,
    Interior,
    Crossing,
    Transit,
}

fn point_distance(a: Point, b: Point) -> f64 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    (dx * dx + dy * dy).sqrt()
}

/// 从 path[0] 沿折线到 path[segment_index] 的累计长度。
fn path_prefix_length(path: &[Point], segment_index: usize) -> f64 {
    path.iter()
        .take(segment_index.min(path.len().saturating_sub(1)))
        .zip(path.iter().skip(1))
        .map(|(a, b)| point_distance(*a, *b))
        .sum()
}

/// 从 path[segment_index + 1] 到 path[last] 的累计长度。
fn path_suffix_length(path: &[Point], segment_index: usize) -> f64 {
    let start = segment_index + 1;
    if start >= path.len().saturating_sub(1) {
        return 0.0;
    }
    path[start..]
        .windows(2)
        .map(|w| point_distance(w[0], w[1]))
        .sum()
}

/// 路径段是否完全落在距 from/to 磁吸点的 stub 区内（累计长度 ≤ `stub_clearance`）。
pub fn segment_within_port_stub_zone(
    path: &[Point],
    segment_index: usize,
    stub_clearance: f64,
) -> bool {
    if path.len() < 2 || segment_index + 1 >= path.len() {
        return false;
    }
    let seg_len = point_distance(path[segment_index], path[segment_index + 1]);
    let from_start = path_prefix_length(path, segment_index);
    let to_end = path_suffix_length(path, segment_index);
    from_start + seg_len <= stub_clearance + EPS || to_end + seg_len <= stub_clearance + EPS
}

pub fn segment_intersects_group_shell(
    a: Point,
    b: Point,
    gl: &GroupLayout,
    pad: f64,
) -> bool {
    let left = gl.x - pad + EPS;
    let right = gl.x + gl.width + pad - EPS;
    let top = gl.y - pad + EPS;
    let bottom = gl.y + gl.height + pad - EPS;

    if (a.x - b.x).abs() < EPS {
        let x = a.x;
        let min_y = a.y.min(b.y);
        let max_y = a.y.max(b.y);
        x > left && x < right && max_y > top && min_y < bottom
    } else if (a.y - b.y).abs() < EPS {
        let y = a.y;
        let min_x = a.x.min(b.x);
        let max_x = a.x.max(b.x);
        y > top && y < bottom && max_x > left && min_x < right
    } else {
        false
    }
}

fn point_strictly_inside_group(p: Point, gl: &GroupLayout) -> bool {
    p.x > gl.x + EPS
        && p.x < gl.x + gl.width - EPS
        && p.y > gl.y + EPS
        && p.y < gl.y + gl.height - EPS
}

fn classify_segment_vs_group(
    a: Point,
    b: Point,
    gl: &GroupLayout,
    pad: f64,
) -> SegmentGroupRelation {
    if !segment_intersects_group_shell(a, b, gl, pad) {
        return SegmentGroupRelation::Free;
    }
    let a_inside = point_strictly_inside_group(a, gl);
    let b_inside = point_strictly_inside_group(b, gl);
    if a_inside && b_inside {
        return SegmentGroupRelation::Interior;
    }
    if a_inside != b_inside {
        return SegmentGroupRelation::Crossing;
    }
    if segment_transits_group_interior(a, b, gl) {
        SegmentGroupRelation::Transit
    } else {
        SegmentGroupRelation::Free
    }
}

fn segment_allowed_by_relation(relation: SegmentGroupRelation, endpoint_in_group: bool) -> bool {
    match relation {
        SegmentGroupRelation::Free => true,
        SegmentGroupRelation::Interior | SegmentGroupRelation::Crossing => endpoint_in_group,
        SegmentGroupRelation::Transit => false,
    }
}

pub fn segment_hugs_group_border(a: Point, b: Point, gl: &GroupLayout, pad: f64) -> bool {
    let left = gl.x;
    let right = gl.x + gl.width;
    let top = gl.y;
    let bottom = gl.y + gl.height;
    let min_x = a.x.min(b.x);
    let max_x = a.x.max(b.x);
    let min_y = a.y.min(b.y);
    let max_y = a.y.max(b.y);

    if (a.y - b.y).abs() < EPS {
        if max_x <= left + EPS || min_x >= right - EPS {
            return false;
        }
        let y = a.y;
        (y - top).abs() < pad || (y - bottom).abs() < pad
    } else if (a.x - b.x).abs() < EPS {
        if max_y <= top + EPS || min_y >= bottom - EPS {
            return false;
        }
        let x = a.x;
        (x - left).abs() < pad || (x - right).abs() < pad
    } else {
        false
    }
}

fn segment_transits_group_interior(a: Point, b: Point, gl: &GroupLayout) -> bool {
    let left = gl.x;
    let right = gl.x + gl.width;
    let top = gl.y;
    let bottom = gl.y + gl.height;
    let min_x = a.x.min(b.x);
    let max_x = a.x.max(b.x);
    let min_y = a.y.min(b.y);
    let max_y = a.y.max(b.y);

    if (a.y - b.y).abs() < EPS {
        let y = a.y;
        y > top + EPS
            && y < bottom - EPS
            && max_x > left + EPS
            && min_x < right - EPS
    } else if (a.x - b.x).abs() < EPS {
        let x = a.x;
        x > left + EPS
            && x < right - EPS
            && max_y > top + EPS
            && min_y < bottom - EPS
    } else {
        false
    }
}

/// Border Shell 硬违规：贴边平行（非 stub 区）或 Transit。
pub fn group_segment_violates_border_shell(
    path: &[Point],
    segment_index: usize,
    gl: &GroupLayout,
    pad: f64,
    endpoint_in_group: bool,
    stub_clearance: f64,
) -> bool {
    if segment_index + 1 >= path.len() {
        return false;
    }
    let a = path[segment_index];
    let b = path[segment_index + 1];
    if segment_hugs_group_border(a, b, gl, pad)
        && !segment_within_port_stub_zone(path, segment_index, stub_clearance)
    {
        return true;
    }
    let relation = classify_segment_vs_group(a, b, gl, pad);
    !segment_allowed_by_relation(relation, endpoint_in_group)
}

pub fn segment_near_misses_group_shell(
    a: Point,
    b: Point,
    gl: &GroupLayout,
    pad: f64,
    near_extra: f64,
) -> bool {
    let near_outer = pad + near_extra;
    let left = gl.x;
    let right = gl.x + gl.width;
    let top = gl.y;
    let bottom = gl.y + gl.height;

    if (a.y - b.y).abs() < EPS {
        let y = a.y;
        let min_x = a.x.min(b.x);
        let max_x = a.x.max(b.x);
        if max_x <= left + EPS || min_x >= right - EPS {
            return false;
        }
        let dist_below = y - bottom;
        let dist_above = top - y;
        let below = dist_below >= pad - EPS && dist_below <= near_outer;
        let above = dist_above >= pad - EPS && dist_above <= near_outer;
        below || above
    } else if (a.x - b.x).abs() < EPS {
        let x = a.x;
        let min_y = a.y.min(b.y);
        let max_y = a.y.max(b.y);
        if max_y <= top + EPS || min_y >= bottom - EPS {
            return false;
        }
        let dist_left = x - left;
        let dist_right = right - x;
        let at_left = dist_left >= pad - EPS && dist_left <= near_outer;
        let at_right = dist_right >= pad - EPS && dist_right <= near_outer;
        at_left || at_right
    } else {
        false
    }
}

pub(crate) fn is_vertical_segment(a: Point, b: Point) -> bool {
    (a.x - b.x).abs() < COLLINEAR_EPS && (a.y - b.y).abs() >= COLLINEAR_EPS
}

pub(crate) fn is_horizontal_segment(a: Point, b: Point) -> bool {
    (a.y - b.y).abs() < COLLINEAR_EPS && (a.x - b.x).abs() >= COLLINEAR_EPS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::GroupLayout;
    use crate::layout::group::constants::PORT_STUB_CLEARANCE;

    #[test]
    fn stub_zone_within_port_clearance() {
        let path = vec![Point::new(0.0, 0.0), Point::new(0.0, 10.0), Point::new(100.0, 10.0)];
        assert!(segment_within_port_stub_zone(&path, 0, PORT_STUB_CLEARANCE));
        assert!(!segment_within_port_stub_zone(&path, 1, PORT_STUB_CLEARANCE));
    }

    #[test]
    fn stub_zone_from_target_end() {
        let path = vec![Point::new(0.0, 0.0), Point::new(100.0, 0.0), Point::new(100.0, 10.0), Point::new(100.0, 20.0)];
        assert!(segment_within_port_stub_zone(&path, 2, PORT_STUB_CLEARANCE));
        assert!(!segment_within_port_stub_zone(&path, 0, PORT_STUB_CLEARANCE));
    }

    #[test]
    fn stub_exemption_limited_to_port_clearance() {
        let gl = GroupLayout {
            x: 200.0,
            y: 100.0,
            width: 160.0,
            height: 200.0,
        };
        let path = vec![
            Point::new(208.0, 200.0),
            Point::new(208.0, 184.0),
            Point::new(208.0, 168.0),
            Point::new(208.0, 152.0),
            Point::new(300.0, 152.0),
        ];
        assert!(group_segment_violates_border_shell(
            &path,
            2,
            &gl,
            12.0,
            true,
            PORT_STUB_CLEARANCE,
        ));
    }
}
