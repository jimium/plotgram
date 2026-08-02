//! Independent curved [`EdgeRouter`](plotgram_engine_api::EdgeRouter).
//!
//! Prefer a cubic Bézier ([`EdgePath::Cubic`]) guided by port outward
//! normals; if that hits obstacles, fall back to
//! [`PolylineEdgeRouter`](crate::PolylineEdgeRouter) + Chaikin smoothing as
//! [`EdgePath::Polyline`]. See `docs/design/routing/curved/README.md`.
//!
//! Honesty (R5): group scenes are rejected in this MVP.

use plotgram_engine_api::{
    EdgeRouter, LayoutError, Obstacle, RouteScene, TerminalPair,
};
use plotgram_model::geometry::Point;
use plotgram_model::port::{PortRef, Side};
use plotgram_model::result::{EdgePath, EdgePlacement};

use crate::core::{padding_rect, segment_intersects_rect};
use crate::PolylineEdgeRouter;

const EPS: f64 = 1e-9;
/// Fixed Bézier sample count (inclusive of endpoints) — deterministic.
const BEZIER_SAMPLES: usize = 24;
/// Chaikin iterations on the polyline fallback.
const CHAIKIN_ITERS: usize = 2;

/// Curved edge router registry name: `"curved"`.
#[derive(Debug, Default, Clone, Copy)]
pub struct CurvedEdgeRouter;

impl EdgeRouter for CurvedEdgeRouter {
    fn name(&self) -> &'static str {
        "curved"
    }

    fn route(&self, scene: &RouteScene) -> Result<Vec<EdgePlacement>, LayoutError> {
        if let Err(reason) = scene.validate() {
            return Err(LayoutError::UnsupportedRouteScene { reason });
        }
        if !scene.group_boundaries.is_empty() || !scene.boundary_permissions.is_empty() {
            return Err(LayoutError::UnsupportedRouteScene {
                reason: "curved router does not support group boundaries \
                         (cannot express gate crossings)"
                    .to_string(),
            });
        }
        check_params(scene)?;

        // Polyline fallback is computed once for the whole scene when needed.
        let mut polyline_cache: Option<Vec<EdgePlacement>> = None;

        let mut out = Vec::with_capacity(scene.edge_order.len());
        for edge_id in &scene.edge_order {
            let pair = scene.terminals.get(edge_id).ok_or_else(|| {
                LayoutError::message(format!(
                    "router: edge `{edge_id}` in edge_order but missing from terminals"
                ))
            })?;
            out.push(route_edge(scene, edge_id, pair, &mut polyline_cache)?);
        }
        Ok(out)
    }
}

fn check_params(scene: &RouteScene) -> Result<(), LayoutError> {
    let p = &scene.params;
    if !p.spacing.is_finite() || p.spacing < 0.0 {
        return Err(LayoutError::message(format!(
            "router: param `spacing` must be finite and >= 0, got {}",
            p.spacing
        )));
    }
    if !p.port_stub.is_finite() || p.port_stub < 0.0 {
        return Err(LayoutError::message(format!(
            "router: param `port_stub` must be finite and >= 0, got {}",
            p.port_stub
        )));
    }
    Ok(())
}

fn route_edge(
    scene: &RouteScene,
    edge_id: &str,
    pair: &TerminalPair,
    polyline_cache: &mut Option<Vec<EdgePlacement>>,
) -> Result<EdgePlacement, LayoutError> {
    let p0 = pair.source.point;
    let p3 = pair.target.point;
    let dist = euclid(p0, p3);
    let arm = scene.params.port_stub.max(dist * 0.35);
    let p1 = offset(p0, pair.source.side, arm);
    let p2 = offset(p3, pair.target.side, arm);

    let bezier = sample_cubic_bezier(p0, p1, p2, p3, BEZIER_SAMPLES);
    let path = if path_clear(scene, pair, &bezier) {
        EdgePath::cubic(p0, p3, [p1, p2])
    } else {
        let poly = ensure_polyline(scene, polyline_cache)?;
        let raw = poly
            .iter()
            .find(|e| e.id == edge_id)
            .ok_or_else(|| {
                LayoutError::message(format!(
                    "router: edge `{edge_id}` missing from polyline fallback"
                ))
            })?
            .path
            .polyline_points()
            .ok_or_else(|| {
                LayoutError::message(format!(
                    "router: edge `{edge_id}` polyline fallback is not a polyline"
                ))
            })?
            .to_vec();
        let smoothed = chaikin_open(&raw, CHAIKIN_ITERS);
        // Chaikin can drift into obstacles — keep the clear polyline then.
        let points = if path_clear(scene, pair, &smoothed) {
            force_endpoints(smoothed, p0, p3)
        } else {
            raw
        };
        EdgePath::polyline(points)
    };

    Ok(EdgePlacement {
        id: edge_id.to_string(),
        source: pair.source.node_id.clone(),
        target: pair.target.node_id.clone(),
        path,
        from_port: Some(PortRef {
            side: pair.source.side,
            slot: 0,
        }),
        to_port: Some(PortRef {
            side: pair.target.side,
            slot: 0,
        }),
    })
}

