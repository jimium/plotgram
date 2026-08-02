//! Performance benchmark: large-scale scene routing.
//!
//! Generates a grid of nodes (default 25×20 = 500) with multiple edge
//! patterns (horizontal neighbours, vertical neighbours, long-range
//! diagonals) and measures end-to-end routing time.
//!
//! Run (debug, quick sanity):
//!   cargo run -p plotgram-router --example perf
//!
//! Run (release, real numbers):
//!   cargo run --release -p plotgram-router --example perf
//!
//! Flags:
//!   --cols N        Grid columns (default 25)
//!   --rows N        Grid rows (default 20)
//!   --diags N       Number of long-range diagonal edges (default 50)
//!   --rounds N      Routing rounds 1|2 (default 1)
//!   --verify        Run clearance verification after routing
//!   --warmup N      Warmup iterations before timing (default 1)
//!   --iters N       Timed iterations (default 3)
//!   --json          Machine-readable JSON output

use std::collections::BTreeMap;
use std::time::Instant;

use plotgram_engine_api::{
    EdgeRouter, Obstacle, OrthogonalRouteParams, PortAnchor, RouteScene, TerminalPair,
};
use plotgram_model::geometry::{Point, Rect};
use plotgram_model::port::Side;
use plotgram_router::verify::verify_all;
use plotgram_router::OrthogonalEdgeRouter;

// ─── Scene generation ───────────────────────────────────────

/// Node geometry constants (world units).
const NODE_W: f64 = 80.0;
const NODE_H: f64 = 40.0;
/// Horizontal / vertical gap between nodes.
const GAP_X: f64 = 60.0;
const GAP_Y: f64 = 50.0;

struct GridScene {
    scene: RouteScene,
    node_count: usize,
    edge_count: usize,
}

/// Deterministic pseudo-random (xorshift64) for reproducible edge selection.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn generate_scene(cols: usize, rows: usize, diags: usize, rounds: u8) -> GridScene {
    let cell_x = NODE_W + GAP_X;
    let cell_y = NODE_H + GAP_Y;

    // Build obstacles (nodes on a grid).
    let mut obstacles = Vec::with_capacity(cols * rows);
    let mut node_ids: Vec<Vec<String>> = Vec::with_capacity(rows);
    for r in 0..rows {
        let mut row_ids = Vec::with_capacity(cols);
        for c in 0..cols {
            let id = format!("n{r:02}_{c:02}");
            obstacles.push(Obstacle {
                id: id.clone(),
                rect: Rect::new(c as f64 * cell_x, r as f64 * cell_y, NODE_W, NODE_H),
            });
            row_ids.push(id);
        }
        node_ids.push(row_ids);
    }

    // Helper: port anchor on a node's side.
    let anchor = |r: usize, c: usize, side: Side| -> PortAnchor {
        let x = c as f64 * cell_x;
        let y = r as f64 * cell_y;
        let (px, py) = match side {
            Side::East => (x + NODE_W, y + NODE_H / 2.0),
            Side::West => (x, y + NODE_H / 2.0),
            Side::South => (x + NODE_W / 2.0, y + NODE_H),
            Side::North => (x + NODE_W / 2.0, y),
        };
        PortAnchor {
            point: Point { x: px, y: py },
            side,
            node_id: node_ids[r][c].clone(),
        }
    };

    let mut terminals: BTreeMap<String, TerminalPair> = BTreeMap::new();
    let mut edge_order: Vec<String> = Vec::new();
    let mut eid = 0usize;

    // Pattern 1: horizontal neighbours (east → west).
    for r in 0..rows {
        for c in 0..cols - 1 {
            let id = format!("e{eid:04}");
            eid += 1;
            terminals.insert(
                id.clone(),
                TerminalPair {
                    source: anchor(r, c, Side::East),
                    target: anchor(r, c + 1, Side::West),
                },
            );
            edge_order.push(id);
        }
    }

    // Pattern 2: vertical neighbours (south → north).
    for r in 0..rows - 1 {
        for c in 0..cols {
            let id = format!("e{eid:04}");
            eid += 1;
            terminals.insert(
                id.clone(),
                TerminalPair {
                    source: anchor(r, c, Side::South),
                    target: anchor(r + 1, c, Side::North),
                },
            );
            edge_order.push(id);
        }
    }

    // Pattern 3: long-range diagonals (deterministic pseudo-random pairs).
    let mut rng = Rng(0xDEAD_BEEF_CAFE_1234);
    let mut added = 0;
    let mut attempts = 0;
    while added < diags && attempts < diags * 10 {
        attempts += 1;
        let r1 = rng.below(rows);
        let c1 = rng.below(cols);
        let r2 = rng.below(rows);
        let c2 = rng.below(cols);
        // Skip self and adjacent (already covered by patterns 1 & 2).
        let dist = (r1 as i64 - r2 as i64).abs() + (c1 as i64 - c2 as i64).abs();
        if dist < 3 {
            continue;
        }
        let id = format!("e{eid:04}");
        eid += 1;
        // Choose sides based on relative position.
        let src_side = if c2 > c1 { Side::East } else { Side::West };
        let tgt_side = if c2 > c1 { Side::West } else { Side::East };
        terminals.insert(
            id.clone(),
            TerminalPair {
                source: anchor(r1, c1, src_side),
                target: anchor(r2, c2, tgt_side),
            },
        );
        edge_order.push(id);
        added += 1;
    }

    let params = OrthogonalRouteParams {
        route_rounds: rounds,
        shared_penalty: if rounds >= 2 { 50.0 } else { 0.0 },
        max_search_nodes: 200_000,
        ..Default::default()
    };

    let edge_count = edge_order.len();
    let node_count = cols * rows;

    let scene = RouteScene {
        obstacles,
        terminals,
        edge_order,
        group_boundaries: vec![],
        boundary_permissions: BTreeMap::new(),
        params,
    };

    GridScene {
        scene,
        node_count,
        edge_count,
    }
}

