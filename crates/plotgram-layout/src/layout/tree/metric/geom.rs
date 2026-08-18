//! Shared geometry for placers. No placement policy.

use plotgram_model::geometry::{Point, Rect};
use plotgram_model::port::Side;

use crate::layout::tree::params::TreeRoutingStyle;
use crate::layout::tree::plan::TreeRoute;

pub fn port_point(frame: &Rect, side: Side) -> Point {
    match side {
        Side::North => Point {
            x: frame.center().x,
            y: frame.y,
        },
        Side::South => Point {
            x: frame.center().x,
            y: frame.bottom(),
        },
        Side::East => Point {
            x: frame.right(),
            y: frame.center().y,
        },
        Side::West => Point {
            x: frame.x,
            y: frame.center().y,
        },
    }
}

pub fn opposite(side: Side) -> Side {
    match side {
        Side::North => Side::South,
        Side::South => Side::North,
        Side::East => Side::West,
        Side::West => Side::East,
    }
}

/// Minimum visible drop between an elbow and the child port. Without it the
/// elbow can pin onto the child row's top edge: the final segment degenerates
/// to zero length (arrowhead orientation falls back to the horizontal run)
/// and the horizontal run grazes node tops.
const MIN_FINAL_SEGMENT: f64 = 16.0;

pub fn orthogonal_route(start: Point, from_side: Side, end: Point, to_side: Side) -> TreeRoute {
    let vertical_from = matches!(from_side, Side::North | Side::South);
    let vertical_to = matches!(to_side, Side::North | Side::South);
    if vertical_from && vertical_to {
        let mid_y = (start.y + end.y) / 2.0;
        TreeRoute::OrthoThreeSeg { start, mid_y, end }
    } else if !vertical_from && !vertical_to {
        let mid_x = (start.x + end.x) / 2.0;
        TreeRoute::Polyline {
            points: vec![
                start,
                Point {
                    x: mid_x,
                    y: start.y,
                },
                Point { x: mid_x, y: end.y },
                end,
            ],
        }
    } else if vertical_from {
        TreeRoute::Polyline {
            points: vec![
                start,
                Point {
                    x: start.x,
                    y: end.y,
                },
                end,
            ],
        }
    } else {
        TreeRoute::Polyline {
            points: vec![
                start,
                Point {
                    x: end.x,
                    y: start.y,
                },
                end,
            ],
        }
    }
}

pub fn translate_route(route: &mut TreeRoute, dx: f64, dy: f64) {
    let bump = |p: &mut Point| {
        p.x += dx;
        p.y += dy;
    };
    match route {
        TreeRoute::OrthoThreeSeg { start, mid_y, end } => {
            bump(start);
            *mid_y += dy;
            bump(end);
        }
        TreeRoute::Polyline { points } => {
            for p in points {
                bump(p);
            }
        }
        TreeRoute::Straight { start, end } => {
            bump(start);
            bump(end);
        }
        TreeRoute::HorizontalBus { start, bus_y, end } => {
            bump(start);
            *bus_y += dy;
            bump(end);
        }
    }
}

pub fn parent_child_route(
    start: Point,
    from_side: Side,
    end: Point,
    to_side: Side,
    style: TreeRoutingStyle,
    min_first_segment: f64,
) -> TreeRoute {
    match style {
        TreeRoutingStyle::Straight => TreeRoute::Straight { start, end },
        TreeRoutingStyle::OrthogonalAtRoot => {
            let dir = if matches!(from_side, Side::North) {
                -1.0
            } else {
                1.0
            };
            let gap = (end.y - start.y) * dir;
            let stem = min_first_segment
                .min((gap - MIN_FINAL_SEGMENT).max(gap / 2.0))
                .max(0.0);
            let elbow_y = start.y + dir * stem;
            TreeRoute::Polyline {
                points: vec![
                    start,
                    Point {
                        x: start.x,
                        y: elbow_y,
                    },
                    end,
                ],
            }
        }
        TreeRoutingStyle::Orthogonal | TreeRoutingStyle::Polyline => {
            let mut route = orthogonal_route(start, from_side, end, to_side);
            if let TreeRoute::OrthoThreeSeg { start, mid_y, end } = &mut route {
                if matches!(from_side, Side::South) && *mid_y < start.y + min_first_segment {
                    // Honor the stem minimum when the seam has room, but keep
                    // the elbow above `end.y - MIN_FINAL_SEGMENT` (or at the
                    // natural midpoint on tight seams) so the final drop into
                    // the child stays vertical and visible.
                    let cap = (end.y - MIN_FINAL_SEGMENT).max(*mid_y);
                    *mid_y = (start.y + min_first_segment).min(cap);
                }
            }
            route
        }
    }
}

pub fn polar_point(origin: Point, radius: f64, theta: f64) -> Point {
    Point {
        x: origin.x + radius * theta.cos(),
        y: origin.y + radius * theta.sin(),
    }
}

/// Intersection of the ray `frame.center → toward` with the frame boundary.
pub fn rect_boundary_toward(frame: &Rect, toward: Point) -> Point {
    let c = frame.center();
    let dx = toward.x - c.x;
    let dy = toward.y - c.y;
    if dx.abs() < 1e-12 && dy.abs() < 1e-12 {
        return port_point(frame, Side::South);
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

pub fn spoke_straight(parent: &Rect, child: &Rect) -> TreeRoute {
    TreeRoute::Straight {
        start: rect_boundary_toward(parent, child.center()),
        end: rect_boundary_toward(child, parent.center()),
    }
}

pub fn node_extent(frame: &Rect) -> f64 {
    (frame.width * frame.width + frame.height * frame.height).sqrt() / 2.0
}
