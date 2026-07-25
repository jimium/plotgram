//! Path simplification for orthogonal edge routing.
//!
//! 严格共线压缩委托 [`crate::layout::routing::common::collinear_simplify`]；
//! 改形状逻辑（换角 / overshoot）不在此模块。

use crate::layout::routing::common::collinear_simplify::{
    simplify_collinear_polyline, STRICT_COLLINEAR_EPS,
};
use crate::layout::geometry::Point;

pub fn simplify_path(path: Vec<Point>, preserve_stubs: bool) -> Vec<Point> {
    simplify_path_with_eps(path, preserve_stubs, STRICT_COLLINEAR_EPS)
}

pub fn simplify_path_with_eps(path: Vec<Point>, preserve_stubs: bool, eps: f64) -> Vec<Point> {
    simplify_collinear_polyline(path, preserve_stubs, eps)
}
