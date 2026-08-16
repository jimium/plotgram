//! Geometric invariant verification for routed edge paths.
//!
//! Library code — usable from tests, CLI, and benchmark binaries.
//! Checks correspond to `docs/design/routing/orthogonal/architecture.md` §9.

use plotgram_engine_api::{EdgeRouter, RouteScene};
use plotgram_model::geometry::Point;
use plotgram_model::result::EdgePlacement;

use crate::core::{padding_rect, segment_intersects_rect};
use crate::orthogonal::ovg::group_blocks_segment;

/// Tolerance for geometric comparisons.
const EPS: f64 = 1e-6;

// ─── Report types ───────────────────────────────────────────

/// Result of verifying all edges in a scene.
#[derive(Debug, Clone)]
pub struct VerifyReport {
    pub edge_results: Vec<EdgeVerifyResult>,
    pub all_pass: bool,
}

impl VerifyReport {
    /// Collect failure details for assertion messages.
    pub fn failures(&self) -> Vec<String> {
        self.edge_results
            .iter()
            .filter(|r| !r.pass)
            .flat_map(|r| {
                r.checks
                    .iter()
                    .filter(|c| !c.pass)
                    .map(move |c| format!("[{}] {}: {}", r.edge_id, c.name, c.detail))
            })
            .collect()
    }
}

/// Verification result for a single edge.
#[derive(Debug, Clone)]
pub struct EdgeVerifyResult {
    pub edge_id: String,
    pub checks: Vec<CheckOutcome>,
    pub pass: bool,
}

/// A single check outcome.
#[derive(Debug, Clone)]
pub struct CheckOutcome {
    pub name: &'static str,
    pub pass: bool,
    pub detail: String,
}

// ─── Public API ─────────────────────────────────────────────

/// Verify a single edge path against the scene constraints.
pub fn verify_edge(scene: &RouteScene, edge_id: &str, path: &[Point]) -> EdgeVerifyResult {
    let mut checks = Vec::new();

    checks.push(check_min_points(path));
    checks.push(check_orthogonal(path));

    if let Some(pair) = scene.terminals.get(edge_id) {
        checks.push(check_endpoint_attach(
            path,
            &pair.source.point,
            &pair.target.point,
        ));
        checks.push(check_obstacle_clearance(scene, edge_id, path));
        checks.push(check_group_clearance(scene, edge_id, path));
    }

    let pass = checks.iter().all(|c| c.pass);
    EdgeVerifyResult {
        edge_id: edge_id.to_string(),
        checks,
        pass,
    }
}

/// Verify all edge placements against the scene.
pub fn verify_all(scene: &RouteScene, placements: &[EdgePlacement]) -> VerifyReport {
    let mut edge_results = Vec::new();

    // Edge set conservation check.
    let placement_ids: Vec<&str> = placements.iter().map(|p| p.id.as_str()).collect();
    let order_ids: Vec<&str> = scene.edge_order.iter().map(|s| s.as_str()).collect();
    let conservation_pass = placement_ids == order_ids;

    for p in placements {
        edge_results.push(verify_edge(scene, &p.id, &p.path.samples()));
    }

    // If conservation fails, add a synthetic failing result.
    if !conservation_pass {
        edge_results.push(EdgeVerifyResult {
            edge_id: "<scene>".to_string(),
            checks: vec![CheckOutcome {
                name: "edge_set_conservation",
                pass: false,
                detail: format!("expected {:?}, got {:?}", scene.edge_order, placement_ids),
            }],
            pass: false,
        });
    }

    let all_pass = edge_results.iter().all(|r| r.pass);
    VerifyReport {
        edge_results,
        all_pass,
    }
}

/// Verify determinism: route the same scene twice, assert bit-identical output.
pub fn verify_determinism(scene: &RouteScene, router: &dyn EdgeRouter) -> bool {
    let run1 = router.route(scene);
    let run2 = router.route(scene);
    match (run1, run2) {
        (Ok(a), Ok(b)) => {
            if a.len() != b.len() {
                return false;
            }
            a.iter()
                .zip(b.iter())
                .all(|(ea, eb)| ea.id == eb.id && ea.path == eb.path)
        }
        (Err(_), Err(_)) => true, // Both fail consistently.
        _ => false,
    }
}

