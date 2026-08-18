//! Shelf (row) packing of disconnected components.
//!
//! First-fit-decreasing on shelves toward a target aspect ratio
//! (ebook 04 §7: rows/grid first, polyomino later). Deterministic:
//! boxes sorted by area desc, ties by component id asc.

use std::collections::BTreeMap;

use tautcore_model::geometry::Rect;

/// Returns per-owner translation (dx, dy) placing every box flush to the
/// origin, packed on shelves left→right / top→bottom.
pub fn pack_shelves(
    boxes: &[(u32, Rect)],
    gap: f64,
    aspect_ratio: f64,
) -> BTreeMap<u32, (f64, f64)> {
    let mut offsets = BTreeMap::new();
    if boxes.is_empty() {
        return offsets;
    }

    let mut order: Vec<usize> = (0..boxes.len()).collect();
    order.sort_by(|&a, &b| {
        let area_a = boxes[a].1.width * boxes[a].1.height;
        let area_b = boxes[b].1.width * boxes[b].1.height;
        area_b
            .partial_cmp(&area_a)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| boxes[a].0.cmp(&boxes[b].0))
    });

    let total_area: f64 = boxes.iter().map(|(_, r)| r.width * r.height).sum();
    let max_w = boxes.iter().map(|(_, r)| r.width).fold(0.0, f64::max);
    let target_w = (total_area * aspect_ratio).sqrt().max(max_w);

    let mut cursor_x = 0.0;
    let mut cursor_y = 0.0;
    let mut row_height = 0.0;
    for &bi in &order {
        let (owner, rect) = &boxes[bi];
        let w = rect.width;
        let h = rect.height;
        if cursor_x > 0.0 && cursor_x + w > target_w {
            // New shelf.
            cursor_x = 0.0;
            cursor_y += row_height + gap;
            row_height = 0.0;
        }
        let dx = cursor_x - rect.x;
        let dy = cursor_y - rect.y;
        offsets.insert(*owner, (dx, dy));
        cursor_x += w + gap;
        row_height = row_height.max(h);
    }
    offsets
}

#[cfg(test)]
mod tests {
    use super::*;

    fn box_(owner: u32, x: f64, y: f64, w: f64, h: f64) -> (u32, Rect) {
        (owner, Rect::new(x, y, w, h))
    }

    #[test]
    fn shelves_respect_gap_and_order() {
        let boxes = vec![
            box_(0, -10.0, -5.0, 100.0, 50.0),
            box_(1, 500.0, 500.0, 80.0, 40.0),
            box_(2, 0.0, 0.0, 60.0, 30.0),
        ];
        let offsets = pack_shelves(&boxes, 10.0, 1.0);
        let placed: Vec<Rect> = boxes
            .iter()
            .map(|(o, r)| {
                let (dx, dy) = offsets[o];
                Rect::new(r.x + dx, r.y + dy, r.width, r.height)
            })
            .collect();
        // Largest first.
        assert_eq!(placed[0].x, 0.0);
        assert_eq!(placed[0].y, 0.0);
        // 80-wide goes next on the same shelf (0+100+10+80=190 ≤ target≈136?
        // target = sqrt(100*50+80*40+60*30) ≈ sqrt(9800) ≈ 99 → wraps).
        assert!(placed[1].x + placed[1].width <= placed[0].right() + 1e-9 || placed[1].y >= placed[0].bottom() - 1e-9);
        // No overlap between any two.
        for i in 0..3 {
            for j in (i + 1)..3 {
                let a = &placed[i];
                let b = &placed[j];
                let sep_x = a.x >= b.right() + 10.0 - 1e-6 || b.x >= a.right() + 10.0 - 1e-6;
                let sep_y = a.y >= b.bottom() + 10.0 - 1e-6 || b.y >= a.bottom() + 10.0 - 1e-6;
                assert!(sep_x || sep_y, "boxes {i}/{j} overlap after packing");
            }
        }
    }
}
