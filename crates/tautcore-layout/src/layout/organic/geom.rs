//! Shared organic geometry helpers + deterministic PRNG. No placement policy.

use tautcore_model::geometry::{Point, Rect, Size};
use tautcore_model::port::Side;

/// Deterministic xorshift64* PRNG. Explicit seed only — never a global rng
/// (workspace determinism rule; ebook 04 §6 陷阱清单).
#[derive(Debug, Clone)]
pub struct Prng {
    state: u64,
}

impl Prng {
    /// Any non-zero seed is valid; 0 is canonicalized to the splitmix constant.
    pub fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 0x9E37_79B9_7F4A_7C15 } else { seed },
        }
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Deterministic unit-scale jitter in `(-1, 1)`.
    pub fn jitter(&mut self) -> (f64, f64) {
        let a = (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64 - 0.5;
        let b = (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64 - 0.5;
        (a, b)
    }

    /// Deterministic Fisher–Yates shuffle.
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        if items.len() < 2 {
            return;
        }
        let mut i = items.len() - 1;
        while i > 0 {
            let j = (self.next_u64() % (i as u64 + 1)) as usize;
            items.swap(i, j);
            i -= 1;
        }
    }
}

pub fn rect_boundary_toward(frame: &Rect, toward: Point) -> Point {
    let c = frame.center();
    let dx = toward.x - c.x;
    let dy = toward.y - c.y;
    if dx.abs() < 1e-12 && dy.abs() < 1e-12 {
        return Point {
            x: frame.right(),
            y: c.y,
        };
    }
    let hw = frame.width / 2.0;
    let hh = frame.height / 2.0;
    let tx = if dx.abs() < 1e-12 {
        f64::INFINITY
    } else {
        hw / dx.abs()
    };
    let ty = if dy.abs() < 1e-12 {
        f64::INFINITY
    } else {
        hh / dy.abs()
    };
    let t = tx.min(ty);
    Point {
        x: c.x + t * dx,
        y: c.y + t * dy,
    }
}

pub fn side_of(p: Point, center: Point) -> Side {
    let dx = p.x - center.x;
    let dy = p.y - center.y;
    if dx.abs() >= dy.abs() {
        if dx >= 0.0 {
            Side::East
        } else {
            Side::West
        }
    } else if dy >= 0.0 {
        Side::South
    } else {
        Side::North
    }
}

pub fn frame_at(size: Size, cx: f64, cy: f64) -> Rect {
    Rect::new(
        cx - size.width / 2.0,
        cy - size.height / 2.0,
        size.width,
        size.height,
    )
}

/// Shift a segment along its left-hand normal by `amount`.
pub fn offset_segment(start: Point, end: Point, amount: f64) -> (Point, Point) {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-12 || amount.abs() < 1e-12 {
        return (start, end);
    }
    let nx = -dy / len;
    let ny = dx / len;
    (
        Point {
            x: start.x + nx * amount,
            y: start.y + ny * amount,
        },
        Point {
            x: end.x + nx * amount,
            y: end.y + ny * amount,
        },
    )
}

pub fn aabb(frames: impl Iterator<Item = Rect>) -> Option<Rect> {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut any = false;
    for f in frames {
        any = true;
        min_x = min_x.min(f.x);
        min_y = min_y.min(f.y);
        max_x = max_x.max(f.right());
        max_y = max_y.max(f.bottom());
    }
    if !any || !min_x.is_finite() {
        return None;
    }
    Some(Rect::new(min_x, min_y, max_x - min_x, max_y - min_y))
}

/// Axis-aligned rect overlap with tolerance.
pub fn rects_overlap(a: &Rect, b: &Rect, gap: f64) -> bool {
    let dx = (a.center().x - b.center().x).abs();
    let dy = (a.center().y - b.center().y).abs();
    dx < (a.width + b.width) / 2.0 + gap - 1e-9
        && dy < (a.height + b.height) / 2.0 + gap - 1e-9
}
