//! Independent orthogonal [`EdgeRouter`](plotgram_engine_api::EdgeRouter).
//!
//! Status: **M0 + M1 + M2-group + L4 VPSC nudge 已落地** — reduced-
//! interesting-line OVG + A* with obstacle avoidance; M1 shared-segment /
//! track separation; M2-group single-layer gates; L3 interval coloring +
//! L4 VPSC nudging (see `nudge.rs`, `plotgram_algo::{interval_color,vpsc}`).
//! Nested group scope is intentionally absent.
//!
//! Write authority (R1/R2): the router only writes `EdgePlacement.path`;
//! terminals, obstacles and ports are read-only. Honesty (§3.4): an edge
//! with no collision-free path (or one exceeding the node budget) is a hard
//! error — never a degenerate obstacle-crossing elbow.

pub(crate) mod ovg;
mod nudge;
mod search;
mod track;

use std::collections::BTreeMap;

use plotgram_engine_api::{
    BoundaryCrossing, EdgeRouter, LayoutError, OrthogonalRouteParams, RouteScene,
    TerminalPair,
};
use plotgram_model::geometry::Point;
use plotgram_model::port::PortRef;
use plotgram_model::result::{EdgePath, EdgePlacement};

use crate::core::normalize_polyline;

use ovg::{all_gate_rects, outward, stub_point, Grid, ObstacleIndex};
use search::{astar, SearchOutcome, SearchState};
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
        let gates = all_gate_rects(scene);
        let grid = Grid::build(
            &scene.obstacles,
            &scene.group_boundaries,
            &gates,
            &extra,
            line_offset,
        );

        // Build spatial index once for the whole scene (perf: O(1) build,
        // O(k) query vs O(N) linear scan per step).
        let obs_index = ObstacleIndex::build(&scene.obstacles, scene.params.spacing);
        // node_id → obstacle index for exempt lookups.
        let obs_id_map: BTreeMap<&str, usize> = scene
            .obstacles
            .iter()
            .enumerate()
            .map(|(i, o)| (o.id.as_str(), i))
            .collect();

        // Round 1 (M1 §5.2): no shared-segment penalty. Any failure is a hard
        // error — round 1 is the honest baseline.
        let round1 = route_edges(scene, &grid, &obs_index, &obs_id_map, None, None)?;

        // Round 2 (only when requested): penalize overlap with round-1
        // segments of *other* edges, so parallel edges prefer different
        // corridors. A round-2 failure falls back to that edge's round-1
        // result (round 1 is guaranteed collision-free).
        let result = if scene.params.route_rounds < 2 || scene.params.shared_penalty == 0.0 {
            round1
        } else {
            let round1_segments = Round1Segments::collect(&round1);
            route_edges(
                scene,
                &grid,
                &obs_index,
                &obs_id_map,
                Some(&round1_segments),
                Some(&round1),
            )?
        };

        // L3-M1 / L4: interval-colour tracks, then VPSC-nudge offsets.
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
    obs_index: &ObstacleIndex,
    obs_id_map: &BTreeMap<&str, usize>,
    round1: Option<&Round1Segments>,
    fallback: Option<&[EdgePlacement]>,
) -> Result<Vec<EdgePlacement>, LayoutError> {
    let budget = (scene.params.max_search_nodes != 0).then_some(scene.params.max_search_nodes);
    // Allocate A* arrays once for the whole batch (generation-based reset).
    let (nx, ny) = grid.dims();
    let mut search_state = SearchState::new(nx * ny * 4);
    let mut out = Vec::with_capacity(scene.edge_order.len());
    for (i, edge_id) in scene.edge_order.iter().enumerate() {
        let pair = scene.terminals.get(edge_id).ok_or_else(|| {
            LayoutError::message(format!(
                "router: edge `{edge_id}` in edge_order but missing from terminals"
            ))
        })?;
        match route_edge(scene, grid, obs_index, obs_id_map, edge_id, pair, round1, budget, &mut search_state) {
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
    obs_index: &ObstacleIndex,
    obs_id_map: &BTreeMap<&str, usize>,
    edge_id: &str,
    pair: &TerminalPair,
    round1: Option<&Round1Segments>,
    budget: Option<u32>,
    search_state: &mut SearchState,
) -> Result<EdgePlacement, LayoutError> {
    let params = &scene.params;

    // Pre-extract exempt obstacle indices (avoids BTreeMap lookup per step).
    let exempt_i = obs_id_map.get(pair.source.node_id.as_str()).copied().unwrap_or(usize::MAX);
    let exempt_j = obs_id_map.get(pair.target.node_id.as_str()).copied().unwrap_or(usize::MAX);

    // Pre-extract group crossings for this edge (avoids BTreeMap lookup per step).
    let crossings: &[BoundaryCrossing] = scene
        .boundary_permissions
        .get(edge_id)
        .map(|v| v.as_slice())
        .unwrap_or(&[]);
    let has_groups = !scene.group_boundaries.is_empty();

    // Fast collision predicate: spatial index for obstacles + linear for
    // groups (groups are typically 0–few).
    let is_blocked = |a: Point, b: Point| -> bool {
        if obs_index.segment_blocked(a, b, exempt_i, exempt_j) {
            return true;
        }
        if has_groups {
            ovg::group_blocks_segment(
                a, b,
                &scene.group_boundaries,
                crossings,
                params.spacing,
            )
        } else {
            false
        }
    };

    // Step cost: collision gate → `None`; otherwise length + (round 2)
    // shared-segment penalty. Bend cost is added by the search itself.
    let step_cost = |a: Point, b: Point| -> Option<f64> {
        if is_blocked(a, b) {
            return None;
        }
        let len = (b.x - a.x).abs() + (b.y - a.y).abs();
        let shared = round1.map_or(0.0, |prev| prev.shared_overlap(edge_id, a, b));
        Some(len + params.shared_penalty * shared)
    };

    // Port stub: at least `min_segment` long so the departure segment can
    // carry a corner radius (M1 rounded-corner budget).
    let stub_len = params.port_stub.max(params.min_segment);
    let q_source = stub_point(pair.source.point, pair.source.side, stub_len);
    let q_target = stub_point(pair.target.point, pair.target.side, stub_len);

    // Forced stub segments (terminal expansion, not a router freedom): if one
    // is already blocked, no legal path exists — fail honestly (§3.4).
    if is_blocked(pair.source.point, q_source) {
        return Err(LayoutError::message(format!(
            "router: edge `{edge_id}` source port stub is blocked by an obstacle"
        )));
    }
    if is_blocked(pair.target.point, q_target) {
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
            search_state,
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
        points = remove_short_humps(&points, params.min_segment, &is_blocked);
        points = normalize_polyline(&points);
    }

    Ok(EdgePlacement {
        id: edge_id.to_string(),
        source: pair.source.node_id.clone(),
        target: pair.target.node_id.clone(),
        path: EdgePath::polyline(points),
        from_port: Some(PortRef {
            side: pair.source.side,
            along: plotgram_model::port::AlongSpec::Ordered { order: 0, count: 1 },
        }),
        to_port: Some(PortRef {
            side: pair.target.side,
            along: plotgram_model::port::AlongSpec::Ordered { order: 0, count: 1 },
        }),
    })
}

// ─── M1: shared-segment penalty (round 2) ───────────────────

/// A stored segment with its owning edge index.
struct IndexedSeg {
    edge_idx: usize,
    c: Point,
    d: Point,
}

/// Round-1 routed segments indexed by line coordinate for O(k) overlap
/// queries (k = segments on the same line) instead of O(E²) full scan.
///
/// Segments are hashed by `(horizontal, line_coord_bits)` — only collinear
/// segments can have non-zero overlap, so we never compare across lines.
struct Round1Segments {
    /// edge_id → edge index (for self-exclusion).
    edge_idx: BTreeMap<String, usize>,
    /// Horizontal segments keyed by y-coordinate bits.
    h_lines: BTreeMap<u64, Vec<IndexedSeg>>,
    /// Vertical segments keyed by x-coordinate bits.
    v_lines: BTreeMap<u64, Vec<IndexedSeg>>,
}

impl Round1Segments {
    fn collect(placements: &[EdgePlacement]) -> Self {
        let mut edge_idx = BTreeMap::new();
        let mut h_lines: BTreeMap<u64, Vec<IndexedSeg>> = BTreeMap::new();
        let mut v_lines: BTreeMap<u64, Vec<IndexedSeg>> = BTreeMap::new();
        const EPS: f64 = 1e-9;

        for (i, p) in placements.iter().enumerate() {
            edge_idx.insert(p.id.clone(), i);
            for w in p.path.polyline_points().unwrap().windows(2) {
                let (a, b) = (w[0], w[1]);
                let seg = IndexedSeg { edge_idx: i, c: a, d: b };
                if (a.y - b.y).abs() < EPS {
                    // Horizontal: key by y.
                    h_lines.entry(a.y.to_bits()).or_default().push(seg);
                } else {
                    // Vertical: key by x.
                    v_lines.entry(a.x.to_bits()).or_default().push(seg);
                }
            }
        }
        Self { edge_idx, h_lines, v_lines }
    }

    /// Total overlap of segment `a→b` with round-1 segments of *other* edges
    /// (never the edge's own round-1 path — that would make it dodge itself).
    #[inline]
    fn shared_overlap(&self, edge_id: &str, a: Point, b: Point) -> f64 {
        const EPS: f64 = 1e-9;
        let my_idx = self.edge_idx.get(edge_id).copied().unwrap_or(usize::MAX);
        let mut total = 0.0;

        let horizontal = (a.y - b.y).abs() < EPS;
        if horizontal {
            if let Some(segs) = self.h_lines.get(&a.y.to_bits()) {
                for s in segs {
                    if s.edge_idx == my_idx {
                        continue;
                    }
                    total += crate::core::overlap_len(a, b, s.c, s.d);
                }
            }
        } else {
            if let Some(segs) = self.v_lines.get(&a.x.to_bits()) {
                for s in segs {
                    if s.edge_idx == my_idx {
                        continue;
                    }
                    total += crate::core::overlap_len(a, b, s.c, s.d);
                }
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
        let pts = &placements[0].path.polyline_points().unwrap();
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
        let pts = &placements[0].path.polyline_points().unwrap();
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
    fn group_bypass_routes_around() {
        let mut sc = two_boxes_clear();
        // Leave clearance from node a (right=80) so the stub is not inside
        // the inflated group: group.x >= 80 + spacing(=20) + margin.
        sc.group_boundaries.push(GroupBoundary {
            group_id: "g".to_string(),
            rect: Rect::new(120.0, -10.0, 40.0, 60.0),
        });
        let placements = OrthogonalEdgeRouter.route(&sc).unwrap();
        assert!(verify_all(&sc, &placements).all_pass);
        let pts = &placements[0].path.polyline_points().unwrap();
        assert!(pts.len() > 2, "expected detour around group: {pts:?}");
    }

    #[test]
    fn nested_groups_are_unsupported() {
        let mut sc = two_boxes_clear();
        sc.group_boundaries.push(GroupBoundary {
            group_id: "outer".to_string(),
            rect: Rect::new(-10.0, -10.0, 300.0, 60.0),
        });
        sc.group_boundaries.push(GroupBoundary {
            group_id: "inner".to_string(),
            rect: Rect::new(0.0, 0.0, 80.0, 40.0),
        });
        match OrthogonalEdgeRouter.route(&sc) {
            Err(LayoutError::UnsupportedRouteScene { .. }) => {}
            other => panic!("nested groups must be unsupported, got {other:?}"),
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
            assert_eq!(pa.path.polyline_points().unwrap(), pb.path.polyline_points().unwrap());
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
        // Default params (single round): L3 colouring + L4 VPSC must put
        // the two fully-overlapping edges onto distinct tracks.
        let sc = shared_corridor();
        let p = OrthogonalEdgeRouter.route(&sc).unwrap();
        assert_ne!(
            p[0].path.polyline_points().unwrap(), p[1].path.polyline_points().unwrap(),
            "tracks must separate a shared corridor"
        );
        let report = verify_all(&sc, &p);
        assert!(report.all_pass, "failures: {:?}", report.failures());
        // VPSC centres the bundle: both edges leave the backbone (or one
        // stays if only one shifted) — at least one corridor run is offset
        // by about spacing/2, and the two runs differ by ≥ spacing.
        let run_y = |pts: &[Point]| -> f64 {
            pts.windows(2)
                .find(|w| (w[0].y - w[1].y).abs() < 1e-9 && (w[1].x - w[0].x).abs() > 1.0)
                .map(|w| w[0].y)
                .unwrap_or(pts[0].y)
        };
        let y0 = run_y(&p[0].path.polyline_points().unwrap());
        let y1 = run_y(&p[1].path.polyline_points().unwrap());
        assert!(
            (y0 - y1).abs() >= sc.params.spacing - 1e-6,
            "track gap too small: y0={y0} y1={y1} pts0={:?} pts1={:?}",
            p[0].path.polyline_points().unwrap(),
            p[1].path.polyline_points().unwrap()
        );
    }

    #[test]
    fn two_rounds_still_separate_and_clear() {
        let mut sc = shared_corridor();
        sc.params.route_rounds = 2;
        sc.params.shared_penalty = 60.0;
        let p = OrthogonalEdgeRouter.route(&sc).unwrap();
        assert_ne!(p[0].path.polyline_points().unwrap(), p[1].path.polyline_points().unwrap());
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
        let pts = &p[0].path.polyline_points().unwrap();
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
