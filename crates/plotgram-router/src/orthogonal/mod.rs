//! Independent orthogonal [`EdgeRouter`](plotgram_engine_api::EdgeRouter).
//!
//! Status: **M0 + M1 已落地** — reduced-interesting-line OVG + A* search
//! with obstacle avoidance; M1 adds a second routing round with a
//! shared-segment penalty, corridor track separation (uniform offsets), a
//! node-budget gate, and a `min_segment` short-segment post-pass (see
//! `docs/design/routing/orthogonal/architecture.md` §10). L3 track ordering
//! beyond uniform offsets and L4 VPSC nudging / groups are M2+ and
//! intentionally absent.
//!
//! Write authority (R1/R2): the router only writes `EdgePlacement.path`;
//! terminals, obstacles and ports are read-only. Honesty (§3.4): an edge
//! with no collision-free path (or one exceeding the node budget) is a hard
//! error — never a degenerate obstacle-crossing elbow.

mod ovg;
mod search;
mod track;

use plotgram_engine_api::{
    EdgeRouter, LayoutError, OrthogonalRouteParams, RouteScene, TerminalPair,
};
use plotgram_model::geometry::Point;
use plotgram_model::port::PortRef;
use plotgram_model::result::{EdgePath, EdgePlacement};

use crate::core::normalize_polyline;

use ovg::{outward, segment_blocked, stub_point, Grid};
use search::{astar, SearchOutcome};
use track::spread_tracks;

/// Extra line offset beyond `spacing`: interesting lines sit at
/// `spacing + LINE_CLEARANCE_EPS` from obstacle edges, so routed segments
/// keep strictly positive clearance from `spacing`-inflated obstacles
/// (`verify` counts boundary touch as intersection).
const LINE_CLEARANCE_EPS: f64 = 1e-3;

/// Orthogonal edge router registry name: `"orthogonal"`.
#[derive(Debug, Default, Clone, Copy)]
pub struct OrthogonalEdgeRouter;

impl EdgeRouter for OrthogonalEdgeRouter {
    fn name(&self) -> &'static str {
        "orthogonal"
    }

    fn route(&self, scene: &RouteScene) -> Result<Vec<EdgePlacement>, LayoutError> {
        // R5 honesty: reject unsupported scene features early.
        if let Err(reason) = scene.validate() {
            return Err(LayoutError::UnsupportedRouteScene { reason });
        }
        check_params(&scene.params)?;

        // One shared grid for the whole scene: obstacle-edge lines plus every
        // edge's anchor / stub coordinates. Fixed for the entire call →
        // deterministic across edges (R3). Own-node exemption is per edge and
        // applied lazily in the collision predicate.
        let line_offset = scene.params.spacing + LINE_CLEARANCE_EPS;
        let stub_len = scene.params.port_stub.max(scene.params.min_segment);
        let mut extra = Vec::with_capacity(scene.terminals.len() * 4);
        for pair in scene.terminals.values() {
            extra.push(pair.source.point);
            extra.push(stub_point(pair.source.point, pair.source.side, stub_len));
            extra.push(pair.target.point);
            extra.push(stub_point(pair.target.point, pair.target.side, stub_len));
        }
        let grid = Grid::build(&scene.obstacles, &extra, line_offset);

        // Round 1 (M1 §5.2): no shared-segment penalty. Any failure is a hard
        // error — round 1 is the honest baseline.
        let round1 = route_edges(scene, &grid, None, None)?;

        // Round 2 (only when requested): penalize overlap with round-1
        // segments of *other* edges, so parallel edges prefer different
        // corridors. A round-2 failure falls back to that edge's round-1
        // result (round 1 is guaranteed collision-free).
        let result = if scene.params.route_rounds < 2 || scene.params.shared_penalty == 0.0 {
            round1
        } else {
            let round1_segments = Round1Segments::collect(&round1);
            route_edges(scene, &grid, Some(&round1_segments), Some(&round1))?
        };

        // L3-M1: spread edges that still share a corridor onto distinct tracks.
        let mut result = result;
        spread_tracks(scene, &mut result);
        Ok(result)
    }
}

