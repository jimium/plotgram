//! `run(LayoutContract) -> LayoutResult`.

use std::collections::BTreeMap;

use plotgram_engine_api::{
    EdgeGeometryMode, LayoutError, LayoutInput, Obstacle, OrthogonalRouteParams, PortAnchor,
    RouteScene, TerminalPair,
};
use plotgram_model::contract::LayoutContract;
use plotgram_model::diagnostics::LayoutWarning;
use plotgram_model::result::{EdgePlacement, LayoutResult, NodePlacement};

use crate::finalize::finalize;
use crate::registry::Registry;
use plotgram_router::core::port_anchor;

/// Run layout (+ optional independent edge router) for a contract.
pub fn run(contract: &LayoutContract) -> Result<LayoutResult, LayoutError> {
    contract.validate_sizes()?;

    let registry = Registry::standard();
    let layout = registry
        .layout(&contract.layout.name)
        .ok_or_else(|| LayoutError::UnknownLayout {
            name: contract.layout.name.clone(),
        })?;

    let edge_geometry = if contract.edge_routing.is_some() {
        EdgeGeometryMode::DeferToRouter
    } else {
        EdgeGeometryMode::Builtin
    };

    let mut output = layout.layout(LayoutInput {
        graph: &contract.graph,
        node_sizes: &contract.node_sizes,
        options: &contract.layout.options,
        edge_geometry,
    })?;

    // Conflict semantics (edge-parameters §2.1): an explicit independent
    // edge router takes over edge geometry; an explicitly declared
    // non-default `routing_style` is then ignored — surface that instead of
    // dropping it silently. The check lives here because only the engine
    // sees both the contract's router and the layout options.
    if let Some(ref routing) = contract.edge_routing {
        if let Some(v) = contract.layout.options.get("routing_style") {
            if v.as_str().is_some_and(|s| s != "orthogonal") {
                output.diagnostics.warnings.push(LayoutWarning {
                    message: format!(
                        "layout option `routing_style` is ignored: the independent edge \
                         router `{}` takes over edge geometry",
                        routing.name
                    ),
                });
            }
        }
    }

    let edges = if let Some(ref routing) = contract.edge_routing {
        let router = registry
            .router(&routing.name)
            .ok_or_else(|| LayoutError::UnknownRouter {
                name: routing.name.clone(),
            })?;
        let scene = project_route_scene(&output.nodes, &output.edges)?;
        router.route(&scene)?
    } else {
        output.edges
    };

    // Router rewrites edge paths only; the layout's diagnostics pass through
    // unchanged (routers produce no diagnostics in this build).
    Ok(finalize(
        &contract.graph,
        output.nodes,
        edges,
        output.diagnostics,
    ))
}

