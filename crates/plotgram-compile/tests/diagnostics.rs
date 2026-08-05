//! Layout diagnostics exit through the full compile pipeline (roadmap phase C).
//!
//! Asserts only the diagnostics channel — geometry regression is guarded by
//! `hier_eval` and the coordinate snapshots.

use plotgram_compile::{build_layout, BuildOptions};
use plotgram_model::result::{EdgePath, LayoutResult};

const BASE: &str = "diagram {\n  layout: hierarchical { %OPTIONS% }\n  node a \"A\"\n  node b \"B\"\n  a -> b\n}";

fn source_with(options: &str) -> String {
    if options.is_empty() {
        return BASE.replace(" { %OPTIONS% }", "");
    }
    BASE.replace("%OPTIONS%", options)
}

#[test]
fn build_layout_surfaces_unknown_option_warnings() {
    // Table: (layout options as authored, expected warning count).
    let cases: &[(&str, usize)] = &[("", 0), ("bogus_key: 1", 1), ("aaa: 1, zzz: 2", 2)];
    for (options, expected) in cases {
        let result = build_layout(&source_with(options), &BuildOptions::default())
            .unwrap_or_else(|e| panic!("build with options `{options}` failed: {e}"));
        assert_eq!(
            result.diagnostics.warnings.len(),
            *expected,
            "options=`{options}`"
        );
        // Relaxations channel exists but has no producer in this build.
        assert!(result.diagnostics.relaxations.is_empty());
    }
}

#[test]
fn params_hash_flows_to_layout_result() {
    let opts = BuildOptions::default();
    let a = build_layout(&source_with(""), &opts).unwrap();
    let b = build_layout(&source_with(""), &opts).unwrap();
    assert_eq!(a.diagnostics.params_hash, b.diagnostics.params_hash);
    assert_eq!(a.diagnostics.params_hash.len(), 16);

    let changed = build_layout(&source_with("node_gap: 99"), &opts).unwrap();
    assert_ne!(a.diagnostics.params_hash, changed.diagnostics.params_hash);
}

#[test]
fn layout_result_json_round_trip_keeps_diagnostics() {
    let result = build_layout(&source_with("bogus_key: 1"), &BuildOptions::default()).unwrap();
    let json = serde_json::to_string(&result).unwrap();
    let back: plotgram_model::result::LayoutResult = serde_json::from_str(&json).unwrap();
    assert_eq!(back.diagnostics, result.diagnostics);
}

// ─── routing_style (batch 1) ─────────────────────────────────────────────

const FAN: &str = "diagram {\n  layout: hierarchical { %OPTIONS% }\n  node s {}\n  node a {}\n  node b {}\n  node c {}\n  s -> a\n  s -> b\n  s -> c\n}";

fn fan_with(options: &str) -> String {
    FAN.replace("%OPTIONS%", options)
}

#[test]
fn routing_style_builtin_styles_end_to_end() {
    let opts = BuildOptions::default();

    // Default: orthogonal — every segment axis-aligned.
    let r = build_layout(&fan_with(""), &opts).unwrap();
    assert!(!r.edges.is_empty());
    for e in &r.edges {
        let pts = e.path.polyline_points().expect("orthogonal emits polylines");
        for w in pts.windows(2) {
            let orthogonal = (w[0].x - w[1].x).abs() < 1e-6 || (w[0].y - w[1].y).abs() < 1e-6;
            assert!(orthogonal, "default style must stay orthogonal");
        }
    }

    // Polyline: straight waypoint chains; the outer fan edges are diagonal.
    let r = build_layout(&fan_with("routing_style: polyline"), &opts).unwrap();
    let mut any_diagonal = false;
    for e in &r.edges {
        let pts = e.path.polyline_points().expect("polyline emits polylines");
        assert!(pts.len() >= 2);
        any_diagonal |= pts.windows(2).any(|w| {
            (w[0].x - w[1].x).abs() > 1e-6 && (w[0].y - w[1].y).abs() > 1e-6
        });
    }
    assert!(any_diagonal, "fan edges should run diagonally in polyline style");

    // Curved: every edge is a single cubic between its port anchors.
    let r = build_layout(&fan_with("routing_style: curved"), &opts).unwrap();
    for e in &r.edges {
        assert!(
            matches!(e.path, EdgePath::Cubic { .. }),
            "curved style must emit cubics, got polyline for `{}`",
            e.id
        );
    }
}

// ─── auto_edge_grouping (batch 2) ────────────────────────────────────────

#[test]
fn auto_edge_grouping_octilinear_is_rejected() {
    let src = fan_with("routing_style: octilinear, auto_edge_grouping: true");
    let err = build_layout(&src, &BuildOptions::default())
        .expect_err("auto_edge_grouping + octilinear must fail at bind");
    assert!(
        err.to_string().contains("auto_edge_grouping"),
        "unexpected: {err}"
    );
}

