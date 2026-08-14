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
//! - group gates (strong-macro.md §6 SM-4): nested group frames stay inside
//!   their parent (hard, all grouped fixtures); unrelated sibling frames
//!   sharing a main-axis band keep a gap ≥ GROUP_FRAME_GAP (hard, D₂.1);
//!   and `group_policy:
//!   strong-macro` fixtures must have zero edge/group penetration (weak
//!   fixtures print the count as an observation);
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
//! swapping the cut edge of a cycle never adds reversals.
//!
//! P1 observation metrics (`symmetry_deviation_*`, `gap_uniformity_max`,
//! `straightness`, `overlap_len`, `channel_used_gates`, `relaxations`,
//! `ripup_rounds`) are recorded in the same baseline. `channel_used_gates`
//! is a **hard gate**: a fixture that was `true` must not regress to `false`.
//! Regenerate the baseline only when a change is *intended*:
//! `HIER_EVAL_WRITE_BASELINE=1 cargo test -p plotgram-compile --test hier_eval`.
//!
//! SymmetryAxis D2/D3 gates ([phases/symmetry-axis.md](../../../docs/design/layout/hierarchical/phases/symmetry-axis.md)):
//! D2 — three representative spines collinear (`flat-rest-api`,
//! `constrain-flat-chain`, `order-approval`); D3 FanPack —
//! `smoke.multi-rank-backedge` hub on final fan midpoint with mirrored leaves;
//! twin spine — `mech.constrain-sink` twin on hub column, free sink offset;
//! PortLane face order — sink exit outside twin block on hub South.
//! Diamond capacity: gateway fan-out sources stay on South
//! (`smoke.flat-gateway-fanout`).
//! TrackOrder Cross nest: `smoke.fan-out-four` outer/inner horizontals
//! share rails symmetrically (no right-half cross).
//! Side-corridor polarity: `product.ticket-triage` escalate→handle both West.
//! Upstream axis inheritance: `ticket-triage` resolve_gate shares handle cx.
//! Primary arm on spine: `order-approval` finance under check; approved left;
//! rejected→submit same-face East corridor (no overshoot past submit East);
//! check→approved stays near left leaf (not canvas x≈0).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use plotgram_compile::{
    build_debug_trace, build_layout, compute_hier_metrics, node_gap_from_source, BuildOptions,
    CrossAxis,
};
use plotgram_layout::{group_penetration_violations, GROUP_FRAME_GAP, PARTITION_EMPTY_BAND_MIN};
use plotgram_layout::layout::hierarchical::GROUP_PAD;
use plotgram_model::geometry::{Point, Rect};
use plotgram_model::graph::{Edge as ModelEdge, Group as ModelGroup};
use plotgram_model::port::Side;
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
    symmetry_deviation_max: f64,
    symmetry_deviation_sum: f64,
    gap_uniformity_max: f64,
    straightness: f64,
    overlap_len: f64,
    channel_used_gates: bool,
    relaxations: usize,
    ripup_rounds: u32,
    gate_fallback_events: usize,
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
        "symmetry_deviation_max": m.symmetry_deviation_max,
        "symmetry_deviation_sum": m.symmetry_deviation_sum,
        "gap_uniformity_max": m.gap_uniformity_max,
        "straightness": m.straightness,
        "overlap_len": m.overlap_len,
        "channel_used_gates": m.channel_used_gates,
        "relaxations": m.relaxations,
        "ripup_rounds": m.ripup_rounds,
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
        check_group_gates(&name, &source, &result, &mut hard_failures);
        check_demand_floor(&name, &result, &mut hard_failures);
        check_partition_band_separation(&name, &source, &result, &mut hard_failures);
        check_partition_bit_identical(&name, &source, &mut hard_failures);
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
        all_metrics.push(compute_metrics(&name, &source, &result, reversed_count));
        // D₂.2b §8.12 observability: every gate-fallback event stays loud
        // (per-edge widening trial, derive-level impure rects, whole-diagram
        // gate-route fallback all record under this rule).
        let fb = all_metrics.last().map(|m| m.gate_fallback_events).unwrap_or(0);
        if fb > 0 {
            println!("{name}: [fallback] {fb} channel-group-fallback event(s)");
            for r in &result.diagnostics.relaxations {
                if r.rule == "channel-group-fallback" {
                    println!("{name}:   fallback: {}", r.detail);
                }
            }
        }
        if let Some(obs) = result.diagnostics.hierarchical.as_ref() {
            if obs.gate_capacity_seams > 0 {
                println!(
                    "{name}: [gate-capacity] published on {} seam(s)",
                    obs.gate_capacity_seams
                );
            }
        }
    }

    println!(
        "\n{:<45} {:>6} {:>6} {:>10} {:>10} {:>10} {:>6} {:>6} {:>4} {:>8} {:>14}",
        "fixture",
        "nodes",
        "edges",
        "crossings",
        "max_bends",
        "sum_bends",
        "gates",
        "relax",
        "fb",
        "sym_max",
        "bbox (w x h)"
    );
    for m in &all_metrics {
        println!(
            "{:<45} {:>6} {:>6} {:>10} {:>10} {:>10} {:>6} {:>6} {:>4} {:>8.2} {:>14}",
            m.name,
            m.nodes,
            m.edges,
            m.crossings,
            m.max_bends,
            m.sum_bends,
            m.channel_used_gates as u8,
            m.relaxations,
            m.gate_fallback_events,
            m.symmetry_deviation_max,
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

/// SM-4 group gates (strong-macro.md §6):
/// - containment (hard, every grouped fixture): nested group ⊆ parent **and**
///   each direct member node ⊆ its host group frame (D₂.0 Metric / MacroBlock
///   writers); the gate is a regression guardrail;
/// - sibling separation (hard, D₂.1): unrelated frames sharing a main-axis
///   band keep a cross-axis gap ≥ GROUP_FRAME_GAP (written by the VPSC hard
///   set since D₂.1 moved sibling push-apart into the solve);
/// - penetration (hard, D₂.2a): any edge segment entering a group frame
///   outside its endpoints' group ancestor chains fails — strong-macro and
///   weak alike. A fixture may carry a source annotation
///   `// d2-exempt: group-penetration — <reason>` (class-C registry,
///   group-frame-d2.md §8.10) to downgrade to observation; exemptions are
///   never silent — the count and details stay printed.
fn check_group_gates(
    name: &str,
    source: &str,
    result: &LayoutResult,
    failures: &mut Vec<String>,
) {
    if result.groups.is_empty() {
        return;
    }
    let Ok(parsed) = plotgram_parse::parse(source) else {
        return;
    };
    let graph = &parsed.graph;

    let frame_of: BTreeMap<&str, Rect> = result
        .groups
        .iter()
        .map(|g| (g.id.as_str(), g.frame))
        .collect();

    // --- containment (hard): nested group ⊆ parent; member node ⊆ host ----
    group_containment(&graph.groups, &frame_of, name, failures);
    member_containment(&graph.groups, result, &frame_of, name, failures);

    // --- sibling separation: unrelated frames sharing a main-axis band ----
    sibling_separation(&graph.groups, &frame_of, source, name, failures);

    // --- penetration: endpoint ancestor chains whitelist ------------------
    let mut chain_of: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    collect_group_chains(&graph.groups, &mut BTreeSet::new(), &mut chain_of);
    let mut all_edges: Vec<&ModelEdge> = graph.edges.iter().collect();
    collect_group_edges(&graph.groups, &mut all_edges);
    let mut allowed: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for e in all_edges {
        let mut set = BTreeSet::new();
        if let Some(c) = chain_of.get(&e.source) {
            set.extend(c.iter().cloned());
        }
        if let Some(c) = chain_of.get(&e.target) {
            set.extend(c.iter().cloned());
        }
        allowed.insert(e.id.clone(), set);
    }
    let violations = group_penetration_violations(&result.edges, &result.groups, &allowed);
    if violations.is_empty() {
        return;
    }
    if source.contains("group_policy: strong-macro") {
        for v in &violations {
            failures.push(format!(
                "{name}: edge `{}` penetrates group `{}` (strong-macro gate)",
                v.edge, v.group
            ));
        }
    } else if penetration_exempt(source) {
        // D₂.2a class-C registry (group-frame-d2.md §8.10): documented
        // exemption, observation stays loud — count + details + the
        // relaxations that explain the residual routing.
        println!(
            "{name}: [exempt] {} group penetration violation(s) (D₂.2a class-C registry)",
            violations.len()
        );
        for v in &violations {
            println!("{name}:   pen: edge `{}` -> group `{}`", v.edge, v.group);
        }
        for r in &result.diagnostics.relaxations {
            println!("{name}:   relax: [{}] {}", r.rule, r.detail);
        }
    } else {
        for v in &violations {
            failures.push(format!(
                "{name}: edge `{}` penetrates group `{}` (weak gate; \
                 class-C exemption via `// d2-exempt: group-penetration`) ",
                v.edge, v.group
            ));
        }
    }
}

/// D₂.2a exemption marker (group-frame-d2.md §8.10): a fixture source
/// comment downgrades the weak penetration gate to observation. Only
/// class-C registry entries may carry it; each must state the reason.
fn penetration_exempt(source: &str) -> bool {
    source.contains("d2-exempt: group-penetration")
}

/// D₂.2b MetricVerifier demand floor (coordinate-and-demand.md §9,
/// group-frame-d2.md §8.13): every published LayerGap demand must be met by
/// the resolved gap — the resolution is max-based by construction, so this
/// gate guards against future producers bypassing the DemandBoard.
fn check_demand_floor(name: &str, result: &LayoutResult, failures: &mut Vec<String>) {
    let Some(obs) = result.diagnostics.hierarchical.as_ref() else {
        return;
    };
    for (&seam, &demand) in &obs.layer_gap_demands {
        let got = obs.layer_gaps.get(seam as usize).copied().unwrap_or(0.0);
        if got + EPS < demand {
            failures.push(format!(
                "{name}: demand floor violated — seam {seam} resolved {got} < demand {demand}"
            ));
        }
    }
}

/// PG-1/PG-3/PG-4 hard gate: consumed partition fixtures keep globally
/// separated column bands (physical x, declaration order) and, when rows
/// are consumed, row bands (physical y). Unassigned nodes are free.
fn check_partition_band_separation(
    name: &str,
    source: &str,
    result: &LayoutResult,
    failures: &mut Vec<String>,
) {
    let Ok(parsed) = plotgram_parse::parse(source) else {
        return;
    };
    let graph = &parsed.graph;
    let Some(grid) = &graph.partition else {
        return;
    };
    if grid.columns.is_empty() && grid.rows.is_empty() {
        return;
    }
    // StrongMacro writes node frames via MacroBlockWriter; column bands are
    // Fit envelopes after expand and may stack (not side-by-side). Membership
    // / separation are Weak+partition invariants (partition-grid.md PG-4).
    if source.contains("group_policy: strong-macro") {
        return;
    }

    let col_index: BTreeMap<&str, usize> = grid
        .columns
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id.as_str(), i))
        .collect();
    let row_index: BTreeMap<&str, usize> = grid
        .rows
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id.as_str(), i))
        .collect();
    let frame_of: BTreeMap<&str, Rect> = result
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.frame))
        .collect();

    let n_cols = grid.columns.len();
    let mut min_left = vec![f64::INFINITY; n_cols];
    let mut max_right = vec![f64::NEG_INFINITY; n_cols];
    for node in graph.all_nodes() {
        let Some(cell) = &node.partition_cell else {
            continue;
        };
        let Some(col) = &cell.column else {
            continue;
        };
        let Some(&ci) = col_index.get(col.as_str()) else {
            continue;
        };
        let Some(&frame) = frame_of.get(node.id.as_str()) else {
            continue;
        };
        min_left[ci] = min_left[ci].min(frame.x);
        max_right[ci] = max_right[ci].max(frame.right());
    }
    for i in 0..n_cols {
        if !max_right[i].is_finite() {
            continue;
        }
        for j in i + 1..n_cols {
            if !min_left[j].is_finite() {
                continue;
            }
            if max_right[i] + EPS > min_left[j] {
                failures.push(format!(
                    "{name}: partition column separation violated — column `{}` right \
                     {:.3} exceeds column `{}` left {:.3}",
                    grid.columns[i].id, max_right[i], grid.columns[j].id, min_left[j]
                ));
            }
        }
    }

    let bands = result
        .diagnostics
        .hierarchical
        .as_ref()
        .map(|o| o.partition_bands.clone())
        .unwrap_or_default();
    if n_cols > 0 {
        if bands.len() != n_cols {
            failures.push(format!(
                "{name}: partition obs bands count {} != declared columns {n_cols}",
                bands.len()
            ));
        } else {
            for (ci, band) in bands.iter().enumerate() {
                if band.column != grid.columns[ci].id {
                    failures.push(format!(
                        "{name}: partition obs band {ci} column `{}` != declared `{}`",
                        band.column, grid.columns[ci].id
                    ));
                }
                if band.empty && band.end - band.start + EPS < PARTITION_EMPTY_BAND_MIN {
                    failures.push(format!(
                        "{name}: partition obs band `{}` width {:.3} < empty-band minimum {:.1}",
                        band.column,
                        band.end - band.start,
                        PARTITION_EMPTY_BAND_MIN
                    ));
                }
            }
            for node in graph.all_nodes() {
                let Some(cell) = &node.partition_cell else {
                    continue;
                };
                let Some(col) = &cell.column else {
                    continue;
                };
                let Some(&ci) = col_index.get(col.as_str()) else {
                    continue;
                };
                let Some(&frame) = frame_of.get(node.id.as_str()) else {
                    continue;
                };
                let band = &bands[ci];
                if frame.x + GROUP_PAD + EPS < band.start || frame.right() - GROUP_PAD - EPS > band.end {
                    failures.push(format!(
                        "{name}: node `{}` frame [{:.3}, {:.3}] escapes partition band `{}` [{:.3}, {:.3}]",
                        node.id,
                        frame.x,
                        frame.right(),
                        band.column,
                        band.start,
                        band.end
                    ));
                }
            }
            for i in 0..n_cols {
                for j in i + 1..n_cols {
                    if bands[i].end + EPS > bands[j].start {
                        failures.push(format!(
                            "{name}: partition obs band separation violated — `{}` end {:.3} \
                             exceeds `{}` start {:.3}",
                            bands[i].column, bands[i].end, bands[j].column, bands[j].start
                        ));
                    }
                }
            }
        }
    }

    let n_rows = grid.rows.len();
    let row_bands = result
        .diagnostics
        .hierarchical
        .as_ref()
        .map(|o| o.partition_row_bands.clone())
        .unwrap_or_default();
    if n_rows == 0 {
        return;
    }
    if row_bands.len() != n_rows {
        failures.push(format!(
            "{name}: partition obs row bands count {} != declared rows {n_rows}",
            row_bands.len()
        ));
        return;
    }
    for (ri, band) in row_bands.iter().enumerate() {
        if band.row != grid.rows[ri].id {
            failures.push(format!(
                "{name}: partition obs row band {ri} row `{}` != declared `{}`",
                band.row, grid.rows[ri].id
            ));
        }
        if band.empty && band.end - band.start + EPS < PARTITION_EMPTY_BAND_MIN {
            failures.push(format!(
                "{name}: partition obs row band `{}` height {:.3} < empty-band minimum {:.1}",
                band.row,
                band.end - band.start,
                PARTITION_EMPTY_BAND_MIN
            ));
        }
    }
    for node in graph.all_nodes() {
        let Some(cell) = &node.partition_cell else {
            continue;
        };
        let Some(row) = &cell.row else {
            continue;
        };
        let Some(&ri) = row_index.get(row.as_str()) else {
            continue;
        };
        let Some(&frame) = frame_of.get(node.id.as_str()) else {
            continue;
        };
        let band = &row_bands[ri];
        if frame.y + GROUP_PAD + EPS < band.start || frame.bottom() - GROUP_PAD - EPS > band.end {
            failures.push(format!(
                "{name}: node `{}` frame y [{:.3}, {:.3}] escapes partition row band `{}` [{:.3}, {:.3}]",
                node.id,
                frame.y,
                frame.bottom(),
                band.row,
                band.start,
                band.end
            ));
        }
    }
    for i in 0..n_rows {
        for j in i + 1..n_rows {
            if row_bands[i].end + EPS > row_bands[j].start {
                failures.push(format!(
                    "{name}: partition obs row band separation violated — `{}` end {:.3} \
                     exceeds `{}` start {:.3}",
                    row_bands[i].row, row_bands[i].end, row_bands[j].row, row_bands[j].start
                ));
            }
        }
    }
}

