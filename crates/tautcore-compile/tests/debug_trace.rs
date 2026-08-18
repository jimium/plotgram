//! LayoutDebugTrace acceptance (debug-profile.md §5).
//!
//! Fixture exercises every projection-relevant decision at once: FAS
//! reversal, long-edge dummies, nested groups, and a self-loop.

use tautcore_compile::{build_debug_trace, build_layout, BuildOptions};

const FIXTURE: &str = r#"
diagram {
    profile: flowchart,
    layout: hierarchical

    group outer {
        label: "Outer"
        group inner {
            label: "Inner"
            node a { label: "A" }
            node b { label: "B" }
        }
        node c { label: "C" }
    }
    node d { label: "D" }

    a -> b
    b -> c
    c -> d
    a -> d
    d -> a
    b -> b
}
"#;

fn trace_json(source: &str) -> serde_json::Value {
    let trace = build_debug_trace(source, &BuildOptions::default())
        .expect("trace should build for hierarchical fixtures");
    serde_json::to_value(&trace).expect("trace serializes")
}

/// Snapshot stability + envelope invariants (acceptance 1, 4).
#[test]
fn trace_snapshot() {
    insta::assert_json_snapshot!(trace_json(FIXTURE));
}

/// Table-driven behavioral checks over the shared fixture.
#[test]
fn trace_behavior() {
    type Case = (&'static str, &'static str, fn(serde_json::Value));
    let cases: &[Case] = &[
        ("envelope", FIXTURE, check_envelope),
        ("determinism", FIXTURE, check_determinism),
        ("reversed-edge", FIXTURE, check_reversed),
        ("dummy-chain", FIXTURE, check_dummy_chain),
        ("self-loop", FIXTURE, check_self_loop),
        ("groups", FIXTURE, check_groups),
    ];
    for (name, source, check) in cases {
        check(trace_json(source));
        let _ = name;
    }
}

fn check_envelope(v: serde_json::Value) {
    assert_eq!(v["schema_version"], 1);
    assert_eq!(v["layout"], "hierarchical");
    assert_eq!(v["extension"]["kind"], "hierarchical");
    assert_eq!(v["space"], "physical");
    assert_eq!(v["orientation"], "top-to-bottom");
    assert!(
        v["extension"]["channels"].is_object(),
        "D1.2 ChannelDebug must be present"
    );
    // Dense indices never leak into element keys (acceptance 2 is enforced by
    // the type shape; the snapshot review double-checks).
    for elem in v["extension"]["elems"].as_array().unwrap() {
        let key = &elem["key"];
        assert!(
            key["type"] == "real" || key["type"] == "virtual" || key["type"] == "group-boundary"
        );
        assert!(key.get("elem_idx").is_none());
    }
}

fn check_determinism(v: serde_json::Value) {
    let a = serde_json::to_string(&v).unwrap();
    let b = serde_json::to_string(&trace_json(FIXTURE)).unwrap();
    assert_eq!(a, b, "same input must produce byte-identical traces");
}

fn check_reversed(v: serde_json::Value) {
    let plans = v["extension"]["edge_plans"].as_array().unwrap();
    let back = plans
        .iter()
        .find(|p| p["original"]["source"] == "d" && p["original"]["target"] == "a")
        .expect("fixture back edge present");
    assert_eq!(back["reversed"], true);
    assert_eq!(
        back["working"]["source"], "a",
        "FAS swaps working direction"
    );
}

fn check_dummy_chain(v: serde_json::Value) {
    let plans = v["extension"]["edge_plans"].as_array().unwrap();
    let long = plans
        .iter()
        .find(|p| p["original"]["source"] == "a" && p["original"]["target"] == "d")
        .expect("fixture long edge present");
    let dummies = long["dummy_chain"].as_array().unwrap();
    assert!(
        !dummies.is_empty(),
        "a→d spans multiple ranks and must properify into dummies"
    );
    for d in dummies {
        assert_eq!(d["type"], "virtual");
        assert_eq!(d["owner_edge"], long["edge_id"]);
    }
}

