//! Coordinate stability snapshots for the hierarchical cross-axis
//! (roadmap phase A): BK ideal + global VPSC + two-pass port alignment.
//! Any coordinate change shows up here for review — intentional algorithm
//! tuning updates the snapshot, accidental regressions get caught.

use tautcore_engine_api::{EdgeGeometryMode, LayoutAlgorithm, LayoutInput};
use tautcore_layout::HierarchicalLayout;
use tautcore_model::attr::AttrMap;
use tautcore_model::geometry::Size;
use tautcore_model::graph::{Arrow, Edge, Graph, Node};
use tautcore_model::sizes::NodeSizes;

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

fn layout_json(graph: Graph, size: Size) -> serde_json::Value {
    let mut sizes = NodeSizes::new();
    for n in &graph.nodes {
        sizes.insert(n.id.clone(), size);
    }
    let options = AttrMap::new();
    let output = HierarchicalLayout
        .layout(LayoutInput {
            graph: &graph,
            node_sizes: &sizes,
            options: &options,
            edge_geometry: EdgeGeometryMode::Builtin,
        })
        .expect("small fixture must lay out");
    serde_json::json!({
        "nodes": output.nodes,
        "edges": output.edges,
    })
}

/// Span-3 chain `a→b→c→d` plus a long `a→d` edge: the dummy trunk of the
/// long edge must stay on one column (the phase-A straightness fix).
#[test]
fn long_edge_trunk_coordinates() {
    let graph = Graph {
        nodes: ["a", "b", "c", "d"].map(node).into(),
        edges: vec![
            edge("e0", "a", "b"),
            edge("e1", "b", "c"),
            edge("e2", "c", "d"),
            edge("e3", "a", "d"),
        ],
        groups: Vec::new(),
        partition: None,
    };
    insta::assert_json_snapshot!(layout_json(graph, Size::new(120.0, 40.0)));
}

/// Three-port fan-out of a wide source: end dummies land on slot anchor
/// columns, not the node center.
#[test]
fn fanout_port_anchor_coordinates() {
    let graph = Graph {
        nodes: ["src", "t0", "t1", "t2"].map(node).into(),
        edges: vec![
            edge("e0", "src", "t0"),
            edge("e1", "src", "t1"),
            edge("e2", "src", "t2"),
            edge("e3", "t0", "t1"),
        ],
        groups: Vec::new(),
        partition: None,
    };
    insta::assert_json_snapshot!(layout_json(graph, Size::new(120.0, 40.0)));
}