/// PG-1 determinism gate: partition fixtures run twice produce bit-identical
/// geometry (node frames and edge path samples).
fn check_partition_bit_identical(name: &str, source: &str, failures: &mut Vec<String>) {
    let Ok(parsed) = plotgram_parse::parse(source) else {
        return;
    };
    let Some(grid) = &parsed.graph.partition else {
        return;
    };
    if grid.columns.is_empty() && grid.rows.is_empty() {
        return;
    }
    let (first, second) = match (
        build_layout(source, &BuildOptions::default()),
        build_layout(source, &BuildOptions::default()),
    ) {
        (Ok(a), Ok(b)) => (a, b),
        _ => return, // pipeline errors are reported by the main loop
    };
    let frames = |r: &LayoutResult| -> Vec<(String, Rect)> {
        let mut v: Vec<(String, Rect)> = r.nodes.iter().map(|n| (n.id.clone(), n.frame)).collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    };
    if frames(&first) != frames(&second) {
        failures.push(format!("{name}: partition layout not bit-identical across two runs"));
    }
    let paths = |r: &LayoutResult| -> Vec<(String, Vec<Point>)> {
        let mut v: Vec<(String, Vec<Point>)> = r
            .edges
            .iter()
            .map(|e| (e.id.clone(), e.path.samples().to_vec()))
            .collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    };
    if paths(&first) != paths(&second) {
        failures.push(format!(
            "{name}: partition edge paths not bit-identical across two runs"
        ));
    }
}

