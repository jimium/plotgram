//! Shared circular geometry. No placement policy.

use std::f64::consts::PI;

use plotgram_model::geometry::{Point, Rect, Size};
use plotgram_model::port::Side;

pub const TAU: f64 = 2.0 * PI;
/// Start at 12 o'clock; increasing theta is clockwise on a y-down canvas.
pub const THETA0: f64 = -PI / 2.0;
const EPS: f64 = 1e-9;

pub fn polar_point(origin: Point, radius: f64, theta: f64) -> Point {
    Point {
        x: origin.x + radius * theta.cos(),
        y: origin.y + radius * theta.sin(),
    }
}

pub fn node_extent(size: Size) -> f64 {
    (size.width * size.width + size.height * size.height).sqrt() / 2.0
}

pub fn wrap_delta(mut d: f64) -> f64 {
    while d > PI {
        d -= TAU;
    }
    while d < -PI {
        d += TAU;
    }
    d
}

/// Short-arc helper (signed delta wrapped to (-π, π]).
#[allow(dead_code)]
pub fn arc_points(origin: Point, radius: f64, t0: f64, t1: f64) -> Vec<Point> {
    arc_points_span(origin, radius, t0, wrap_delta(t1 - t0))
}

/// Sample an arc of signed `span` radians starting at `t0` (may exceed π).
pub fn arc_points_span(origin: Point, radius: f64, t0: f64, span: f64) -> Vec<Point> {
    let steps = (span.abs() / (PI / 12.0)).ceil().max(1.0) as usize;
    let mut out = Vec::with_capacity(steps + 1);
    for i in 0..=steps {
        let t = t0 + span * (i as f64 / steps as f64);
        out.push(polar_point(origin, radius, t));
    }
    out
}

/// Clockwise delta from `t0` to `t1` in `[0, TAU)`.
pub fn clockwise_span(t0: f64, t1: f64) -> f64 {
    let mut d = t1 - t0;
    while d < 0.0 {
        d += TAU;
    }
    while d >= TAU {
        d -= TAU;
    }
    d
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

/// Short horseshoe on the side of `frame` facing away from `origin`.
pub fn loop_short_arc(frame: &Rect, origin: Point, extra: f64) -> Vec<Point> {
    let c = frame.center();
    let dx = c.x - origin.x;
    let dy = c.y - origin.y;
    let len = (dx * dx + dy * dy).sqrt();
    let (ux, uy) = if len > 1e-6 {
        (dx / len, dy / len)
    } else {
        (1.0, 0.0)
    };
    let r = 16.0 + extra.max(0.0);
    let offset = node_extent(frame.size()) + 2.0;
    let arc_c = Point {
        x: c.x + ux * offset,
        y: c.y + uy * offset,
    };
    let theta = uy.atan2(ux);
    let span = 4.0;
    let t0 = theta - span / 2.0;
    let start_aim = polar_point(arc_c, r, t0);
    let end_aim = polar_point(arc_c, r, t0 + span);
    let start = rect_boundary_toward(frame, start_aim);
    let end = rect_boundary_toward(frame, end_aim);
    let mut points = Vec::with_capacity(16);
    points.push(start);
    points.extend(arc_points_span(arc_c, r, t0, span));
    points.push(end);
    points
}

/// Intersection of the ray `frame.center → toward` with the frame boundary.
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

#[derive(Debug, Clone)]
pub struct CycleGeom {
    pub angles: Vec<f64>,
    pub radius: f64,
}

/// Weighted CYCLE: each node owns a wedge proportional to extent, sits at
/// the wedge centre. Radius from adjacent chord constraints (architecture §5.2).
pub fn cycle_geom(sizes: &[Size], node_gap: f64, min_radius: f64, rotation: f64) -> CycleGeom {
    let n = sizes.len();
    if n == 0 {
        return CycleGeom {
            angles: Vec::new(),
            radius: min_radius,
        };
    }
    if n == 1 {
        return CycleGeom {
            angles: vec![THETA0 + rotation],
            radius: min_radius,
        };
    }

    let extents: Vec<f64> = sizes.iter().copied().map(node_extent).collect();

    if n == 2 {
        let r = min_radius.max((extents[0] + extents[1] + node_gap) / 2.0);
        return CycleGeom {
            angles: vec![THETA0 + rotation, THETA0 + rotation + PI],
            radius: r,
        };
    }

    let weights: Vec<f64> = extents.iter().map(|e| e.max(1e-6)).collect();
    let total: f64 = weights.iter().sum();
    let mut angles = Vec::with_capacity(n);
    let mut cursor = THETA0 + rotation;
    for w in &weights {
        let wedge = TAU * (*w / total);
        angles.push(cursor + wedge / 2.0);
        cursor += wedge;
    }

    let mut radius = min_radius;
    for i in 0..n {
        let j = (i + 1) % n;
        let dtheta = (weights[i] + weights[j]) / total * TAU / 2.0;
        if dtheta >= PI - EPS {
            continue;
        }
        let half = (dtheta / 2.0).max(1e-4);
        let need = (extents[i] + extents[j] + node_gap) / (2.0 * half.sin());
        radius = radius.max(need);
    }

    CycleGeom { angles, radius }
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

/// Angle reserved on a non-root balloon ring for the parent spoke.
pub const BALLOON_RESERVED: f64 = 0.4;

pub fn ring_fits(r: f64, radii: &[f64], reserved: f64) -> bool {
    let mut sum = 0.0;
    for &ri in radii {
        if ri >= r - 1e-9 {
            return false;
        }
        sum += 2.0 * (ri / r).asin();
    }
    sum <= TAU - reserved + 1e-9
}

pub fn fit_ring_radius(radii: &[f64], reserved: f64, lo_floor: f64) -> f64 {
    if radii.is_empty() {
        return lo_floor.max(0.0);
    }
    let r_max = radii.iter().copied().fold(0.0, f64::max);
    let mut lo = lo_floor.max(r_max + 1e-6);
    let mut hi = lo * (radii.len() as f64).max(2.0);
    for _ in 0..24 {
        if ring_fits(hi, radii, reserved) {
            break;
        }
        hi *= 2.0;
    }
    for _ in 0..40 {
        let mid = (lo + hi) / 2.0;
        if ring_fits(mid, radii, reserved) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    hi
}

pub fn rotate_point(p: Point, pivot: Point, angle: f64) -> Point {
    let (s, c) = angle.sin_cos();
    let dx = p.x - pivot.x;
    let dy = p.y - pivot.y;
    Point {
        x: pivot.x + c * dx - s * dy,
        y: pivot.y + s * dx + c * dy,
    }
}