fn ensure_polyline<'a>(
    scene: &RouteScene,
    cache: &'a mut Option<Vec<EdgePlacement>>,
) -> Result<&'a [EdgePlacement], LayoutError> {
    if cache.is_none() {
        *cache = Some(PolylineEdgeRouter.route(scene)?);
    }
    Ok(cache.as_ref().unwrap().as_slice())
}

fn path_clear(scene: &RouteScene, pair: &TerminalPair, points: &[Point]) -> bool {
    if points.len() < 2 {
        return false;
    }
    let spacing = scene.params.spacing;
    let exempt_src = pair.source.node_id.as_str();
    let exempt_tgt = pair.target.node_id.as_str();
    for w in points.windows(2) {
        if segment_hits_obstacles(w[0], w[1], &scene.obstacles, spacing, exempt_src, exempt_tgt)
        {
            return false;
        }
    }
    true
}

fn segment_hits_obstacles(
    a: Point,
    b: Point,
    obstacles: &[Obstacle],
    spacing: f64,
    exempt_src: &str,
    exempt_tgt: &str,
) -> bool {
    for o in obstacles {
        if o.id == exempt_src || o.id == exempt_tgt {
            continue;
        }
        let r = padding_rect(o.rect, spacing);
        if segment_intersects_rect(a, b, r) {
            return true;
        }
    }
    false
}

fn sample_cubic_bezier(
    p0: Point,
    p1: Point,
    p2: Point,
    p3: Point,
    samples: usize,
) -> Vec<Point> {
    let n = samples.max(2);
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f64 / (n - 1) as f64;
        out.push(cubic_bezier(p0, p1, p2, p3, t));
    }
    out
}

fn cubic_bezier(p0: Point, p1: Point, p2: Point, p3: Point, t: f64) -> Point {
    let u = 1.0 - t;
    let uu = u * u;
    let tt = t * t;
    let uuu = uu * u;
    let ttt = tt * t;
    Point {
        x: uuu * p0.x + 3.0 * uu * t * p1.x + 3.0 * u * tt * p2.x + ttt * p3.x,
        y: uuu * p0.y + 3.0 * uu * t * p1.y + 3.0 * u * tt * p2.y + ttt * p3.y,
    }
}

/// Open-curve Chaikin: keep exact endpoints, refine interiors.
fn chaikin_open(points: &[Point], iters: usize) -> Vec<Point> {
    if points.len() < 2 || iters == 0 {
        return points.to_vec();
    }
    let start = points[0];
    let end = *points.last().unwrap();
    let mut pts = points.to_vec();
    for _ in 0..iters {
        if pts.len() < 2 {
            break;
        }
        let mut next = Vec::with_capacity(pts.len() * 2);
        for w in pts.windows(2) {
            let a = w[0];
            let b = w[1];
            next.push(Point {
                x: 0.75 * a.x + 0.25 * b.x,
                y: 0.75 * a.y + 0.25 * b.y,
            });
            next.push(Point {
                x: 0.25 * a.x + 0.75 * b.x,
                y: 0.25 * a.y + 0.75 * b.y,
            });
        }
        pts = next;
    }
    force_endpoints(pts, start, end)
}

fn force_endpoints(mut pts: Vec<Point>, start: Point, end: Point) -> Vec<Point> {
    if pts.is_empty() {
        return vec![start, end];
    }
    pts[0] = start;
    let last = pts.len() - 1;
    pts[last] = end;
    // Drop consecutive duplicates after endpoint snap.
    let mut out = Vec::with_capacity(pts.len());
    for p in pts {
        if out
            .last()
            .is_some_and(|l: &Point| (l.x - p.x).abs() < EPS && (l.y - p.y).abs() < EPS)
        {
            continue;
        }
        out.push(p);
    }
    if out.len() < 2 {
        vec![start, end]
    } else {
        out
    }
}

fn offset(p: Point, side: Side, len: f64) -> Point {
    match side {
        Side::North => Point {
            x: p.x,
            y: p.y - len,
        },
        Side::South => Point {
            x: p.x,
            y: p.y + len,
        },
        Side::East => Point {
            x: p.x + len,
            y: p.y,
        },
        Side::West => Point {
            x: p.x - len,
            y: p.y,
        },
    }
}

