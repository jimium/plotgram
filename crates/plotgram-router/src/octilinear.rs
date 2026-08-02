//! Independent octilinear [`EdgeRouter`](plotgram_engine_api::EdgeRouter).
//!
//! Segments are restricted to horizontal, vertical, or 45° diagonals.
//! Search graph: interesting-line vertices + line-bucket nearest neighbours
//! (not Steiner all-pairs — that is O(V²) and hung `viz.sh` on large fixtures).
//! See `docs/design/routing/octilinear/README.md`.
//!
//! Honesty (R5): group scenes are rejected in this MVP.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BinaryHeap};

use plotgram_engine_api::{EdgeRouter, LayoutError, Obstacle, RouteScene, TerminalPair};
use plotgram_model::geometry::{Point, Rect};
use plotgram_model::port::{PortRef, Side};
use plotgram_model::result::{EdgePath, EdgePlacement};

use crate::core::{padding_rect, segment_intersects_rect};

const LINE_CLEARANCE_EPS: f64 = 1e-3;
const EPS: f64 = 1e-9;
/// Above this, skip full `xs×ys` cartesian grid and use sparse vertices only.
const FULL_GRID_MAX: usize = 2_500;

/// Octilinear edge router registry name: `"octilinear"`.
#[derive(Debug, Default, Clone, Copy)]
pub struct OctilinearEdgeRouter;

