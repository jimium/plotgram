//! Generate `tests/scenes/*.json` fixture files from programmatic definitions.
//!
//! Run: `cargo run -p plotgram-router --example gen-scenes`
//!
//! This is a **one-way generator** — after initial export, edit the JSON files
//! directly. Re-run only if you intentionally change scene definitions here.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use plotgram_engine_api::{
    Obstacle, OrthogonalRouteParams, PortAnchor, RouteScene, TerminalPair,
};
use plotgram_model::geometry::{Point, Rect};
use plotgram_model::port::Side;
use plotgram_router::fixture::{Requires, SceneFixture};

// ─── Helpers ────────────────────────────────────────────────

fn obs(id: &str, x: f64, y: f64, w: f64, h: f64) -> Obstacle {
    Obstacle {
        id: id.to_string(),
        rect: Rect::new(x, y, w, h),
    }
}

fn anchor(node: &str, side: Side, x: f64, y: f64) -> PortAnchor {
    PortAnchor {
        point: Point { x, y },
        side,
        node_id: node.to_string(),
    }
}

fn make_scene(obstacles: Vec<Obstacle>, terms: Vec<(String, TerminalPair)>) -> RouteScene {
    let edge_order: Vec<String> = terms.iter().map(|(id, _)| id.clone()).collect();
    let terminals: BTreeMap<String, TerminalPair> = terms.into_iter().collect();
    RouteScene {
        obstacles,
        terminals,
        edge_order,
        group_boundaries: Vec::new(),
        boundary_permissions: BTreeMap::new(),
        params: OrthogonalRouteParams::default(),
    }
}

fn fixture(name: &str, desc: &str, requires: Requires, scene: RouteScene) -> SceneFixture {
    SceneFixture {
        name: name.to_string(),
        description: desc.to_string(),
        requires,
        scene,
    }
}

// ─── Scene definitions ──────────────────────────────────────