/// D₂.2a canvas-bloat guard (group-frame-d2.md §8.10: no fake clearance by
/// enlarging frames): a weak fixture bbox dimension beyond +10% of the
/// checked-in baseline fails.
fn bbox_over_guard(w: f64, h: f64, base_w: f64, base_h: f64) -> bool {
    const GUARD: f64 = 1.10;
    w > base_w * GUARD + EPS || h > base_h * GUARD + EPS
}

/// D₂.1 sibling separation gate (group-frame-d2.md §8.9): unrelated group
/// frames whose main-axis projections intersect must keep a cross-axis gap
/// ≥ `GROUP_FRAME_GAP` — the cross-axis VPSC hard set writes that degree of
/// freedom since D₂.1 moved sibling push-apart into the solve. Stacked siblings on
/// disjoint main-axis bands are exempt (their main-axis gap is
/// `layer_gap − 2×GROUP_PAD`, intentionally tighter).
fn sibling_separation(
    groups: &[ModelGroup],
    frame_of: &BTreeMap<&str, Rect>,
    source: &str,
    name: &str,
    failures: &mut Vec<String>,
) {
    // All showcase fixtures are top-to-bottom; scan for LR aliases anyway.
    let lr = source.contains("left-to-right") || source.contains("ltr");

    // Flatten to (id, frame, strict ancestor ids).
    let mut flat: Vec<(String, Rect, BTreeSet<String>)> = Vec::new();
    fn flatten(
        groups: &[ModelGroup],
        ancestors: &mut BTreeSet<String>,
        frame_of: &BTreeMap<&str, Rect>,
        flat: &mut Vec<(String, Rect, BTreeSet<String>)>,
    ) {
        for g in groups {
            if let Some(&frame) = frame_of.get(g.id.as_str()) {
                flat.push((g.id.clone(), frame, ancestors.clone()));
            }
            ancestors.insert(g.id.clone());
            flatten(&g.groups, ancestors, frame_of, flat);
            ancestors.remove(&g.id);
        }
    }
    flatten(groups, &mut BTreeSet::new(), frame_of, &mut flat);

    let main_band = |r: Rect| (if lr { r.x } else { r.y }, if lr { r.right() } else { r.bottom() });
    let cross_edge = |r: Rect| (if lr { r.y } else { r.x }, if lr { r.bottom() } else { r.right() });

    let mut checked = 0usize;
    for i in 0..flat.len() {
        for j in (i + 1)..flat.len() {
            let (id_a, a, anc_a) = &flat[i];
            let (id_b, b, anc_b) = &flat[j];
            if anc_a.contains(id_b) || anc_b.contains(id_a) {
                continue; // nested pair: containment, not sibling
            }
            let (a_main_lo, a_main_hi) = main_band(*a);
            let (b_main_lo, b_main_hi) = main_band(*b);
            if b_main_lo >= a_main_hi - EPS || a_main_lo >= b_main_hi - EPS {
                continue; // disjoint main-axis bands
            }
            checked += 1;
            let (a_cross_lo, a_cross_hi) = cross_edge(*a);
            let (b_cross_lo, b_cross_hi) = cross_edge(*b);
            let gap = if b_cross_lo >= a_cross_hi {
                b_cross_lo - a_cross_hi
            } else if a_cross_lo >= b_cross_hi {
                a_cross_lo - b_cross_hi
            } else {
                -1.0 // cross-axis projections overlap: illegal sibling overlap
            };
            if gap < GROUP_FRAME_GAP - EPS {
                failures.push(format!(
                    "{name}: sibling groups `{id_a}` / `{id_b}` frames closer than \
                     GROUP_FRAME_GAP (gap {gap:.3})"
                ));
            }
        }
    }
    if checked > 0 {
        println!("{name}: [observed] {checked} sibling separation pair(s) checked");
    }
}

