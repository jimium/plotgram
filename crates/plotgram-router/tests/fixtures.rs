//! File-driven integration tests for edge routers.
//!
//! Scenes live as JSON in `tests/scenes/*.json` (see [`plotgram_router::fixture`]).
//! Tests load all scenes, filter by capability level, and verify invariants.

use std::fs;
use std::path::PathBuf;

use plotgram_engine_api::EdgeRouter;
use plotgram_router::fixture::{Requires, SceneFixture};
use plotgram_router::verify::{verify_all, verify_determinism};
use plotgram_router::OrthogonalEdgeRouter;

// ─── Scene loading ──────────────────────────────────────────

fn scenes_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/scenes")
}

/// Load all `tests/scenes/*.json` fixtures, sorted by filename for determinism.
fn load_all_fixtures() -> Vec<SceneFixture> {
    let dir = scenes_dir();
    let mut paths: Vec<_> = fs::read_dir(dir)
        .expect("tests/scenes/ must exist")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "json"))
        .collect();
    paths.sort();

    paths
        .iter()
        .map(|p| {
            let text = fs::read_to_string(p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()));
            serde_json::from_str(&text)
                .unwrap_or_else(|e| panic!("parse {}: {e}", p.display()))
        })
        .collect()
}

/// Fixtures passable by the stub (no search needed).
fn stub_passable(fixtures: &[SceneFixture]) -> Vec<&SceneFixture> {
    fixtures.iter().filter(|f| f.requires == Requires::None).collect()
}

/// Fixtures requiring obstacle-avoiding search.
fn search_required(fixtures: &[SceneFixture]) -> Vec<&SceneFixture> {
    fixtures.iter().filter(|f| f.requires >= Requires::Search).collect()
}

// ─── Tests ──────────────────────────────────────────────────

/// All fixtures: orthogonality + endpoint attachment + min points must always hold.
#[test]
fn fixture_ortho_and_attach() {
    let router = OrthogonalEdgeRouter;
    for fix in load_all_fixtures() {
        let result = router.route(&fix.scene);
        let placements = match result {
            Ok(p) => p,
            Err(e) => panic!("{}: route failed: {e}", fix.name),
        };
        let report = verify_all(&fix.scene, &placements);
        for r in &report.edge_results {
            for c in &r.checks {
                if c.name == "orthogonal" || c.name == "endpoint_attach" || c.name == "min_points"
                {
                    assert!(c.pass, "{} [{}]: {} — {}", fix.name, r.edge_id, c.name, c.detail);
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

/// All fixtures: determinism (double-run bit-identical).
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
