//! File-driven integration tests for edge routers.
//!
//! Scenes live as JSON in `tests/scenes/*.json` (see [`plotgram_router::fixture`]).
//! Tests load all scenes, filter by capability level, and verify invariants.

use std::path::PathBuf;

use plotgram_engine_api::{EdgeRouter, LayoutError};
use plotgram_router::fixture::{load_dir, Requires, RouteExpect, SceneFixture};
use plotgram_router::verify::{verify_all, verify_determinism};
use plotgram_router::OrthogonalEdgeRouter;

// ─── Scene loading ──────────────────────────────────────────

fn scenes_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/scenes")
}

/// Load all `tests/scenes/*.json` fixtures, sorted by filename for determinism.
fn load_all_fixtures() -> Vec<SceneFixture> {
    load_dir(&scenes_dir())
}

fn expect_ok(fixtures: &[SceneFixture]) -> Vec<&SceneFixture> {
    fixtures
        .iter()
        .filter(|f| f.expect == RouteExpect::Ok)
        .collect()
}

/// Fixtures passable by the stub (no search needed).
fn stub_passable(fixtures: &[SceneFixture]) -> Vec<&SceneFixture> {
    fixtures
        .iter()
        .filter(|f| f.expect == RouteExpect::Ok && f.requires == Requires::None)
        .collect()
}

/// Fixtures requiring obstacle-avoiding search (includes track/group success cases).
fn search_required(fixtures: &[SceneFixture]) -> Vec<&SceneFixture> {
    fixtures
        .iter()
        .filter(|f| f.expect == RouteExpect::Ok && f.requires >= Requires::Search)
        .collect()
}

/// Fixtures requiring corridor track separation (M1).
fn track_required(fixtures: &[SceneFixture]) -> Vec<&SceneFixture> {
    fixtures
        .iter()
        .filter(|f| f.expect == RouteExpect::Ok && f.requires >= Requires::Track)
        .collect()
}

/// Fixtures requiring group-boundary crossing (M2 success cases).
fn group_required(fixtures: &[SceneFixture]) -> Vec<&SceneFixture> {
    fixtures
        .iter()
        .filter(|f| f.expect == RouteExpect::Ok && f.requires == Requires::Group)
        .collect()
}

// ─── Tests ──────────────────────────────────────────────────

/// Success fixtures: orthogonality + endpoint attachment + min points.
#[test]
fn fixture_ortho_and_attach() {
    let router = OrthogonalEdgeRouter;
    for fix in expect_ok(&load_all_fixtures()) {
        let placements = router
            .route(&fix.scene)
            .unwrap_or_else(|e| panic!("{}: route failed: {e}", fix.name));
        let report = verify_all(&fix.scene, &placements);
        for r in &report.edge_results {
            for c in &r.checks {
                if c.name == "orthogonal" || c.name == "endpoint_attach" || c.name == "min_points" {
                    assert!(
                        c.pass,
                        "{} [{}]: {} — {}",
                        fix.name, r.edge_id, c.name, c.detail
                    );
                }
            }
        }
    }
}

/// Stub-passable fixtures: full verification (including obstacle clearance).
#[test]
fn fixture_clearance_stub() {
    let router = OrthogonalEdgeRouter;
    for fix in stub_passable(&load_all_fixtures()) {
        let placements = router
            .route(&fix.scene)
            .unwrap_or_else(|e| panic!("{}: {e}", fix.name));
        let report = verify_all(&fix.scene, &placements);
        assert!(
            report.all_pass,
            "{} failures: {:?}",
            fix.name,
            report.failures()
        );
    }
}

/// Fixtures requiring real search: obstacle clearance (M0: OVG + A*).
#[test]
fn fixture_clearance_m0() {
    let router = OrthogonalEdgeRouter;
    for fix in search_required(&load_all_fixtures()) {
        let placements = router
            .route(&fix.scene)
            .unwrap_or_else(|e| panic!("{}: {e}", fix.name));
        let report = verify_all(&fix.scene, &placements);
        assert!(
            report.all_pass,
            "{} failures: {:?}",
            fix.name,
            report.failures()
        );
    }
}

/// M1: corridor track separation — edges sharing a corridor must not fully
/// overlap (M1 acceptance: 多边不完全重合).
#[test]
fn fixture_track_separation() {
    let router = OrthogonalEdgeRouter;
    for fix in track_required(&load_all_fixtures()) {
        let placements = router
            .route(&fix.scene)
            .unwrap_or_else(|e| panic!("{}: {e}", fix.name));
        let report = verify_all(&fix.scene, &placements);
        assert!(
            report.all_pass,
            "{} failures: {:?}",
            fix.name,
            report.failures()
        );
        // Any pair of edges must not fully overlap on a shared corridor.
        for i in 0..placements.len() {
            for j in (i + 1)..placements.len() {
                assert_ne!(
                    placements[i].path.polyline_points().unwrap(),
                    placements[j].path.polyline_points().unwrap(),
                    "{}: edges {} and {} fully overlap",
                    fix.name,
                    placements[i].id,
                    placements[j].id
                );
            }
        }
    }
}

/// M2: group success fixtures — full verify including group_clearance.
#[test]
fn fixture_group_crossing() {
    let router = OrthogonalEdgeRouter;
    for fix in group_required(&load_all_fixtures()) {
        let placements = router
            .route(&fix.scene)
            .unwrap_or_else(|e| panic!("{}: {e}", fix.name));
        let report = verify_all(&fix.scene, &placements);
        assert!(
            report.all_pass,
            "{} failures: {:?}",
            fix.name,
            report.failures()
        );
    }
}

/// M2: fixtures that must hard-fail or be unsupported.
#[test]
fn fixture_group_expect_fail() {
    let router = OrthogonalEdgeRouter;
    for fix in load_all_fixtures() {
        match fix.expect {
            RouteExpect::Ok => continue,
            RouteExpect::NoPath => {
                let err = router
                    .route(&fix.scene)
                    .expect_err(&format!("{}: expected no-path failure", fix.name));
                assert!(
                    err.to_string().contains("no collision-free path")
                        || err.to_string().contains("stub"),
                    "{}: unexpected error: {err}",
                    fix.name
                );
            }
            RouteExpect::Unsupported => match router.route(&fix.scene) {
                Err(LayoutError::UnsupportedRouteScene { .. }) => {}
                other => panic!(
                    "{}: expected UnsupportedRouteScene, got {other:?}",
                    fix.name
                ),
            },
        }
    }
}

/// All fixtures: determinism (double-run bit-identical, including consistent failures).
#[test]
fn fixture_determinism() {
    let router = OrthogonalEdgeRouter;
    for fix in load_all_fixtures() {
        assert!(
            verify_determinism(&fix.scene, &router),
            "{}: non-deterministic output",
            fix.name
        );
    }
}