fn group_containment(
    groups: &[ModelGroup],
    frame_of: &BTreeMap<&str, Rect>,
    name: &str,
    failures: &mut Vec<String>,
) {
    for g in groups {
        if let Some(&parent) = frame_of.get(g.id.as_str()) {
            for c in &g.groups {
                if let Some(&child) = frame_of.get(c.id.as_str()) {
                    if child.x < parent.x - EPS
                        || child.right() > parent.right() + EPS
                        || child.y < parent.y - EPS
                        || child.bottom() > parent.bottom() + EPS
                    {
                        failures.push(format!(
                            "{name}: group `{}` frame escapes parent `{}`",
                            c.id, g.id
                        ));
                    }
                }
            }
        }
        group_containment(&g.groups, frame_of, name, failures);
    }
}

/// D₂.0 containment (group-frame-d2.md §8.5): each direct member node frame
/// must sit inside its host group's frame (pad already baked into the frame).
fn member_containment(
    groups: &[ModelGroup],
    result: &LayoutResult,
    frame_of: &BTreeMap<&str, Rect>,
    name: &str,
    failures: &mut Vec<String>,
) {
    let node_of: BTreeMap<&str, Rect> = result
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.frame))
        .collect();
    fn walk(
        groups: &[ModelGroup],
        node_of: &BTreeMap<&str, Rect>,
        frame_of: &BTreeMap<&str, Rect>,
        name: &str,
        failures: &mut Vec<String>,
    ) {
        for g in groups {
            if let Some(&gf) = frame_of.get(g.id.as_str()) {
                for n in &g.nodes {
                    let Some(&nf) = node_of.get(n.id.as_str()) else {
                        continue;
                    };
                    if nf.x < gf.x - EPS
                        || nf.right() > gf.right() + EPS
                        || nf.y < gf.y - EPS
                        || nf.bottom() > gf.bottom() + EPS
                    {
                        failures.push(format!(
                            "{name}: node `{}` escapes host group `{}`",
                            n.id, g.id
                        ));
                    }
                }
            }
            walk(&g.groups, node_of, frame_of, name, failures);
        }
    }
    walk(groups, &node_of, frame_of, name, failures);
}

/// `node_id → group ancestor chain` (inclusive of the host group).
fn collect_group_chains(
    groups: &[ModelGroup],
    chain: &mut BTreeSet<String>,
    chain_of: &mut BTreeMap<String, BTreeSet<String>>,
) {
    for g in groups {
        chain.insert(g.id.clone());
        for n in &g.nodes {
            chain_of.insert(n.id.clone(), chain.clone());
        }
        collect_group_chains(&g.groups, chain, chain_of);
        chain.remove(&g.id);
    }
}