/// Route every edge in `edge_order` (stable order, R3).
///
/// - `round1`: `None` for the first round; for the second round, the round-1
///   segment index whose overlap adds `shared_penalty` per unit length.
/// - `fallback`: per-edge round-1 result to return when a second-round search
///   fails (no path / budget exceeded). `None` in round 1 → propagate errors.
fn route_edges(
    scene: &RouteScene,
    grid: &Grid,
    round1: Option<&Round1Segments>,
    fallback: Option<&[EdgePlacement]>,
) -> Result<Vec<EdgePlacement>, LayoutError> {
    let budget = (scene.params.max_search_nodes != 0).then_some(scene.params.max_search_nodes);
    let mut out = Vec::with_capacity(scene.edge_order.len());
    for (i, edge_id) in scene.edge_order.iter().enumerate() {
        let pair = scene.terminals.get(edge_id).ok_or_else(|| {
            LayoutError::message(format!(
                "router: edge `{edge_id}` in edge_order but missing from terminals"
            ))
        })?;
        match route_edge(scene, grid, edge_id, pair, round1, budget) {
            Ok(p) => out.push(p),
            Err(e) => match fallback {
                Some(prev) => {
                    out.push(prev[i].clone());
                    let _ = e; // round-1 path is legal; second round is best-effort
                }
                None => return Err(e),
            },
        }
    }
    Ok(out)
}

