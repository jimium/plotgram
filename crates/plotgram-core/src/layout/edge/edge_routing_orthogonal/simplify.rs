//! Path simplification for orthogonal edge routing

use super::*;
use crate::layout::geometry::Point;

pub fn simplify_path(mut path: Vec<Point>, preserve_stubs: bool) -> Vec<Point> {
    if path.len() <= 2 {
        return path;
    }
    path.dedup_by(|a, b| (a.x - b.x).abs() < EPS && (a.y - b.y).abs() < EPS);
    // preserve_stubs needs at least 5 points (start, stub, mid..., stub, end) to be meaningful;
    // non-preserve mode needs at least 3 points to simplify.
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
        if preserves_node_exit || !is_collinear(prev, curr, next) {
            simplified.push(curr);
        }
    }

    simplified.push(*path.last().unwrap());
    simplified
}

pub fn is_collinear(a: Point, b: Point, c: Point) -> bool {
    let cross = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    cross.abs() < 0.1
}
