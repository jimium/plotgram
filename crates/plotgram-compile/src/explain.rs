//! Layout facts channel (ADR-007): engine writes, LLM reads.
//!
//! Derives qualitative, predicate-style facts from solved geometry — the
//! readable counterpart to `render` (pixels) and `measure` (numbers).
//! Pure reader over (`Graph`, `LayoutResult`): never touches geometry, never
//! branches on diagram type / layout name (ADR-001). Deterministic: output
//! ordering comes from declaration / index order only (AGENTS.md §2).
//!
//! Narrative structure per ADR-007: conclusion first (summary + defect
//! rollup), then named anomalies (crossing pairs, high-bend / detouring
//! edges), then structure (rank bands, group relations). Raw coordinates are
//! deliberately absent — they are what LLMs read poorly.

use plotgram_model::geometry::{Point, Rect};
use plotgram_model::graph::{Graph, NodeRole};
use plotgram_model::result::LayoutResult;

/// Dialect version of the fact text. Bump when predicate wording changes
/// *meaning*; adding new facts does not require a bump.
const FACTS_VERSION: u32 = 0;

/// Edges with at least this many bends are listed as anomalies.
const HIGH_BEND_THRESHOLD: usize = 4;

/// Path length / straight-line distance at or above which an edge counts as
/// detouring (25% overhead is clearly not a direct route).
const DETOUR_RATIO: f64 = 1.25;

/// Cap on per-fact anomaly detail lines so large graphs do not flood the
/// reader; the remainder is summarised as `+N`.
const MAX_ANOMALY_LINES: usize = 10;

