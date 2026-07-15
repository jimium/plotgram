//! 严格共线折线压缩（等价变换）。
//!
//! 与「换角 / overshoot / 量化位移」等改形状路径分离：本模块只删严格共线中点，
//! 覆盖范围不变，可跳过 `validate_route_edit` 全量验证。
//!
//! 正交 `simplify_path` 与 `grid_snap` 量化后简化共用此入口，避免两套叉积阈值分叉。

use crate::layout::geometry::Point;

/// 默认严格共线容差（与历史正交 `is_collinear` 对齐）。
pub const STRICT_COLLINEAR_EPS: f64 = 0.1;

/// 三点是否在给定叉积容差下共线。
pub fn is_collinear_eps(a: Point, b: Point, c: Point, eps: f64) -> bool {
    let cross = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    cross.abs() < eps
}

/// 删除严格共线折点；`preserve_stubs` 时保留 index 1 与倒数第二点。
pub fn simplify_collinear_polyline(
    mut path: Vec<Point>,
    preserve_stubs: bool,
    eps: f64,
) -> Vec<Point> {
    if path.len() <= 2 {
        return path;
    }
    path.dedup_by(|a, b| (a.x - b.x).abs() < eps && (a.y - b.y).abs() < eps);
    // preserve_stubs 至少需要 5 点才有意义；否则至少 3 点才可压缩。
    let min_len = if preserve_stubs { 5 } else { 3 };
    if path.len() < min_len {
        return path;
    }

    let first_stub_index = 1;
    let last_stub_index = path.len() - 2;
    let mut simplified = vec![path[0]];

    for i in 1..path.len() - 1 {
        let prev = *simplified.last().unwrap();
        let curr = path[i];
        let next = path[i + 1];
        let preserves_node_exit = preserve_stubs && (i == first_stub_index || i == last_stub_index);
        if preserves_node_exit || !is_collinear_eps(prev, curr, next, eps) {
            simplified.push(curr);
        }
    }

    simplified.push(*path.last().unwrap());
    simplified
}