impl EdgeRouter for OctilinearEdgeRouter {
    fn name(&self) -> &'static str {
        "octilinear"
    }

    fn route(&self, scene: &RouteScene) -> Result<Vec<EdgePlacement>, LayoutError> {
        if let Err(reason) = scene.validate() {
            return Err(LayoutError::UnsupportedRouteScene { reason });
        }
        if !scene.group_boundaries.is_empty() || !scene.boundary_permissions.is_empty() {
            return Err(LayoutError::UnsupportedRouteScene {
                reason: "octilinear router does not support group boundaries \
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

    let line_offset = spacing + LINE_CLEARANCE_EPS;
    let seeds = [pair.source.point, q_source, q_target, pair.target.point];
    let (xs, ys) = build_interesting_lines(&scene.obstacles, &seeds, line_offset);
    let mut verts = build_vertices(&scene.obstacles, &seeds, &xs, &ys, line_offset);

    verts.retain(|p| !blocked_vertex(*p, &inflated, exempt_src, exempt_tgt));
    sort_dedupe(&mut verts);

    let start = find_vertex(&verts, q_source).ok_or_else(|| {
        LayoutError::message(format!(
            "router: edge `{edge_id}` source stub is not a search vertex"
        ))
    })?;
    let goal = find_vertex(&verts, q_target).ok_or_else(|| {
        LayoutError::message(format!(
            "router: edge `{edge_id}` target stub is not a search vertex"
        ))
    })?;

    let adj = build_octilinear_adj(&verts, &inflated, exempt_src, exempt_tgt);

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
    let points = normalize_octilinear(&points);

    for w in points.windows(2) {
        if !is_octilinear(w[0], w[1]) {
            return Err(LayoutError::message(format!(
                "router: edge `{edge_id}` produced a non-octilinear segment"
            )));
        }
    }

    Ok(EdgePlacement {
        id: edge_id.to_string(),
        source: pair.source.node_id.clone(),
        target: pair.target.node_id.clone(),
        path: EdgePath::polyline(points),
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

fn build_interesting_lines(
    obstacles: &[Obstacle],
    seeds: &[Point; 4],
    line_offset: f64,
) -> (Vec<f64>, Vec<f64>) {
    let mut xs = Vec::with_capacity(obstacles.len() * 2 + 8);
    let mut ys = Vec::with_capacity(xs.capacity());
    for o in obstacles {
        let r = o.rect;
        xs.push(r.x - line_offset);
        xs.push(r.right() + line_offset);
        ys.push(r.y - line_offset);
        ys.push(r.bottom() + line_offset);
    }
    for p in seeds {
        xs.push(p.x);
        ys.push(p.y);
    }
    sort_dedupe_f64(&mut xs);
    sort_dedupe_f64(&mut ys);
    (xs, ys)
}

/// Sparse vertices always; full `xs×ys` grid only when small enough.
fn build_vertices(
    obstacles: &[Obstacle],
    seeds: &[Point; 4],
    xs: &[f64],
    ys: &[f64],
    line_offset: f64,
) -> Vec<Point> {
    let mut verts = Vec::with_capacity(256);
    for &p in seeds {
        verts.push(p);
    }
    for o in obstacles {
        let r = padding_rect(o.rect, line_offset);
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

    // Stub rays hitting every interesting line (H / V / 45°) — O(|stubs|·(|xs|+|ys|)).
    for &p in &seeds[1..=2] {
        for &x in xs {
            verts.push(Point { x, y: p.y });
            verts.push(Point {
                x,
                y: p.y + (x - p.x),
            });
            verts.push(Point {
                x,
                y: p.y - (x - p.x),
            });
        }
        for &y in ys {
            verts.push(Point { x: p.x, y });
            verts.push(Point {
                x: p.x + (y - p.y),
                y,
            });
            verts.push(Point {
                x: p.x - (y - p.y),
                y,
            });
        }
    }

    if xs.len().saturating_mul(ys.len()) <= FULL_GRID_MAX {
        for &x in xs {
            for &y in ys {
                verts.push(Point { x, y });
            }
        }
    }

    verts
}

fn blocked_vertex(
    p: Point,
    inflated: &[(String, Rect)],
    exempt_src: &str,
    exempt_tgt: &str,
) -> bool {
    inflated.iter().any(|(id, r)| {
        id.as_str() != exempt_src && id.as_str() != exempt_tgt && point_strictly_inside(p, *r)
    })
}

fn build_octilinear_adj(
    verts: &[Point],
    inflated: &[(String, Rect)],
    exempt_src: &str,
    exempt_tgt: &str,
) -> Vec<Vec<(usize, f64)>> {
    let n = verts.len();
    let mut adj: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];

    let mut horiz: BTreeMap<u64, Vec<(f64, usize)>> = BTreeMap::new();
    let mut vert: BTreeMap<u64, Vec<(f64, usize)>> = BTreeMap::new();
    let mut diag_a: BTreeMap<u64, Vec<(f64, usize)>> = BTreeMap::new();
    let mut diag_b: BTreeMap<u64, Vec<(f64, usize)>> = BTreeMap::new();

    for (i, &p) in verts.iter().enumerate() {
        horiz.entry(p.y.to_bits()).or_default().push((p.x, i));
        vert.entry(p.x.to_bits()).or_default().push((p.y, i));
        diag_a
            .entry((p.x - p.y).to_bits())
            .or_default()
            .push((p.x, i));
        diag_b
            .entry((p.x + p.y).to_bits())
            .or_default()
            .push((p.x, i));
    }

    let mut link = |buckets: &mut BTreeMap<u64, Vec<(f64, usize)>>| {
        for list in buckets.values_mut() {
            list.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            for w in list.windows(2) {
                let (i, j) = (w[0].1, w[1].1);
                if segment_clear(verts[i], verts[j], inflated, exempt_src, exempt_tgt) {
                    let cost = euclid(verts[i], verts[j]);
                    adj[i].push((j, cost));
                    adj[j].push((i, cost));
                }
            }
        }
    };
    link(&mut horiz);
    link(&mut vert);
    link(&mut diag_a);
    link(&mut diag_b);

    for a in &mut adj {
        a.sort_by(|x, y| x.0.cmp(&y.0));
        a.dedup_by(|x, y| x.0 == y.0);
    }
    adj
}

fn is_octilinear(a: Point, b: Point) -> bool {
    let dx = (b.x - a.x).abs();
    let dy = (b.y - a.y).abs();
    dx < EPS || dy < EPS || (dx - dy).abs() < EPS
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

fn near(a: Point, b: Point) -> bool {
    (a.x - b.x).abs() < EPS && (a.y - b.y).abs() < EPS
}

fn point_strictly_inside(p: Point, r: Rect) -> bool {
    p.x > r.x + EPS
        && p.x < r.right() - EPS
        && p.y > r.y + EPS
        && p.y < r.bottom() - EPS
}

fn find_vertex(verts: &[Point], p: Point) -> Option<usize> {
    verts.iter().position(|v| near(*v, p))
}

fn sort_dedupe(pts: &mut Vec<Point>) {
    pts.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    pts.dedup_by(|a, b| (a.x - b.x).abs() < EPS && (a.y - b.y).abs() < EPS);
}

fn sort_dedupe_f64(v: &mut Vec<f64>) {
    v.sort_by(|a, b| a.total_cmp(b));
    v.dedup_by(|a, b| (*a - *b).abs() < EPS);
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
        // First pop settles the node — critical on grids with many equal-cost
        // paths (re-pushing equal relaxations OOMs / hangs).
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
            if nd < dist[next] - EPS {
                dist[next] = nd;
                prev[next] = Some(idx);
                heap.push(HeapNode {
                    cost: nd,
                    idx: next,
                });
            } else if (nd - dist[next]).abs() <= EPS {
                // Deterministic tie-break on predecessor only; no re-push.
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
        match other.cost.total_cmp(&self.cost) {
            Ordering::Equal => other.idx.cmp(&self.idx),
            ord => ord,
        }
    }
}

fn normalize_octilinear(points: &[Point]) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::with_capacity(points.len());
    for &p in points {
        if out.last().is_some_and(|l| near(*l, p)) {
            continue;
        }
        out.push(p);
    }
    let mut i = 0;
    while i + 2 < out.len() {
        let a = out[i];
        let b = out[i + 1];
        let c = out[i + 2];
        if is_octilinear(a, c) {
            let abx = b.x - a.x;
            let aby = b.y - a.y;
            let bcx = c.x - b.x;
            let bcy = c.y - b.y;
            let cross = abx * bcy - aby * bcx;
            let dot = abx * bcx + aby * bcy;
            if cross.abs() < EPS && dot >= -EPS {
                out.remove(i + 1);
                continue;
            }
        }
        i += 1;
    }
    if out.len() < 2 {
        if let Some(p) = points.first().copied() {
            return vec![p, p];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use plotgram_engine_api::{GroupBoundary, OrthogonalRouteParams, PortAnchor};
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

    fn diagonal_boxes() -> RouteScene {
        scene(
            vec![
                obstacle("a", 0.0, 0.0, 80.0, 40.0),
                obstacle("b", 200.0, 120.0, 80.0, 40.0),
            ],
            anchor(80.0, 20.0, Side::East, "a"),
            anchor(200.0, 140.0, Side::West, "b"),
        )
    }

    #[test]
    fn clear_horizontal_is_octilinear() {
        let p = OctilinearEdgeRouter.route(&two_boxes_clear()).unwrap();
        let pts = &p[0].path.polyline_points().unwrap();
        assert_eq!(pts.first().copied(), Some(Point { x: 80.0, y: 20.0 }));
        assert_eq!(pts.last().copied(), Some(Point { x: 200.0, y: 20.0 }));
        for w in pts.windows(2) {
            assert!(is_octilinear(w[0], w[1]), "{:?}→{:?}", w[0], w[1]);
        }
    }

    #[test]
    fn diagonal_scene_uses_only_octilinear_segments() {
        let p = OctilinearEdgeRouter.route(&diagonal_boxes()).unwrap();
        let pts = &p[0].path.polyline_points().unwrap();
        assert!(pts.len() >= 2);
        for w in pts.windows(2) {
            assert!(is_octilinear(w[0], w[1]), "{:?}→{:?}", w[0], w[1]);
        }
    }

    #[test]
    fn blocker_forces_octilinear_detour() {
        let sc = blocker_center();
        let p = OctilinearEdgeRouter.route(&sc).unwrap();
        let pts = &p[0].path.polyline_points().unwrap();
        assert!(pts.len() > 2, "expected detour: {pts:?}");
        let inflated = padding_rect(sc.obstacles[2].rect, sc.params.spacing);
        for w in pts.windows(2) {
            assert!(is_octilinear(w[0], w[1]));
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
        let err = OctilinearEdgeRouter.route(&sc).unwrap_err();
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
        match OctilinearEdgeRouter.route(&sc) {
            Err(LayoutError::UnsupportedRouteScene { reason }) => {
                assert!(reason.contains("group"), "{reason}");
            }
            other => panic!("expected UnsupportedRouteScene, got {other:?}"),
        }
    }

    #[test]
    fn double_run_is_bit_identical() {
        let sc = blocker_center();
        let a = OctilinearEdgeRouter.route(&sc).unwrap();
        let b = OctilinearEdgeRouter.route(&sc).unwrap();
        for (pa, pb) in a.iter().zip(b.iter()) {
            assert_eq!(pa.path.polyline_points().unwrap(), pb.path.polyline_points().unwrap());
        }
    }

    #[test]
    fn large_obstacle_count_finishes_quickly() {
        let mut obstacles = Vec::new();
        for r in 0..6 {
            for c in 0..6 {
                obstacles.push(obstacle(
                    &format!("n{r}_{c}"),
                    c as f64 * 100.0,
                    r as f64 * 80.0,
                    60.0,
                    40.0,
                ));
            }
        }
        let sc = scene(
            obstacles,
            anchor(60.0, 20.0, Side::East, "n0_0"),
            anchor(500.0, 420.0, Side::West, "n5_5"),
        );
        let t0 = std::time::Instant::now();
        let _ = OctilinearEdgeRouter.route(&sc);
        assert!(
            t0.elapsed().as_secs_f64() < 2.0,
            "took {:?}",
            t0.elapsed()
        );
    }
}