#[test]
fn bus_routing_option_is_rejected_as_mistaken_mapping() {
    let src = fan_with("bus_routing: true");
    let err = build_layout(&src, &BuildOptions::default())
        .expect_err("bus_routing must hard-fail (demo mis-mapping)");
    let msg = err.to_string();
    assert!(msg.contains("bus_routing"), "got: {msg}");
    assert!(
        msg.contains("auto_edge_grouping") || msg.contains("removed"),
        "got: {msg}"
    );
}

#[test]
fn auto_edge_grouping_fans_share_source_port_and_bus() {
    let opts = BuildOptions::default();
    let src = r#"diagram {
  layout: hierarchical { auto_edge_grouping: true }
  node hub {}
  node a {}
  node b {}
  node c {}
  hub -> a
  hub -> b
  hub -> c
}"#;
    let r = build_layout(src, &opts).unwrap();
    assert_eq!(r.edges.len(), 3);

    // Shared source PortPoint (yFiles bus): all members start at the same point.
    let starts: Vec<_> = r
        .edges
        .iter()
        .map(|e| e.path.samples()[0])
        .collect();
    assert!(
        starts.iter().all(|p| (p.x - starts[0].x).abs() < 1e-6 && (p.y - starts[0].y).abs() < 1e-6),
        "clustered fan must share one source port, got {starts:?}"
    );

    // Shared trunk tip: some vertex on each path has (start.x, bus_y).
    let start = starts[0];
    for e in &r.edges {
        let pts = e.path.samples();
        assert!(
            pts.iter()
                .any(|p| (p.x - start.x).abs() < 1e-6 && (p.y - start.y).abs() > 1.0),
            "edge {} must leave the shared port along a trunk, got {pts:?}",
            e.id
        );
    }
}

// ─── critical (batch 2) ─────────────────────────────────────────────────

fn sum_bends(result: &LayoutResult) -> usize {
    result
        .edges
        .iter()
        .map(|e| {
            e.path
                .polyline_points()
                .map(|pts| pts.len().saturating_sub(2))
                .unwrap_or(0)
        })
        .sum()
}

#[test]
fn critical_marked_path_not_worse_than_control() {
    // Diamond plus a competing chain crossing one arm; the marked arm must
    // never bend more than the identical unmarked control (observable
    // acceptance, edge-parameters §2.4).
    let src = |critical: bool| {
        let mark = if critical { " { critical: true }" } else { "" };
        format!(
            "diagram {{
  layout: hierarchical {{ }}
  node s {{}} node m1 {{}} node m2 {{}} node t {{}} node c {{}} node d {{}}
  s -> m1{mark}
  m1 -> t{mark}
  s -> m2
  m2 -> t
  c -> m2
  m2 -> d
}}"
        )
    };
    let opts = BuildOptions::default();
    let marked = build_layout(&src(true), &opts).unwrap();
    let control = build_layout(&src(false), &opts).unwrap();
    assert!(
        sum_bends(&marked) <= sum_bends(&control),
        "critical marking must not worsen bends: {} > {}",
        sum_bends(&marked),
        sum_bends(&control)
    );
}

// ─── D1.0 TrackOrder ────────────────────────────────────────────────────

#[test]
fn edge_gap_binds_and_affects_params_hash() {
    let opts = BuildOptions::default();
    let a = build_layout(&source_with(""), &opts).unwrap();
    let b = build_layout(&source_with("edge_gap: 24"), &opts).unwrap();
    assert_ne!(
        a.diagnostics.params_hash, b.diagnostics.params_hash,
        "edge_gap must be consumed in params_hash"
    );
}

/// Horizontal rails of a grouping-off fan must not all share one Y
/// (channel-d1.md D1.0 acceptance — no fake bus via mid_y).
#[test]
fn fan_out_without_grouping_uses_distinct_horizontal_tracks() {
    let src = r#"diagram {
  layout: hierarchical { auto_edge_grouping: false }
  node hub {}
  node a {}
  node b {}
  node c {}
  node d {}
  hub -> a
  hub -> b
  hub -> c
  hub -> d
}"#;
    let r = build_layout(src, &BuildOptions::default()).unwrap();
    assert_eq!(r.edges.len(), 4);

    let mut rail_ys: Vec<f64> = Vec::new();
    for e in &r.edges {
        let pts = e
            .path
            .polyline_points()
            .expect("orthogonal fan edges are polylines");
        // Collect Y of every strictly horizontal segment.
        for w in pts.windows(2) {
            if (w[0].y - w[1].y).abs() < 1e-6 && (w[0].x - w[1].x).abs() > 1e-6 {
                rail_ys.push(w[0].y);
            }
        }
    }
    assert!(
        !rail_ys.is_empty(),
        "fan-out must produce horizontal rails"
    );
    rail_ys.sort_by(|a, b| a.total_cmp(b));
    rail_ys.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    assert!(
        rail_ys.len() >= 2,
        "grouping-off fan must use ≥2 distinct track Y, got {rail_ys:?}"
    );
}
