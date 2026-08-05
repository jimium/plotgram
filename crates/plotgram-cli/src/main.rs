//! Thin CLI: args + file I/O only. Build orchestration lives in `plotgram-compile`.
//!
//! Subcommands (showcase-redesign-2026-08.md §5):
//!   validate <file>          parse + model validate; no layout/render
//!   render   <file> [-o]      .pgm → SVG (default: stdout)
//!   measure  <file> [--json]  layout metrics (§5.2): correctness / quality / observation
//!   debug-layout <file> [-o]  .pgm → LayoutDebugTrace JSON (debug-inspector.md T1)

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use clap::{Parser, Subcommand};
use plotgram_compile::{build_svg, build_svg_with_layout, compute_metrics, validate, BuildOptions};
use serde_json::json;

#[derive(Debug, Parser)]
#[command(name = "plotgram", about = "Plotgram DSL → SVG")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Parse + model validate only; no layout, no render.
    Validate {
        /// Input `.pgm` file
        input: PathBuf,
    },
    /// Render `.pgm` → SVG.
    Render {
        /// Input `.pgm` file
        input: PathBuf,
        /// Output SVG path (default: stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Theme id override
        #[arg(long)]
        theme: Option<String>,
    },
    /// Compute layout metrics (§5.2): correctness / quality / observation.
    Measure {
        /// Input `.pgm` file
        input: PathBuf,
        /// Emit JSON (default). Without --json, prints a one-line human summary.
        #[arg(long)]
        json: bool,
    },
    /// Emit the layout debug trace (LayoutDebugTrace JSON, debug-inspector.md).
    DebugLayout {
        /// Input `.pgm` file
        input: PathBuf,
        /// Output trace JSON path (default: stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Validate { input } => run_validate(&input),
        Command::Render {
            input,
            output,
            theme,
        } => run_render(&input, output, theme),
        Command::Measure { input, json } => run_measure(&input, json),
        Command::DebugLayout { input, output } => run_debug_layout(&input, output),
    }
}

