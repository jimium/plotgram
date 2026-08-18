//! Organic layout integration tests: determinism, overlap freedom,
//! component packing, param binding, and a coordinate snapshot.

use plotgram_engine_api::{EdgeGeometryMode, LayoutAlgorithm, LayoutInput};
use plotgram_layout::{OrganicLayout, OrganicParams};
use plotgram_model::attr::{AttrMap, AttrValue};
use plotgram_model::geometry::Size;
use plotgram_model::graph::{Arrow, Edge, Graph, Node};
use plotgram_model::sizes::NodeSizes;

fn node(id: &str) -> Node {
    Node {
        id: id.into(),
        label: None,
        shape: None,
        role: Default::default(),
        host_group: None,
        anchor: None,
        partition_cell: None,
        attrs: AttrMap::new(),
    }
}

fn edge(id: &str, source: &str, target: &str) -> Edge {
    Edge {
        id: id.into(),
        source: source.into(),
        target: target.into(),
        arrow: Arrow::Forward,
        label: None,
        head_label: None,
        tail_label: None,
        from_port: None,
        to_port: None,
        weight: None,
        undirected: false,
        attrs: AttrMap::new(),
    }
}

fn sizes_for(graph: &Graph, size: Size) -> NodeSizes {
    let mut s = NodeSizes::new();
    for n in graph.all_nodes() {
        s.insert(n.id.clone(), size);
    }
    s
}

fn run(graph: &Graph, options: &AttrMap) -> plotgram_engine_api::LayoutOutput {
    OrganicLayout
        .layout(LayoutInput {
            graph,
            node_sizes: &sizes_for(graph, Size::new(60.0, 28.0)),
            options,
            edge_geometry: EdgeGeometryMode::Builtin,
        })
        .expect("organic layout must succeed")
}

fn mesh_graph() -> Graph {
    // 3×3 mesh.
    let mut nodes = Vec::new();
    for r in 0..3 {
        for c in 0..3 {
            nodes.push(node(&format!("n{r}{c}")));
        }
    }
    let id = |r: usize, c: usize| format!("n{r}{c}");
    let mut edges = Vec::new();
    let mut e = 0;
    for r in 0..3 {
        for c in 0..3 {
            if c + 1 < 3 {
                edges.push(edge(&format!("h{e}"), &id(r, c), &id(r, c + 1)));
                e += 1;
            }
            if r + 1 < 3 {
                edges.push(edge(&format!("v{e}"), &id(r, c), &id(r + 1, c)));
                e += 1;
            }
        }
    }
    Graph {
        nodes,
        edges,
        groups: Vec::new(),
        partition: None,
    }
}

/// Same input + same seed ⇒ bit-identical output (workspace determinism rule).
#[test]
fn deterministic_for_same_seed() {
    for seed in [0u64, 1, 42, 0xDEAD_BEEF] {
        let graph = mesh_graph();
        let mut options = AttrMap::new();
        options.insert("seed".into(), AttrValue::Num(seed as f64));
        let a = serde_json::to_string(&run(&graph, &options).nodes).unwrap();
        let b = serde_json::to_string(&run(&graph, &options).nodes).unwrap();
        assert_eq!(a, b, "seed {seed}: output must be bit-identical");
    }
}

/// Stress must keep adjacent nodes near the preferred edge length.
#[test]
fn path_respects_preferred_edge_length() {
    let graph = Graph {
        nodes: vec![node("a"), node("b"), node("c"), node("d")],
        edges: vec![
            edge("e0", "a", "b"),
            edge("e1", "b", "c"),
            edge("e2", "c", "d"),
        ],
        groups: Vec::new(),
        partition: None,
    };
    let out = run(&graph, &AttrMap::new());
    let frame = |id: &str| out.nodes.iter().find(|n| n.id == id).unwrap().frame;
    let k = 60.0;
    for (a, b) in [("a", "b"), ("b", "c"), ("c", "d")] {
        let fa = frame(a);
        let fb = frame(b);
        let d = ((fa.center().x - fb.center().x).powi(2)
            + (fa.center().y - fb.center().y).powi(2))
        .sqrt();
        assert!(
            (0.4 * k..=1.8 * k).contains(&d),
            "edge {a}-{b} distance {d} outside [0.4k, 1.8k]"
        );
    }
}

