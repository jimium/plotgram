//! Independent polyline [`EdgeRouter`](tautcore_engine_api::EdgeRouter).
//!
//! Any-angle obstacle-avoiding routes via a corner visibility graph +
//! Euclidean Dijkstra. See `docs/design/routing/polyline/README.md`.
//!
//! Honesty (R5): group scenes are rejected in this MVP.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use tautcore_engine_api::{EdgeRouter, LayoutError, RouteScene, TerminalPair};
use tautcore_model::geometry::{Point, Rect};
use tautcore_model::port::{PortRef, Side};
use tautcore_model::result::{EdgePath, EdgePlacement};

use crate::core::{padding_rect, segment_intersects_rect};

/// Keep corner vertices just outside the `spacing`-inflated collision box so
/// same-obstacle corner chords lie in free space (boundary touch is blocked).
const VERTEX_CLEARANCE_EPS: f64 = 1e-3;

/// Collinearity / duplicate tolerance for path normalization.
const NORM_EPS: f64 = 1e-9;

/// Polyline edge router registry name: `"polyline"`.
#[derive(Debug, Default, Clone, Copy)]
pub struct PolylineEdgeRouter;

impl EdgeRouter for PolylineEdgeRouter {
    fn name(&self) -> &'static str {
        "polyline"
    }

    fn route(&self, scene: &RouteScene) -> Result<Vec<EdgePlacement>, LayoutError> {
        if let Err(reason) = scene.validate() {
            return Err(LayoutError::UnsupportedRouteScene { reason });
        }
        if !scene.group_boundaries.is_empty() || !scene.boundary_permissions.is_empty() {
            return Err(LayoutError::UnsupportedRouteScene {
                reason: "polyline router does not support group boundaries \
                         (cannot express gate crossings)"
                    .to_string(),
            });
        }
        check_params(scene)?;

        let mut out = Vec::with_capacity(scene.edge_order.len());
        for edge_id in &scene.edge_order {
            let pair = scene.terminals.get(edge_id).ok_or_else(|| {
                LayoutError::message(format!(
                    "router: edge `{edge_id}` in edge_order but missing from terminals"
                ))
            })?;
            out.push(route_edge(scene, edge_id, pair)?);
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
) -> Result<EdgePlacement, LayoutError> {
    let spacing = scene.params.spacing;
    let stub_len = scene.params.port_stub;
    let q_source = stub_point(pair.source.point, pair.source.side, stub_len);
    let q_target = stub_point(pair.target.point, pair.target.side, stub_len);

    let exempt_src = pair.source.node_id.as_str();
    let exempt_tgt = pair.target.node_id.as_str();

    let inflated: Vec<(String, Rect)> = scene
        .obstacles
        .iter()
        .map(|o| (o.id.clone(), padding_rect(o.rect, spacing)))
        .collect();

    // Vertices: terminals, stubs, and corners of (spacing+eps)-padded obstacles.
    let corner_pad = spacing + VERTEX_CLEARANCE_EPS;
    let mut verts: Vec<Point> = Vec::with_capacity(4 + scene.obstacles.len() * 4);
    verts.push(pair.source.point);
    verts.push(q_source);
    verts.push(q_target);
    verts.push(pair.target.point);
    for o in &scene.obstacles {
        let r = padding_rect(o.rect, corner_pad);
        verts.push(Point { x: r.x, y: r.y });
        verts.push(Point {
            x: r.right(),
            y: r.y,
        });
        verts.push(Point {
            x: r.right(),
            y: r.bottom(),
        });
        verts.push(Point {
            x: r.x,
            y: r.bottom(),
        });
    }

    // Stable unique vertices: sort then dedupe.
    verts.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    verts.dedup_by(|a, b| (a.x - b.x).abs() < NORM_EPS && (a.y - b.y).abs() < NORM_EPS);

    // Drop vertices strictly inside a non-exempt inflated obstacle.
    verts.retain(|p| {
        !inflated.iter().any(|(id, r)| {
            id.as_str() != exempt_src && id.as_str() != exempt_tgt && point_strictly_inside(*p, *r)
        })
    });

    let start = find_vertex(&verts, q_source).ok_or_else(|| {
        LayoutError::message(format!(
            "router: edge `{edge_id}` source stub is not a visibility vertex"
        ))
    })?;
    let goal = find_vertex(&verts, q_target).ok_or_else(|| {
        LayoutError::message(format!(
            "router: edge `{edge_id}` target stub is not a visibility vertex"
        ))
    })?;

    let n = verts.len();
    let mut adj: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
    for i in 0..n {
        for j in (i + 1)..n {
            if segment_clear(verts[i], verts[j], &inflated, exempt_src, exempt_tgt) {
                let w = euclid(verts[i], verts[j]);
                adj[i].push((j, w));
                adj[j].push((i, w));
            }
        }
        // Stable neighbour order for deterministic expansion ties.
        adj[i].sort_by(|a, b| a.0.cmp(&b.0));
    }

    let path_idx = dijkstra(&adj, start, goal).ok_or_else(|| {
        LayoutError::message(format!(
            "router: edge `{edge_id}` has no collision-free path"
        ))
    })?;

    let mut points = Vec::with_capacity(path_idx.len() + 2);
    points.push(pair.source.point);
    for &i in &path_idx {
        points.push(verts[i]);
    }
    points.push(pair.target.point);
    let points = normalize_any_angle(&points);

    Ok(EdgePlacement {
        id: edge_id.to_string(),
        source: pair.source.node_id.clone(),
        target: pair.target.node_id.clone(),
        path: EdgePath::polyline(points),
        from_port: Some(PortRef {
            side: pair.source.side,
            along: tautcore_model::port::AlongSpec::Ordered { order: 0, count: 1 },
        }),
        to_port: Some(PortRef {
            side: pair.target.side,
            along: tautcore_model::port::AlongSpec::Ordered { order: 0, count: 1 },
        }),
    })
}

fn stub_point(anchor: Point, side: Side, stub_len: f64) -> Point {
    match side {
        Side::North => Point {
            x: anchor.x,
            y: anchor.y - stub_len,
        },
        Side::South => Point {
            x: anchor.x,
            y: anchor.y + stub_len,
        },
        Side::East => Point {
            x: anchor.x + stub_len,
            y: anchor.y,
        },
        Side::West => Point {
            x: anchor.x - stub_len,
            y: anchor.y,
        },
    }
}

fn euclid(a: Point, b: Point) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    (dx * dx + dy * dy).sqrt()
}