fn run_debug_layout(input: &PathBuf, output: Option<PathBuf>) -> ExitCode {
    let source = match read_source(input) {
        Ok(s) => s,
        Err(code) => return code,
    };
    match plotgram_compile::build_debug_trace(&source, &BuildOptions::default()) {
        Ok(trace) => {
            let json = match serde_json::to_string_pretty(&trace) {
                Ok(j) => j,
                Err(e) => {
                    eprintln!("debug-layout {}: serialize: {e}", input.display());
                    return ExitCode::FAILURE;
                }
            };
            if let Some(path) = output {
                if let Err(e) = fs::write(&path, json) {
                    eprintln!("write {}: {e}", path.display());
                    return ExitCode::FAILURE;
                }
            } else {
                println!("{json}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("debug-layout {}: {e}", input.display());
            ExitCode::FAILURE
        }
    }
}

fn run_validate(input: &PathBuf) -> ExitCode {
    let source = match read_source(input) {
        Ok(s) => s,
        Err(code) => return code,
    };
    match validate(&source, &BuildOptions::default()) {
        Ok(()) => {
            eprintln!("ok: {}", input.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("validate {}: {e}", input.display());
            ExitCode::FAILURE
        }
    }
}

fn run_render(input: &PathBuf, output: Option<PathBuf>, theme: Option<String>) -> ExitCode {
    let source = match read_source(input) {
        Ok(s) => s,
        Err(code) => return code,
    };
    let options = BuildOptions {
        theme,
        ..BuildOptions::default()
    };
    match build_svg_with_layout(&source, &options) {
        Ok((layout, svg)) => {
            // Layout diagnostics exit (roadmap phase C): warnings never
            // affect geometry or exit code — structured output to stderr.
            for w in &layout.diagnostics.warnings {
                eprintln!("warning: {}: {}", input.display(), w.message);
            }
            if let Some(path) = output {
                if let Err(e) = fs::write(&path, svg) {
                    eprintln!("write {}: {e}", path.display());
                    return ExitCode::FAILURE;
                }
            } else {
                print!("{svg}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("render {}: {e}", input.display());
            ExitCode::FAILURE
        }
    }
}

// ── measure (§5.2) ────────────────────────────────────────────
// CLI owns: wall-clock timing (not WASM), det (double-render byte compare),
// path/layout/role derivation, and JSON assembly. Geometry math lives in
// plotgram-compile::audit (pure, WASM-safe).

const METRICS_SCHEMA_VERSION: u32 = 1;

fn run_measure(input: &PathBuf, json: bool) -> ExitCode {
    let path_str = input.display().to_string();
    let (layout, role) = derive_layout_role(&path_str);
    let source = match read_source(input) {
        Ok(s) => s,
        Err(code) => return code, // read error already reported
    };
    let opts = BuildOptions::default();

    // First (timed) build: layout + svg1. On failure → error report.
    let start = Instant::now();
    let built = build_svg_with_layout(&source, &opts);
    let elapsed_ms = start.elapsed().as_millis() as u64;

    let report = match built {
        Ok((layout_result, svg1)) => {
            // det: render once more, byte-compare.
            let det = match build_svg(&source, &opts) {
                Ok(svg2) => svg1 == svg2,
                Err(_) => false, // second render failed → non-deterministic by definition
            };
            let m = compute_metrics(&layout_result);
            let diag = &layout_result.diagnostics;
            json!({
                "schema_version": METRICS_SCHEMA_VERSION,
                "path": path_str,
                "layout": layout,
                "role": role,
                "status": "ok",
                "error": null,
                "elapsed_ms": elapsed_ms,
                "correctness": {
                    "parse_error": false,
                    "node_overlap_count": m.node_overlap_count,
                    "edge_crosses_group_interior": m.edge_crosses_group_interior,
                    "label_overlap_count": m.label_overlap_count,
                    "det": det,
                },
                "quality": {
                    "edge_crossing_count": m.edge_crossing_count,
                    "total_edge_length": m.total_edge_length,
                    "canvas_area": m.canvas_area,
                    "aspect_ratio": m.aspect_ratio,
                },
                "observation": {
                    "node_count": m.node_count,
                    "edge_count": m.edge_count,
                },
                // Layout diagnostics (roadmap phase C): attribute regressions
                // to params vs code; warnings count the non-fatal observations.
                "diagnostics": {
                    "warning_count": diag.warnings.len(),
                    "params_hash": diag.params_hash,
                },
            })
        }
        Err(e) => {
            // Distinguish parse vs render error by variant for the gallery / gate.
            let status = match &e {
                plotgram_compile::BuildError::Parse(_) => "parse-error",
                _ => "render-error",
            };
            json!({
                "schema_version": METRICS_SCHEMA_VERSION,
                "path": path_str,
                "layout": layout,
                "role": role,
                "status": status,
                "error": e.to_string(),
                "elapsed_ms": elapsed_ms,
                "correctness": null,
                "quality": null,
                "observation": null,
            })
        }
    };

    if json {
        println!("{}", serde_json::to_string(&report).unwrap());
    } else {
        // Human one-liner.
        let status = report["status"].as_str().unwrap_or("?");
        let det = report["correctness"]["det"].as_bool();
        let nc = report["correctness"]["node_overlap_count"].as_i64();
        let ec = report["correctness"]["edge_crosses_group_interior"].as_i64();
        let lc = report["correctness"]["label_overlap_count"].as_i64();
        let xc = report["quality"]["edge_crossing_count"].as_i64();
        println!(
            "{} [{}] det={:?} overlaps=node:{:?}/edge-group:{:?}/label:{:?} crossings={:?} {}ms",
            path_str,
            status,
            det,
            nc,
            ec,
            lc,
            xc,
            report["elapsed_ms"].as_u64().unwrap_or(0),
        );
    }

    // Exit non-zero on any correctness failure or parse/render error, so a bare
    // `plotgram measure` call surfaces defects; snapshot.sh reads stdout JSON
    // and also keys on `status`.
    let status = report["status"].as_str().unwrap_or("");
    if status != "ok" {
        return ExitCode::FAILURE;
    }
    let det = report["correctness"]["det"].as_bool().unwrap_or(false);
    if !det {
        return ExitCode::FAILURE;
    }
    let hard_fail = [
        "node_overlap_count",
        "edge_crosses_group_interior",
        "label_overlap_count",
    ]
    .iter()
    .any(|k| report["correctness"][k].as_i64().map_or(false, |v| v > 0));
    if hard_fail {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// Derive (layout, role) from a path: layout = first segment, role = first
/// dot-split of the filename. Matches the showcase `{layout}/{role}.{slug}.pgm`
/// convention so the gate can key on path without a second registry.
fn derive_layout_role(path: &str) -> (String, String) {
    let layout = path.split('/').next().unwrap_or("").to_string();
    let fname = path.rsplit('/').next().unwrap_or(path);
    let role = fname.split('.').next().unwrap_or("").to_string();
    (layout, role)
}

fn read_source(input: &PathBuf) -> Result<String, ExitCode> {
    match fs::read_to_string(input) {
        Ok(s) => Ok(s),
        Err(e) => {
            eprintln!("read {}: {e}", input.display());
            Err(ExitCode::FAILURE)
        }
    }
}
