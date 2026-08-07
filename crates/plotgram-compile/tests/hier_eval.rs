//! Hierarchical layout regression/scoring check.
//!
//! Runs the full pipeline (parse → measure → `hierarchical` layout) over
//! every fixture in `showcase/hierarchical/` (recursively: fixtures live in
//! category subdirectories like `flat/`, `group/`, `partition/`) and asserts hard geometric
//! invariants the layout must never violate, independent of visual
//! inspection:
//!
//! - no two node frames overlap;
//! - every edge path segment is axis-aligned (orthogonal) — asserted only
//!   for the default/`orthogonal` routing style; fixtures that declare
//!   `routing_style: polyline` / `routing_style: curved` opt out;
//! - every edge path's first/last point lands exactly on its source/target
//!   node frame boundary (the port anchor, not just "near" it);
//! - orthogonal edge segments must not penetrate the open interior of any
//!   non-endpoint node (same rule as InkVerifier);
//! - a self-contained segment-intersection crossing count and max-bend count
//!   are printed per file as an informational regression signal (not a hard
//!   failure — a dense graph legitimately has crossings).
//!
//! This is a coarse "does the geometry make sense" check, not a substitute
//! for visual review (see `docs/design/layout/hierarchical/notes/`).
//!
//! Bend regression gate: per-fixture `max_bends` / `sum_bends` /
//! `reversed_count` must not exceed the checked-in baseline in
//! `tests/hier_eval_baseline.json` (crossings / canvas bbox are printed as
//! observational deltas). `reversed_count` guards the FAS cycle-entry rule:
//! swapping the cut edge of a cycle never adds reversals. Regenerate the
//! baseline only when a change is *intended*:
//! `HIER_EVAL_WRITE_BASELINE=1 cargo test -p plotgram-compile --test hier_eval`.
//!
//! SymmetryAxis D2 gate ([phases/symmetry-axis.md](../../../docs/design/layout/hierarchical/phases/symmetry-axis.md)):
//! three representative spines must stay collinear on the cross axis
//! (`flat-rest-api`, `constrain-flat-chain`, `order-approval`).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use plotgram_compile::{build_debug_trace, build_layout, BuildOptions};
use plotgram_model::geometry::{Point, Rect};
use plotgram_model::result::LayoutResult;
use serde_json::{json, Value};

const EPS: f64 = 1e-6;

fn showcase_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("apps/showcase/hierarchical")
}

fn collect_pgm(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for entry in rd.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            collect_pgm(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("pgm") {
            out.push(path);
        }
    }
}

fn rects_overlap(a: Rect, b: Rect) -> bool {
    a.x < b.right() - EPS
        && b.x < a.right() - EPS
        && a.y < b.bottom() - EPS
        && b.y < a.bottom() - EPS
}

fn on_boundary(p: Point, frame: Rect) -> bool {
    let on_vertical_edge = (p.x - frame.x).abs() < 1.0 || (p.x - frame.right()).abs() < 1.0;
    let on_horizontal_edge = (p.y - frame.y).abs() < 1.0 || (p.y - frame.bottom()).abs() < 1.0;
    let within_x = p.x >= frame.x - 1.0 && p.x <= frame.right() + 1.0;
    let within_y = p.y >= frame.y - 1.0 && p.y <= frame.bottom() + 1.0;
    (on_vertical_edge && within_y) || (on_horizontal_edge && within_x)
}

/// Orthogonal-segment intersection: both segments axis-aligned; counts a
/// crossing when one strictly straddles the other's line (touching at a
/// shared endpoint is not a crossing).
fn segments_cross(a0: Point, a1: Point, b0: Point, b1: Point) -> bool {
    let a_horiz = (a0.y - a1.y).abs() < EPS;
    let b_horiz = (b0.y - b1.y).abs() < EPS;
    if a_horiz == b_horiz {
        return false; // parallel orthogonal segments never "cross" (only overlap, not counted)
    }
    let (h0, h1, v0, v1) = if a_horiz {
        (a0, a1, b0, b1)
    } else {
        (b0, b1, a0, a1)
    };
    let (hx_lo, hx_hi) = (h0.x.min(h1.x), h0.x.max(h1.x));
    let (vy_lo, vy_hi) = (v0.y.min(v1.y), v0.y.max(v1.y));
    let hy = h0.y;
    let vx = v0.x;
    vx > hx_lo + EPS && vx < hx_hi - EPS && hy > vy_lo + EPS && hy < vy_hi - EPS
}

struct FileMetrics {
    name: String,
    nodes: usize,
    edges: usize,
    crossings: usize,
    max_bends: usize,
    sum_bends: usize,
    reversed_count: usize,
    bbox_w: f64,
    bbox_h: f64,
}

fn baseline_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/hier_eval_baseline.json")
}

