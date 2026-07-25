//! snap / repulse 后处理：将通道段投影到边框壳层外。

use std::collections::HashMap;

use crate::layout::geometry::Point;
use crate::layout::{EdgeLayout, GroupLayout};

use super::border_shell::{
    is_horizontal_segment, is_vertical_segment, segment_hugs_group_border,
    segment_within_port_stub_zone,
};
use super::constants::PORT_STUB_CLEARANCE;

fn snap_ceil_to_grid(value: f64, step: f64) -> f64 {
    if step <= f64::EPSILON {
        return value;
    }
    (value / step).ceil() * step
}

fn snap_floor_to_grid(value: f64, step: f64) -> f64 {
    if step <= f64::EPSILON {
        return value;
    }
    (value / step).floor() * step
}

fn project_vertical_coord(x: f64, gl: &GroupLayout, pad: f64, step: f64) -> f64 {
    let left = gl.x;
    let right = gl.x + gl.width;
    if x < left {
        if left - x < pad {
            return snap_floor_to_grid(left - pad, step);
        }
    } else if x > right {
        if x - right < pad {
            return snap_ceil_to_grid(right + pad, step);
        }
    } else if x - left < pad {
        return snap_ceil_to_grid(left + pad, step);
    } else if right - x < pad {
        return snap_floor_to_grid(right - pad, step);
    }
    x
}

fn project_horizontal_coord(y: f64, gl: &GroupLayout, pad: f64, step: f64) -> f64 {
    let top = gl.y;
    let bottom = gl.y + gl.height;
    if y < top {
        if top - y < pad {
            return snap_floor_to_grid(top - pad, step);
        }
    } else if y > bottom {
        if y - bottom < pad {
            return snap_ceil_to_grid(bottom + pad, step);
        }
    } else if y - top < pad {
        return snap_ceil_to_grid(top + pad, step);
    } else if bottom - y < pad {
        return snap_floor_to_grid(bottom - pad, step);
    }
    y
}

/// snap 后将通道段投影到边框壳层外的合法格点（不动 stub/磁吸点）。
pub fn project_path_off_group_borders(
    path: &mut [Point],
    groups: &HashMap<String, GroupLayout>,
    pad: f64,
    step: f64,
) -> usize {
    project_path_off_group_borders_with_stub(path, groups, pad, step, PORT_STUB_CLEARANCE)
}

/// snap 后将通道段投影到边框壳层外的合法格点（stub 区内段不修改）。
pub fn project_path_off_group_borders_with_stub(
    path: &mut [Point],
    groups: &HashMap<String, GroupLayout>,
    pad: f64,
    step: f64,
    stub_clearance: f64,
) -> usize {
    let n = path.len();
    if n < 2 || groups.is_empty() {
        return 0;
    }
    let mut group_ids: Vec<&String> = groups.keys().collect();
    group_ids.sort();
    let mut count = 0usize;

    for i in 0..n.saturating_sub(1) {
        if segment_within_port_stub_zone(path, i, stub_clearance) {
            continue;
        }
        let a = path[i];
        let b = path[i + 1];
        if is_vertical_segment(a, b) {
            let mut x = a.x;
            for gid in &group_ids {
                if let Some(gl) = groups.get(*gid) {
                    if segment_hugs_group_border(a, b, gl, pad) {
                        x = project_vertical_coord(x, gl, pad, step);
                    }
                }
            }
            if (path[i].x - x).abs() > f64::EPSILON {
                path[i].x = x;
                count += 1;
            }
            if (path[i + 1].x - x).abs() > f64::EPSILON {
                path[i + 1].x = x;
                count += 1;
            }
        } else if is_horizontal_segment(a, b) {
            let mut y = a.y;
            for gid in &group_ids {
                if let Some(gl) = groups.get(*gid) {
                    if segment_hugs_group_border(a, b, gl, pad) {
                        y = project_horizontal_coord(y, gl, pad, step);
                    }
                }
            }
            if (path[i].y - y).abs() > f64::EPSILON {
                path[i].y = y;
                count += 1;
            }
            if (path[i + 1].y - y).abs() > f64::EPSILON {
                path[i + 1].y = y;
                count += 1;
            }
        }
    }
    count
}

/// 路由 + snap 后的贴边安全网。
pub fn repulse_edges_from_group_borders(
    edges: &mut [EdgeLayout],
    groups: &HashMap<String, GroupLayout>,
    pad: f64,
    step: f64,
    max_rounds: usize,
) -> usize {
    if groups.is_empty() {
        return 0;
    }
    let mut total = 0usize;
    for edge in edges.iter_mut() {
        if edge.is_bezier() || edge.path_len() <= 2 {
            continue;
        }
        let Some(points) = edge.polyline_points_mut() else {
            continue;
        };
        for _ in 0..max_rounds {
            let moved = project_path_off_group_borders(points, groups, pad, step);
            total += moved;
            if moved == 0 {
                break;
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::group::constants::EPS;

    #[test]
    fn project_vertical_off_left_border() {
        let gl = GroupLayout {
            x: 200.0,
            y: 100.0,
            width: 160.0,
            height: 200.0,
        };
        let mut path = vec![
            Point::new(208.0, 100.0),
            Point::new(208.0, 110.0),
            Point::new(208.0, 120.0),
            Point::new(208.0, 130.0),
            Point::new(208.0, 140.0),
            Point::new(208.0, 280.0),
        ];
        let moved =
            project_path_off_group_borders(&mut path, &HashMap::from([("g".into(), gl)]), 12.0, 8.0);
        assert!(moved > 0);
        assert!(path[2].x >= 212.0 - EPS);
    }
}