fn collect_group_edges<'a>(groups: &'a [ModelGroup], out: &mut Vec<&'a ModelEdge>) {
    for g in groups {
        out.extend(g.edges.iter());
        collect_group_edges(&g.groups, out);
    }
}

fn compute_metrics(
    name: &str,
    source: &str,
    result: &LayoutResult,
    reversed_count: usize,
) -> FileMetrics {
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

    let hq = compute_hier_metrics(
        result,
        CrossAxis::from_source(source),
        node_gap_from_source(source),
    );

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
        symmetry_deviation_max: hq.symmetry_deviation_max,
        symmetry_deviation_sum: hq.symmetry_deviation_sum,
        gap_uniformity_max: hq.gap_uniformity_max,
        straightness: hq.straightness,
        overlap_len: hq.overlap_len,
        channel_used_gates: hq.channel_used_gates,
        relaxations: hq.relaxations,
        ripup_rounds: hq.ripup_rounds,
        gate_fallback_events: hq.gate_fallback_events,
    }
}

/// Bend + P1 observation regression gate against `tests/hier_eval_baseline.json`.
/// Hard failures: `max_bends` / `sum_bends` / `reversed_count` must not exceed
/// baseline; `channel_used_gates` must not regress from true → false; weak
/// fixtures keep their bbox within +10% of baseline (D₂.2a canvas-bloat
/// guard). Other P1 fields and crossings are observational (printed deltas).
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
        "\n{:<45} {:>14} {:>14} {:>12} {:>12} {:>8} {:>8} {:>16}",
        "fixture",
        "d max_bends",
        "d sum_bends",
        "d crossings",
        "d reversed",
        "d gates",
        "d relax",
        "d bbox (w x h)"
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
        let base_gates = entry["channel_used_gates"].as_bool().unwrap_or(false);
        let base_relax = entry["relaxations"].as_u64().unwrap_or(0) as isize;

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
        if base_gates && !m.channel_used_gates {
            failures.push(format!(
                "{}: channel_used_gates regression — baseline d1.3-gate fell back to root-scope",
                m.name
            ));
        }
        // D₂.2a canvas-bloat guard (group-frame-d2.md §8.10): penetration
        // clearance must not be bought by enlarging the canvas.
        if m.name.starts_with("group-weak/")
            && bbox_over_guard(m.bbox_w, m.bbox_h, base_w, base_h)
        {
            failures.push(format!(
                "{}: canvas-bloat guard — bbox {:.0} x {:.0} exceeds baseline \
                 {:.0} x {:.0} by more than 10%",
                m.name, m.bbox_w, m.bbox_h, base_w, base_h
            ));
        }
        println!(
            "{:<45} {:>+14} {:>+14} {:>+12} {:>+12} {:>+8} {:>+8} {:>+16}",
            m.name,
            m.max_bends as isize - base_max as isize,
            m.sum_bends as isize - base_sum as isize,
            m.crossings as isize - base_cross,
            m.reversed_count as isize - base_rev as isize,
            m.channel_used_gates as isize - base_gates as isize,
            m.relaxations as isize - base_relax,
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

/// SymmetryAxis D3 FanPack: hub on final fan midpoint; leaves mirrored about hub.
#[test]
fn symmetry_axis_d3_fan_pack_multi_rank_backedge() {
    let path = showcase_dir().join("flat/smoke.multi-rank-backedge.pgm");
    let source = fs::read_to_string(&path).expect("multi-rank-backedge fixture");
    let result = build_layout(&source, &BuildOptions::default()).expect("layout");
    let cx = |id: &str| -> f64 {
        let n = result
            .nodes
            .iter()
            .find(|n| n.id == id)
            .unwrap_or_else(|| panic!("missing node {id}"));
        n.frame.x + n.frame.width / 2.0
    };
    let top = cx("top");
    let mid = cx("mid");
    let bot = cx("bot");
    let side = cx("side");
    let fan_mid = (side + mid) / 2.0;
    let mut failures = Vec::new();
    for (id, x) in [("top", top), ("bot", bot)] {
        let drift = (x - fan_mid).abs();
        if drift > 1.0 {
            failures.push(format!(
                "{id} not on final fan midpoint (x={x:.3}, fan_mid={fan_mid:.3}, drift={drift:.3})"
            ));
        }
    }
    let dx_side = side - top;
    let dx_mid = mid - top;
    if dx_side * dx_mid >= 0.0 {
        failures.push(format!(
            "leaves not on opposite sides of hub: side_dx={dx_side:.3}, mid_dx={dx_mid:.3}"
        ));
    } else {
        let ratio = dx_side.abs() / dx_mid.abs();
        if !(0.5..=2.0).contains(&ratio) {
            failures.push(format!(
                "leaf distance ratio vs hub out of [0.5, 2.0]: ratio={ratio:.3} (side={dx_side:.3}, mid={dx_mid:.3})"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "SymmetryAxis D3 FanPack gate:\n{}",
        failures.join("\n")
    );
}

/// Twin spine privilege: 2-cycle peer stays on hub column; free sink offsets
/// (yFiles / expectations §6.1 — not even-fan mirrored off the return path).
#[test]
fn twin_spine_constrain_sink_side_on_axis() {
    let path = showcase_dir().join("flat/mech.constrain-sink.pgm");
    let source = fs::read_to_string(&path).expect("constrain-sink fixture");
    let result = build_layout(&source, &BuildOptions::default()).expect("layout");
    let cx = |id: &str| -> f64 {
        let n = result
            .nodes
            .iter()
            .find(|n| n.id == id)
            .unwrap_or_else(|| panic!("missing node {id}"));
        n.frame.x + n.frame.width / 2.0
    };
    let a = cx("a");
    let side = cx("side");
    let end = cx("end");
    let mut failures = Vec::new();
    let twin_drift = (side - a).abs();
    if twin_drift > 1.0 {
        failures.push(format!(
            "twin side not on hub column (a={a:.3}, side={side:.3}, drift={twin_drift:.3})"
        ));
    }
    let sink_dx = (end - a).abs();
    if sink_dx < 8.0 {
        failures.push(format!(
            "offset sink end should leave the spine (a={a:.3}, end={end:.3}, |dx|={sink_dx:.3})"
        ));
    }
    assert!(
        failures.is_empty(),
        "twin spine constrain-sink gate:\n{}",
        failures.join("\n")
    );
}

/// PortLane face order: on hub South, sink exit must sit outside the twin
/// corridor block (expectations §6.2 — no Ordered/LocalOffset sandwich).
#[test]
fn port_lane_constrain_sink_outer_leaf_outside_twin_block() {
    let path = showcase_dir().join("flat/mech.constrain-sink.pgm");
    let source = fs::read_to_string(&path).expect("constrain-sink fixture");
    let result = build_layout(&source, &BuildOptions::default()).expect("layout");
    let by_id: BTreeMap<&str, Rect> = result
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.frame))
        .collect();
    let a = by_id["a"];
    let mut twin_xs = Vec::new();
    let mut end_x = None;
    for e in &result.edges {
        let pts = e.path.samples();
        if pts.is_empty() {
            continue;
        }
        let pair = (e.source.as_str(), e.target.as_str());
        // Port on `a`'s south: first sample if a is source, last if a is target.
        let on_a = if e.source == "a" {
            pts[0]
        } else if e.target == "a" {
            *pts.last().unwrap()
        } else {
            continue;
        };
        // South face: y ≈ a.bottom()
        if (on_a.y - a.bottom()).abs() > 1.0 {
            continue;
        }
        match pair {
            ("a", "side") | ("side", "a") => twin_xs.push(on_a.x),
            ("a", "end") => end_x = Some(on_a.x),
            _ => {}
        }
    }
    assert!(
        twin_xs.len() >= 2,
        "expected both twin ports on a South, got {}",
        twin_xs.len()
    );
    let end_x = end_x.expect("a→end port on a South");
    let twin_lo = twin_xs.iter().cloned().fold(f64::INFINITY, f64::min);
    let twin_hi = twin_xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    assert!(
        end_x > twin_hi + 0.5 || end_x < twin_lo - 0.5,
        "end port must sit outside twin block: end={end_x:.3}, twin=[{twin_lo:.3},{twin_hi:.3}]"
    );
}

/// PortLane: twin N/S corridors must not invent mid-gap horizontal jogs
/// from unequal-width Ordered×width expansion (expectations §6.2).
#[test]
fn port_lane_three_tier_has_no_mid_gap_horizontal_jogs() {
    let path = showcase_dir().join("flat/product.three-tier.pgm");
    let source = fs::read_to_string(&path).expect("three-tier fixture");
    let result = build_layout(&source, &BuildOptions::default()).expect("layout");
    let by_id: BTreeMap<&str, Rect> = result
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.frame))
        .collect();
    let mut failures = Vec::new();
    for e in &result.edges {
        let pts = e.path.samples();
        if pts.len() < 2 {
            continue;
        }
        let Some(sf) = by_id.get(e.source.as_str()) else {
            continue;
        };
        let Some(tf) = by_id.get(e.target.as_str()) else {
            continue;
        };
        let (upper, lower) = if sf.y <= tf.y { (sf, tf) } else { (tf, sf) };
        let gap_top = upper.bottom();
        let gap_bot = lower.y;
        if gap_bot <= gap_top + EPS {
            continue;
        }
        for w in pts.windows(2) {
            let (a, b) = (w[0], w[1]);
            let dx = (b.x - a.x).abs();
            let dy = (b.y - a.y).abs();
            if dy < EPS && dx > EPS {
                let y = a.y;
                if y > gap_top + EPS && y < gap_bot - EPS {
                    failures.push(format!(
                        "{}->{}: mid-gap horizontal jog Δx={dx:.4} at y={y:.4}",
                        e.source, e.target
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "PortLane three-tier gate:\n{}",
        failures.join("\n")
    );
}

/// Diamond capacity soft-overflow stays on the flow face: gateway fan-out
/// sources are all South (`smoke.flat-gateway-fanout`).
#[test]
fn diamond_fanout_gateway_sources_stay_on_south() {
    let path = showcase_dir().join("flat/smoke.flat-gateway-fanout.pgm");
    let source = fs::read_to_string(&path).expect("gateway-fanout fixture");
    let result = build_layout(&source, &BuildOptions::default()).expect("layout");
    let gw = result
        .nodes
        .iter()
        .find(|n| n.id == "gw")
        .expect("gw node");
    let mut failures = Vec::new();
    let mut fan = 0usize;
    for e in &result.edges {
        if e.source != "gw" {
            continue;
        }
        fan += 1;
        let Some(fp) = e.from_port.as_ref() else {
            failures.push(format!("{}→{}: missing from_port", e.source, e.target));
            continue;
        };
        if fp.side != plotgram_model::port::Side::South {
            failures.push(format!(
                "{}→{}: expected South source, got {:?}",
                e.source, e.target, fp.side
            ));
        }
        let top = gw.frame.y;
        for p in e.path.samples() {
            if p.y < top - 1.0 {
                failures.push(format!(
                    "{}→{}: path climbs above gateway top (y={:.3} < {:.3})",
                    e.source, e.target, p.y, top
                ));
                break;
            }
        }
    }
    if fan < 2 {
        failures.push(format!("expected ≥2 gw fan-out edges, got {fan}"));
    }
    assert!(
        failures.is_empty(),
        "diamond gateway fan-out gate:\n{}",
        failures.join("\n")
    );
}

/// Longest axis-aligned horizontal segment Y (Cross rail for adjacent-layer fan).
fn primary_horiz_y(samples: &[Point]) -> Option<f64> {
    let mut best: Option<(f64, f64)> = None;
    for w in samples.windows(2) {
        let (a, b) = (w[0], w[1]);
        if (a.y - b.y).abs() >= EPS {
            continue;
        }
        let len = (a.x - b.x).abs();
        if len < EPS {
            continue;
        }
        if best.map_or(true, |(l, _)| len > l) {
            best = Some((len, a.y));
        }
    }
    best.map(|(_, y)| y)
}

/// TrackOrder outer/inner nest on `smoke.fan-out-four` (grouping off):
/// outer leaves share the upper Cross rail, inner share the lower; no fan
/// path-segment crossings.
#[test]
fn fan_out_four_cross_rails_nest_outer_inner() {
    let path = showcase_dir().join("flat/smoke.fan-out-four.pgm");
    let source = fs::read_to_string(&path).expect("fan-out-four fixture");
    let result = build_layout(&source, &BuildOptions::default()).expect("layout");

    // Nesting is read off the face itself — the port column each edge leaves
    // from — not off the `Ordered` slot, which PortLane resolves to an absolute
    // offset once the frames are known.
    let mut fan: Vec<(f64, String, f64, Vec<Point>)> = Vec::new();
    for e in &result.edges {
        if e.source != "hub" {
            continue;
        }
        let samples = e.path.samples();
        let y = primary_horiz_y(&samples).unwrap_or_else(|| {
            panic!("hub→{}: no horizontal segment", e.target);
        });
        fan.push((samples[0].x, e.target.clone(), y, samples));
    }
    assert_eq!(fan.len(), 4, "expected 4 hub fan-out edges");
    fan.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));

    let n = fan.len();
    let fan: Vec<(u32, u32, String, f64, Vec<Point>)> = fan
        .into_iter()
        .enumerate()
        .map(|(i, (_, tgt, y, s))| ((i.min(n - 1 - i)) as u32, i as u32, tgt, y, s))
        .collect();

    let mut by_nest: BTreeMap<u32, Vec<(String, f64)>> = BTreeMap::new();
    for (nest, _, tgt, y, _) in &fan {
        by_nest.entry(*nest).or_default().push((tgt.clone(), *y));
    }
    assert!(
        by_nest.contains_key(&0) && by_nest.contains_key(&1),
        "expected nest 0 and 1, got {:?}",
        by_nest.keys().collect::<Vec<_>>()
    );

    let mut failures = Vec::new();
    for (nest, members) in &by_nest {
        let y0 = members[0].1;
        for (tgt, y) in members.iter().skip(1) {
            if (y - y0).abs() > 1e-3 {
                failures.push(format!(
                    "nest {nest}: {} y={y:.3} ≠ peer y={y0:.3} (outer/inner must share rail)",
                    tgt
                ));
            }
        }
    }
    let outer_y = by_nest[&0][0].1;
    let inner_y = by_nest[&1][0].1;
    if !(outer_y < inner_y - 1e-3) {
        failures.push(format!(
            "outer rail y={outer_y:.3} must be above (smaller than) inner y={inner_y:.3}"
        ));
    }

    for i in 0..fan.len() {
        for j in i + 1..fan.len() {
            let si = &fan[i].4;
            let sj = &fan[j].4;
            for wi in si.windows(2) {
                for wj in sj.windows(2) {
                    if segments_cross(wi[0], wi[1], wj[0], wj[1]) {
                        failures.push(format!(
                            "hub→{} crosses hub→{}",
                            fan[i].2, fan[j].2
                        ));
                    }
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "fan-out-four Cross nest gate:\n{}",
        failures.join("\n")
    );
}

/// Side-corridor polarity: left-tip feedback uses West on both ends
/// (`product.ticket-triage` escalate → handle). Escalate sits left of
/// `close` on L6; within-layer rightness picks West (not raw order equality).
#[test]
fn ticket_triage_escalate_handle_side_corridor_west() {
    let path = showcase_dir().join("flat/product.ticket-triage.pgm");
    let source = fs::read_to_string(&path).expect("ticket-triage fixture");
    let result = build_layout(&source, &BuildOptions::default()).expect("layout");
    let e = result
        .edges
        .iter()
        .find(|e| e.source == "escalate" && e.target == "handle")
        .expect("escalate→handle edge");
    let from = e.from_port.as_ref().expect("from_port");
    let to = e.to_port.as_ref().expect("to_port");
    assert_eq!(
        from.side,
        Side::West,
        "escalate (left tip) must exit West, got {:?}",
        from.side
    );
    assert_eq!(
        to.side,
        Side::West,
        "handle must enter West (shared left corridor), got {:?}",
        to.side
    );
}

/// Upstream axis inheritance: `resolve_gate` stays on the same cross-axis as
/// `handle` (`product.ticket-triage`).
#[test]
fn ticket_triage_resolve_gate_centered_under_handle() {
    let path = showcase_dir().join("flat/product.ticket-triage.pgm");
    let source = fs::read_to_string(&path).expect("ticket-triage fixture");
    let result = build_layout(&source, &BuildOptions::default()).expect("layout");
    let handle = result
        .nodes
        .iter()
        .find(|n| n.id == "handle")
        .expect("handle");
    let gate = result
        .nodes
        .iter()
        .find(|n| n.id == "resolve_gate")
        .expect("resolve_gate");
    let hx = handle.frame.x + handle.frame.width / 2.0;
    let gx = gate.frame.x + gate.frame.width / 2.0;
    assert!(
        (gx - hx).abs() < 1.0,
        "resolve_gate cx={gx:.3} must match handle cx={hx:.3} (upstream axis inherit)"
    );
}

/// yFiles-style decision fan: short-span primary on spine, long branch aside
/// without hugging the canvas left edge (`product.order-approval`).
/// Same-face East back-edge shares one outer Main (no overshoot past submit).
#[test]
fn order_approval_primary_arm_on_spine_approved_left() {
    let path = showcase_dir().join("flat/product.order-approval.pgm");
    let source = fs::read_to_string(&path).expect("order-approval fixture");
    let result = build_layout(&source, &BuildOptions::default()).expect("layout");
    let cx = |id: &str| {
        let n = result
            .nodes
            .iter()
            .find(|n| n.id == id)
            .unwrap_or_else(|| panic!("missing node {id}"));
        n.frame.x + n.frame.width / 2.0
    };
    let check_x = cx("check");
    let approved_x = cx("approved");
    // "On the spine" is a statement about the arm, not about the two centers:
    // when `check` carries several ports the node center pays the offset, and
    // what must stay collinear is the edge's own pair of ports.
    let primary = result
        .edges
        .iter()
        .find(|e| e.source == "check" && e.target == "finance")
        .expect("check→finance");
    let arm = primary.path.samples();
    let (arm_from, arm_to) = (arm[0].x, arm[arm.len() - 1].x);
    assert!(
        (arm_to - arm_from).abs() < 1.0,
        "finance (short primary) must sit on check spine: port x {arm_from:.3} → {arm_to:.3}"
    );
    assert!(
        approved_x < check_x - 1.0,
        "approved (long side leaf) must sit left of check: approved={approved_x:.3} check={check_x:.3}"
    );
    let edge = result
        .edges
        .iter()
        .find(|e| e.source == "check" && e.target == "approved")
        .expect("check→approved");
    let pts = edge.path.samples();
    let min_x = pts.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    let approved = result
        .nodes
        .iter()
        .find(|n| n.id == "approved")
        .expect("approved");
    // Side corridor may run slightly left of the leaf, but must not hug x≈0.
    assert!(
        min_x >= approved.frame.x - 8.0,
        "check→approved must stay near the left leaf column (min_x={min_x:.3}, approved.left={:.3})",
        approved.frame.x
    );

    let back = result
        .edges
        .iter()
        .find(|e| e.source == "rejected" && e.target == "submit")
        .expect("rejected→submit");
    let submit = result
        .nodes
        .iter()
        .find(|n| n.id == "submit")
        .expect("submit");
    // East port on submit ≈ right face; top Cross must not overshoot left of it.
    let submit_east_x = submit.frame.x + submit.frame.width;
    let back_min_x = back
        .path
        .samples()
        .iter()
        .map(|p| p.x)
        .fold(f64::INFINITY, f64::min);
    assert!(
        back_min_x >= submit_east_x - 8.0,
        "rejected→submit same-face East corridor must not cross left of submit East port \
         (min_x={back_min_x:.3}, submit.east={submit_east_x:.3})"
    );
}

/// D₂.2a table-driven: exemption marker parsing + canvas-bloat guard.
#[test]
fn d22a_exemption_marker_and_bbox_guard() {
    // Exemption marker: exact phrase only, position-agnostic.
    let cases: &[(&str, bool)] = &[
        ("diagram { }", false),
        ("// d2-exempt: group-penetration — C-fallback", true),
        ("// d2-exempt: group-penetration", true),
        ("// d2-exempt: something-else", false),
        ("diagram { // d2-exempt: group-penetration — reason }", true),
    ];
    for (source, want) in cases {
        assert_eq!(penetration_exempt(source), *want, "source: {source:?}");
    }

    // Canvas-bloat guard: +10% per dimension, independent axes.
    let cases: &[(f64, f64, f64, f64, bool)] = &[
        (100.0, 100.0, 100.0, 100.0, false), // unchanged
        (109.0, 109.0, 100.0, 100.0, false), // under the bar
        (111.0, 100.0, 100.0, 100.0, true),  // width over
        (100.0, 111.0, 100.0, 100.0, true),  // height over
        (90.0, 90.0, 100.0, 100.0, false),   // shrink is fine
    ];
    for (w, h, bw, bh, want) in cases {
        assert_eq!(bbox_over_guard(*w, *h, *bw, *bh), *want, "({w}, {h}) vs ({bw}, {bh})");
    }
}

/// D₂.2b table-driven (group-frame-d2.md §8.12/§8.13):
/// - fallback observability: every `channel-group-fallback` relaxation is
///   counted into `gate_fallback_events` (whole-diagram gate-route fallback
///   and derive-level impure rects alike), and fixtures without fallback
///   report zero;
/// - MetricVerifier demand floor: on every fixture, each published LayerGap
///   demand (channel / group shell / gate producers merged) is met by the
///   resolved gap. Gate capacity monotonicity itself is covered by the
///   demand.rs table-driven unit tests (§8.13-2).
#[test]
fn d22b_fallback_observability_and_demand_floor() {
    // (fixture, expect_fallback) — the five D₂.2a class-C-fallback fixtures
    // plus the derive-level one; two gate-on fixtures stay clean.
    let cases: &[(&str, bool)] = &[
        ("group-weak/demo.k8s-blue-green-release-topology", false),
        ("group-weak/demo.k8s-multi-namespace-overview", true),
        ("group-weak/demo.k8s-platform-stack", true),
        ("group-weak/demo.k8s-tenant-isolation", true),
        ("group-weak/demo.plotgram-core-mod-deps", false),
        ("group-weak/product.d2-cell-tower-network", true),
        ("group-weak/demo.ai-agent-docops-pipeline", false),
        ("group-weak/demo.hybrid-cloud-dr-topology", false),
    ];
    let mut demand_seen = 0usize;
    for &(rel, expect_fb) in cases {
        let path = showcase_dir().join(format!("{rel}.pgm"));
        let source = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{rel}: {e}"));
        let result = build_layout(&source, &BuildOptions::default())
            .unwrap_or_else(|e| panic!("{rel}: layout: {e}"));
        let obs = result
            .diagnostics
            .hierarchical
            .as_ref()
            .unwrap_or_else(|| panic!("{rel}: no hierarchical obs"));
        let rule_count = result
            .diagnostics
            .relaxations
            .iter()
            .filter(|r| r.rule == "channel-group-fallback")
            .count();
        assert_eq!(
            obs.gate_fallback_events,
            rule_count,
            "{rel}: events must equal relaxation count"
        );
        assert_eq!(obs.gate_fallback_events > 0, expect_fb, "{rel}: fallback expectation");
        // §8.11: gate capacity publishes only when gates are in use.
        if obs.channel_used_gates {
            assert!(
                obs.gate_capacity_seams > 0,
                "{rel}: gate-on fixture must publish gate capacity demand"
            );
        } else {
            assert_eq!(
                obs.gate_capacity_seams, 0,
                "{rel}: fallback diagram must skip gate capacity demand"
            );
        }
        // Demand floor (MetricVerifier §9): every published lower bound met.
        for (&seam, &demand) in &obs.layer_gap_demands {
            let got = obs.layer_gaps.get(seam as usize).copied().unwrap_or(0.0);
            assert!(
                got + EPS >= demand,
                "{rel}: seam {seam} resolved {got} < demand {demand}"
            );
        }
        demand_seen += obs.layer_gap_demands.len();
    }
    assert!(demand_seen > 0, "weak fixtures must publish LayerGap demands");
}