fn metrics_to_value(m: &FileMetrics) -> Value {
    json!({
        "max_bends": m.max_bends,
        "sum_bends": m.sum_bends,
        "crossings": m.crossings,
        "reversed_count": m.reversed_count,
        "bbox_w": m.bbox_w,
        "bbox_h": m.bbox_h,
    })
}

#[test]
fn hierarchical_showcase_geometry_invariants() {
    let dir = showcase_dir();
    let mut entries: Vec<PathBuf> = Vec::new();
    collect_pgm(&dir, &mut entries);
    entries.sort();
    assert!(
        !entries.is_empty(),
        "expected showcase/hierarchical/**/*.pgm fixtures"
    );

    let mut all_metrics = Vec::new();
    let mut hard_failures: Vec<String> = Vec::new();

    for path in &entries {
        let name = path
            .strip_prefix(&dir)
            .unwrap_or(path)
            .with_extension("")
            .display()
            .to_string();
        let source = fs::read_to_string(path).unwrap();
        let result = match build_layout(&source, &BuildOptions::default()) {
            Ok(r) => r,
            Err(e) => {
                hard_failures.push(format!("{name}: pipeline error: {e}"));
                continue;
            }
        };

        check_no_node_overlaps(&name, &result, &mut hard_failures);
        check_orthogonal_and_ports(&name, &source, &result, &mut hard_failures);
        check_no_edge_node_penetration(&name, &source, &result, &mut hard_failures);
        let reversed_count = match build_debug_trace(&source, &BuildOptions::default()) {
            Ok(trace) => serde_json::to_value(&trace)
                .ok()
                .and_then(|v| v.pointer("/extension/edge_plans").cloned())
                .and_then(|plans| plans.as_array().cloned())
                .map(|plans| {
                    plans
                        .iter()
                        .filter(|p| p["reversed"].as_bool().unwrap_or(false))
                        .count()
                })
                .unwrap_or(0),
            Err(e) => {
                hard_failures.push(format!("{name}: debug trace error: {e}"));
                0
            }
        };
        all_metrics.push(compute_metrics(&name, &result, reversed_count));
    }

    println!(
        "\n{:<45} {:>6} {:>6} {:>10} {:>10} {:>10} {:>10} {:>14}",
        "fixture", "nodes", "edges", "crossings", "max_bends", "sum_bends", "reversed", "bbox (w x h)"
    );
    for m in &all_metrics {
        println!(
            "{:<45} {:>6} {:>6} {:>10} {:>10} {:>10} {:>10} {:>14}",
            m.name,
            m.nodes,
            m.edges,
            m.crossings,
            m.max_bends,
            m.sum_bends,
            m.reversed_count,
            format!("{:.0} x {:.0}", m.bbox_w, m.bbox_h)
        );
    }

    let write_baseline = std::env::var("HIER_EVAL_WRITE_BASELINE")
        .map(|v| v == "1")
        .unwrap_or(false);
    if write_baseline {
        let mut baseline = serde_json::Map::new();
        for m in &all_metrics {
            baseline.insert(m.name.clone(), metrics_to_value(m));
        }
        let path = baseline_path();
        fs::write(&path, serde_json::to_string_pretty(&Value::Object(baseline)).unwrap())
            .unwrap_or_else(|e| panic!("writing baseline {}: {e}", path.display()));
        println!(
            "\nhier_eval: wrote baseline for {} fixtures to {}",
            all_metrics.len(),
            path.display()
        );
    } else {
        check_bend_gate(&all_metrics, &mut hard_failures);
    }

    assert!(
        hard_failures.is_empty(),
        "hard geometric invariant violations:\n{}",
        hard_failures.join("\n")
    );
}

fn check_no_node_overlaps(name: &str, result: &LayoutResult, failures: &mut Vec<String>) {
    for i in 0..result.nodes.len() {
        for j in (i + 1)..result.nodes.len() {
            let (a, b) = (&result.nodes[i], &result.nodes[j]);
            if rects_overlap(a.frame, b.frame) {
                failures.push(format!("{name}: nodes `{}` and `{}` overlap", a.id, b.id));
            }
        }
    }
}