fn all_scenes() -> Vec<SceneFixture> {
    vec![
        fixture(
            "two_boxes_clear",
            "Two nodes side by side, no obstacles between them.",
            Requires::None,
            make_scene(
                vec![
                    obs("a", 0.0, 0.0, 80.0, 40.0),
                    obs("b", 200.0, 0.0, 80.0, 40.0),
                ],
                vec![(
                    "e0".to_string(),
                    TerminalPair {
                        source: anchor("a", Side::East, 80.0, 20.0),
                        target: anchor("b", Side::West, 200.0, 20.0),
                    },
                )],
            ),
        ),
        fixture(
            "two_boxes_vertical",
            "Two nodes stacked vertically.",
            Requires::None,
            make_scene(
                vec![
                    obs("a", 0.0, 0.0, 80.0, 40.0),
                    obs("b", 0.0, 120.0, 80.0, 40.0),
                ],
                vec![(
                    "e0".to_string(),
                    TerminalPair {
                        source: anchor("a", Side::South, 40.0, 40.0),
                        target: anchor("b", Side::North, 40.0, 120.0),
                    },
                )],
            ),
        ),
        fixture(
            "blocker_center",
            "A blocker obstacle between source and target, forcing a detour.",
            Requires::Search,
            make_scene(
                vec![
                    obs("a", 0.0, 80.0, 80.0, 40.0),
                    obs("b", 300.0, 80.0, 80.0, 40.0),
                    obs("blocker", 150.0, 60.0, 80.0, 80.0),
                ],
                vec![(
                    "e0".to_string(),
                    TerminalPair {
                        source: anchor("a", Side::East, 80.0, 100.0),
                        target: anchor("b", Side::West, 300.0, 100.0),
                    },
                )],
            ),
        ),
        fixture(
            "narrow_corridor",
            "Two obstacles with a narrow corridor (1x spacing) between them.",
            Requires::Search,
            {
                let gap = OrthogonalRouteParams::default().spacing; // 20
                make_scene(
                    vec![
                        obs("a", 0.0, 0.0, 60.0, 40.0),
                        obs("b", 250.0, 0.0, 60.0, 40.0),
                        obs("wall_top", 100.0, -100.0, 50.0, 100.0 + 20.0 - gap / 2.0),
                        obs("wall_bot", 100.0, 20.0 + gap / 2.0, 50.0, 100.0),
                    ],
                    vec![(
                        "e0".to_string(),
                        TerminalPair {
                            source: anchor("a", Side::East, 60.0, 20.0),
                            target: anchor("b", Side::West, 250.0, 20.0),
                        },
                    )],
                )
            },
        ),
        fixture(
            "parallel_two_edges",
            "Two edges between the same pair of nodes (parallel).",
            Requires::None,
            make_scene(
                vec![
                    obs("a", 0.0, 0.0, 80.0, 60.0),
                    obs("b", 200.0, 0.0, 80.0, 60.0),
                ],
                vec![
                    (
                        "e0".to_string(),
                        TerminalPair {
                            source: anchor("a", Side::East, 80.0, 20.0),
                            target: anchor("b", Side::West, 200.0, 20.0),
                        },
                    ),
                    (
                        "e1".to_string(),
                        TerminalPair {
                            source: anchor("a", Side::East, 80.0, 40.0),
                            target: anchor("b", Side::West, 200.0, 40.0),
                        },
                    ),
                ],
            ),
        ),
        fixture(
            "four_boxes_cross",
            "Four nodes in a cross pattern with edges that cross.",
            Requires::None,
            make_scene(
                vec![
                    obs("n", 100.0, 0.0, 60.0, 40.0),
                    obs("s", 100.0, 200.0, 60.0, 40.0),
                    obs("w", 0.0, 100.0, 60.0, 40.0),
                    obs("e", 200.0, 100.0, 60.0, 40.0),
                ],
                vec![
                    (
                        "ns".to_string(),
                        TerminalPair {
                            source: anchor("n", Side::South, 130.0, 40.0),
                            target: anchor("s", Side::North, 130.0, 200.0),
                        },
                    ),
                    (
                        "we".to_string(),
                        TerminalPair {
                            source: anchor("w", Side::East, 60.0, 120.0),
                            target: anchor("e", Side::West, 200.0, 120.0),
                        },
                    ),
                ],
            ),
        ),
        fixture(
            "u_turn",
            "Source and target ports face the same direction (U-turn required).",
            Requires::None,
            make_scene(
                vec![
                    obs("a", 0.0, 0.0, 80.0, 40.0),
                    obs("b", 0.0, 80.0, 80.0, 40.0),
                ],
                vec![(
                    "e0".to_string(),
                    TerminalPair {
                        source: anchor("a", Side::East, 80.0, 20.0),
                        target: anchor("b", Side::East, 80.0, 100.0),
                    },
                )],
            ),
        ),
        fixture(
            "grid_3x3",
            "3x3 grid of obstacles, route from top-left to bottom-right.",
            Requires::Search,
            {
                let mut obstacles = Vec::new();
                for row in 0..3 {
                    for col in 0..3 {
                        let id = format!("g{row}{col}");
                        obstacles.push(obs(&id, col as f64 * 120.0, row as f64 * 100.0, 80.0, 60.0));
                    }
                }
                make_scene(
                    obstacles,
                    vec![(
                        "diag".to_string(),
                        TerminalPair {
                            source: anchor("g00", Side::East, 80.0, 30.0),
                            target: anchor("g22", Side::West, 240.0, 230.0),
                        },
                    )],
                )
            },
        ),
    ]
}

// ─── Main ───────────────────────────────────────────────────

fn main() {
    let out_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/scenes");
    fs::create_dir_all(&out_dir).expect("create tests/scenes/");

    for fix in all_scenes() {
        let path = out_dir.join(format!("{}.json", fix.name));
        let json = serde_json::to_string_pretty(&fix).expect("serialize");
        fs::write(&path, json + "\n").expect("write");
        println!("  wrote {}", path.display());
    }
    println!("done: {} scenes", all_scenes().len());
}
