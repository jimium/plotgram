//! Generate a self-contained HTML visualization of routing results.
//!
//! Run: `cargo run -p plotgram-router --example viz -- orthogonal`
//! Output: `target/router-viz.html` — open in any browser.
//!
//! First positional argument = algorithm name (required).
//! Draws obstacles (nodes), port anchors, and routed edge paths per scene.

use std::fmt::Write as FmtWrite;
use std::fs;
use std::path::PathBuf;

use plotgram_engine_api::EdgeRouter;
use plotgram_model::result::EdgePlacement;
use plotgram_router::fixture::{load_dir_board, BoardFixture, SceneFixture};
use plotgram_router::verify::verify_all;
use plotgram_router::OrthogonalEdgeRouter;

// ─── Algorithm registry ─────────────────────────────────────

fn lookup_algorithm(name: &str) -> Box<dyn EdgeRouter> {
    match name {
        "orthogonal" => Box::new(OrthogonalEdgeRouter),
        _ => {
            eprintln!("unknown algorithm: {name}");
            eprintln!("available: orthogonal");
            std::process::exit(1);
        }
    }
}

// ─── Scene loading ──────────────────────────────────────────

fn load_fixtures() -> Vec<SceneFixture> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/scenes");
    let mut boards: Vec<BoardFixture> = load_dir_board(&dir);
    boards.sort_by(|a, b| a.id.cmp(&b.id));
    boards.into_iter().map(|b| b.to_scene_fixture()).collect()
}

// ─── SVG rendering ──────────────────────────────────────────

const PAD: f64 = 30.0;
const COLORS: &[&str] = &["#2563eb", "#dc2626", "#16a34a", "#9333ea", "#ea580c", "#0891b2"];

fn compute_bbox(fix: &SceneFixture, placements: &[EdgePlacement]) -> (f64, f64, f64, f64) {
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;

    for obs in &fix.scene.obstacles {
        min_x = min_x.min(obs.rect.x);
        min_y = min_y.min(obs.rect.y);
        max_x = max_x.max(obs.rect.x + obs.rect.width);
        max_y = max_y.max(obs.rect.y + obs.rect.height);
    }
    for p in placements {
        for pt in &p.path.points {
            min_x = min_x.min(pt.x);
            min_y = min_y.min(pt.y);
            max_x = max_x.max(pt.x);
            max_y = max_y.max(pt.y);
        }
    }
    (min_x - PAD, min_y - PAD, max_x + PAD, max_y + PAD)
}

