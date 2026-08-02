//! Hierarchical layout regression/scoring check.
//!
//! Runs the full pipeline (parse → measure → `hierarchical` layout) over
//! every fixture in `showcase/hierarchical/` and asserts hard geometric
//! invariants the layout must never violate, independent of visual
//! inspection:
//!
//! - no two node frames overlap;
//! - every edge path segment is axis-aligned (orthogonal);
//! - every edge path's first/last point lands exactly on its source/target
//!   node frame boundary (the port anchor, not just "near" it);
//! - a self-contained segment-intersection crossing count and max-bend count
//!   are printed per file as an informational regression signal (not a hard
//!   failure — a dense graph legitimately has crossings).
//!
//! This is a coarse "does the geometry make sense" check, not a substitute
//! for visual review (see `docs/design/layout/hierarchical/notes/`).

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use plotgram_model::geometry::{Point, Rect};
use plotgram_model::result::LayoutResult;
use plotgram_pipeline::{compile_layout, PipelineOptions};

const EPS: f64 = 1e-6;

fn showcase_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("showcase/hierarchical")
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
}

#[test]
fn hierarchical_showcase_geometry_invariants() {
    let dir = showcase_dir();
    let mut entries: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read_dir {dir:?}: {e}"))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("pgm"))
        .collect();
    entries.sort();
    assert!(
        !entries.is_empty(),
        "expected showcase/hierarchical/*.pgm fixtures"
    );

    let mut all_metrics = Vec::new();
    let mut hard_failures: Vec<String> = Vec::new();

    for path in &entries {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let source = fs::read_to_string(path).unwrap();
        let result = match compile_layout(&source, &PipelineOptions::default()) {
            Ok(r) => r,
            Err(e) => {
                hard_failures.push(format!("{name}: pipeline error: {e}"));
                continue;
            }
        };

        check_no_node_overlaps(&name, &result, &mut hard_failures);
        check_orthogonal_and_ports(&name, &result, &mut hard_failures);
        all_metrics.push(compute_metrics(&name, &result));
    }

    println!(
        "\n{:<45} {:>6} {:>6} {:>10} {:>10}",
        "fixture", "nodes", "edges", "crossings", "max_bends"
    );
    for m in &all_metrics {
        println!(
            "{:<45} {:>6} {:>6} {:>10} {:>10}",
            m.name, m.nodes, m.edges, m.crossings, m.max_bends
        );
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

fn check_orthogonal_and_ports(name: &str, result: &LayoutResult, failures: &mut Vec<String>) {
    let frame_of: BTreeMap<&str, Rect> = result
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.frame))
        .collect();

    for e in &result.edges {
        let pts = &e.path.points;
        if pts.len() < 2 {
            continue; // degenerate edge type, not this layout's output shape
        }
        for w in pts.windows(2) {
            let (a, b) = (w[0], w[1]);
            if (a.x - b.x).abs() > EPS && (a.y - b.y).abs() > EPS {
                failures.push(format!(
                    "{name}: edge `{}` has a non-orthogonal segment {:?} -> {:?}",
                    e.id, a, b
                ));
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

fn compute_metrics(name: &str, result: &LayoutResult) -> FileMetrics {
    let mut crossings = 0usize;
    let mut max_bends = 0usize;

    let mut segments: Vec<(Point, Point)> = Vec::new();
    for e in &result.edges {
        let pts = &e.path.points;
        if pts.len() >= 2 {
            max_bends = max_bends.max(pts.len() - 2);
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

    FileMetrics {
        name: name.to_string(),
        nodes: result.nodes.len(),
        edges: result.edges.len(),
        crossings,
        max_bends,
    }
}
