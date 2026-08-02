//! Rectangle helpers for group envelopes and obstacles.

use plotgram_model::geometry::{Point, Rect};

/// Tolerance for geometric comparisons.
const EPS: f64 = 1e-6;

/// Axis-aligned union of rectangles. Empty input → `None`.
pub fn union_rects(rects: &[Rect]) -> Option<Rect> {
    let mut iter = rects.iter().copied();
    let first = iter.next()?;
    Some(iter.fold(first, |a, b| {
        let x = a.x.min(b.x);
        let y = a.y.min(b.y);
        let right = a.right().max(b.right());
        let bottom = a.bottom().max(b.bottom());
        Rect::new(x, y, right - x, bottom - y)
    }))
}

/// Expand a rect by uniform padding on all sides.
pub fn padding_rect(r: Rect, pad: f64) -> Rect {
    Rect::new(
        r.x - pad,
        r.y - pad,
        r.width + pad * 2.0,
        r.height + pad * 2.0,
    )
}

/// Union then pad. Empty → `None`.
pub fn expand_union(rects: &[Rect], pad: f64) -> Option<Rect> {
    union_rects(rects).map(|u| padding_rect(u, pad))
}

/// Test whether a segment intersects a (closed) axis-aligned rectangle.
///
/// Touching the rectangle boundary counts as intersecting — routed paths must
/// keep strictly positive clearance from `spacing`-inflated obstacles.
/// Liang–Barsky parametric clipping.
///
/// Shared by the router (collision model) and `verify` (acceptance gate) so
/// both sides judge clearance with the same geometry truth.
pub fn segment_intersects_rect(a: Point, b: Point, r: Rect) -> bool {
    let rx0 = r.x;
    let ry0 = r.y;
    let rx1 = r.x + r.width;
    let ry1 = r.y + r.height;

    let dx = b.x - a.x;
    let dy = b.y - a.y;

    // Liang–Barsky parametric clipping: find t_enter..t_leave overlap with [0,1].
    let mut t0: f64 = 0.0;
    let mut t1: f64 = 1.0;

    // p/q pairs for each slab.
    let checks: [(f64, f64); 4] = [
        (-dx, a.x - rx0), // left
        (dx, rx1 - a.x),  // right
        (-dy, a.y - ry0), // top
        (dy, ry1 - a.y),  // bottom
    ];

    for (p, q) in checks {
        if p.abs() < 1e-12 {
            // Parallel to slab.
            if q < -EPS {
                return false; // Outside.
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
            if t0 > t1 + EPS {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn union_two_rects() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(5.0, 5.0, 10.0, 10.0);
        let u = union_rects(&[a, b]).unwrap();
        assert_eq!(u, Rect::new(0.0, 0.0, 15.0, 15.0));
    }

    #[test]
    fn segment_rect_cases() {
        // inside / outside / crossing / boundary-touch (table-driven).
        let cases: &[((Point, Point), Rect, bool)] = &[
            (
                (Point { x: 2.0, y: 5.0 }, Point { x: 8.0, y: 5.0 }),
                Rect::new(0.0, 0.0, 10.0, 10.0),
                true, // fully inside
            ),
            (
                (Point { x: 20.0, y: 5.0 }, Point { x: 30.0, y: 5.0 }),
                Rect::new(0.0, 0.0, 10.0, 10.0),
                false, // fully outside
            ),
            (
                (Point { x: 0.0, y: 10.0 }, Point { x: 20.0, y: 10.0 }),
                Rect::new(5.0, 5.0, 10.0, 10.0),
                true, // crosses
            ),
            (
                (Point { x: -5.0, y: 0.0 }, Point { x: 15.0, y: 0.0 }),
                Rect::new(0.0, 0.0, 10.0, 10.0),
                true, // runs along the top edge: touching = intersecting
            ),
        ];
        for (i, ((a, b), r, want)) in cases.iter().enumerate() {
            assert_eq!(segment_intersects_rect(*a, *b, *r), *want, "case {i}");
        }
    }
}