// ─── Individual checks ──────────────────────────────────────

fn check_min_points(path: &[Point]) -> CheckOutcome {
    let pass = path.len() >= 2;
    CheckOutcome {
        name: "min_points",
        pass,
        detail: if pass {
            String::new()
        } else {
            format!("path has {} points (need >= 2)", path.len())
        },
    }
}

fn check_orthogonal(path: &[Point]) -> CheckOutcome {
    for (i, w) in path.windows(2).enumerate() {
        let dx = (w[1].x - w[0].x).abs();
        let dy = (w[1].y - w[0].y).abs();
        if dx > EPS && dy > EPS {
            return CheckOutcome {
                name: "orthogonal",
                pass: false,
                detail: format!("segment {} ({:?} → {:?}) is diagonal", i, w[0], w[1]),
            };
        }
    }
    CheckOutcome {
        name: "orthogonal",
        pass: true,
        detail: String::new(),
    }
}

fn check_endpoint_attach(path: &[Point], source: &Point, target: &Point) -> CheckOutcome {
    if path.len() < 2 {
        return CheckOutcome {
            name: "endpoint_attach",
            pass: false,
            detail: "path too short to check endpoints".to_string(),
        };
    }
    let first = &path[0];
    let last = &path[path.len() - 1];
    let src_ok = (first.x - source.x).abs() < EPS && (first.y - source.y).abs() < EPS;
    let tgt_ok = (last.x - target.x).abs() < EPS && (last.y - target.y).abs() < EPS;

    if src_ok && tgt_ok {
        CheckOutcome {
            name: "endpoint_attach",
            pass: true,
            detail: String::new(),
        }
    } else {
        CheckOutcome {
            name: "endpoint_attach",
            pass: false,
            detail: format!(
                "source: {} (got {:?}, want {:?}); target: {} (got {:?}, want {:?})",
                if src_ok { "ok" } else { "MISMATCH" },
                first,
                source,
                if tgt_ok { "ok" } else { "MISMATCH" },
                last,
                target,
            ),
        }
    }
}

fn check_obstacle_clearance(scene: &RouteScene, edge_id: &str, path: &[Point]) -> CheckOutcome {
    // Determine own node ids (exempt from collision).
    let own_nodes: Vec<&str> = scene
        .terminals
        .get(edge_id)
        .map(|pair| vec![pair.source.node_id.as_str(), pair.target.node_id.as_str()])
        .unwrap_or_default();

    let inflate = scene.params.spacing;

    for obs in &scene.obstacles {
        if own_nodes.contains(&obs.id.as_str()) {
            continue;
        }
        let inflated = padding_rect(obs.rect, inflate);
        for (i, w) in path.windows(2).enumerate() {
            if segment_intersects_rect(w[0], w[1], inflated) {
                return CheckOutcome {
                    name: "obstacle_clearance",
                    pass: false,
                    detail: format!(
                        "segment {} ({:?} → {:?}) intersects obstacle `{}`",
                        i, w[0], w[1], obs.id
                    ),
                };
            }
        }
    }
    CheckOutcome {
        name: "obstacle_clearance",
        pass: true,
        detail: String::new(),
    }
}

fn check_group_clearance(scene: &RouteScene, edge_id: &str, path: &[Point]) -> CheckOutcome {
    if scene.group_boundaries.is_empty() {
        return CheckOutcome {
            name: "group_clearance",
            pass: true,
            detail: String::new(),
        };
    }
    let crossings = scene
        .boundary_permissions
        .get(edge_id)
        .map(|v| v.as_slice())
        .unwrap_or(&[]);
    for (i, w) in path.windows(2).enumerate() {
        if group_blocks_segment(
            w[0],
            w[1],
            &scene.group_boundaries,
            crossings,
            scene.params.spacing,
        ) {
            return CheckOutcome {
                name: "group_clearance",
                pass: false,
                detail: format!(
                    "segment {} ({:?} → {:?}) illegally crosses a group boundary",
                    i, w[0], w[1]
                ),
            };
        }
    }
    CheckOutcome {
        name: "group_clearance",
        pass: true,
        detail: String::new(),
    }
}

// Segment–rectangle intersection lives in `core` (single geometry truth
// shared by the router's collision model and this acceptance gate).