fn point_strictly_inside(p: Point, r: Rect) -> bool {
    p.x > r.x + NORM_EPS
        && p.x < r.right() - NORM_EPS
        && p.y > r.y + NORM_EPS
        && p.y < r.bottom() - NORM_EPS
}

fn find_vertex(verts: &[Point], p: Point) -> Option<usize> {
    verts
        .iter()
        .position(|v| (v.x - p.x).abs() < NORM_EPS && (v.y - p.y).abs() < NORM_EPS)
}

fn segment_clear(
    a: Point,
    b: Point,
    inflated: &[(String, Rect)],
    exempt_src: &str,
    exempt_tgt: &str,
) -> bool {
    for (id, r) in inflated {
        if id.as_str() == exempt_src || id.as_str() == exempt_tgt {
            continue;
        }
        if segment_intersects_rect(a, b, *r) {
            return false;
        }
    }
    true
}

/// Min-cost path indices from `start` to `goal`. Tie-break: prefer smaller
/// predecessor index then smaller vertex index (deterministic).
fn dijkstra(adj: &[Vec<(usize, f64)>], start: usize, goal: usize) -> Option<Vec<usize>> {
    if start == goal {
        return Some(vec![start]);
    }
    let n = adj.len();
    let mut dist = vec![f64::INFINITY; n];
    let mut prev = vec![None; n];
    let mut settled = vec![false; n];
    dist[start] = 0.0;

    let mut heap = BinaryHeap::new();
    heap.push(HeapNode {
        cost: 0.0,
        idx: start,
    });

    while let Some(HeapNode { cost, idx }) = heap.pop() {
        if settled[idx] {
            continue;
        }
        settled[idx] = true;
        if idx == goal {
            break;
        }
        if cost > dist[idx] {
            continue;
        }
        for &(next, w) in &adj[idx] {
            if settled[next] {
                continue;
            }
            let nd = cost + w;
            if nd < dist[next] - NORM_EPS {
                dist[next] = nd;
                prev[next] = Some(idx);
                heap.push(HeapNode {
                    cost: nd,
                    idx: next,
                });
            } else if (nd - dist[next]).abs() <= NORM_EPS {
                if prev[next].is_none_or(|p| idx < p) {
                    prev[next] = Some(idx);
                }
            }
        }
    }

    if !dist[goal].is_finite() {
        return None;
    }
    let mut path = Vec::new();
    let mut cur = goal;
    path.push(cur);
    while cur != start {
        cur = prev[cur]?;
        path.push(cur);
    }
    path.reverse();
    Some(path)
}

#[derive(Copy, Clone)]
struct HeapNode {
    cost: f64,
    idx: usize,
}

impl PartialEq for HeapNode {
    fn eq(&self, other: &Self) -> bool {
        self.cost == other.cost && self.idx == other.idx
    }
}
impl Eq for HeapNode {}