/// Project layout output into a self-contained [`RouteScene`].
///
/// This is the **only** place that translates node frames + resolved ports
/// into the router's algorithm-facing input. Routers never see `Graph` or
/// `NodePlacement` directly.
fn project_route_scene(
    nodes: &[NodePlacement],
    edges: &[EdgePlacement],
) -> Result<RouteScene, LayoutError> {
    let frames: BTreeMap<&str, &NodePlacement> =
        nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    // Obstacles = all node frames (not inflated here; router inflates internally).
    let obstacles: Vec<Obstacle> = nodes
        .iter()
        .map(|n| Obstacle {
            id: n.id.clone(),
            rect: n.frame,
        })
        .collect();

    // Terminals: project resolved ports → absolute anchor points.
    let mut terminals = BTreeMap::new();
    let mut edge_order = Vec::with_capacity(edges.len());

    for edge in edges {
        let from_port = edge.from_port.ok_or_else(|| {
            LayoutError::message(format!(
                "projection: edge `{}` missing from_port (layout must resolve ports before routing)",
                edge.id
            ))
        })?;
        let to_port = edge.to_port.ok_or_else(|| {
            LayoutError::message(format!(
                "projection: edge `{}` missing to_port (layout must resolve ports before routing)",
                edge.id
            ))
        })?;

        let src_node = frames.get(edge.source.as_str()).ok_or_else(|| {
            LayoutError::message(format!(
                "projection: edge `{}` references missing node `{}`",
                edge.id, edge.source
            ))
        })?;
        let tgt_node = frames.get(edge.target.as_str()).ok_or_else(|| {
            LayoutError::message(format!(
                "projection: edge `{}` references missing node `{}`",
                edge.id, edge.target
            ))
        })?;

        terminals.insert(
            edge.id.clone(),
            TerminalPair {
                source: PortAnchor {
                    point: port_anchor(&src_node.frame, from_port),
                    side: from_port.side,
                    node_id: edge.source.clone(),
                },
                target: PortAnchor {
                    point: port_anchor(&tgt_node.frame, to_port),
                    side: to_port.side,
                    node_id: edge.target.clone(),
                },
            },
        );
        edge_order.push(edge.id.clone());
    }

    Ok(RouteScene {
        obstacles,
        terminals,
        edge_order,
        group_boundaries: Vec::new(),
        boundary_permissions: BTreeMap::new(),
        params: OrthogonalRouteParams::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use plotgram_model::attr::AttrMap;
    use plotgram_model::contract::AlgorithmRef;
    use plotgram_model::geometry::Size;
    use plotgram_model::graph::{Arrow, Edge, Graph, Node, NodeRole};
    use plotgram_model::sizes::NodeSizes;

    fn node(id: &str) -> Node {
        Node {
            id: id.to_string(),
            label: Some(id.to_string()),
            shape: None,
            role: NodeRole::Entity,
            host_group: None,
            anchor: None,
            partition_cell: None,
            attrs: AttrMap::new(),
        }
    }

    fn edge(id: &str, s: &str, t: &str) -> Edge {
        Edge {
            id: id.to_string(),
            source: s.to_string(),
            target: t.to_string(),
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

    fn sizes(ids: &[&str]) -> NodeSizes {
        let mut s = NodeSizes::new();
        for id in ids {
            s.insert(*id, Size::new(60.0, 30.0));
        }
        s
    }

    #[test]
    fn hierarchical_builtin_produces_nodes_and_edges() {
        let graph = Graph {
            nodes: vec![node("a"), node("b")],
            edges: vec![edge("e0", "a", "b")],
            groups: vec![],
            partition: None,
        };
        let contract = LayoutContract {
            layout: AlgorithmRef::new("hierarchical"),
            edge_routing: None,
            graph,
            node_sizes: sizes(&["a", "b"]),
        };
        let result = run(&contract).unwrap();
        assert_eq!(result.nodes.len(), 2);
        assert_eq!(result.edges.len(), 1);
        assert!(result.edges[0].path.samples().len() >= 2);
        assert!(result.edges[0].from_port.is_some());
        assert!(result.canvas_width > 0.0);
    }

    #[test]
    fn hierarchical_with_independent_orthogonal_router() {
        let graph = Graph {
            nodes: vec![node("a"), node("b")],
            edges: vec![edge("e0", "a", "b")],
            groups: vec![],
            partition: None,
        };
        let contract = LayoutContract {
            layout: AlgorithmRef::new("hierarchical"),
            edge_routing: Some(AlgorithmRef::new("orthogonal")),
            graph,
            node_sizes: sizes(&["a", "b"]),
        };
        let result = run(&contract).unwrap();
        assert_eq!(result.edges.len(), 1);
        assert!(result.edges[0].path.samples().len() >= 2);
    }

    #[test]
    fn hierarchical_with_independent_straight_router() {
        let graph = Graph {
            nodes: vec![node("a"), node("b")],
            edges: vec![edge("e0", "a", "b")],
            groups: vec![],
            partition: None,
        };
        let contract = LayoutContract {
            layout: AlgorithmRef::new("hierarchical"),
            edge_routing: Some(AlgorithmRef::new("straight")),
            graph,
            node_sizes: sizes(&["a", "b"]),
        };
        let result = run(&contract).unwrap();
        assert_eq!(result.edges.len(), 1);
        assert_eq!(
            result.edges[0].path.polyline_points().map(|p| p.len()),
            Some(2)
        );
    }

    #[test]
    fn hierarchical_with_independent_polyline_router() {
        let graph = Graph {
            nodes: vec![node("a"), node("b")],
            edges: vec![edge("e0", "a", "b")],
            groups: vec![],
            partition: None,
        };
        let contract = LayoutContract {
            layout: AlgorithmRef::new("hierarchical"),
            edge_routing: Some(AlgorithmRef::new("polyline")),
            graph,
            node_sizes: sizes(&["a", "b"]),
        };
        let result = run(&contract).unwrap();
        assert_eq!(result.edges.len(), 1);
        assert!(result.edges[0].path.samples().len() >= 2);
    }

    #[test]
    fn hierarchical_with_independent_octilinear_router() {
        let graph = Graph {
            nodes: vec![node("a"), node("b")],
            edges: vec![edge("e0", "a", "b")],
            groups: vec![],
            partition: None,
        };
        let contract = LayoutContract {
            layout: AlgorithmRef::new("hierarchical"),
            edge_routing: Some(AlgorithmRef::new("octilinear")),
            graph,
            node_sizes: sizes(&["a", "b"]),
        };
        let result = run(&contract).unwrap();
        assert_eq!(result.edges.len(), 1);
        assert!(result.edges[0].path.samples().len() >= 2);
    }

    #[test]
    fn hierarchical_with_independent_curved_router() {
        let graph = Graph {
            nodes: vec![node("a"), node("b")],
            edges: vec![edge("e0", "a", "b")],
            groups: vec![],
            partition: None,
        };
        let contract = LayoutContract {
            layout: AlgorithmRef::new("hierarchical"),
            edge_routing: Some(AlgorithmRef::new("curved")),
            graph,
            node_sizes: sizes(&["a", "b"]),
        };
        let result = run(&contract).unwrap();
        assert_eq!(result.edges.len(), 1);
        assert!(result.edges[0].path.samples().len() >= 2);
        assert!(matches!(
            result.edges[0].path,
            plotgram_model::result::EdgePath::Cubic { .. }
                | plotgram_model::result::EdgePath::Polyline { .. }
        ));
    }

    #[test]
    fn missing_size_errors() {
        let graph = Graph {
            nodes: vec![node("a"), node("b")],
            edges: vec![],
            groups: vec![],
            partition: None,
        };
        let contract = LayoutContract {
            layout: AlgorithmRef::new("hierarchical"),
            edge_routing: None,
            graph,
            node_sizes: sizes(&["a"]),
        };
        assert!(matches!(
            run(&contract),
            Err(LayoutError::MissingNodeSize(_))
        ));
    }

    /// Conflict semantics: an explicit independent router ignores a
    /// non-default `routing_style` option and must say so (edge-parameters
    /// §2.1). Orthogonal (= the builtin default shape family) stays silent.
    #[test]
    fn routing_style_conflict_warns_when_router_takes_over() {
        use plotgram_model::attr::AttrValue;
        let graph = || Graph {
            nodes: vec![node("a"), node("b")],
            edges: vec![edge("e0", "a", "b")],
            groups: vec![],
            partition: None,
        };
        let layout_with_style = |style: &str| {
            AlgorithmRef::with_options(
                "hierarchical",
                [(
                    "routing_style".to_string(),
                    AttrValue::Atom(style.to_string()),
                )]
                .into_iter()
                .collect(),
            )
        };

        // Non-default style + explicit router → warning.
        let contract = LayoutContract {
            layout: layout_with_style("polyline"),
            edge_routing: Some(AlgorithmRef::new("orthogonal")),
            graph: graph(),
            node_sizes: sizes(&["a", "b"]),
        };
        let result = run(&contract).unwrap();
        assert!(
            result
                .diagnostics
                .warnings
                .iter()
                .any(|w| w.message.contains("routing_style")),
            "expected a routing_style conflict warning, got {:?}",
            result.diagnostics.warnings
        );

        // Orthogonal style or no router → no conflict warning.
        for contract in [
            LayoutContract {
                layout: layout_with_style("orthogonal"),
                edge_routing: Some(AlgorithmRef::new("orthogonal")),
                graph: graph(),
                node_sizes: sizes(&["a", "b"]),
            },
            LayoutContract {
                layout: layout_with_style("polyline"),
                edge_routing: None,
                graph: graph(),
                node_sizes: sizes(&["a", "b"]),
            },
        ] {
            let result = run(&contract).unwrap();
            assert!(
                !result
                    .diagnostics
                    .warnings
                    .iter()
                    .any(|w| w.message.contains("routing_style")),
                "no conflict warning expected, got {:?}",
                result.diagnostics.warnings
            );
        }
    }
}