// ─── CLI ────────────────────────────────────────────────────

struct Config {
    cols: usize,
    rows: usize,
    diags: usize,
    rounds: u8,
    verify: bool,
    warmup: usize,
    iters: usize,
    json: bool,
}

fn parse_args() -> Config {
    let args: Vec<String> = std::env::args().collect();
    let mut cfg = Config {
        cols: 25,
        rows: 20,
        diags: 50,
        rounds: 1,
        verify: false,
        warmup: 1,
        iters: 3,
        json: false,
    };
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--cols" => {
                i += 1;
                cfg.cols = args[i].parse().expect("--cols expects a number")
            }
            "--rows" => {
                i += 1;
                cfg.rows = args[i].parse().expect("--rows expects a number")
            }
            "--diags" => {
                i += 1;
                cfg.diags = args[i].parse().expect("--diags expects a number")
            }
            "--rounds" => {
                i += 1;
                cfg.rounds = args[i].parse().expect("--rounds expects 1 or 2")
            }
            "--verify" => cfg.verify = true,
            "--warmup" => {
                i += 1;
                cfg.warmup = args[i].parse().expect("--warmup expects a number")
            }
            "--iters" => {
                i += 1;
                cfg.iters = args[i].parse().expect("--iters expects a number")
            }
            "--json" => cfg.json = true,
            other => {
                eprintln!("unknown flag: {other}");
                std::process::exit(1);
            }
        }
        i += 1;
    }
    cfg
}

// ─── Main ───────────────────────────────────────────────────

