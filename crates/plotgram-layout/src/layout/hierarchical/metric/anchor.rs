//! Port point expansion: the single implementation of `along_spec × frame`
//! (architecture.md §3.2 — `port_points` is Metric's deterministic expansion
//! of the Compose-frozen `PortPlan`; Ink reads the same function, never owns
//! the formula). Canonical (TB) space.
//!
//! `Ordered` carries **relative order only** — it is never a pixel truth
//! (roadmap phase B): Metric expands the dense, centered anchor
//! `(order + 1) / (count + 1)` against the final frame. `Ratio` /
//! `LocalOffset` are author-pinned pins resolved by Compose.

use plotgram_algo::orientation::Side::*;
use plotgram_model::geometry::{Point, Rect};
use plotgram_model::port::AlongSpec;

use crate::layout::hierarchical::compose::ports::ResolvedPort;

/// Pixel anchor of a finalized port on `frame`'s boundary.
pub fn port_anchor(frame: Rect, port: ResolvedPort) -> Point {
    match port.along {
        AlongSpec::Ordered { order, count } => {
            let t = (order as f64 + 1.0) / (count as f64 + 1.0);
            side_point(frame, port.side, t)
        }
        AlongSpec::Ratio(r) => side_point(frame, port.side, r),
        // Compose validated + snapped the point onto the boundary; Metric
        // only translates it onto the frame (no new decisions).
        AlongSpec::LocalOffset(offset) => Point {
            x: frame.x + offset.x,
            y: frame.y + offset.y,
        },
    }
}

/// Point at fraction `t ∈ [0,1]` along `side` of `frame`.
fn side_point(frame: Rect, side: plotgram_algo::orientation::Side, t: f64) -> Point {
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
    use plotgram_algo::orientation::Side;

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
    fn ratio_lands_linearly_along_the_side() {
        let f = frame();
        let cases = [
            (Side::North, 0.25, Point { x: 30.0, y: 20.0 }),
            (Side::South, 0.0, Point { x: 10.0, y: 60.0 }),
            (Side::West, 0.5, Point { x: 10.0, y: 40.0 }),
            (Side::East, 1.0, Point { x: 90.0, y: 60.0 }),
        ];
        for (side, r, want) in cases {
            let port = ResolvedPort {
                side,
                along: AlongSpec::Ratio(r),
            };
            assert_eq!(port_anchor(f, port), want, "{side:?} ratio {r}");
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