impl PartialOrd for HeapNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for HeapNode {
    fn cmp(&self, other: &Self) -> Ordering {
        // BinaryHeap is max-heap: reverse cost so smallest pops first.
        match other.cost.total_cmp(&self.cost) {
            Ordering::Equal => other.idx.cmp(&self.idx),
            ord => ord,
        }
    }
}

fn normalize_any_angle(points: &[Point]) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::with_capacity(points.len());
    for &p in points {
        if out
            .last()
            .is_some_and(|l| (l.x - p.x).abs() < NORM_EPS && (l.y - p.y).abs() < NORM_EPS)
        {
            continue;
        }
        out.push(p);
    }
    // Merge nearly-collinear triples.
    let mut i = 0;
    while i + 2 < out.len() {
        let a = out[i];
        let b = out[i + 1];
        let c = out[i + 2];
        let abx = b.x - a.x;
        let aby = b.y - a.y;
        let bcx = c.x - b.x;
        let bcy = c.y - b.y;
        let cross = abx * bcy - aby * bcx;
        let dot = abx * bcx + aby * bcy;
        if cross.abs() < NORM_EPS && dot >= -NORM_EPS {
            out.remove(i + 1);
        } else {
            i += 1;
        }
    }
    if out.len() < 2 {
        // Degenerate: keep a two-point path for render contracts.
        if let Some(p) = points.first().copied() {
            return vec![p, p];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tautcore_engine_api::{GroupBoundary, Obstacle, OrthogonalRouteParams, PortAnchor};
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
                port_stub: 5.0,
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
    fn clear_scene_is_near_straight() {
        let p = PolylineEdgeRouter.route(&two_boxes_clear()).unwrap();
        assert_eq!(p.len(), 1);
        let pts = &p[0].path.polyline_points().unwrap();
        assert_eq!(pts.first().copied(), Some(Point { x: 80.0, y: 20.0 }));
        assert_eq!(pts.last().copied(), Some(Point { x: 200.0, y: 20.0 }));
        // Clear corridor → few points (stub + direct).
        assert!(pts.len() <= 4, "unexpected detour: {pts:?}");
    }

    #[test]
    fn blocker_forces_detour() {
        let sc = blocker_center();
        let p = PolylineEdgeRouter.route(&sc).unwrap();
        let pts = &p[0].path.polyline_points().unwrap();
        assert_eq!(pts.first().copied(), Some(Point { x: 80.0, y: 100.0 }));
        assert_eq!(pts.last().copied(), Some(Point { x: 300.0, y: 100.0 }));
        assert!(pts.len() > 2, "expected detour around blocker: {pts:?}");

        // No segment may pierce the spacing-inflated blocker (own nodes exempt).
        let inflated = padding_rect(sc.obstacles[2].rect, sc.params.spacing);
        for w in pts.windows(2) {
            assert!(
                !segment_intersects_rect(w[0], w[1], inflated),
                "segment {:?}→{:?} hits blocker",
                w[0],
                w[1]
            );
        }
    }

    #[test]
    fn sealed_pocket_is_hard_error() {
        let sc = scene(
            vec![
                obstacle("a", 0.0, 0.0, 40.0, 40.0),
                obstacle("b", 300.0, 0.0, 40.0, 40.0),
                obstacle("wall_n", 250.0, -100.0, 140.0, 90.0),
                obstacle("wall_s", 250.0, 50.0, 140.0, 100.0),
                obstacle("wall_e", 330.0, -100.0, 60.0, 250.0),
                obstacle("wall_w", 240.0, -100.0, 15.0, 250.0),
            ],
            anchor(40.0, 20.0, Side::East, "a"),
            anchor(300.0, 20.0, Side::West, "b"),
        );
        let err = PolylineEdgeRouter.route(&sc).unwrap_err();
        assert!(
            err.to_string().contains("no collision-free path"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn group_scene_is_unsupported() {
        let mut sc = two_boxes_clear();
        sc.group_boundaries.push(GroupBoundary {
            group_id: "g".to_string(),
            rect: Rect::new(100.0, 0.0, 40.0, 40.0),
        });
        match PolylineEdgeRouter.route(&sc) {
            Err(LayoutError::UnsupportedRouteScene { reason }) => {
                assert!(reason.contains("group"), "{reason}");
            }
            other => panic!("expected UnsupportedRouteScene, got {other:?}"),
        }
    }

    #[test]
    fn double_run_is_bit_identical() {
        let sc = blocker_center();
        let a = PolylineEdgeRouter.route(&sc).unwrap();
        let b = PolylineEdgeRouter.route(&sc).unwrap();
        assert_eq!(a.len(), b.len());
        for (pa, pb) in a.iter().zip(b.iter()) {
            assert_eq!(pa.id, pb.id);
            assert_eq!(
                pa.path.polyline_points().unwrap(),
                pb.path.polyline_points().unwrap()
            );
        }
    }
}
