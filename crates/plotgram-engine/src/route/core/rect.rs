//! Rectangle helpers for group envelopes and obstacles.

use plotgram_model::geometry::Rect;

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
}