fn check_orthogonal_and_ports(
    name: &str,
    source: &str,
    result: &LayoutResult,
    failures: &mut Vec<String>,
) {
    // Orthogonality is a property of the default/`orthogonal` ink style only;
    // fixtures that opt into `polyline` / `curved` declare it in the source
    // and legitimately emit diagonal / curved segments.
    let orthogonal_style = !source.contains("routing_style: polyline")
        && !source.contains("routing_style: curved");

    let frame_of: BTreeMap<&str, Rect> = result
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.frame))
        .collect();

    for e in &result.edges {
        let pts = e.path.samples();
        if pts.len() < 2 {
            continue; // degenerate edge type, not this layout's output shape
        }
        if orthogonal_style {
            for w in pts.windows(2) {
                let (a, b) = (w[0], w[1]);
                if (a.x - b.x).abs() > EPS && (a.y - b.y).abs() > EPS {
                    failures.push(format!(
                        "{name}: edge `{}` has a non-orthogonal segment {:?} -> {:?}",
                        e.id, a, b
                    ));
                }
            }
        }
        if let Some(&src_frame) = frame_of.get(e.source.as_str()) {
            let first = pts[0];
            if !on_boundary(first, src_frame) {
                failures.push(format!(
                    "{name}: edge `{}` start {:?} not on source `{}` boundary {:?}",
                    e.id, first, e.source, src_frame
                ));
            }
        }
        if let Some(&tgt_frame) = frame_of.get(e.target.as_str()) {
            let last = *pts.last().unwrap();
            if !on_boundary(last, tgt_frame) {
                failures.push(format!(
                    "{name}: edge `{}` end {:?} not on target `{}` boundary {:?}",
                    e.id, last, e.target, tgt_frame
                ));
            }
        }
    }
}

fn check_no_edge_node_penetration(
    name: &str,
    source: &str,
    result: &LayoutResult,
    failures: &mut Vec<String>,
) {
    // Same orthogonal-only scope as check_orthogonal_and_ports.
    if source.contains("routing_style: polyline") || source.contains("routing_style: curved") {
        return;
    }
    const INSET: f64 = 1e-3;
    let frames: Vec<(&str, Rect)> = result
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.frame))
        .collect();

    for e in &result.edges {
        let pts = e.path.samples();
        if pts.len() < 2 {
            continue;
        }
        for w in pts.windows(2) {
            let (a, b) = (w[0], w[1]);
            let ortho = (a.x - b.x).abs() <= EPS || (a.y - b.y).abs() <= EPS;
            if !ortho {
                continue;
            }
            for &(nid, frame) in &frames {
                if nid == e.source || nid == e.target {
                    continue;
                }
                if segment_hits_rect_interior(a, b, frame, INSET) {
                    failures.push(format!(
                        "{name}: edge `{}` penetrates node `{nid}`",
                        e.id
                    ));
                }
            }
        }
    }
}

fn segment_hits_rect_interior(a: Point, b: Point, frame: Rect, inset: f64) -> bool {
    let left = frame.x + inset;
    let right = frame.right() - inset;
    let top = frame.y + inset;
    let bottom = frame.bottom() - inset;
    if left >= right - EPS || top >= bottom - EPS {
        return false;
    }
    if (a.x - b.x).abs() <= EPS {
        let vx = a.x;
        if vx <= left + EPS || vx >= right - EPS {
            return false;
        }
        let y0 = a.y.min(b.y);
        let y1 = a.y.max(b.y);
        y0 < bottom - EPS && y1 > top + EPS
    } else if (a.y - b.y).abs() <= EPS {
        let hy = a.y;
        if hy <= top + EPS || hy >= bottom - EPS {
            return false;
        }
        let x0 = a.x.min(b.x);
        let x1 = a.x.max(b.x);
        x0 < right - EPS && x1 > left + EPS
    } else {
        false
    }
}

fn compute_metrics(name: &str, result: &LayoutResult, reversed_count: usize) -> FileMetrics {
    let mut crossings = 0usize;
    let mut max_bends = 0usize;
    let mut sum_bends = 0usize;

    let mut segments: Vec<(Point, Point)> = Vec::new();
    for e in &result.edges {
        let pts = e.path.samples();
        if pts.len() >= 2 {
            // A cubic Bézier has no bends (its 24 samples are one smooth
            // segment); only polyline waypoints carry bend counts.
            let bends = if e.path.polyline_points().is_some() {
                pts.len() - 2
            } else {
                0
            };
            max_bends = max_bends.max(bends);
            sum_bends += bends;
        }
        for w in pts.windows(2) {
            segments.push((w[0], w[1]));
        }
    }
    for i in 0..segments.len() {
        for j in (i + 1)..segments.len() {
            let (a0, a1) = segments[i];
            let (b0, b1) = segments[j];
            if segments_cross(a0, a1, b0, b1) {
                crossings += 1;
            }
        }
    }

    let (mut min_x, mut min_y) = (f64::INFINITY, f64::INFINITY);
    let (mut max_x, mut max_y) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    let mut grow = |p: Point| {
        min_x = min_x.min(p.x);
        min_y = min_y.min(p.y);
        max_x = max_x.max(p.x);
        max_y = max_y.max(p.y);
    };
    for n in &result.nodes {
        grow(Point {
            x: n.frame.x,
            y: n.frame.y,
        });
        grow(Point {
            x: n.frame.right(),
            y: n.frame.bottom(),
        });
    }
    for e in &result.edges {
        for p in e.path.samples() {
            grow(p);
        }
    }
    let (bbox_w, bbox_h) = if min_x.is_finite() {
        (max_x - min_x, max_y - min_y)
    } else {
        (0.0, 0.0)
    };

    FileMetrics {
        name: name.to_string(),
        nodes: result.nodes.len(),
        edges: result.edges.len(),
        crossings,
        max_bends,
        sum_bends,
        reversed_count,
        bbox_w,
        bbox_h,
    }
}

