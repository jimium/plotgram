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

/// Normalize an orthogonal polyline: drop consecutive duplicate points and
/// merge collinear consecutive segments. Geometry-preserving: a middle point
/// is removed only when it lies exactly on the segment between its kept
/// neighbours (exact float compare — path coordinates come from a discrete
/// line set, so equality is exact).
pub fn normalize_polyline(points: &[Point]) -> Vec<Point> {
    // Drop consecutive duplicates.
    let mut out: Vec<Point> = Vec::with_capacity(points.len());
    for &p in points {
        if out.last().is_some_and(|l| l.x == p.x && l.y == p.y) {
            continue;
        }
        out.push(p);
    }
    // Merge collinear runs (axis-aligned segments only).
    let mut i = 1;
    while i + 1 < out.len() {
        let (a, b, c) = (out[i - 1], out[i], out[i + 1]);
        let collinear_vertical = a.x == b.x && b.x == c.x && between(a.y, b.y, c.y);
        let collinear_horizontal = a.y == b.y && b.y == c.y && between(a.x, b.x, c.x);
        if collinear_vertical || collinear_horizontal {
            out.remove(i);
        } else {
            i += 1;
        }
    }
    out
}

/// Is `b` strictly between `a` and `c` (1-D)? Guards against erasing a
/// backtracking spur (a → b → c with c between a and b).
fn between(a: f64, b: f64, c: f64) -> bool {
    (a < b && b < c) || (c < b && b < a)
}

#[cfg(test)]
mod tests {
    use super::*;
    use plotgram_model::port::Side;

    #[test]
    fn normalize_cases() {
        let p = |x: f64, y: f64| Point { x, y };
        let cases: &[(Vec<Point>, Vec<Point>)] = &[
            // consecutive duplicates collapse
            (vec![p(0.0, 0.0), p(0.0, 0.0), p(10.0, 0.0)], vec![p(0.0, 0.0), p(10.0, 0.0)]),
            // collinear middle point merges (horizontal and vertical)
            (
                vec![p(0.0, 0.0), p(5.0, 0.0), p(10.0, 0.0), p(10.0, 4.0), p(10.0, 8.0)],
                vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 8.0)],
            ),
            // backtracking spur is preserved
            (
                vec![p(0.0, 0.0), p(10.0, 0.0), p(5.0, 0.0)],
                vec![p(0.0, 0.0), p(10.0, 0.0), p(5.0, 0.0)],
            ),
            // genuine bend is preserved
            (
                vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 5.0)],
                vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 5.0)],
            ),
        ];
        for (i, (input, want)) in cases.iter().enumerate() {
            assert_eq!(&normalize_polyline(input), want, "case {i}");
        }
    }

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
