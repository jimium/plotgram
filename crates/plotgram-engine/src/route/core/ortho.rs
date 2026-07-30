//! Minimal orthogonal path helpers (strategy-free).

use plotgram_model::geometry::{Point, Rect};
use plotgram_model::port::{PortRef, Side};

/// Anchor point on a node frame for a resolved port (slot ignored in stub geometry).
pub fn port_anchor(frame: &Rect, port: PortRef) -> Point {
    let c = frame.center();
    match port.side {
        Side::North => Point {
            x: c.x,
            y: frame.y,
        },
        Side::South => Point {
            x: c.x,
            y: frame.bottom(),
        },
        Side::West => Point {
            x: frame.x,
            y: c.y,
        },
        Side::East => Point {
            x: frame.right(),
            y: c.y,
        },
    }
}

/// Single-bend orthogonal polyline from `from` to `to` (horizontal then vertical,
/// or vertical then horizontal based on dominant delta).
pub fn orthogonal_elbow(from: Point, to: Point) -> Vec<Point> {
    if (from.x - to.x).abs() < 1e-9 || (from.y - to.y).abs() < 1e-9 {
        return vec![from, to];
    }
    // Prefer horizontal-first when |dx| >= |dy|.
    if (to.x - from.x).abs() >= (to.y - from.y).abs() {
        let mid = Point {
            x: to.x,
            y: from.y,
        };
        vec![from, mid, to]
    } else {
        let mid = Point {
            x: from.x,
            y: to.y,
        };
        vec![from, mid, to]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plotgram_model::port::Side;

    #[test]
    fn elbow_is_orthogonal() {
        let pts = orthogonal_elbow(Point { x: 0.0, y: 0.0 }, Point { x: 10.0, y: 5.0 });
        assert_eq!(pts.len(), 3);
        assert_eq!(pts[1], Point { x: 10.0, y: 0.0 });
    }

    #[test]
    fn port_anchor_south() {
        let frame = Rect::new(0.0, 0.0, 20.0, 10.0);
        let p = port_anchor(
            &frame,
            PortRef {
                side: Side::South,
                slot: 0,
            },
        );
        assert_eq!(p, Point { x: 10.0, y: 10.0 });
    }
}