fn euclid(a: Point, b: Point) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    (dx * dx + dy * dy).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use plotgram_engine_api::{GroupBoundary, OrthogonalRouteParams, PortAnchor};
    use plotgram_model::geometry::Rect;
    use std::collections::BTreeMap;

    fn obstacle(id: &str, x: f64, y: f64, w: f64, h: f64) -> Obstacle {
        Obstacle {
            id: id.to_string(),
            rect: Rect::new(x, y, w, h),
        }
    }

    fn anchor(x: f64, y: f64, side: Side, node: &str) -> PortAnchor {
        PortAnchor {
            point: Point { x, y },
            side,
            node_id: node.to_string(),
        }
    }

    fn scene(obstacles: Vec<Obstacle>, source: PortAnchor, target: PortAnchor) -> RouteScene {
        let mut terminals = BTreeMap::new();
        terminals.insert("e0".to_string(), TerminalPair { source, target });
        RouteScene {
            obstacles,
            terminals,
            edge_order: vec!["e0".to_string()],
            group_boundaries: vec![],
            boundary_permissions: BTreeMap::new(),
            params: OrthogonalRouteParams {
                spacing: 10.0,
                port_stub: 20.0,
                ..OrthogonalRouteParams::default()
            },
        }
    }

    fn two_boxes_clear() -> RouteScene {
        scene(
            vec![
                obstacle("a", 0.0, 0.0, 80.0, 40.0),
                obstacle("b", 200.0, 0.0, 80.0, 40.0),
            ],
            anchor(80.0, 20.0, Side::East, "a"),
            anchor(200.0, 20.0, Side::West, "b"),
        )
    }

    fn blocker_center() -> RouteScene {
        scene(
            vec![
                obstacle("a", 0.0, 80.0, 80.0, 40.0),
                obstacle("b", 300.0, 80.0, 80.0, 40.0),
                obstacle("blocker", 150.0, 60.0, 80.0, 80.0),
            ],
            anchor(80.0, 100.0, Side::East, "a"),
            anchor(300.0, 100.0, Side::West, "b"),
        )
    }

    #[test]
    fn clear_scene_is_smooth_bezier() {
        let p = CurvedEdgeRouter.route(&two_boxes_clear()).unwrap();
        match &p[0].path {
            EdgePath::Cubic {
                start,
                end,
                controls,
            } => {
                assert_eq!(*start, Point { x: 80.0, y: 20.0 });
                assert_eq!(*end, Point { x: 200.0, y: 20.0 });
                assert!(controls[0].x > start.x);
                assert!(controls[1].x < end.x);
            }
            other => panic!("expected Cubic, got {other:?}"),
        }
    }

    #[test]
    fn blocker_avoids_obstacle() {
        let sc = blocker_center();
        let p = CurvedEdgeRouter.route(&sc).unwrap();
        let samples = p[0].path.samples();
        assert_eq!(samples.first().copied(), Some(Point { x: 80.0, y: 100.0 }));
        assert_eq!(samples.last().copied(), Some(Point { x: 300.0, y: 100.0 }));
        let inflated = padding_rect(sc.obstacles[2].rect, sc.params.spacing);
        for w in samples.windows(2) {
            assert!(
                !segment_intersects_rect(w[0], w[1], inflated),
                "segment {:?}→{:?} hits blocker",
                w[0],
                w[1]
            );
        }
        // Obstacle forces polyline (+ Chaikin) fallback, not a free cubic.
        assert!(
            matches!(p[0].path, EdgePath::Polyline { .. }),
            "expected polyline fallback, got {:?}",
            p[0].path
        );
    }

    #[test]
    fn group_scene_is_unsupported() {
        let mut sc = two_boxes_clear();
        sc.group_boundaries.push(GroupBoundary {
            group_id: "g".to_string(),
            rect: Rect::new(100.0, 0.0, 40.0, 40.0),
        });
        match CurvedEdgeRouter.route(&sc) {
            Err(LayoutError::UnsupportedRouteScene { reason }) => {
                assert!(reason.contains("group"), "{reason}");
            }
            other => panic!("expected UnsupportedRouteScene, got {other:?}"),
        }
    }

    #[test]
    fn double_run_is_bit_identical() {
        let sc = blocker_center();
        let a = CurvedEdgeRouter.route(&sc).unwrap();
        let b = CurvedEdgeRouter.route(&sc).unwrap();
        for (pa, pb) in a.iter().zip(b.iter()) {
            assert_eq!(pa.path, pb.path);
        }
    }
}