/// Overlap removal must clear every node pair by `minimum_node_distance`.
#[test]
fn nodes_do_not_overlap() {
    // Star-ish dense graph: many nodes mutually attracted to a hub.
    let mut nodes = vec![node("hub")];
    let mut edges = Vec::new();
    for i in 0..8 {
        let id = format!("leaf{i}");
        nodes.push(node(&id));
        edges.push(edge(&format!("e{i}"), "hub", &id));
    }
    // Extra ring among leaves to pull them together.
    for i in 0..8 {
        edges.push(edge(&format!("r{i}"), &format!("leaf{i}"), &format!("leaf{}", (i + 1) % 8)));
    }
    let graph = Graph {
        nodes,
        edges,
        groups: Vec::new(),
        partition: None,
    };
    let out = run(&graph, &AttrMap::new());
    let gap = 24.0;
    for i in 0..out.nodes.len() {
        for j in (i + 1)..out.nodes.len() {
            let a = &out.nodes[i].frame;
            let b = &out.nodes[j].frame;
            let dx = (a.center().x - b.center().x).abs();
            let dy = (a.center().y - b.center().y).abs();
            assert!(
                dx >= (a.width + b.width) / 2.0 + gap - 0.5
                    || dy >= (a.height + b.height) / 2.0 + gap - 0.5,
                "nodes {} / {} overlap (dx={dx}, dy={dy})",
                out.nodes[i].id,
                out.nodes[j].id
            );
        }
    }
}

/// Disconnected components are packed on shelves without interpenetrating.
#[test]
fn components_pack_disjointly() {
    let mut graph = mesh_graph();
    graph.nodes.push(node("iso1"));
    graph.nodes.push(node("iso2"));
    graph.edges.push(edge("iso_e", "iso1", "iso2"));
    let out = run(&graph, &AttrMap::new());
    assert_eq!(out.nodes.len(), 11);

    let frame = |id: &str| out.nodes.iter().find(|n| n.id == id).unwrap().frame;
    let mesh_box = (0..3)
        .flat_map(move |r| (0..3).map(move |c| (r, c)))
        .map(|(r, c)| frame(&format!("n{r}{c}")))
        .fold(
            (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY),
            |acc, f| {
                (
                    acc.0.min(f.x),
                    acc.1.min(f.y),
                    acc.2.max(f.right()),
                    acc.3.max(f.bottom()),
                )
            },
        );
    let iso1 = frame("iso1");
    let iso2 = frame("iso2");
    for f in [&iso1, &iso2] {
        let outside = f.right() <= mesh_box.0 - 47.5
            || f.x >= mesh_box.2 + 47.5
            || f.bottom() <= mesh_box.1 - 47.5
            || f.y >= mesh_box.3 + 47.5;
        assert!(outside, "component box interpenetrates the mesh AABB");
    }
}