fn render_panel(
    algo_name: &str,
    fix: &SceneFixture,
    placements: Option<&[EdgePlacement]>,
    error: Option<&str>,
) -> String {
    let mut svg = String::new();

    let (vx, vy, vx2, vy2) = match placements {
        Some(p) => compute_bbox(fix, p),
        None => compute_bbox(fix, &[]),
    };
    let w = vx2 - vx;
    let h = vy2 - vy;

    // Status
    let (status_text, status_color) = match placements {
        Some(p) => {
            let report = verify_all(&fix.scene, p);
            if report.all_pass {
                ("PASS", "#16a34a")
            } else {
                ("FAIL", "#dc2626")
            }
        }
        None => ("ERR", "#dc2626"),
    };

    let _ = writeln!(
        svg,
        r#"<div class="panel">
<div class="panel-header">
  <span class="scene-name">{}</span>
  <span class="algo">{}</span>
  <span class="status" style="color:{}">{}</span>
  <span class="requires">requires: {}</span>
</div>"#,
        fix.name, algo_name, status_color, status_text, fix.requires
    );

    let _ = writeln!(
        svg,
        r#"<svg viewBox="{vx} {vy} {w} {h}" xmlns="http://www.w3.org/2000/svg">"#
    );

    // Grid dots (subtle)
    let _ = writeln!(svg, r##"<defs>
<pattern id="grid" width="40" height="40" patternUnits="userSpaceOnUse">
  <circle cx="20" cy="20" r="0.8" fill="#e5e7eb"/>
</pattern>
</defs>
<rect x="{vx}" y="{vy}" width="{w}" height="{h}" fill="url(#grid)"/>"##);

    // Obstacles
    for obs in &fix.scene.obstacles {
        let _ = writeln!(
            svg,
            r##"<rect x="{}" y="{}" width="{}" height="{}" rx="3" fill="#f1f5f9" stroke="#64748b" stroke-width="1.5"/>"##,
            obs.rect.x, obs.rect.y, obs.rect.width, obs.rect.height
        );
        let cx = obs.rect.x + obs.rect.width / 2.0;
        let cy = obs.rect.y + obs.rect.height / 2.0;
        let _ = writeln!(
            svg,
            r##"<text x="{cx}" y="{cy}" text-anchor="middle" dominant-baseline="middle" font-size="10" fill="#64748b">{}</text>"##,
            obs.id
        );
    }

    // Edge paths
    if let Some(placements) = placements {
        for (i, p) in placements.iter().enumerate() {
            let color = COLORS[i % COLORS.len()];
            if p.path.points.len() >= 2 {
                let points: Vec<String> = p
                    .path
                    .points
                    .iter()
                    .map(|pt| format!("{},{}", pt.x, pt.y))
                    .collect();
                let _ = writeln!(
                    svg,
                    r#"<polyline points="{}" fill="none" stroke="{color}" stroke-width="2" stroke-linejoin="round"/>"#,
                    points.join(" ")
                );
                // Bend dots
                for pt in &p.path.points[1..p.path.points.len() - 1] {
                    let _ = writeln!(
                        svg,
                        r#"<circle cx="{}" cy="{}" r="2.5" fill="{color}"/>"#,
                        pt.x, pt.y
                    );
                }
            }
            // Edge label
            if let Some(mid) = p.path.points.get(p.path.points.len() / 2) {
                let _ = writeln!(
                    svg,
                    r#"<text x="{}" y="{}" font-size="9" fill="{color}" font-weight="bold">{}</text>"#,
                    mid.x + 3.0,
                    mid.y - 4.0,
                    p.id
                );
            }
        }
    }

    // Port anchors
    for (_eid, term) in &fix.scene.terminals {
        for anchor in [&term.source, &term.target] {
            let _ = writeln!(
                svg,
                r##"<circle cx="{}" cy="{}" r="4" fill="#f97316" stroke="#fff" stroke-width="1.5"/>"##,
                anchor.point.x, anchor.point.y
            );
        }
    }

    // Error message
    if let Some(err) = error {
        let cx = vx + w / 2.0;
        let cy = vy + h / 2.0;
        let _ = writeln!(
            svg,
            r##"<text x="{cx}" y="{cy}" text-anchor="middle" font-size="12" fill="#dc2626">{err}</text>"##
        );
    }

    let _ = writeln!(svg, "</svg></div>");
    svg
}

// ─── HTML page ──────────────────────────────────────────────

fn render_html(algo_name: &str, router: &dyn EdgeRouter, fixtures: &[SceneFixture]) -> String {
    let mut html = String::new();

    let _ = writeln!(
        html,
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8"/>
<title>plotgram-router · {algo_name}</title>
<style>
* {{ margin: 0; padding: 0; box-sizing: border-box; }}
body {{ font-family: -apple-system, 'Segoe UI', sans-serif; background: #fafafa; padding: 24px; }}
h1 {{ font-size: 20px; margin-bottom: 4px; }}
.subtitle {{ color: #6b7280; font-size: 13px; margin-bottom: 20px; }}
.grid {{ display: grid; grid-template-columns: repeat(auto-fill, minmax(380px, 1fr)); gap: 16px; }}
.panel {{ background: #fff; border: 1px solid #e5e7eb; border-radius: 8px; overflow: hidden; }}
.panel-header {{ display: flex; align-items: center; gap: 8px; padding: 8px 12px; border-bottom: 1px solid #f3f4f6; font-size: 12px; }}
.scene-name {{ font-weight: 600; }}
.algo {{ color: #6b7280; }}
.status {{ font-weight: 700; margin-left: auto; }}
.requires {{ color: #9ca3af; font-size: 11px; }}
svg {{ display: block; width: 100%; height: 260px; }}
</style>
</head>
<body>
<h1>plotgram-router · {algo_name}</h1>
<p class="subtitle">{} scene(s) — orange dots = port anchors, colored lines = edge paths, dots on path = bends</p>
<div class="grid">"#,
        fixtures.len()
    );

    for fix in fixtures {
        let result = router.route(&fix.scene);
        match &result {
            Ok(placements) => {
                html.push_str(&render_panel(algo_name, fix, Some(placements), None));
            }
            Err(e) => {
                let msg = e.to_string();
                html.push_str(&render_panel(algo_name, fix, None, Some(&msg)));
            }
        }
    }

    let _ = writeln!(
        html,
        r#"</div>
</body>
</html>"#
    );
    html
}

// ─── Main ───────────────────────────────────────────────────

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let algo_name = args.get(1).map(|s| s.as_str()).unwrap_or_else(|| {
        eprintln!("usage: viz <algorithm>");
        eprintln!("available: orthogonal");
        std::process::exit(1);
    });

    let router = lookup_algorithm(algo_name);
    let fixtures = load_fixtures();
    let html = render_html(algo_name, router.as_ref(), &fixtures);

    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/router-viz.html");
    fs::write(&out, &html).expect("write html");
    println!("wrote {} ({} bytes)", out.display(), html.len());
    println!("open: file://{}", out.canonicalize().unwrap().display());
}
