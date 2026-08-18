//! Port point expansion: the single implementation of `along_spec × frame`
//! (architecture.md §3.2). Canonical (TB) space.
//!
//! `Ordered` carries **relative order only** — Metric expands the dense,
//! centered anchor `(order + 1) / (count + 1)` against the final frame.
//! `LocalOffset` is algorithm-owned (e.g. self-loop).

use tautcore_algo::orientation::Side::*;
use tautcore_model::geometry::{Point, Rect};
use tautcore_model::port::AlongSpec;

use crate::layout::hierarchical::compose::ports::ResolvedPort;

/// Pixel anchor of a finalized port on `frame`'s boundary.
pub fn port_anchor(frame: Rect, port: ResolvedPort) -> Point {
    match port.along {
        AlongSpec::Ordered { order, count } => {
            let t = (order as f64 + 1.0) / (count as f64 + 1.0);
            side_point(frame, port.side, t)
        }
        AlongSpec::LocalOffset(offset) => Point {
            x: frame.x + offset.x,
            y: frame.y + offset.y,
        },
    }
}

/// Point at fraction `t ∈ [0,1]` along `side` of `frame`.
fn side_point(frame: Rect, side: tautcore_algo::orientation::Side, t: f64) -> Point {
    match side {
        North => Point {
            x: frame.x + t * frame.width,
            y: frame.y,
        },
        South => Point {
            x: frame.x + t * frame.width,
            y: frame.bottom(),
        },
        West => Point {
            x: frame.x,
            y: frame.y + t * frame.height,
        },
        East => Point {
            x: frame.right(),
            y: frame.y + t * frame.height,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tautcore_algo::orientation::Side;

    fn frame() -> Rect {
        Rect::new(10.0, 20.0, 80.0, 40.0) // x: 10..90, y: 20..60
    }

    fn ordered(side: Side, order: u32, count: u32) -> ResolvedPort {
        ResolvedPort {
            side,
            along: AlongSpec::Ordered { order, count },
        }
    }

    #[test]
    fn ordered_is_dense_and_centered() {
        let f = frame();
        // Single port → exact center of the side.
        let p = port_anchor(f, ordered(Side::South, 0, 1));
        assert_eq!(p, Point { x: 50.0, y: 60.0 });
        // Three ports → quarters (dense, no gap arithmetic from author keys).
        for (order, want_x) in [(0u32, 30.0), (1, 50.0), (2, 70.0)] {
            let p = port_anchor(f, ordered(Side::South, order, 3));
            assert_eq!(p.x, want_x, "order {order}");
            assert_eq!(p.y, 60.0);
        }
    }

    #[test]
    fn local_offset_is_frame_relative() {
        let f = frame();
        let cases = [
            (Point { x: 12.0, y: 0.0 }, Point { x: 22.0, y: 20.0 }),
            (Point { x: 80.0, y: 7.0 }, Point { x: 90.0, y: 27.0 }),
        ];
        for (offset, want) in cases {
            let port = ResolvedPort {
                side: Side::North,
                along: AlongSpec::LocalOffset(offset),
            };
            assert_eq!(port_anchor(f, port), want, "offset {offset:?}");
        }
    }
}
