//! Algorithm benchmark runner: routes all scenes for a specified algorithm.
//!
//! Run: `cargo run -p plotgram-router --example bench -- orthogonal`
//!
//! First positional argument = algorithm name (required).
//! Flags: `--json` (JSON output), `--baseline <path>` (compare mode).

use std::fs;
use std::path::PathBuf;

use plotgram_engine_api::EdgeRouter;
use plotgram_router::fixture::{load_dir, Requires, SceneFixture};
use plotgram_router::score::{score_scene, SceneScore};
use plotgram_router::verify::verify_all;
use plotgram_router::{
    CurvedEdgeRouter, OctilinearEdgeRouter, OrthogonalEdgeRouter, PolylineEdgeRouter,
    StraightEdgeRouter,
};

// ─── Algorithm registry ─────────────────────────────────────

struct AlgoEntry {
    name: &'static str,
    router: Box<dyn EdgeRouter>,
    /// Maximum capability level this algorithm supports.
    capability: Requires,
}

fn lookup_algorithm(name: &str) -> AlgoEntry {
    match name {
        "orthogonal" => AlgoEntry {
            name: "orthogonal",
            router: Box::new(OrthogonalEdgeRouter),
            capability: Requires::Track, // M1: 避障 + 走廊 track 分离（组场景仍不支持，走诚实拒绝）
        },
        "straight" => AlgoEntry {
            name: "straight",
            router: Box::new(StraightEdgeRouter),
            // Direct terminal→terminal; no search / track / group.
            capability: Requires::None,
        },
        "polyline" => AlgoEntry {
            name: "polyline",
            router: Box::new(PolylineEdgeRouter),
            // Visibility search + obstacle avoidance; no track / group yet.
            capability: Requires::Search,
        },
        "octilinear" => AlgoEntry {
            name: "octilinear",
            router: Box::new(OctilinearEdgeRouter),
            // Octilinear visibility + obstacle avoidance; no track / group yet.
            capability: Requires::Search,
        },
        "curved" => AlgoEntry {
            name: "curved",
            router: Box::new(CurvedEdgeRouter),
            // Bézier / smoothed polyline; obstacle fallback via polyline.
            capability: Requires::Search,
        },
        _ => {
            eprintln!("unknown algorithm: {name}");
            eprintln!("available: orthogonal, straight, polyline, octilinear, curved");
            std::process::exit(1);
        }
    }
}

// ─── Scene loading ──────────────────────────────────────────

fn load_fixtures() -> Vec<SceneFixture> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/scenes");
    load_dir(&dir)
}

// ─── Result row ─────────────────────────────────────────────

#[derive(serde::Serialize, serde::Deserialize)]
struct BenchRow {
    algorithm: String,
    scene: String,
    requires: String,
    applicable: bool,
    clearance: String,
    #[serde(flatten)]
    score: Option<SceneScore>,
}

// ─── Comparison ─────────────────────────────────────────────

#[derive(serde::Serialize)]
struct CompareRow {
    algorithm: String,
    scene: String,
    clearance_old: String,
    clearance_new: String,
    bends_old: Option<usize>,
    bends_new: Option<usize>,
    length_old: Option<f64>,
    length_new: Option<f64>,
    crossings_old: Option<usize>,
    crossings_new: Option<usize>,
}

#[derive(serde::Serialize)]
struct CompareVerdict {
    clearance_improved: usize,
    clearance_regressed: usize,
    bends_delta: i64,
    length_delta: f64,
    crossings_delta: i64,
    verdict: String,
}

