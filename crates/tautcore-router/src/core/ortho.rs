//! Minimal orthogonal path helpers (strategy-free).

use tautcore_model::geometry::{Point, Rect};
use tautcore_model::port::{PortRef, Side};

/// Length of the collinear overlap between two segments (0 if not collinear
/// or disjoint). Grid coordinates are exact, so `1e-9` only guards float
/// jitter from normalization.
///
/// Single geometry truth for shared-corridor detection (M1 track separation)
/// and shared-segment scoring; also the base of `segments_overlap` in `score`.
pub fn overlap_len(a: Point, b: Point, c: Point, d: Point) -> f64 {
    const EPS: f64 = 1e-9;
    let a_horiz = (a.y - b.y).abs() < EPS;
    let b_horiz = (c.y - d.y).abs() < EPS;
    if a_horiz != b_horiz {
        return 0.0;
    }
    if a_horiz {
        if (a.y - c.y).abs() > EPS {
            return 0.0;
        }
        let lo = a.x.min(b.x).max(c.x.min(d.x));
        let hi = a.x.max(b.x).min(c.x.max(d.x));
        (hi - lo).max(0.0)
    } else {
        if (a.x - c.x).abs() > EPS {
            return 0.0;
        }
        let lo = a.y.min(b.y).max(c.y.min(d.y));
        let hi = a.y.max(b.y).min(c.y.max(d.y));
        (hi - lo).max(0.0)
    }
}

/// Anchor point on a node frame for a resolved port (along-spec ignored in
/// stub geometry — routers anchor at the side midpoint).
pub fn port_anchor(frame: &Rect, port: PortRef) -> Point {
    let c = frame.center();
    match port.side {
        Side::North => Point { x: c.x, y: frame.y },
        Side::South => Point {
            x: c.x,
            y: frame.bottom(),
        },
        Side::West => Point { x: frame.x, y: c.y },
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
        let mid = Point { x: to.x, y: from.y };
        vec![from, mid, to]
    } else {
        let mid = Point { x: from.x, y: to.y };
        vec![from, mid, to]
    }
}

/// Normalize an orthogonal polyline: drop consecutive duplicate points and
/// merge axis-collinear consecutive triples (including reverse spurs
/// `a → b → c` where `c` lies between `a` and `b`). Exact float compare —
/// path coordinates come from a discrete line set, so equality is exact.
pub fn normalize_polyline(points: &[Point]) -> Vec<Point> {
    // Drop consecutive duplicates.
    let mut out: Vec<Point> = Vec::with_capacity(points.len());
    for &p in points {
        if out.last().is_some_and(|l| l.x == p.x && l.y == p.y) {
            continue;
        }
        out.push(p);
    }
    // Merge collinear runs (axis-aligned segments only). Reverse spurs on the
    // same line collapse to the direct segment `a → c`.
    let mut i = 1;
    while i + 1 < out.len() {
        let (a, b, c) = (out[i - 1], out[i], out[i + 1]);
        let collinear_vertical = a.x == b.x && b.x == c.x;
        let collinear_horizontal = a.y == b.y && b.y == c.y;
        if collinear_vertical || collinear_horizontal {
            out.remove(i);
        } else {
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tautcore_model::port::Side;

    #[test]
    fn overlap_len_cases() {
        let p = |x: f64, y: f64| Point { x, y };
        let cases: &[((Point, Point, Point, Point), f64)] = &[
            // full overlap (same direction)
            ((p(0.0, 0.0), p(10.0, 0.0), p(0.0, 0.0), p(10.0, 0.0)), 10.0),
            // partial overlap
            ((p(0.0, 0.0), p(10.0, 0.0), p(5.0, 0.0), p(20.0, 0.0)), 5.0),
            // opposite direction still overlaps
            ((p(0.0, 0.0), p(10.0, 0.0), p(10.0, 0.0), p(2.0, 0.0)), 8.0),
            // disjoint
            ((p(0.0, 0.0), p(10.0, 0.0), p(12.0, 0.0), p(20.0, 0.0)), 0.0),
            // vertical overlap
            ((p(5.0, 0.0), p(5.0, 10.0), p(5.0, 4.0), p(5.0, 8.0)), 4.0),
            // not collinear (crossing) → 0
            ((p(0.0, 5.0), p(10.0, 5.0), p(5.0, 0.0), p(5.0, 10.0)), 0.0),
        ];
        for (i, ((a, b, c, d), want)) in cases.iter().enumerate() {
            assert_eq!(overlap_len(*a, *b, *c, *d), *want, "case {i}");
        }
    }

    #[test]
    fn normalize_cases() {
        let p = |x: f64, y: f64| Point { x, y };
        let cases: &[(Vec<Point>, Vec<Point>)] = &[
            // consecutive duplicates collapse
            (
                vec![p(0.0, 0.0), p(0.0, 0.0), p(10.0, 0.0)],
                vec![p(0.0, 0.0), p(10.0, 0.0)],
            ),
            // collinear middle point merges (horizontal and vertical)
            (
                vec![
                    p(0.0, 0.0),
                    p(5.0, 0.0),
                    p(10.0, 0.0),
                    p(10.0, 4.0),
                    p(10.0, 8.0),
                ],
                vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 8.0)],
            ),
            // reverse spur collapses (overshoot then back)
            (
                vec![p(0.0, 0.0), p(10.0, 0.0), p(5.0, 0.0)],
                vec![p(0.0, 0.0), p(5.0, 0.0)],
            ),
            // reverse spur then continues on the same line → single segment
            (
                vec![p(0.0, 0.0), p(10.0, 0.0), p(5.0, 0.0), p(15.0, 0.0)],
                vec![p(0.0, 0.0), p(15.0, 0.0)],
            ),
            // state_machine-style port approach spur
            (
                vec![
                    p(140.0, 70.0),
                    p(190.0, 70.0),
                    p(180.0, 70.0),
                    p(180.0, 60.0),
                    p(190.0, 60.0),
                ],
                vec![
                    p(140.0, 70.0),
                    p(180.0, 70.0),
                    p(180.0, 60.0),
                    p(190.0, 60.0),
                ],
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
                along: tautcore_model::port::AlongSpec::Ordered { order: 0, count: 1 },
            },
        );
        assert_eq!(p, Point { x: 10.0, y: 10.0 });
    }
}