/// Self-loops and parallel edges produce valid ink and ports.
#[test]
fn self_loop_and_parallel_edges_have_ink() {
    let graph = Graph {
        nodes: vec![node("a"), node("b")],
        edges: vec![
            edge("loop", "a", "a"),
            edge("p0", "a", "b"),
            edge("p1", "a", "b"),
        ],
        groups: Vec::new(),
        partition: None,
    };
    let out = run(&graph, &AttrMap::new());
    assert_eq!(out.edges.len(), 3);
    for e in &out.edges {
        let pts = e.path.samples();
        assert!(pts.len() >= 2, "edge {} path too short", e.id);
        assert!(e.from_port.is_some() && e.to_port.is_some());
        for p in pts {
            assert!(p.x.is_finite() && p.y.is_finite());
        }
    }
    // Parallel edges must fan out (distinct terminal lines).
    let p0 = out.edges.iter().find(|e| e.id == "p0").unwrap();
    let p1 = out.edges.iter().find(|e| e.id == "p1").unwrap();
    let s0 = p0.path.samples()[0];
    let s1 = p1.path.samples()[0];
    let separated = (s0.x - s1.x).abs() + (s0.y - s1.y).abs();
    assert!(separated > 1.0, "parallel edges not separated ({separated})");
}

/// Parameter binding: presets, aliases, validation, unknown-key warning.
#[test]
fn params_bind_presets_and_aliases() {
    let mut options = AttrMap::new();
    options.insert("preset".into(), AttrValue::Atom("compact".into()));
    options.insert("edge_length".into(), AttrValue::Num(50.0));
    options.insert("node_gap".into(), AttrValue::Num(8.0));
    options.insert("iterations".into(), AttrValue::Num(12.0));
    options.insert("seed".into(), AttrValue::Num(99.0));
    options.insert("bogus".into(), AttrValue::Atom("x".into()));
    let bound = OrganicParams::bind(&options).unwrap();
    assert_eq!(bound.params.preferred_edge_length, 50.0);
    assert_eq!(bound.params.minimum_node_distance, 8.0);
    assert_eq!(bound.params.iterations, 12);
    assert_eq!(bound.params.seed, 99);
    // Preset compact values overridden by explicit options above.
    assert_eq!(bound.params.component_gap, 32.0);
    assert!(bound
        .warnings
        .iter()
        .any(|w| w.message.contains("unknown option `bogus`")));

    // Spacious preset defaults.
    let mut options = AttrMap::new();
    options.insert("preset".into(), AttrValue::Atom("spacious".into()));
    let bound = OrganicParams::bind(&options).unwrap();
    assert_eq!(bound.params.preferred_edge_length, 96.0);
    assert_eq!(bound.params.minimum_node_distance, 40.0);

    // Validation: zero edge length rejected.
    let mut options = AttrMap::new();
    options.insert("edge_length".into(), AttrValue::Num(0.0));
    assert!(OrganicParams::bind(&options).is_err());

    // Validation: iterations out of range.
    let mut options = AttrMap::new();
    options.insert("iterations".into(), AttrValue::Num(5000.0));
    assert!(OrganicParams::bind(&options).is_err());
}

/// Two-node graph: stress pulls to the preferred edge length, overlap removal
/// enforces the (harder) minimum clearance (60px boxes + 24px gap ⇒ ≥ 84).
#[test]
fn two_nodes_respect_clearance() {
    let graph = Graph {
        nodes: vec![node("a"), node("b")],
        edges: vec![edge("e0", "a", "b")],
        groups: Vec::new(),
        partition: None,
    };
    let out = run(&graph, &AttrMap::new());
    let fa = out.nodes.iter().find(|n| n.id == "a").unwrap().frame;
    let fb = out.nodes.iter().find(|n| n.id == "b").unwrap().frame;
    let dx = (fa.center().x - fb.center().x).abs();
    let dy = (fa.center().y - fb.center().y).abs();
    assert!(
        dx >= 84.0 - 0.5 || dy >= 28.0 + 24.0 - 0.5,
        "two-node clearance violated (dx={dx}, dy={dy})"
    );
}

/// Coordinate snapshot for the mesh fixture — intentional tuning updates the
/// snapshot; accidental regressions get caught.
#[test]
fn mesh_coordinates_snapshot() {
    let graph = mesh_graph();
    let out = run(&graph, &AttrMap::new());
    insta::assert_json_snapshot!(serde_json::json!({
        "nodes": out.nodes,
        "edges": out.edges,
    }));
}