/// Route one edge: stub out of both terminals, A* between the stub ends on
/// the shared grid, assemble, normalize, then enforce `min_segment`.
fn route_edge(
    scene: &RouteScene,
    grid: &Grid,
    edge_id: &str,
    pair: &TerminalPair,
    round1: Option<&Round1Segments>,
    budget: Option<u32>,
) -> Result<EdgePlacement, LayoutError> {
    let params = &scene.params;
    let exempt = [pair.source.node_id.as_str(), pair.target.node_id.as_str()];

    // Step cost: collision gate → `None`; otherwise length + (round 2)
    // shared-segment penalty. Bend cost is added by the search itself.
    let step_cost = |a: Point, b: Point| -> Option<f64> {
        if segment_blocked(a, b, &scene.obstacles, &exempt, params.spacing) {
            return None;
        }
        let len = (b.x - a.x).abs() + (b.y - a.y).abs();
        let shared = round1.map_or(0.0, |prev| prev.shared_overlap(edge_id, a, b));
        Some(len + params.shared_penalty * shared)
    };
    let blocked =
        |a: Point, b: Point| segment_blocked(a, b, &scene.obstacles, &exempt, params.spacing);

    // Port stub: at least `min_segment` long so the departure segment can
    // carry a corner radius (M1 rounded-corner budget).
    let stub_len = params.port_stub.max(params.min_segment);
    let q_source = stub_point(pair.source.point, pair.source.side, stub_len);
    let q_target = stub_point(pair.target.point, pair.target.side, stub_len);

    // Forced stub segments (terminal expansion, not a router freedom): if one
    // is already blocked, no legal path exists — fail honestly (§3.4).
    if blocked(pair.source.point, q_source) {
        return Err(LayoutError::message(format!(
            "router: edge `{edge_id}` source port stub is blocked by an obstacle"
        )));
    }
    if blocked(pair.target.point, q_target) {
        return Err(LayoutError::message(format!(
            "router: edge `{edge_id}` target port stub is blocked by an obstacle"
        )));
    }

    let start = grid.find(q_source).ok_or_else(|| {
        LayoutError::message(format!(
            "router: edge `{edge_id}` source stub is off the search grid"
        ))
    })?;
    let goal = grid.find(q_target).ok_or_else(|| {
        LayoutError::message(format!(
            "router: edge `{edge_id}` target stub is off the search grid"
        ))
    })?;

    let mut points = vec![pair.source.point];
    if start == goal {
        // Both stubs land on the same grid vertex.
        points.push(q_source);
    } else {
        let outcome = astar(
            grid,
            start,
            outward(pair.source.side),
            goal,
            outward(pair.target.side).opposite(),
            params.bend_penalty,
            &step_cost,
            budget,
        );
        match outcome {
            SearchOutcome::Found(searched) => points.extend(searched),
            SearchOutcome::BudgetExceeded => {
                return Err(LayoutError::message(format!(
                    "router: edge `{edge_id}` search budget exceeded (max_search_nodes={})",
                    params.max_search_nodes
                )));
            }
            SearchOutcome::NoPath => {
                return Err(LayoutError::message(format!(
                    "router: edge `{edge_id}` has no collision-free path"
                )));
            }
        }
    }
    points.push(pair.target.point);

    let mut points = normalize_polyline(&points);
    if params.min_segment > 0.0 {
        points = remove_short_humps(&points, params.min_segment, &blocked);
        points = normalize_polyline(&points);
    }

    Ok(EdgePlacement {
        id: edge_id.to_string(),
        source: pair.source.node_id.clone(),
        target: pair.target.node_id.clone(),
        path: EdgePath { points },
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

// ─── M1: shared-segment penalty (round 2) ───────────────────

/// Round-1 routed segments, grouped per edge, for round-2 shared-penalty
/// lookups. Built from stable `edge_order` order — deterministic (R3).
struct Round1Segments {
    /// (edge_id, normalized segments) in `edge_order` order.
    per_edge: Vec<(String, Vec<(Point, Point)>)>,
}

impl Round1Segments {
    fn collect(placements: &[EdgePlacement]) -> Self {
        let per_edge = placements
            .iter()
            .map(|p| {
                let segs: Vec<(Point, Point)> =
                    p.path.points.windows(2).map(|w| (w[0], w[1])).collect();
                (p.id.clone(), segs)
            })
            .collect();
        Self { per_edge }
    }

    /// Total overlap of segment `a→b` with round-1 segments of *other* edges
    /// (never the edge's own round-1 path — that would make it dodge itself).
    fn shared_overlap(&self, edge_id: &str, a: Point, b: Point) -> f64 {
        let mut total = 0.0;
        for (eid, segs) in &self.per_edge {
            if eid == edge_id {
                continue;
            }
            for &(c, d) in segs {
                total += crate::core::overlap_len(a, b, c, d);
            }
        }
        total
    }
}

// ─── M1: min_segment post-pass ──────────────────────────────

/// Remove "hump" segments shorter than `min_segment` (M1 rounded-corner
/// budget): a short segment whose two flanking perpendicular segments end on
/// the same axis line can be deleted — the neighbours then connect directly
/// and stay orthogonal. Deletion is accepted only when the new segment is
/// collision-free. Segments that cannot be removed orthogonally keep as-is;
/// geometry is never faked.
fn remove_short_humps(
    points: &[Point],
    min_segment: f64,
    blocked: &dyn Fn(Point, Point) -> bool,
) -> Vec<Point> {
    if points.len() < 4 {
        return points.to_vec();
    }
    let eps = 1e-9;
    let mut pts = points.to_vec();
    let mut changed = true;
    while changed {
        changed = false;
        let mut i = 1;
        while i + 2 < pts.len() {
            let (p, a, b, n) = (pts[i - 1], pts[i], pts[i + 1], pts[i + 2]);
            let removed = if (p.x - a.x).abs() < eps
                && (b.x - n.x).abs() < eps
                && (a.y - b.y).abs() < eps
                && (b.x - a.x).abs() < min_segment
                && (p.y - n.y).abs() < eps
            {
                // horizontal hump: p→a and b→n vertical, a→b short horizontal
                !blocked(p, n)
            } else if (p.y - a.y).abs() < eps
                && (b.y - n.y).abs() < eps
                && (a.x - b.x).abs() < eps
                && (b.y - a.y).abs() < min_segment
                && (p.x - n.x).abs() < eps
            {
                // vertical hump: p→a and b→n horizontal, a→b short vertical
                !blocked(p, n)
            } else {
                false
            };
            if removed {
                pts.remove(i + 1); // b
                pts.remove(i); // a
                changed = true;
                // Re-examine the same window position (neighbours shifted in).
            } else {
                i += 1;
            }
        }
    }
    pts
}

// ─── Parameters ─────────────────────────────────────────────

/// Parameter discipline (§6): M0 + M1 implemented params are range-checked;
/// unknown future params must be rejected, never silently ignored.
fn check_params(p: &OrthogonalRouteParams) -> Result<(), LayoutError> {
    for (name, v) in [
        ("spacing", p.spacing),
        ("port_stub", p.port_stub),
        ("bend_penalty", p.bend_penalty),
        ("shared_penalty", p.shared_penalty),
        ("min_segment", p.min_segment),
    ] {
        if !v.is_finite() || v < 0.0 {
            return Err(LayoutError::message(format!(
                "router: param `{name}` must be finite and >= 0, got {v}"
            )));
        }
    }
    if p.route_rounds != 1 && p.route_rounds != 2 {
        return Err(LayoutError::message(format!(
            "router: param `route_rounds` must be 1 or 2, got {}",
            p.route_rounds
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use plotgram_engine_api::{GroupBoundary, Obstacle, PortAnchor};
    use plotgram_model::geometry::Rect;
    use plotgram_model::port::Side;

    use crate::verify::verify_all;
    use std::collections::BTreeMap;

    // ─── Scene builders ─────────────────────────────────────

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

    /// Scene: obstacles + one edge `e0` (a → b) with default params.
    fn scene(obstacles: Vec<Obstacle>, source: PortAnchor, target: PortAnchor) -> RouteScene {
        let mut terminals = BTreeMap::new();
        terminals.insert("e0".to_string(), TerminalPair { source, target });
        RouteScene {
            obstacles,
            terminals,
            edge_order: vec!["e0".to_string()],
            group_boundaries: vec![],
            boundary_permissions: BTreeMap::new(),
            params: OrthogonalRouteParams::default(),
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

    /// Two edges share one anchor pair: without separation they fully overlap
    /// on the same corridor; M1 must put them on distinct tracks.
    fn shared_corridor() -> RouteScene {
        let mut terminals = BTreeMap::new();
        for id in ["e0", "e1"] {
            terminals.insert(
                id.to_string(),
                TerminalPair {
                    source: anchor(80.0, 70.0, Side::East, "a"),
                    target: anchor(200.0, 70.0, Side::West, "b"),
                },
            );
        }
        RouteScene {
            obstacles: vec![
                obstacle("a", 0.0, 40.0, 80.0, 60.0),
                obstacle("b", 200.0, 40.0, 80.0, 60.0),
            ],
            terminals,
            edge_order: vec!["e0".to_string(), "e1".to_string()],
            group_boundaries: vec![],
            boundary_permissions: BTreeMap::new(),
            params: OrthogonalRouteParams::default(),
        }
    }

    // ─── M0 tests ───────────────────────────────────────────

    #[test]
    fn clear_scene_routes_straight() {
        let placements = OrthogonalEdgeRouter.route(&two_boxes_clear()).unwrap();
        assert_eq!(placements.len(), 1);
        let pts = &placements[0].path.points;
        // Stub + return collinear → fully merged straight segment.
        assert_eq!(
            pts,
            &vec![Point { x: 80.0, y: 20.0 }, Point { x: 200.0, y: 20.0 }]
        );
        assert!(verify_all(&two_boxes_clear(), &placements).all_pass);
    }

    #[test]
    fn blocker_forces_detour() {
        let sc = blocker_center();
        let placements = OrthogonalEdgeRouter.route(&sc).unwrap();
        let report = verify_all(&sc, &placements);
        assert!(report.all_pass, "failures: {:?}", report.failures());

        // Minimum detour around facing stubs = 4 bends → 6 points after
        // collinear merging (turn positions are tie-broken deterministically).
        let pts = &placements[0].path.points;
        assert_eq!(pts.len(), 6, "expected a 4-bend detour, got {pts:?}");
        assert_eq!(pts.first(), Some(&Point { x: 80.0, y: 100.0 }));
        assert_eq!(pts.last(), Some(&Point { x: 300.0, y: 100.0 }));
        // Detour leaves the blocker's inflated y-span [40, 160].
        let detour_y = pts[2].y;
        assert!(detour_y < 40.0 || detour_y > 160.0, "detour y = {detour_y}");
    }

    #[test]
    fn blocked_stub_is_a_hard_error() {
        // A plug obstacle sits right outside the source port, covering the
        // stub segment after inflation.
        let sc = scene(
            vec![
                obstacle("a", 0.0, 0.0, 80.0, 40.0),
                obstacle("b", 200.0, 0.0, 80.0, 40.0),
                obstacle("plug", 85.0, 10.0, 10.0, 20.0),
            ],
            anchor(80.0, 20.0, Side::East, "a"),
            anchor(200.0, 20.0, Side::West, "b"),
        );
        let err = OrthogonalEdgeRouter.route(&sc).unwrap_err();
        assert!(err.to_string().contains("stub"), "unexpected: {err}");
    }

    #[test]
    fn unreachable_edge_is_a_hard_error() {
        // The target stub ends inside a sealed pocket: four walls whose
        // inflated rects overlap at the corners and close every escape step.
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
        let err = OrthogonalEdgeRouter.route(&sc).unwrap_err();
        assert!(
            err.to_string().contains("no collision-free path"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn invalid_params_are_rejected() {
        let cases: &[(&str, fn(&mut OrthogonalRouteParams))] = &[
            ("spacing", |p| p.spacing = -1.0),
            ("port_stub", |p| p.port_stub = -1.0),
            ("bend_penalty", |p| p.bend_penalty = f64::NAN),
            ("shared_penalty", |p| p.shared_penalty = -1.0),
            ("min_segment", |p| p.min_segment = -1.0),
            ("route_rounds", |p| p.route_rounds = 3),
        ];
        for (name, mutate) in cases {
            let mut sc = two_boxes_clear();
            mutate(&mut sc.params);
            let err = OrthogonalEdgeRouter.route(&sc).unwrap_err();
            assert!(err.to_string().contains(name), "param `{name}`: {err}");
        }
    }

    #[test]
    fn group_scene_is_honestly_unsupported() {
        let mut sc = two_boxes_clear();
        sc.group_boundaries.push(GroupBoundary {
            group_id: "g".to_string(),
            rect: Rect::new(-10.0, -10.0, 300.0, 60.0),
        });
        match OrthogonalEdgeRouter.route(&sc) {
            Err(LayoutError::UnsupportedRouteScene { .. }) => {}
            other => panic!("groups must be unsupported at M0/M1, got {other:?}"),
        }
    }

    #[test]
    fn double_run_is_bit_identical() {
        let sc = blocker_center();
        let a = OrthogonalEdgeRouter.route(&sc).unwrap();
        let b = OrthogonalEdgeRouter.route(&sc).unwrap();
        assert_eq!(a.len(), b.len());
        for (pa, pb) in a.iter().zip(b.iter()) {
            assert_eq!(pa.id, pb.id);
            assert_eq!(pa.path.points, pb.path.points);
        }
    }

    // ─── M1 tests ───────────────────────────────────────────

    #[test]
    fn m1_params_are_now_supported() {
        // Non-default M1 values must route (they were rejected before M1).
        let mut sc = two_boxes_clear();
        sc.params.shared_penalty = 50.0;
        sc.params.route_rounds = 2;
        sc.params.max_search_nodes = 100_000;
        sc.params.min_segment = 12.0;
        let placements = OrthogonalEdgeRouter.route(&sc).unwrap();
        assert!(verify_all(&sc, &placements).all_pass);
    }

    #[test]
    fn shared_corridor_edges_are_separated() {
        // Default params (single round): corridor track separation must put
        // the two fully-overlapping edges onto distinct tracks.
        let sc = shared_corridor();
        let p = OrthogonalEdgeRouter.route(&sc).unwrap();
        assert_ne!(
            p[0].path.points, p[1].path.points,
            "tracks must separate a shared corridor"
        );
        let report = verify_all(&sc, &p);
        assert!(report.all_pass, "failures: {:?}", report.failures());
        // First edge (in order) keeps the straight line; e1 rides track 1.
        assert_eq!(p[0].path.points.len(), 2, "e0 stays straight");
        assert!(
            p[1].path.points.len() > 2,
            "e1 rides a track: {:?}",
            p[1].path.points
        );
        // Track offset = spacing (20): e1's corridor run leaves backbone y=70.
        assert!(
            (p[1].path.points[1].y - 70.0).abs() > 19.9,
            "e1 must leave the backbone y=70: {:?}",
            p[1].path.points
        );
    }

    #[test]
    fn two_rounds_still_separate_and_clear() {
        let mut sc = shared_corridor();
        sc.params.route_rounds = 2;
        sc.params.shared_penalty = 60.0;
        let p = OrthogonalEdgeRouter.route(&sc).unwrap();
        assert_ne!(p[0].path.points, p[1].path.points);
        let report = verify_all(&sc, &p);
        assert!(report.all_pass, "failures: {:?}", report.failures());
    }

    #[test]
    fn budget_gate_is_observable() {
        let sc = blocker_center();
        let mut gated = sc.clone();
        gated.params.max_search_nodes = 1;
        let err = OrthogonalEdgeRouter.route(&gated).unwrap_err();
        assert!(err.to_string().contains("budget"), "unexpected: {err}");
        // Generous budget routes fine.
        let mut ok = sc.clone();
        ok.params.max_search_nodes = 1_000_000;
        assert!(OrthogonalEdgeRouter.route(&ok).is_ok());
    }

    #[test]
    fn min_segment_lengthens_port_stub() {
        // min_segment > port_stub: the stub must grow to min_segment so the
        // departure segment can carry a corner radius.
        let mut sc = blocker_center();
        sc.params.min_segment = 25.0;
        let p = OrthogonalEdgeRouter.route(&sc).unwrap();
        let report = verify_all(&sc, &p);
        assert!(report.all_pass, "failures: {:?}", report.failures());
        // min_segment is a segment-length floor: every segment must be >= 25.
        // (A collinear stub may merge into its successor, so no point-by-point
        // stub assertion — segment lengths are the observable contract.)
        let pts = &p[0].path.points;
        assert_eq!(pts.first(), Some(&Point { x: 80.0, y: 100.0 }));
        assert_eq!(pts.last(), Some(&Point { x: 300.0, y: 100.0 }));
        for w in pts.windows(2) {
            let len = (w[1].x - w[0].x).abs() + (w[1].y - w[0].y).abs();
            assert!(
                len >= 25.0 - 1e-9,
                "segment {w:?} length {len} < min_segment"
            );
        }
    }

    #[test]
    fn min_segment_removes_short_humps() {
        let p = |x: f64, y: f64| Point { x, y };
        // Hump: (0,10)→(1,10) is 1 < 4; p and n share y=0 → deletable.
        let hump = vec![
            p(0.0, 0.0),
            p(0.0, 10.0),
            p(1.0, 10.0),
            p(1.0, 0.0),
            p(2.0, 0.0),
        ];
        let out = remove_short_humps(&hump, 4.0, &|_, _| false);
        assert_eq!(out, vec![p(0.0, 0.0), p(1.0, 0.0), p(2.0, 0.0)]);
        // Vertical hump symmetric case.
        let vhump = vec![
            p(0.0, 0.0),
            p(10.0, 0.0),
            p(10.0, 1.0),
            p(0.0, 1.0),
            p(0.0, 2.0),
        ];
        let out = remove_short_humps(&vhump, 4.0, &|_, _| false);
        assert_eq!(out, vec![p(0.0, 0.0), p(0.0, 1.0), p(0.0, 2.0)]);
        // Blocked deletion → kept.
        let blocked = hump.clone();
        let out = remove_short_humps(&blocked, 4.0, &|a, b| a == p(0.0, 0.0) && b == p(1.0, 0.0));
        assert_eq!(out, hump);
        // Genuine Z shape (p and n not aligned) is never deleted.
        let z = vec![
            p(0.0, 0.0),
            p(0.0, 10.0),
            p(1.0, 10.0),
            p(1.0, 5.0),
            p(12.0, 5.0),
        ];
        let out = remove_short_humps(&z, 4.0, &|_, _| false);
        assert_eq!(out, z);
    }
}