fn main() {
    let cfg = parse_args();
    let gs = generate_scene(cfg.cols, cfg.rows, cfg.diags, cfg.rounds);

    eprintln!(
        "scene: {} nodes ({}×{}), {} edges ({} horiz + {} vert + {} diag), rounds={}",
        gs.node_count,
        cfg.cols,
        cfg.rows,
        gs.edge_count,
        (cfg.cols - 1) * cfg.rows,
        cfg.cols * (cfg.rows - 1),
        cfg.diags,
        cfg.rounds,
    );

    let router = OrthogonalEdgeRouter;

    // Warmup
    for _ in 0..cfg.warmup {
        let _ = router.route(&gs.scene);
    }

    // Timed iterations
    let mut durations_ms: Vec<f64> = Vec::with_capacity(cfg.iters);
    let mut last_result = None;
    for _ in 0..cfg.iters {
        let t0 = Instant::now();
        let result = router.route(&gs.scene);
        let elapsed = t0.elapsed();
        durations_ms.push(elapsed.as_secs_f64() * 1000.0);
        last_result = Some(result);
    }

    durations_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let min_ms = durations_ms[0];
    let max_ms = *durations_ms.last().unwrap();
    let median_ms = durations_ms[durations_ms.len() / 2];
    let mean_ms: f64 = durations_ms.iter().sum::<f64>() / durations_ms.len() as f64;
    let per_edge_us = median_ms * 1000.0 / gs.edge_count as f64;

    // Verification (optional)
    let mut clearance = String::from("—");
    if cfg.verify {
        if let Some(Ok(ref placements)) = last_result {
            let report = verify_all(&gs.scene, placements);
            if report.all_pass {
                clearance = "PASS".to_string();
            } else {
                clearance = format!("FAIL({})", report.failures().len());
            }
        } else if let Some(Err(ref e)) = last_result {
            clearance = format!("ERR({e})");
        }
    }

    // Route result summary
    let route_ok = match &last_result {
        Some(Ok(p)) => Some(p.len()),
        _ => None,
    };

    if cfg.json {
        let obj = serde_json::json!({
            "nodes": gs.node_count,
            "edges": gs.edge_count,
            "grid": format!("{}x{}", cfg.cols, cfg.rows),
            "diags": cfg.diags,
            "rounds": cfg.rounds,
            "warmup": cfg.warmup,
            "iters": cfg.iters,
            "routed_edges": route_ok,
            "clearance": clearance,
            "timing_ms": {
                "min": format!("{min_ms:.2}"),
                "max": format!("{max_ms:.2}"),
                "median": format!("{median_ms:.2}"),
                "mean": format!("{mean_ms:.2}"),
            },
            "per_edge_us": format!("{per_edge_us:.1}"),
        });
        println!("{}", serde_json::to_string_pretty(&obj).unwrap());
    } else {
        println!();
        println!("┌─────────────────────────────────────────────────┐");
        println!("│  plotgram-router performance benchmark          │");
        println!("├─────────────────────────────────────────────────┤");
        println!("│  nodes:        {:>8}                       │", gs.node_count);
        println!("│  edges:        {:>8}                       │", gs.edge_count);
        println!("│  routed:       {:>8}                       │", route_ok.map(|n| n.to_string()).unwrap_or("ERR".into()));
        println!("│  clearance:    {:>8}                       │", clearance);
        println!("├─────────────────────────────────────────────────┤");
        println!("│  min:          {:>8.2} ms                    │", min_ms);
        println!("│  median:       {:>8.2} ms                    │", median_ms);
        println!("│  mean:         {:>8.2} ms                    │", mean_ms);
        println!("│  max:          {:>8.2} ms                    │", max_ms);
        println!("│  per-edge:     {:>8.1} µs                    │", per_edge_us);
        println!("└─────────────────────────────────────────────────┘");
        println!();
        println!("  warmup={} iters={} rounds={}", cfg.warmup, cfg.iters, cfg.rounds);
    }

    // Exit code: fail if routing errored.
    if route_ok.is_none() {
        if let Some(Err(e)) = &last_result {
            eprintln!("routing error: {e}");
        }
        std::process::exit(1);
    }
}
