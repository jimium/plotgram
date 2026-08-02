//! Independent orthogonal [`EdgeRouter`](plotgram_engine_api::EdgeRouter).
//!
//! Status: **M0 已落地** — reduced-interesting-line OVG + A* search with
//! obstacle avoidance, single round, no groups (see
//! `docs/design/layout/routing/architecture.md` §10). L3 track ordering and
//! L4 nudging are M1+ and intentionally absent.
//!
//! Write authority (R1/R2): the router only writes `EdgePlacement.path`;
//! terminals, obstacles and ports are read-only. Honesty (§3.4): an edge
//! with no collision-free path is a hard error — never a degenerate
//! obstacle-crossing elbow.

mod ovg;
mod search;

use plotgram_engine_api::{EdgeRouter, LayoutError, OrthogonalRouteParams, RouteScene, TerminalPair};
use plotgram_model::geometry::Point;
use plotgram_model::port::PortRef;
use plotgram_model::result::{EdgePath, EdgePlacement};

use crate::core::normalize_polyline;

use ovg::{outward, segment_blocked, stub_point, Grid};
use search::astar;

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
        let mut extra = Vec::with_capacity(scene.terminals.len() * 4);
        for pair in scene.terminals.values() {
            extra.push(pair.source.point);
            extra.push(stub_point(pair.source.point, pair.source.side, scene.params.port_stub));
            extra.push(pair.target.point);
            extra.push(stub_point(pair.target.point, pair.target.side, scene.params.port_stub));
        }
        let grid = Grid::build(&scene.obstacles, &extra, line_offset);

        let mut out = Vec::with_capacity(scene.edge_order.len());
        for edge_id in &scene.edge_order {
            let pair = scene.terminals.get(edge_id).ok_or_else(|| {
                LayoutError::message(format!(
                    "router: edge `{edge_id}` in edge_order but missing from terminals"
                ))
            })?;
            out.push(route_edge(scene, &grid, edge_id, pair)?);
        }
        Ok(out)
    }
}

/// Route one edge: stub out of both terminals, A* between the stub ends on
/// the shared grid, then assemble and normalize the polyline.
fn route_edge(
    scene: &RouteScene,
    grid: &Grid,
    edge_id: &str,
    pair: &TerminalPair,
) -> Result<EdgePlacement, LayoutError> {
    let params = &scene.params;
    let exempt = [pair.source.node_id.as_str(), pair.target.node_id.as_str()];
    let blocked = |a: Point, b: Point| segment_blocked(a, b, &scene.obstacles, &exempt, params.spacing);

    let q_source = stub_point(pair.source.point, pair.source.side, params.port_stub);
    let q_target = stub_point(pair.target.point, pair.target.side, params.port_stub);

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
        LayoutError::message(format!("router: edge `{edge_id}` source stub is off the search grid"))
    })?;
    let goal = grid.find(q_target).ok_or_else(|| {
        LayoutError::message(format!("router: edge `{edge_id}` target stub is off the search grid"))
    })?;

    let mut points = vec![pair.source.point];
    if start == goal {
        // Both stubs land on the same grid vertex.
        points.push(q_source);
    } else {
        let searched = astar(
            grid,
            start,
            outward(pair.source.side),
            goal,
            outward(pair.target.side).opposite(),
            params.bend_penalty,
            &blocked,
        )
        .ok_or_else(|| {
            LayoutError::message(format!(
                "router: edge `{edge_id}` has no collision-free path"
            ))
        })?;
        points.extend(searched);
    }
    points.push(pair.target.point);
    let points = normalize_polyline(&points);

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

/// Parameter discipline (§6): M0 implements `spacing` / `port_stub` /
/// `bend_penalty`. Parameters scheduled for M1+ must not be silently ignored
/// — reject non-default values as explicitly unsupported.
fn check_params(p: &OrthogonalRouteParams) -> Result<(), LayoutError> {
    for (name, v) in [
        ("spacing", p.spacing),
        ("port_stub", p.port_stub),
        ("bend_penalty", p.bend_penalty),
    ] {
        if !v.is_finite() || v < 0.0 {
            return Err(LayoutError::message(format!(
                "router: param `{name}` must be finite and >= 0, got {v}"
            )));
        }
    }
    let default = OrthogonalRouteParams::default();
    let unimplemented: [(&str, bool); 4] = [
        ("shared_penalty", p.shared_penalty != 0.0),
        ("route_rounds", p.route_rounds != 1),
        ("max_search_nodes", p.max_search_nodes != 0),
        ("min_segment", p.min_segment != default.min_segment),
    ];
    for (name, violated) in unimplemented {
        if violated {
            return Err(LayoutError::UnsupportedRouteScene {
                reason: format!("router: param `{name}` is not implemented yet (M1+)"),
            });
        }
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
            vec![obstacle("a", 0.0, 0.0, 80.0, 40.0), obstacle("b", 200.0, 0.0, 80.0, 40.0)],
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

    // ─── Tests ──────────────────────────────────────────────

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
        assert!(err.to_string().contains("no collision-free path"), "unexpected: {err}");
    }

    #[test]
    fn m1_params_are_explicitly_unsupported() {
        let cases: &[(&str, fn(&mut OrthogonalRouteParams))] = &[
            ("shared_penalty", |p| p.shared_penalty = 1.0),
            ("route_rounds", |p| p.route_rounds = 2),
            ("max_search_nodes", |p| p.max_search_nodes = 64),
            ("min_segment", |p| p.min_segment = 99.0),
        ];
        for (name, mutate) in cases {
            let mut sc = two_boxes_clear();
            mutate(&mut sc.params);
            match OrthogonalEdgeRouter.route(&sc) {
                Err(LayoutError::UnsupportedRouteScene { reason }) => {
                    assert!(reason.contains(name), "param `{name}`: {reason}")
                }
                other => panic!("param `{name}` must be rejected, got {other:?}"),
            }
        }
    }

    #[test]
    fn invalid_params_are_rejected() {
        let mut sc = two_boxes_clear();
        sc.params.spacing = -1.0;
        assert!(OrthogonalEdgeRouter.route(&sc).is_err());
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
            other => panic!("groups must be unsupported at M0, got {other:?}"),
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
}
