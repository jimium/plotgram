//! Path simplification for orthogonal edge routing

use super::*;
use crate::layout::geometry::Point;

pub fn simplify_path_preserving_stubs(mut path: Vec<Point>) -> Vec<Point> {
    if path.len() <= 2 {
        return path;
    }
    path.dedup_by(|a, b| (a.x - b.x).abs() < EPS && (a.y - b.y).abs() < EPS);
    if path.len() <= 4 {
        return path;
    }

    let first_stub_index = 1;
    let last_stub_index = path.len() - 2;
    let mut simplified = vec![path[0]];

    for i in 1..path.len() - 1 {
        let prev = *simplified.last().unwrap();
        let curr = path[i];
        let next = path[i + 1];
        let preserves_node_exit = i == first_stub_index || i == last_stub_index;
        if preserves_node_exit || !is_collinear(prev, curr, next) {
            simplified.push(curr);
        }
    }

    simplified.push(*path.last().unwrap());
    simplified
}

pub fn simplify_path(mut path: Vec<Point>) -> Vec<Point> {
    if path.len() <= 2 {
        return path;
    }
    path.dedup_by(|a, b| (a.x - b.x).abs() < EPS && (a.y - b.y).abs() < EPS);
    if path.len() <= 2 {
        return path;
    }

    let mut simplified = vec![path[0]];
    for i in 1..path.len() - 1 {
        let prev = *simplified.last().unwrap();
        let curr = path[i];
        let next = path[i + 1];
        if !is_collinear(prev, curr, next) {
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