/// Bend regression gate against `tests/hier_eval_baseline.json`: per-fixture
/// `max_bends` / `sum_bends` / `reversed_count` must not exceed the baseline
/// (hard failure). Crossings and canvas bbox are observational: printed as
/// deltas only, since a dense graph legitimately crosses and straightening
/// may widen the canvas.
fn check_bend_gate(metrics: &[FileMetrics], failures: &mut Vec<String>) {
    let path = baseline_path();
    let raw = fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "missing bend baseline {}: {e} — regenerate with \
             HIER_EVAL_WRITE_BASELINE=1 cargo test -p plotgram-compile --test hier_eval",
            path.display()
        )
    });
    let baseline: Value = serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("corrupt bend baseline {}: {e}", path.display()));

    println!(
        "\n{:<45} {:>14} {:>14} {:>12} {:>12} {:>16}",
        "fixture", "d max_bends", "d sum_bends", "d crossings", "d reversed", "d bbox (w x h)"
    );
    for m in metrics {
        let Some(entry) = baseline.get(&m.name) else {
            failures.push(format!(
                "{}: missing from bend baseline — regenerate it",
                m.name
            ));
            continue;
        };
        let base_max = entry["max_bends"].as_u64().unwrap_or(0) as usize;
        let base_sum = entry["sum_bends"].as_u64().unwrap_or(0) as usize;
        let base_cross = entry["crossings"].as_u64().unwrap_or(0) as isize;
        let base_rev = entry["reversed_count"].as_u64().unwrap_or(0) as usize;
        let base_w = entry["bbox_w"].as_f64().unwrap_or(0.0);
        let base_h = entry["bbox_h"].as_f64().unwrap_or(0.0);

        if m.max_bends > base_max {
            failures.push(format!(
                "{}: bend regression — max_bends {} > baseline {}",
                m.name, m.max_bends, base_max
            ));
        }
        if m.sum_bends > base_sum {
            failures.push(format!(
                "{}: bend regression — sum_bends {} > baseline {}",
                m.name, m.sum_bends, base_sum
            ));
        }
        if m.reversed_count > base_rev {
            failures.push(format!(
                "{}: FAS regression — reversed_count {} > baseline {}",
                m.name, m.reversed_count, base_rev
            ));
        }
        println!(
            "{:<45} {:>+14} {:>+14} {:>+12} {:>+12} {:>+16}",
            m.name,
            m.max_bends as isize - base_max as isize,
            m.sum_bends as isize - base_sum as isize,
            m.crossings as isize - base_cross,
            m.reversed_count as isize - base_rev as isize,
            format!(
                "{:+.0} x {:+.0}",
                m.bbox_w - base_w,
                m.bbox_h - base_h
            )
        );
    }
}

/// SymmetryAxis D2: main-chain centers collinear (no staircase fold).
#[test]
fn symmetry_axis_d2_representative_spines_collinear() {
    // (relative path under showcase/hierarchical, spine node ids in TB order)
    const CASES: &[(&str, &[&str])] = &[
        ("flat/product.flat-rest-api.pgm", &["web", "lb", "api"]),
        ("flat/mech.constrain-flat-chain.pgm", &["gw", "api", "worker"]),
        ("flat/product.order-approval.pgm", &["submit", "review", "check"]),
    ];
    let dir = showcase_dir();
    let mut failures = Vec::new();
    for &(rel, spine) in CASES {
        let path = dir.join(rel);
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let result = build_layout(&source, &BuildOptions::default())
            .unwrap_or_else(|e| panic!("{rel}: layout error: {e}"));
        let by_id: BTreeMap<&str, &plotgram_model::result::NodePlacement> =
            result.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
        let mut xs = Vec::with_capacity(spine.len());
        for &id in spine {
            let Some(n) = by_id.get(id) else {
                failures.push(format!("{rel}: missing spine node `{id}`"));
                continue;
            };
            xs.push(n.frame.x + n.frame.width / 2.0);
        }
        if xs.len() != spine.len() {
            continue;
        }
        let spread = xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
            - xs.iter().cloned().fold(f64::INFINITY, f64::min);
        if spread > 1e-3 {
            failures.push(format!(
                "{rel}: spine {spine:?} cross-centers not collinear (spread={spread:.4}, xs={xs:?})"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "SymmetryAxis D2 spine gate:\n{}",
        failures.join("\n")
    );
}