/// Render the layout facts narrative for a solved layout.
pub fn explain(graph: &Graph, layout: &LayoutResult) -> String {
    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("layout-facts v{FACTS_VERSION}"));

    // Entity-node frames in graph declaration order (anchors excluded — they
    // are routing scaffolding, not business entities).
    let frames: Vec<(&str, Rect)> = graph
        .all_nodes()
        .iter()
        .filter(|n| n.role == NodeRole::Entity)
        .filter_map(|n| {
            layout
                .nodes
                .iter()
                .find(|p| p.id == n.id)
                .map(|p| (n.id.as_str(), p.frame))
        })
        .collect();

    let curved = layout
        .edges
        .iter()
        .filter(|e| e.path.polyline_points().is_none())
        .count();

    // ── summary ──────────────────────────────────────────────
    lines.push(format!(
        "summary: nodes={} edges={} groups={} canvas={:.0}x{:.0}",
        frames.len(),
        layout.edges.len(),
        graph.groups.len(),
        layout.canvas_width,
        layout.canvas_height,
    ));
    if curved > 0 {
        lines.push(format!("summary: curved_edges={curved}"));
    }

    // ── defect rollup (conclusion) ───────────────────────────
    let metrics = crate::audit::compute(layout);
    lines.push(format!(
        "defects: crossings={} node_overlaps={} label_overlaps={} group_intrusions={}",
        metrics.edge_crossing_count,
        metrics.node_overlap_count,
        metrics.label_overlap_count,
        metrics.edge_crosses_group_interior,
    ));

    // ── anomalies: crossing pairs, by name ───────────────────
    let crossings = crate::audit::edge_crossing_pairs(&layout.edges);
    for (a, b) in crossings.iter().take(MAX_ANOMALY_LINES) {
        lines.push(format!("crossing({a}, {b})"));
    }
    if crossings.len() > MAX_ANOMALY_LINES {
        lines.push(format!("+{} more_crossings", crossings.len() - MAX_ANOMALY_LINES));
    }

    // ── bands (geometric layering; the one fact .pgm cannot tell) ──
    let bands = cluster_bands(&frames);
    let band_of = |id: &str| -> Option<usize> {
        bands
            .iter()
            .position(|band| band.iter().any(|(nid, _)| *nid == id))
    };

    // ── anomalies: high-bend / detouring edges ───────────────
    let mut anomaly_lines = Vec::new();
    for edge in &layout.edges {
        let Some(points) = edge.path.polyline_points() else {
            continue; // curved: bends are meaningless for a Bézier
        };
        let bends = points.len().saturating_sub(2);
        let Some((start, end)) = edge.path.start_end() else {
            continue;
        };
        let straight = dist(start, end);
        let length: f64 = points
            .windows(2)
            .map(|w| dist(w[0], w[1]))
            .sum();
        let ratio = if straight > f64::EPSILON {
            length / straight
        } else {
            continue; // degenerate span; nothing qualitative to say
        };
        let detouring = ratio >= DETOUR_RATIO;
        if bends < HIGH_BEND_THRESHOLD && !detouring {
            continue;
        }
        let mut fact = format!(
            "edge({}) {}->{} bends={bends}",
            edge.id, edge.source, edge.target
        );
        if let (Some(sa), Some(sb)) = (band_of(&edge.source), band_of(&edge.target)) {
            fact.push_str(&format!(" span={}", sb.abs_diff(sa)));
        }
        if detouring {
            fact.push_str(&format!(" detour={} ratio={ratio:.2}", dominant_side(points, start, end)));
        }
        anomaly_lines.push(fact);
    }
    for fact in anomaly_lines.iter().take(MAX_ANOMALY_LINES) {
        lines.push(fact.clone());
    }
    if anomaly_lines.len() > MAX_ANOMALY_LINES {
        lines.push(format!(
            "+{} more_anomalous_edges",
            anomaly_lines.len() - MAX_ANOMALY_LINES
        ));
    }

    // ── structure: rank bands ────────────────────────────────
    if !bands.is_empty() {
        lines.push(format!("bands: {}", bands.len()));
        for (idx, band) in bands.iter().enumerate() {
            let mean_y = band.iter().map(|(_, f)| f.center().y).sum::<f64>() / band.len() as f64;
            let ids: Vec<&str> = band.iter().map(|(id, _)| *id).collect();
            lines.push(format!("band {} y≈{mean_y:.0}: {}", idx + 1, ids.join(" ")));
        }
    }

    // ── structure: group descriptors + pairwise relations ────
    if !graph.groups.is_empty() {
        lines.push(format!("groups: {}", graph.groups.len()));
        for g in &graph.groups {
            match g.label.as_deref() {
                Some(label) => lines.push(format!(
                    "group {} label=\"{label}\" members={}",
                    g.id,
                    g.node_count()
                )),
                None => lines.push(format!("group {} members={}", g.id, g.node_count())),
            }
        }
        // Pairwise qualitative positions, declaration order (i < j).
        let placements: Vec<(&str, Rect)> = graph
            .groups
            .iter()
            .filter_map(|g| {
                layout
                    .groups
                    .iter()
                    .find(|p| p.id == g.id)
                    .map(|p| (g.id.as_str(), p.frame))
            })
            .collect();
        for i in 0..placements.len() {
            for j in (i + 1)..placements.len() {
                let (a_id, a) = placements[i];
                let (b_id, b) = placements[j];
                let h_gap = horizontal_gap(a, b);
                let v_gap = vertical_gap(a, b);
                if h_gap > 0.0 {
                    let rel = if a.x < b.x { "left_of" } else { "right_of" };
                    lines.push(format!("group {a_id} {rel} group {b_id}"));
                }
                if v_gap > 0.0 {
                    let rel = if a.y < b.y { "above" } else { "below" };
                    lines.push(format!("group {a_id} {rel} group {b_id}"));
                }
                if h_gap <= 0.0 && v_gap <= 0.0 {
                    lines.push(format!("group {a_id} overlaps group {b_id}"));
                }
            }
        }
    }

    lines.join("\n")
}

// ── band clustering ──────────────────────────────────────────
// Parameter-free layering: nodes whose y-ranges overlap share a band; a new
// band starts at the first node (in y-center order) strictly below every
// member of the current band. Members are listed left→right by x center.