fn check_self_loop(v: serde_json::Value) {
    let plans = v["extension"]["edge_plans"].as_array().unwrap();
    let loop_plan = plans
        .iter()
        .find(|p| p["original"]["source"] == p["original"]["target"])
        .expect("fixture self-loop present");
    assert_eq!(loop_plan["note"], "self-loop");
    assert_eq!(loop_plan["reversed"], false);
    assert!(loop_plan["segments"].as_array().unwrap().is_empty());
    assert!(loop_plan["dummy_chain"].as_array().unwrap().is_empty());

    // The common view still carries the stub geometry.
    let common_edge = v["common"]["edges"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["edge_id"] == loop_plan["edge_id"])
        .expect("self-loop appears in common edges");
    assert!(common_edge["path"].as_array().unwrap().len() >= 2);
}

fn check_groups(v: serde_json::Value) {
    let groups = v["common"]["groups"].as_array().unwrap();
    assert_eq!(groups.len(), 2);
    let inner = groups
        .iter()
        .find(|g| g["group_id"] == "inner")
        .expect("inner group present");
    assert_eq!(inner["parent"], "outer");
    assert!(
        inner["frame"].is_null(),
        "layout kernel never writes group frames"
    );
    assert_eq!(inner["frame_source"], "none");
}

/// Trace geometry must coincide with the product layout output — same
/// decisions, one code path (debug-inspector.md §4.1).
///
/// `finalize` applies one uniform translation to frame the canvas with
/// symmetric margins; that is canvas framing, not a layout decision, so the
/// comparison is done modulo that shift.
#[test]
fn trace_geometry_matches_product_output() {
    let trace = build_debug_trace(FIXTURE, &BuildOptions::default()).unwrap();
    let product = build_layout(FIXTURE, &BuildOptions::default()).unwrap();

    assert_eq!(trace.common.nodes.len(), product.nodes.len());

    // The canvas translation round-trips floats; compare coordinates with a
    // sub-pixel tolerance.
    const EPS: f64 = 1e-6;
    let near = |a: f64, b: f64| (a - b).abs() <= EPS;

    let shift = {
        let t = &trace.common.nodes[0].frame;
        let p = &product.nodes[0].frame;
        (p.x - t.x, p.y - t.y)
    };

    for (tn, pn) in trace.common.nodes.iter().zip(&product.nodes) {
        assert_eq!(tn.id, pn.id);
        assert_eq!(
            tn.frame.width, pn.frame.width,
            "node `{}` width diverges",
            tn.id
        );
        assert_eq!(
            tn.frame.height, pn.frame.height,
            "node `{}` height diverges",
            tn.id
        );
        assert!(
            near(tn.frame.x + shift.0, pn.frame.x) && near(tn.frame.y + shift.1, pn.frame.y),
            "node `{}` frame diverges beyond the canvas shift: trace {:?} + {:?} vs product {:?}",
            tn.id,
            (tn.frame.x, tn.frame.y),
            shift,
            (pn.frame.x, pn.frame.y)
        );
    }

    assert_eq!(trace.common.edges.len(), product.edges.len());
    for (te, pe) in trace.common.edges.iter().zip(&product.edges) {
        assert_eq!(te.edge_id, pe.id);
        let product_pts = pe
            .path
            .polyline_points()
            .expect("hierarchical ink writes polylines");
        assert_eq!(
            te.path.len(),
            product_pts.len(),
            "edge `{}` path length diverges",
            te.edge_id
        );
        for (tp, pp) in te.path.iter().zip(product_pts) {
            assert!(
                near(tp.x + shift.0, pp.x) && near(tp.y + shift.1, pp.y),
                "edge `{}` point diverges beyond the canvas shift: trace {:?} + {:?} vs product {:?}",
                te.edge_id,
                (tp.x, tp.y),
                shift,
                (pp.x, pp.y)
            );
        }
    }
}
