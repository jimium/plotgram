//! Layout diagnostics exit through the full compile pipeline (roadmap phase C).
//!
//! Asserts only the diagnostics channel — geometry regression is guarded by
//! `hier_eval` and the coordinate snapshots.

use plotgram_compile::{build_layout, BuildOptions};

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
