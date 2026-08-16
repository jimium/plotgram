//! Generate a self-contained HTML visualization of routing results.
//!
//! Run: `cargo run -p plotgram-router --example viz -- orthogonal`
//! Output: `target/router-viz.html` — open in any browser.
//!
//! First positional argument = algorithm name (required).
//! Draws obstacles (nodes), group boundaries, gate regions, port anchors,
//! and routed edge paths per scene.

use std::fmt::Write as FmtWrite;
use std::fs;
use std::path::PathBuf;

use plotgram_engine_api::EdgeRouter;
use plotgram_model::geometry::Rect;
use plotgram_model::result::{EdgePath, EdgePlacement};
use plotgram_router::fixture::{load_dir_board, BoardFixture, RouteExpect, SceneFixture};
use plotgram_router::verify::verify_all;
use plotgram_router::{
    CurvedEdgeRouter, OctilinearEdgeRouter, OrthogonalEdgeRouter, PolylineEdgeRouter,
    StraightEdgeRouter,
};

// ─── Algorithm registry ─────────────────────────────────────

fn lookup_algorithm(name: &str) -> Box<dyn EdgeRouter> {
    match name {
        "orthogonal" => Box::new(OrthogonalEdgeRouter),
        "straight" => Box::new(StraightEdgeRouter),
        "polyline" => Box::new(PolylineEdgeRouter),
        "octilinear" => Box::new(OctilinearEdgeRouter),
        "curved" => Box::new(CurvedEdgeRouter),
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
    let mut boards: Vec<BoardFixture> = load_dir_board(&dir);
    boards.sort_by(|a, b| a.id.cmp(&b.id));
    boards.into_iter().map(|b| b.to_scene_fixture()).collect()
}

// ─── SVG rendering ──────────────────────────────────────────

const PAD: f64 = 30.0;
const COLORS: &[&str] = &[
    "#2563eb", "#dc2626", "#16a34a", "#9333ea", "#ea580c", "#0891b2",
];

fn expand_bbox(bbox: &mut (f64, f64, f64, f64), r: Rect) {
    bbox.0 = bbox.0.min(r.x);
    bbox.1 = bbox.1.min(r.y);
    bbox.2 = bbox.2.max(r.right());
    bbox.3 = bbox.3.max(r.bottom());
}

fn compute_bbox(fix: &SceneFixture, placements: &[EdgePlacement]) -> (f64, f64, f64, f64) {
    let mut bbox = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);

    for obs in &fix.scene.obstacles {
        expand_bbox(&mut bbox, obs.rect);
    }
    for g in &fix.scene.group_boundaries {
        expand_bbox(&mut bbox, g.rect);
    }
    for crossings in fix.scene.boundary_permissions.values() {
        for c in crossings {
            if let Some(gate) = c.gate_region {
                expand_bbox(&mut bbox, gate);
            }
        }
    }
    for p in placements {
        for pt in p.path.samples() {
            bbox.0 = bbox.0.min(pt.x);
            bbox.1 = bbox.1.min(pt.y);
            bbox.2 = bbox.2.max(pt.x);
            bbox.3 = bbox.3.max(pt.y);
        }
    }
    if !bbox.0.is_finite() {
        return (-PAD, -PAD, PAD, PAD);
    }
    (bbox.0 - PAD, bbox.1 - PAD, bbox.2 + PAD, bbox.3 + PAD)
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

    // Status vs fixture `expect`: NoPath / Unsupported that fail as intended → PASS.
    let (status_text, status_color) = match (&fix.expect, placements, error) {
        (RouteExpect::Ok, Some(p), _) => {
            let report = verify_all(&fix.scene, p);
            if report.all_pass {
                ("PASS", "#16a34a")
            } else {
                ("FAIL", "#dc2626")
            }
        }
        (RouteExpect::Ok, None, _) => ("ERR", "#dc2626"),
        (RouteExpect::NoPath, None, Some(msg))
            if msg.contains("no collision-free path") || msg.contains("stub") =>
        {
            ("PASS", "#16a34a")
        }
        (RouteExpect::NoPath, None, _) => ("FAIL", "#dc2626"),
        (RouteExpect::NoPath, Some(_), _) => ("FAIL", "#dc2626"), // routed but should not
        (RouteExpect::Unsupported, None, Some(msg))
            if msg.to_ascii_lowercase().contains("unsupported route scene") =>
        {
            ("PASS", "#16a34a")
        }
        (RouteExpect::Unsupported, None, _) => ("FAIL", "#dc2626"),
        (RouteExpect::Unsupported, Some(_), _) => ("FAIL", "#dc2626"),
    };

    let _ = writeln!(
        svg,
        r#"<div class="panel">
<div class="panel-header">
  <span class="scene-name">{}</span>
  <span class="algo">{}</span>
  <span class="status" style="color:{}">{}</span>
  <span class="requires">requires: {}</span>
  <span class="expect">expect: {:?}</span>
</div>"#,
        fix.name, algo_name, status_color, status_text, fix.requires, fix.expect
    );

    let _ = writeln!(
        svg,
        r#"<svg viewBox="{vx} {vy} {w} {h}" xmlns="http://www.w3.org/2000/svg">"#
    );

    // Pattern ids must be unique per panel (many SVGs share one HTML doc).
    let grid_id = format!("grid-{}", fix.name);
    let hatch_id = format!("gate-hatch-{}", fix.name);
    let _ = writeln!(
        svg,
        r##"<defs>
<pattern id="{grid_id}" width="40" height="40" patternUnits="userSpaceOnUse">
  <circle cx="20" cy="20" r="0.8" fill="#e5e7eb"/>
</pattern>
<pattern id="{hatch_id}" width="6" height="6" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
  <line x1="0" y1="0" x2="0" y2="6" stroke="#f59e0b" stroke-width="2" stroke-opacity="0.55"/>
</pattern>
</defs>
<rect x="{vx}" y="{vy}" width="{w}" height="{h}" fill="url(#{grid_id})"/>"##
    );

    // Group boundaries (behind nodes): dashed outline + light fill.
    for g in &fix.scene.group_boundaries {
        let _ = writeln!(
            svg,
            r##"<rect x="{}" y="{}" width="{}" height="{}" rx="2" fill="#dbeafe" fill-opacity="0.35" stroke="#3b82f6" stroke-width="1.5" stroke-dasharray="6 3"/>"##,
            g.rect.x, g.rect.y, g.rect.width, g.rect.height
        );
        let _ = writeln!(
            svg,
            r##"<text x="{}" y="{}" font-size="9" fill="#2563eb" font-weight="600">{}</text>"##,
            g.rect.x + 4.0,
            g.rect.y + 12.0,
            g.group_id
        );
    }

    // Gate regions (permission seams): hatch fill on top of group.
    for crossings in fix.scene.boundary_permissions.values() {
        for c in crossings {
            let Some(gate) = c.gate_region else {
                continue;
            };
            let _ = writeln!(
                svg,
                r##"<rect x="{}" y="{}" width="{}" height="{}" fill="url(#{hatch_id})" stroke="#d97706" stroke-width="1.25"/>"##,
                gate.x, gate.y, gate.width, gate.height
            );
        }
    }

    // Obstacles (nodes)
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
            match &p.path {
                EdgePath::Polyline { points } if points.len() >= 2 => {
                    let pts: Vec<String> = points
                        .iter()
                        .map(|pt| format!("{},{}", pt.x, pt.y))
                        .collect();
                    let _ = writeln!(
                        svg,
                        r#"<polyline points="{}" fill="none" stroke="{color}" stroke-width="2" stroke-linejoin="round"/>"#,
                        pts.join(" ")
                    );
                    for pt in &points[1..points.len() - 1] {
                        let _ = writeln!(
                            svg,
                            r#"<circle cx="{}" cy="{}" r="2.5" fill="{color}"/>"#,
                            pt.x, pt.y
                        );
                    }
                }
                EdgePath::Cubic {
                    start,
                    end,
                    controls,
                } => {
                    let _ = writeln!(
                        svg,
                        r#"<path d="M {} {} C {} {} {} {} {} {}" fill="none" stroke="{color}" stroke-width="2"/>"#,
                        start.x,
                        start.y,
                        controls[0].x,
                        controls[0].y,
                        controls[1].x,
                        controls[1].y,
                        end.x,
                        end.y
                    );
                    for pt in controls {
                        let _ = writeln!(
                            svg,
                            r#"<circle cx="{}" cy="{}" r="2.5" fill="{color}" opacity="0.5"/>"#,
                            pt.x, pt.y
                        );
                    }
                }
                _ => {}
            }
            let samples = p.path.samples();
            if let Some(mid) = samples.get(samples.len() / 2) {
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

    if let Some(err) = error {
        let cx = vx + w / 2.0;
        let cy = vy + h / 2.0;
        // Expected failures stay informative but not alarm-red when status is PASS.
        let fill = if status_text == "PASS" {
            "#6b7280"
        } else {
            "#dc2626"
        };
        let _ = writeln!(
            svg,
            r##"<text x="{cx}" y="{cy}" text-anchor="middle" font-size="12" fill="{fill}">{err}</text>"##
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
.panel-header {{ display: flex; align-items: center; gap: 8px; padding: 8px 12px; border-bottom: 1px solid #f3f4f6; font-size: 12px; flex-wrap: wrap; }}
.scene-name {{ font-weight: 600; }}
.algo {{ color: #6b7280; }}
.status {{ font-weight: 700; margin-left: auto; }}
.requires, .expect {{ color: #9ca3af; font-size: 11px; }}
svg {{ display: block; width: 100%; height: 260px; }}
</style>
</head>
<body>
<h1>plotgram-router · {algo_name}</h1>
<p class="subtitle">{} scene(s) — slate = nodes · blue dashed = groups · amber hatch = gates · orange dots = ports · colored lines = paths</p>
<div class="grid">"#,
        fixtures.len()
    );

    for (i, fix) in fixtures.iter().enumerate() {
        eprint!(
            "\r  [{}/{}] {} ...                    ",
            i + 1,
            fixtures.len(),
            fix.name
        );
        let _ = std::io::Write::flush(&mut std::io::stderr());
        let result = router.route(&fix.scene);
        match &result {
            Ok(placements) => {
                html.push_str(&render_panel(algo_name, fix, Some(placements), None));
            }
            Err(e) => {
                let msg = e.to_string();
                // Still draw scene geometry (groups/gates/nodes) on failure.
                html.push_str(&render_panel(algo_name, fix, None, Some(&msg)));
            }
        }
    }
    eprintln!();

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
        eprintln!("available: orthogonal, straight, polyline, octilinear, curved");
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
