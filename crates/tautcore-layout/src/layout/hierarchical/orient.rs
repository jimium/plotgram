//! Bridge between `tautcore_model`'s physical-frame geometry types and
//! `tautcore_algo::orientation`'s canonical-TB transform types.
//!
//! The core (rank/order/ports/metric/ink) runs entirely in canonical TB
//! space; only [`crate::layout::hierarchical::mod`]'s input/output edges
//! call into this module (architecture.md §9.3 — "核心只实现 canonical TB").

use tautcore_algo::orientation as algo;
use tautcore_model::geometry::{Point, Size};
use tautcore_model::port::Side;

use super::params::Orientation;

pub fn to_algo_orientation(o: Orientation) -> algo::Orientation {
    match o {
        Orientation::TopToBottom => algo::Orientation::Tb,
        Orientation::BottomToTop => algo::Orientation::Bt,
        Orientation::LeftToRight => algo::Orientation::Lr,
        Orientation::RightToLeft => algo::Orientation::Rl,
    }
}

pub fn to_algo_size(s: Size) -> algo::Size {
    algo::Size::new(s.width, s.height)
}

pub fn to_algo_point(p: Point) -> algo::Point {
    algo::Point::new(p.x, p.y)
}

pub fn from_algo_point(p: algo::Point) -> Point {
    Point { x: p.x, y: p.y }
}

pub fn to_algo_side(s: Side) -> algo::Side {
    match s {
        Side::North => algo::Side::North,
        Side::South => algo::Side::South,
        Side::East => algo::Side::East,
        Side::West => algo::Side::West,
    }
}

pub fn from_algo_side(s: algo::Side) -> Side {
    match s {
        algo::Side::North => Side::North,
        algo::Side::South => Side::South,
        algo::Side::East => Side::East,
        algo::Side::West => Side::West,
    }
}