fn cluster_bands<'a>(frames: &'a [(&'a str, Rect)]) -> Vec<Vec<(&'a str, Rect)>> {
    let mut sorted: Vec<(&str, Rect)> = frames.to_vec();
    sorted.sort_by(|a, b| {
        a.1.center()
            .y
            .partial_cmp(&b.1.center().y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                a.1.center()
                    .x
                    .partial_cmp(&b.1.center().x)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.0.cmp(b.0))
    });

    let mut bands: Vec<Vec<(&str, Rect)>> = Vec::new();
    let mut current: Vec<(&str, Rect)> = Vec::new();
    let mut band_bottom = f64::NEG_INFINITY;
    for item in sorted {
        let (_, frame) = item;
        if !current.is_empty() && frame.y >= band_bottom {
            bands.push(std::mem::take(&mut current));
            band_bottom = f64::NEG_INFINITY;
        }
        band_bottom = band_bottom.max(frame.bottom());
        current.push(item);
    }
    if !current.is_empty() {
        bands.push(current);
    }

    for band in &mut bands {
        band.sort_by(|a, b| {
            a.1.center()
                .x
                .partial_cmp(&b.1.center().x)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(b.0))
        });
    }
    bands
}

// ── geometry helpers ─────────────────────────────────────────

fn dist(a: Point, b: Point) -> f64 {
    ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt()
}

/// Which side of the direct source→target line the path's largest excursion
/// lies on. Screen coords (y grows down): positive cross ⇒ "right" of travel.
fn dominant_side(points: &[Point], start: Point, end: Point) -> &'static str {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let mut best_abs = 0.0f64;
    let mut best_sign = 0.0f64;
    for p in points {
        let cross = dx * (p.y - start.y) - dy * (p.x - start.x);
        if cross.abs() > best_abs {
            best_abs = cross.abs();
            best_sign = cross;
        }
    }
    if best_sign > 0.0 {
        "right"
    } else {
        "left"
    }
}

fn horizontal_gap(a: Rect, b: Rect) -> f64 {
    if a.right() < b.x {
        b.x - a.right()
    } else if b.right() < a.x {
        a.x - b.right()
    } else {
        0.0 // x-ranges overlap
    }
}

fn vertical_gap(a: Rect, b: Rect) -> f64 {
    if a.bottom() < b.y {
        b.y - a.bottom()
    } else if b.bottom() < a.y {
        a.y - b.bottom()
    } else {
        0.0 // y-ranges overlap
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{build_explain, BuildOptions};

    /// Observable-output tests: run the full pipeline on inline `.pgm` and
    /// assert on fact lines, never on coordinates (AGENTS.md §4).
    #[test]
    fn explain_reports_summary_bands_and_groups() {
        let cases = [
            // grouped flowchart: two groups stacked by the a→b→c flow
            "diagram {\n  profile: flowchart\n  group g1 {\n    label: \"入口\"\n    node a \"A\"\n    node b \"B\"\n    a -> b\n  }\n  group g2 {\n    node c \"C\"\n  }\n  b -> c\n}",
            // flat flowchart, no groups
            "diagram {\n  profile: flowchart\n  node a \"A\"\n  node b \"B\"\n  node c \"C\"\n  a -> b\n  b -> c\n}",
        ];
        for src in cases {
            let text =
                build_explain(src, &BuildOptions::default()).unwrap_or_else(|e| panic!("{e}"));
            assert!(text.starts_with("layout-facts v0"), "{src}");
            assert!(text.contains("nodes=3"), "{src}");
            assert!(text.contains("defects: "), "{src}");
            assert!(text.contains("band 1"), "{src}");
            // determinism: same input, byte-identical facts (ADR-007)
            let again = build_explain(src, &BuildOptions::default()).unwrap();
            assert_eq!(text, again, "{src}");
        }
    }

    #[test]
    fn explain_lists_group_descriptors_and_relations() {
        let src = "diagram {\n  profile: flowchart\n  group g1 {\n    label: \"入口\"\n    node a \"A\"\n  }\n  group g2 {\n    node b \"B\"\n  }\n  a -> b\n}";
        let text = build_explain(src, &BuildOptions::default()).unwrap();
        assert!(text.contains("groups: 2"));
        assert!(text.contains("group g1 label=\"入口\" members=1"));
        // g1 feeds g2 ⇒ stacked, so a vertical relation must be stated
        assert!(
            text.contains("group g1 above group g2") || text.contains("group g1 below group g2"),
            "{text}"
        );
    }
}
