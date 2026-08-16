//! Polar helpers for radial / balloon. No placement policy beyond geometry.

use std::f64::consts::PI;

use plotgram_model::geometry::{Point, Rect};

use super::super::geom::{node_extent, polar_point, rect_boundary_toward, spoke_straight};
use super::super::shape::SubtreeShape;
use crate::layout::tree::plan::TreeRoute;

pub const TAU: f64 = 2.0 * PI;
/// Start at 12 o'clock; increasing theta is clockwise on a y-down canvas.
pub const THETA0: f64 = -PI / 2.0;

pub fn wrap_delta(mut d: f64) -> f64 {
    while d > PI {
        d -= TAU;
    }
    while d < -PI {
        d += TAU;
    }
    d
}

pub fn arc_points(origin: Point, radius: f64, t0: f64, t1: f64) -> Vec<Point> {
    let d = wrap_delta(t1 - t0);
    let steps = (d.abs() / (PI / 12.0)).ceil().max(1.0) as usize;
    let mut out = Vec::with_capacity(steps + 1);
    for i in 0..=steps {
        let t = t0 + d * (i as f64 / steps as f64);
        out.push(polar_point(origin, radius, t));
    }
    out
}

pub fn polar_spoke(
    origin: Point,
    parent: &Rect,
    parent_r: f64,
    parent_th: f64,
    child: &Rect,
    child_r: f64,
    child_th: f64,
) -> TreeRoute {
    if parent_r < 1e-6 || wrap_delta(child_th - parent_th).abs() < 1e-6 {
        return spoke_straight(parent, child);
    }
    let r_arc = (parent_r + child_r) / 2.0;
    let mut points = Vec::new();
    points.push(rect_boundary_toward(
        parent,
        polar_point(origin, r_arc.max(parent_r), parent_th),
    ));
    points.extend(arc_points(origin, r_arc, parent_th, child_th));
    points.push(rect_boundary_toward(
        child,
        polar_point(origin, r_arc.min(child_r), child_th),
    ));
    TreeRoute::Polyline { points }
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

pub fn rotate_shape_upright(shape: &mut SubtreeShape, pivot: Point, angle: f64) {
    if angle.abs() < 1e-12 {
        return;
    }
    for f in shape.frames.values_mut() {
        let c = rotate_point(f.center(), pivot, angle);
        f.x = c.x - f.width / 2.0;
        f.y = c.y - f.height / 2.0;
    }
    for route in shape.routes.values_mut() {
        rotate_route(route, pivot, angle);
    }
}

fn rotate_route(route: &mut TreeRoute, pivot: Point, angle: f64) {
    match route {
        TreeRoute::Straight { start, end } => {
            *start = rotate_point(*start, pivot, angle);
            *end = rotate_point(*end, pivot, angle);
        }
        TreeRoute::Polyline { points } => {
            for p in points {
                *p = rotate_point(*p, pivot, angle);
            }
        }
        TreeRoute::OrthoThreeSeg { start, mid_y, end } => {
            *start = rotate_point(*start, pivot, angle);
            *end = rotate_point(*end, pivot, angle);
            *mid_y = (*start).y;
        }
        TreeRoute::HorizontalBus { start, bus_y, end } => {
            *start = rotate_point(*start, pivot, angle);
            *end = rotate_point(*end, pivot, angle);
            *bus_y = (*start).y;
        }
    }
}

pub fn radius_from_root(shape: &SubtreeShape, root: &str) -> f64 {
    let Some(rf) = shape.frames.get(root) else {
        return 0.0;
    };
    let c = rf.center();
    let mut r = node_extent(rf);
    for f in shape.frames.values() {
        let corners = [
            Point { x: f.x, y: f.y },
            Point {
                x: f.right(),
                y: f.y,
            },
            Point {
                x: f.x,
                y: f.bottom(),
            },
            Point {
                x: f.right(),
                y: f.bottom(),
            },
        ];
        for p in corners {
            let d = ((p.x - c.x).powi(2) + (p.y - c.y).powi(2)).sqrt();
            r = r.max(d);
        }
    }
    r
}