fn compare(current: &[BenchRow], baseline: &[BenchRow]) {
    let mut rows = Vec::new();
    let mut clearance_improved = 0usize;
    let mut clearance_regressed = 0usize;
    let mut bends_delta: i64 = 0;
    let mut length_delta: f64 = 0.0;
    let mut crossings_delta: i64 = 0;

    for cur in current {
        let base = baseline
            .iter()
            .find(|b| b.algorithm == cur.algorithm && b.scene == cur.scene);
        let base = match base {
            Some(b) => b,
            None => continue,
        };

        let old_pass = base.clearance == "PASS";
        let new_pass = cur.clearance == "PASS";
        if !old_pass && new_pass {
            clearance_improved += 1;
        } else if old_pass && !new_pass {
            clearance_regressed += 1;
        }

        let bends_old = base.score.as_ref().map(|s| s.total_bends);
        let bends_new = cur.score.as_ref().map(|s| s.total_bends);
        if let (Some(o), Some(n)) = (bends_old, bends_new) {
            bends_delta += n as i64 - o as i64;
        }

        let length_old = base.score.as_ref().map(|s| s.total_length);
        let length_new = cur.score.as_ref().map(|s| s.total_length);
        if let (Some(o), Some(n)) = (length_old, length_new) {
            length_delta += n - o;
        }

        let crossings_old = base.score.as_ref().map(|s| s.crossings);
        let crossings_new = cur.score.as_ref().map(|s| s.crossings);
        if let (Some(o), Some(n)) = (crossings_old, crossings_new) {
            crossings_delta += n as i64 - o as i64;
        }

        rows.push(CompareRow {
            algorithm: cur.algorithm.clone(),
            scene: cur.scene.clone(),
            clearance_old: base.clearance.clone(),
            clearance_new: cur.clearance.clone(),
            bends_old,
            bends_new,
            length_old,
            length_new,
            crossings_old,
            crossings_new,
        });
    }

    let verdict = if clearance_regressed > 0 {
        format!("REGRESSED ({clearance_regressed} scene(s) lost clearance)")
    } else if clearance_improved > 0 {
        format!("IMPROVED ({clearance_improved} scene(s) gained clearance)")
    } else if bends_delta == 0 && length_delta.abs() < 1e-9 && crossings_delta == 0 {
        "UNCHANGED".to_string()
    } else {
        let mut parts = Vec::new();
        if bends_delta != 0 {
            parts.push(format!("bends {bends_delta:+}"));
        }
        if length_delta.abs() > 1e-9 {
            parts.push(format!("length {length_delta:+.0}"));
        }
        if crossings_delta != 0 {
            parts.push(format!("crossings {crossings_delta:+}"));
        }
        format!("CHANGED ({})", parts.join(", "))
    };

    // Print table
    println!(
        "{:<14} {:<22} {:<12} {:<12} {:>10} {:>12} {:>10}",
        "algorithm", "scene", "clear(old)", "clear(new)", "bends", "length", "cross"
    );
    println!("{}", "─".repeat(96));
    for r in &rows {
        let bends = match (r.bends_old, r.bends_new) {
            (Some(o), Some(n)) if o != n => format!("{o}→{n}"),
            (_, Some(n)) => format!("{n}"),
            _ => "—".into(),
        };
        let length = match (r.length_old, r.length_new) {
            (Some(o), Some(n)) if (o - n).abs() > 0.5 => format!("{o:.0}→{n:.0}"),
            (_, Some(n)) => format!("{n:.0}"),
            _ => "—".into(),
        };
        let cross = match (r.crossings_old, r.crossings_new) {
            (Some(o), Some(n)) if o != n => format!("{o}→{n}"),
            (_, Some(n)) => format!("{n}"),
            _ => "—".into(),
        };
        println!(
            "{:<14} {:<22} {:<12} {:<12} {:>10} {:>12} {:>10}",
            r.algorithm, r.scene, r.clearance_old, r.clearance_new, bends, length, cross
        );
    }
    println!();
    println!("verdict: {verdict}");

    let v = CompareVerdict {
        clearance_improved,
        clearance_regressed,
        bends_delta,
        length_delta,
        crossings_delta,
        verdict,
    };
    // Machine-readable verdict on stderr (so stdout stays clean for piping)
    eprintln!("{}", serde_json::to_string(&v).unwrap());
}

// ─── Main ───────────────────────────────────────────────────

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let json_output = args.iter().any(|a| a == "--json");

    // First positional arg = algorithm name (skip flags)
    let algo_name = args
        .iter()
        .skip(1)
        .find(|a| !a.starts_with('-'))
        .map(|s| s.as_str())
        .unwrap_or_else(|| {
            eprintln!("usage: bench <algorithm> [--json] [--baseline <path>]");
            eprintln!("available algorithms: orthogonal");
            std::process::exit(1);
        });

    let algo = lookup_algorithm(algo_name);

    // --baseline <path>: compare mode
    if let Some(pos) = args.iter().position(|a| a == "--baseline") {
        let path = args.get(pos + 1).expect("--baseline requires a path");
        let text = fs::read_to_string(path).unwrap_or_else(|e| {
            eprintln!("no baseline at {path}: {e}");
            eprintln!("run `score.sh baseline` first");
            std::process::exit(1);
        });
        let baseline: Vec<BenchRow> = serde_json::from_str(&text).expect("parse baseline");
        let current = run_algo(&algo);
        compare(&current, &baseline);
        return;
    }

    let rows = run_algo(&algo);

    if json_output {
        println!("{}", serde_json::to_string_pretty(&rows).unwrap());
        return;
    }

    print_table(&rows);
}

fn run_algo(algo: &AlgoEntry) -> Vec<BenchRow> {
    let fixtures = load_fixtures();
    let mut rows: Vec<BenchRow> = Vec::new();

    for fix in &fixtures {
        let applicable = fix.requires <= algo.capability;
        let result = algo.router.route(&fix.scene);

        let (clearance, score) = match result {
            Ok(placements) => {
                let report = verify_all(&fix.scene, &placements);
                let clearance = if report.all_pass {
                    "PASS".to_string()
                } else {
                    format!("FAIL({})", report.failures().len())
                };
                let sc = score_scene(&fix.name, &fix.scene, &placements);
                (clearance, Some(sc))
            }
            Err(e) => (format!("ERR({e})"), None),
        };

        rows.push(BenchRow {
            algorithm: algo.name.to_string(),
            scene: fix.name.clone(),
            requires: fix.requires.to_string(),
            applicable,
            clearance,
            score,
        });
    }
    rows
}

fn print_table(rows: &[BenchRow]) {
    println!(
        "{:<14} {:<22} {:<8} {:<5} {:<10} {:>5} {:>8} {:>5}",
        "algorithm", "scene", "requires", "appl", "clearance", "bends", "length", "cross"
    );
    println!("{}", "─".repeat(88));

    for row in rows {
        let (bends, length, cross) = match &row.score {
            Some(s) => (
                format!("{}", s.total_bends),
                format!("{:.0}", s.total_length),
                format!("{}", s.crossings),
            ),
            None => ("—".into(), "—".into(), "—".into()),
        };
        let appl = if row.applicable { "✓" } else { "·" };
        println!(
            "{:<14} {:<22} {:<8} {:<5} {:<10} {:>5} {:>8} {:>5}",
            row.algorithm, row.scene, row.requires, appl, row.clearance, bends, length, cross
        );
    }

    println!();
    println!(
        "  ✓ = algorithm capable of this scene level · · = skip (needs {})",
        "higher capability"
    );
}
