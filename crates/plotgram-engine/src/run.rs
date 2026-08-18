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
    let layout =
        registry
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
        output.groups,
        output.owns_group_frames,
        output.diagnostics,
        output.decorations,
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
    let frames: BTreeMap<&str, &NodePlacement> = nodes.iter().map(|n| (n.id.as_str(), n)).collect();

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
    use plotgram_model::attr::{AttrMap, AttrValue};
    use plotgram_model::contract::AlgorithmRef;
    use plotgram_model::geometry::{Point, Rect, Size};
    use plotgram_model::graph::{Arrow, Edge, Graph, Node, NodeRole};
    use plotgram_model::result::Decoration;
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

    fn node_attr(id: &str, key: &str, val: AttrValue) -> Node {
        let mut n = node(id);
        n.attrs.insert(key.into(), val);
        n
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

    fn frag_edge(id: &str, s: &str, t: &str, fragment: &str, extra: &[(&str, AttrValue)]) -> Edge {
        let mut e = edge(id, s, t);
        e.attrs
            .insert("fragment".into(), AttrValue::Atom(fragment.into()));
        for (k, v) in extra {
            e.attrs.insert((*k).into(), v.clone());
        }
        e
    }

    fn sizes(ids: &[&str]) -> NodeSizes {
        let mut s = NodeSizes::new();
        for id in ids {
            s.insert(*id, Size::new(60.0, 30.0));
        }
        s
    }

    fn tree_options(pairs: &[(&str, AttrValue)]) -> AlgorithmRef {
        AlgorithmRef::with_options(
            "tree",
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        )
    }

    fn seq_options(pairs: &[(&str, AttrValue)]) -> AlgorithmRef {
        AlgorithmRef::with_options(
            "sequence",
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        )
    }

    fn circular_options(pairs: &[(&str, AttrValue)]) -> AlgorithmRef {
        AlgorithmRef::with_options(
            "circular",
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        )
    }

    fn circular_cycle() -> AlgorithmRef {
        circular_options(&[
            ("partitioning", AttrValue::Atom("single-cycle".into())),
            ("order", AttrValue::Atom("bfs".into())),
        ])
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
        assert!(
            result.decorations.is_empty(),
            "hierarchical must not invent sequence decorations"
        );
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

    #[test]
    fn sequence_builtin_places_participants_and_horizontal_message() {
        let graph = Graph {
            nodes: vec![node("alice"), node("bob")],
            edges: vec![edge("e0", "alice", "bob")],
            groups: vec![],
            partition: None,
        };
        let contract = LayoutContract {
            layout: AlgorithmRef::new("sequence"),
            edge_routing: None,
            graph,
            node_sizes: sizes(&["alice", "bob"]),
        };
        let result = run(&contract).unwrap();
        assert_eq!(result.nodes.len(), 2);
        assert_eq!(result.edges.len(), 1);
        let alice = result.nodes.iter().find(|n| n.id == "alice").unwrap();
        let bob = result.nodes.iter().find(|n| n.id == "bob").unwrap();
        assert!(
            alice.frame.x < bob.frame.x,
            "declaration order is left-to-right"
        );
        let pts = result.edges[0].path.polyline_points().unwrap();
        assert_eq!(pts.len(), 2);
        assert!(
            (pts[0].y - pts[1].y).abs() < 1e-6,
            "sync message is horizontal"
        );
        assert!(pts[0].y > alice.frame.bottom());
        assert_eq!(result.decorations.len(), 3); // 2 lifelines + 1 unclosed activation
        assert!(result.decorations.iter().any(|d| matches!(
            d,
            Decoration::Lifeline { participant, .. } if participant == "alice"
        )));
        assert!(result
            .decorations
            .iter()
            .any(|d| matches!(d, Decoration::Activation { .. })));
    }

    #[test]
    fn sequence_self_call_is_u_shaped() {
        let graph = Graph {
            nodes: vec![node("alice")],
            edges: vec![edge("e0", "alice", "alice")],
            groups: vec![],
            partition: None,
        };
        let result = run(&LayoutContract {
            layout: AlgorithmRef::new("sequence"),
            edge_routing: None,
            graph,
            node_sizes: sizes(&["alice"]),
        })
        .unwrap();
        let pts = result.edges[0].path.polyline_points().unwrap();
        assert!(pts.len() >= 4, "SelfLoop expands to a U");
        assert!((pts[0].x - pts[pts.len() - 1].x).abs() < 1e-6);
        assert!(pts[1].x > pts[0].x, "default SelfLoop probes east");
    }

    #[test]
    fn sequence_rejects_independent_router() {
        let graph = Graph {
            nodes: vec![node("a"), node("b")],
            edges: vec![edge("e0", "a", "b")],
            groups: vec![],
            partition: None,
        };
        let err = run(&LayoutContract {
            layout: AlgorithmRef::new("sequence"),
            edge_routing: Some(AlgorithmRef::new("orthogonal")),
            graph,
            node_sizes: sizes(&["a", "b"]),
        })
        .unwrap_err();
        assert!(matches!(
            err,
            LayoutError::LayoutCannotDeferEdges { ref layout } if layout == "sequence"
        ));
    }

    #[test]
    fn sequence_m2_notch_records_crossing_gap() {
        let graph = Graph {
            nodes: vec![node("a"), node("b"), node("c")],
            edges: vec![edge("e0", "a", "c")],
            groups: vec![],
            partition: None,
        };
        let result = run(&LayoutContract {
            layout: AlgorithmRef::new("sequence"),
            edge_routing: None,
            graph,
            node_sizes: sizes(&["a", "b", "c"]),
        })
        .unwrap();
        let mid = result.decorations.iter().find_map(|d| match d {
            Decoration::Lifeline {
                participant, gaps, ..
            } if participant == "b" => Some(gaps.clone()),
            _ => None,
        });
        let gaps = mid.expect("lifeline b");
        assert!(
            !gaps.is_empty(),
            "crossing of b must become a notch y, got {gaps:?}"
        );
    }

    #[test]
    fn sequence_m2_attach_depth_offsets_callee() {
        let graph = Graph {
            nodes: vec![node("a"), node("b")],
            edges: vec![edge("e0", "a", "b")],
            groups: vec![],
            partition: None,
        };
        let result = run(&LayoutContract {
            layout: AlgorithmRef::new("sequence"),
            edge_routing: None,
            graph,
            node_sizes: sizes(&["a", "b"]),
        })
        .unwrap();
        let a = result.nodes.iter().find(|n| n.id == "a").unwrap();
        let b = result.nodes.iter().find(|n| n.id == "b").unwrap();
        let ax = a.frame.x + a.frame.width / 2.0;
        let bx = b.frame.x + b.frame.width / 2.0;
        let pts = result.edges[0].path.polyline_points().unwrap();
        let from_off = (pts[0].x - ax).abs();
        let to_off = (pts[1].x - bx).abs();
        assert!(
            to_off > from_off + 1.0,
            "callee terminal should sit on the activation bar, from_off={from_off} to_off={to_off}"
        );
    }

    #[test]
    fn sequence_m3_greedy_shortens_span() {
        let graph = Graph {
            nodes: vec![node("a"), node("c"), node("b")],
            edges: vec![edge("e0", "a", "b"), edge("e1", "b", "c")],
            groups: vec![],
            partition: None,
        };
        let result = run(&LayoutContract {
            layout: seq_options(&[("lifeline_order", AttrValue::Atom("greedy".into()))]),
            edge_routing: None,
            graph,
            node_sizes: sizes(&["a", "c", "b"]),
        })
        .unwrap();
        let ids: Vec<&str> = result.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(ids, ["a", "b", "c"], "greedy should sit b between a and c");
    }

    #[test]
    fn sequence_m3_pin_is_not_overridden() {
        let mut c = node("c");
        c.attrs
            .insert("lifeline_pin".into(), AttrValue::Atom("left".into()));
        let graph = Graph {
            nodes: vec![node("a"), c, node("b")],
            edges: vec![edge("e0", "a", "b"), edge("e1", "b", "c")],
            groups: vec![],
            partition: None,
        };
        let result = run(&LayoutContract {
            layout: seq_options(&[("lifeline_order", AttrValue::Atom("greedy".into()))]),
            edge_routing: None,
            graph,
            node_sizes: sizes(&["a", "c", "b"]),
        })
        .unwrap();
        assert_eq!(result.nodes[0].id, "c", "left pin must survive greedy");
    }

    #[test]
    fn sequence_m4_fragment_frame_covers_members() {
        let graph = Graph {
            nodes: vec![node("a"), node("b")],
            edges: vec![
                frag_edge(
                    "e0",
                    "a",
                    "b",
                    "retry",
                    &[
                        ("fragment_kind", AttrValue::Atom("loop".into())),
                        ("fragment_label", AttrValue::Str("3x".into())),
                    ],
                ),
                frag_edge("e1", "b", "a", "retry", &[]),
            ],
            groups: vec![],
            partition: None,
        };
        let result = run(&LayoutContract {
            layout: AlgorithmRef::new("sequence"),
            edge_routing: None,
            graph,
            node_sizes: sizes(&["a", "b"]),
        })
        .unwrap();
        assert!(result.groups.is_empty(), "fragments must not enter groups");
        let frame = result.decorations.iter().find_map(|d| match d {
            Decoration::FragmentFrame {
                id,
                operator,
                label,
                frame,
                ..
            } if id == "fragment:retry" => {
                assert_eq!(operator, "loop");
                assert_eq!(label.as_deref(), Some("3x"));
                Some(*frame)
            }
            _ => None,
        });
        let frame = frame.expect("FragmentFrame retry");
        let a = result.nodes.iter().find(|n| n.id == "a").unwrap().frame;
        let b = result.nodes.iter().find(|n| n.id == "b").unwrap().frame;
        assert!(
            frame.x <= a.x + 1.0 && frame.right() + 1.0 >= b.right(),
            "fragment should cover both participants, frame={frame:?} a={a:?} b={b:?}"
        );
        let y0 = result.edges[0].path.polyline_points().unwrap()[0].y;
        let y1 = result.edges[1].path.polyline_points().unwrap()[0].y;
        assert!(
            frame.y < y0 && frame.bottom() > y1,
            "fragment should cover member message y, frame={frame:?} y0={y0} y1={y1}"
        );
    }

    #[test]
    fn sequence_m4_nested_contains_child() {
        let graph = Graph {
            nodes: vec![node("a"), node("b"), node("c")],
            edges: vec![
                frag_edge(
                    "e0",
                    "a",
                    "b",
                    "outer",
                    &[("fragment_kind", AttrValue::Atom("alt".into()))],
                ),
                frag_edge(
                    "e1",
                    "b",
                    "c",
                    "outer/inner",
                    &[("fragment_kind", AttrValue::Atom("loop".into()))],
                ),
                frag_edge("e2", "c", "b", "outer/inner", &[]),
                frag_edge("e3", "b", "a", "outer", &[]),
            ],
            groups: vec![],
            partition: None,
        };
        let result = run(&LayoutContract {
            layout: AlgorithmRef::new("sequence"),
            edge_routing: None,
            graph,
            node_sizes: sizes(&["a", "b", "c"]),
        })
        .unwrap();
        let outer = result.decorations.iter().find_map(|d| match d {
            Decoration::FragmentFrame { id, frame, .. } if id == "fragment:outer" => Some(*frame),
            _ => None,
        });
        let inner = result.decorations.iter().find_map(|d| match d {
            Decoration::FragmentFrame { id, frame, .. } if id == "fragment:inner" => Some(*frame),
            _ => None,
        });
        let outer = outer.expect("outer");
        let inner = inner.expect("inner");
        assert!(
            outer.x <= inner.x + 1e-6
                && outer.y <= inner.y + 1e-6
                && outer.right() + 1e-6 >= inner.right()
                && outer.bottom() + 1e-6 >= inner.bottom(),
            "outer must contain inner, outer={outer:?} inner={inner:?}"
        );
        let outer_area = outer.width * outer.height;
        let inner_area = inner.width * inner.height;
        assert!(
            outer_area > inner_area + 1.0,
            "outer should be strictly larger"
        );
    }

    #[test]
    fn sequence_m4_overlap_is_rejected() {
        let graph = Graph {
            nodes: vec![node("a"), node("b"), node("c")],
            edges: vec![
                frag_edge("e0", "a", "b", "p", &[]),
                frag_edge("e1", "b", "c", "q", &[]),
                frag_edge("e2", "a", "b", "p", &[]),
            ],
            groups: vec![],
            partition: None,
        };
        let err = run(&LayoutContract {
            layout: AlgorithmRef::new("sequence"),
            edge_routing: None,
            graph,
            node_sizes: sizes(&["a", "b", "c"]),
        })
        .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("overlap"),
            "expected overlap infeasibility, got {msg}"
        );
    }

    #[test]
    fn sequence_alt_else_reply_does_not_warn() {
        let mut e_call = frag_edge(
            "e0",
            "a",
            "b",
            "checkout",
            &[("fragment_kind", AttrValue::Atom("alt".into()))],
        );
        e_call
            .attrs
            .insert("fragment_operand".into(), AttrValue::Num(0.0));
        let mut e_ok = frag_edge("e1", "b", "a", "checkout", &[]);
        e_ok.arrow = Arrow::Response;
        e_ok.attrs
            .insert("fragment_operand".into(), AttrValue::Num(0.0));
        let mut e_fail = frag_edge("e2", "b", "a", "checkout", &[]);
        e_fail.arrow = Arrow::Response;
        e_fail
            .attrs
            .insert("fragment_operand".into(), AttrValue::Num(1.0));
        let graph = Graph {
            nodes: vec![node("a"), node("b")],
            edges: vec![e_call, e_ok, e_fail],
            groups: vec![],
            partition: None,
        };
        let result = run(&LayoutContract {
            layout: AlgorithmRef::new("sequence"),
            edge_routing: None,
            graph,
            node_sizes: sizes(&["a", "b"]),
        })
        .unwrap();
        assert!(
            !result
                .diagnostics
                .warnings
                .iter()
                .any(|w| w.message.contains("unpaired reply")),
            "alt else return should not warn, got {:?}",
            result.diagnostics.warnings
        );
        assert!(
            result.decorations.iter().any(|d| matches!(
                d,
                Decoration::FragmentFrame { id, operator, .. }
                    if id == "fragment:checkout" && operator == "alt"
            )),
            "expected alt fragment frame, got {:?}",
            result.decorations
        );
    }

    #[test]
    fn tree_builtin_places_parent_above_children() {
        let graph = Graph {
            nodes: vec![node("root"), node("l"), node("r")],
            edges: vec![edge("e0", "root", "l"), edge("e1", "root", "r")],
            groups: vec![],
            partition: None,
        };
        let result = run(&LayoutContract {
            layout: AlgorithmRef::new("tree"),
            edge_routing: None,
            graph,
            node_sizes: sizes(&["root", "l", "r"]),
        })
        .unwrap();
        let frame = |id: &str| result.nodes.iter().find(|n| n.id == id).unwrap().frame;
        assert!(frame("root").bottom() <= frame("l").y + 1e-6);
        assert!(frame("l").x < frame("r").x);
        let mid = (frame("l").center().x + frame("r").center().x) / 2.0;
        assert!(
            (frame("root").center().x - mid).abs() < 1e-6,
            "parent should be centered over children, root={} mid={}",
            frame("root").center().x,
            mid
        );
        assert_eq!(result.edges.len(), 2);
        assert!(result.edges[0].path.polyline_points().unwrap().len() >= 2);
    }

    #[test]
    fn tree_split_layered_opens_left_and_right() {
        let graph = Graph {
            nodes: vec![node("root"), node("l"), node("r")],
            edges: vec![edge("e0", "root", "l"), edge("e1", "root", "r")],
            groups: vec![],
            partition: None,
        };
        let mut options = AttrMap::new();
        options.insert(
            "placer".into(),
            AttrValue::Atom("single-split-layered".into()),
        );
        let result = run(&LayoutContract {
            layout: AlgorithmRef::with_options("tree", options),
            edge_routing: None,
            graph,
            node_sizes: sizes(&["root", "l", "r"]),
        })
        .unwrap();
        let frame = |id: &str| result.nodes.iter().find(|n| n.id == id).unwrap().frame;
        assert!(
            frame("l").right() <= frame("root").x + 1e-6,
            "primary child should sit to the left, l={:?} root={:?}",
            frame("l"),
            frame("root")
        );
        assert!(
            frame("r").x + 1e-6 >= frame("root").right(),
            "secondary child should sit to the right, r={:?} root={:?}",
            frame("r"),
            frame("root")
        );
    }

    #[test]
    fn tree_named_unimplemented_placer_is_unsupported() {
        let cases = ["single-split", "multi-layer", "fixed"];
        for name in cases {
            let graph = Graph {
                nodes: vec![node("root"), node("a")],
                edges: vec![edge("e0", "root", "a")],
                groups: vec![],
                partition: None,
            };
            let err = run(&LayoutContract {
                layout: tree_options(&[("placer", AttrValue::Atom(name.into()))]),
                edge_routing: None,
                graph,
                node_sizes: sizes(&["root", "a"]),
            })
            .unwrap_err();
            assert!(
                matches!(err, LayoutError::Unsupported { .. }),
                "placer `{name}` must be Unsupported, got {err}"
            );
        }
    }

    #[test]
    fn tree_dendrogram_aligns_leaf_bottoms() {
        let graph = Graph {
            nodes: vec![node("root"), node("a"), node("b"), node("c")],
            edges: vec![
                edge("e0", "root", "a"),
                edge("e1", "root", "b"),
                edge("e2", "b", "c"),
            ],
            groups: vec![],
            partition: None,
        };
        let result = run(&LayoutContract {
            layout: tree_options(&[("placer", AttrValue::Atom("dendrogram".into()))]),
            edge_routing: None,
            graph,
            node_sizes: sizes(&["root", "a", "b", "c"]),
        })
        .unwrap();
        let frame = |id: &str| result.nodes.iter().find(|n| n.id == id).unwrap().frame;
        assert!(
            (frame("a").bottom() - frame("c").bottom()).abs() < 1e-6,
            "leaves should share a baseline, a={} c={}",
            frame("a").bottom(),
            frame("c").bottom()
        );
        assert!(frame("b").bottom() <= frame("c").y + 1e-6);
        assert!(frame("root").bottom() <= frame("b").y + 1e-6);
    }

    #[test]
    fn tree_double_layer_is_narrower_than_single_layer() {
        let ids = ["root", "a", "b", "c", "d", "e", "f"];
        let graph = Graph {
            nodes: ids.iter().copied().map(node).collect(),
            edges: vec![
                edge("e0", "root", "a"),
                edge("e1", "root", "b"),
                edge("e2", "root", "c"),
                edge("e3", "root", "d"),
                edge("e4", "root", "e"),
                edge("e5", "root", "f"),
            ],
            groups: vec![],
            partition: None,
        };
        let span = |placer: &str| {
            let result = run(&LayoutContract {
                layout: tree_options(&[("placer", AttrValue::Atom(placer.into()))]),
                edge_routing: None,
                graph: graph.clone(),
                node_sizes: sizes(&ids),
            })
            .unwrap();
            let min_x = result
                .nodes
                .iter()
                .map(|n| n.frame.x)
                .fold(f64::INFINITY, f64::min);
            let max_r = result
                .nodes
                .iter()
                .map(|n| n.frame.right())
                .fold(f64::NEG_INFINITY, f64::max);
            max_r - min_x
        };
        let single = span("single-layer");
        let double = span("double-layer");
        assert!(
            double + 1.0 < single,
            "double-layer should be narrower, single={single} double={double}"
        );
    }

    #[test]
    fn tree_root_alignment_leading_pins_parent_to_left_child() {
        let graph = Graph {
            nodes: vec![node("root"), node("l"), node("m"), node("r")],
            edges: vec![
                edge("e0", "root", "l"),
                edge("e1", "root", "m"),
                edge("e2", "root", "r"),
            ],
            groups: vec![],
            partition: None,
        };
        let result = run(&LayoutContract {
            layout: tree_options(&[("root_alignment", AttrValue::Atom("leading".into()))]),
            edge_routing: None,
            graph,
            node_sizes: sizes(&["root", "l", "m", "r"]),
        })
        .unwrap();
        let frame = |id: &str| result.nodes.iter().find(|n| n.id == id).unwrap().frame;
        assert!(
            (frame("root").x - frame("l").x).abs() < 1e-6,
            "leading parent.x should match leftmost child, root={} l={}",
            frame("root").x,
            frame("l").x
        );
        let mid = (frame("l").center().x + frame("r").center().x) / 2.0;
        assert!(
            (frame("root").center().x - mid).abs() > 1.0,
            "leading must not keep the Buchheim center"
        );
    }

    #[test]
    fn tree_orthogonal_at_root_drops_vertically_first() {
        let graph = Graph {
            nodes: vec![node("root"), node("l"), node("r")],
            edges: vec![edge("e0", "root", "l"), edge("e1", "root", "r")],
            groups: vec![],
            partition: None,
        };
        let result = run(&LayoutContract {
            layout: tree_options(&[(
                "routing_style",
                AttrValue::Atom("orthogonal-at-root".into()),
            )]),
            edge_routing: None,
            graph,
            node_sizes: sizes(&["root", "l", "r"]),
        })
        .unwrap();
        for e in &result.edges {
            let pts = e.path.polyline_points().unwrap();
            assert!(
                pts.len() >= 3,
                "orthogonal-at-root should bend, got {pts:?}"
            );
            assert!(
                (pts[0].x - pts[1].x).abs() < 1e-6,
                "first segment should be vertical, got {pts:?}"
            );
            assert!(
                pts[1].y > pts[0].y + 1e-6,
                "first segment should drop from the parent, got {pts:?}"
            );
        }
    }

    #[test]
    fn tree_orthogonal_edges_keep_visible_final_drop_into_child() {
        // min_first_segment == layer_gap in every preset; the elbow must not
        // pin onto the child row's top edge, or the final segment degenerates
        // and the arrowhead arrives sideways along the node tops.
        for style in ["orthogonal", "orthogonal-at-root"] {
            let graph = Graph {
                nodes: vec![node("root"), node("l"), node("r")],
                edges: vec![edge("e0", "root", "l"), edge("e1", "root", "r")],
                groups: vec![],
                partition: None,
            };
            let result = run(&LayoutContract {
                layout: tree_options(&[("routing_style", AttrValue::Atom(style.into()))]),
                edge_routing: None,
                graph,
                node_sizes: sizes(&["root", "l", "r"]),
            })
            .unwrap();
            for e in &result.edges {
                let pts = e.path.polyline_points().unwrap();
                let last = pts.len() - 1;
                assert!(
                    pts[last].y > pts[last - 1].y + 8.0,
                    "{style}: arrow should approach the child port downward, got {pts:?}"
                );
                for p in &pts[..last] {
                    assert!(
                        p.y < pts[last].y - 8.0,
                        "{style}: elbow must stay clear of the child row's top edge, got {pts:?}"
                    );
                }
                if style == "orthogonal" {
                    assert!(
                        (pts[last].x - pts[last - 1].x).abs() < 1e-6,
                        "{style}: final segment should be vertical, got {pts:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn tree_assistant_sits_beside_bus_regulars_below() {
        let graph = Graph {
            nodes: vec![
                node("root"),
                node_attr("asst", "assistant", AttrValue::Bool(true)),
                node("a"),
                node("b"),
            ],
            edges: vec![
                edge("e0", "root", "asst"),
                edge("e1", "root", "a"),
                edge("e2", "root", "b"),
            ],
            groups: vec![],
            partition: None,
        };
        let result = run(&LayoutContract {
            layout: tree_options(&[("placer", AttrValue::Atom("assistant".into()))]),
            edge_routing: None,
            graph,
            node_sizes: sizes(&["root", "asst", "a", "b"]),
        })
        .unwrap();
        let frame = |id: &str| result.nodes.iter().find(|n| n.id == id).unwrap().frame;
        assert!(
            frame("asst").right() <= frame("root").center().x + 1e-6,
            "first assistant should sit on the left bus, asst={:?} root={:?}",
            frame("asst"),
            frame("root")
        );
        assert!(
            frame("a").y + 1e-6 >= frame("asst").bottom(),
            "regular children should sit below assistants, a={:?} asst={:?}",
            frame("a"),
            frame("asst")
        );
        assert!(frame("a").x < frame("b").x);
        let mid = (frame("a").center().x + frame("b").center().x) / 2.0;
        assert!(
            (frame("root").center().x - mid).abs() < 1.0,
            "regulars should be centered under the parent"
        );
    }

    #[test]
    fn tree_assistant_without_flags_behaves_like_single_layer() {
        let graph = Graph {
            nodes: vec![node("root"), node("l"), node("r")],
            edges: vec![edge("e0", "root", "l"), edge("e1", "root", "r")],
            groups: vec![],
            partition: None,
        };
        let result = run(&LayoutContract {
            layout: tree_options(&[("placer", AttrValue::Atom("assistant".into()))]),
            edge_routing: None,
            graph,
            node_sizes: sizes(&["root", "l", "r"]),
        })
        .unwrap();
        let frame = |id: &str| result.nodes.iter().find(|n| n.id == id).unwrap().frame;
        let mid = (frame("l").center().x + frame("r").center().x) / 2.0;
        assert!(
            (frame("root").center().x - mid).abs() < 1e-6,
            "barren assistant should stamp to single-layer"
        );
    }

    #[test]
    fn tree_compact_area_beats_single_layer_fanout() {
        let ids = ["root", "a", "b", "c", "d", "e", "f"];
        let graph = Graph {
            nodes: ids.iter().copied().map(node).collect(),
            edges: vec![
                edge("e0", "root", "a"),
                edge("e1", "root", "b"),
                edge("e2", "root", "c"),
                edge("e3", "root", "d"),
                edge("e4", "root", "e"),
                edge("e5", "root", "f"),
            ],
            groups: vec![],
            partition: None,
        };
        let area = |placer: &str| {
            let result = run(&LayoutContract {
                layout: tree_options(&[("placer", AttrValue::Atom(placer.into()))]),
                edge_routing: None,
                graph: graph.clone(),
                node_sizes: sizes(&ids),
            })
            .unwrap();
            let min_x = result
                .nodes
                .iter()
                .map(|n| n.frame.x)
                .fold(f64::INFINITY, f64::min);
            let min_y = result
                .nodes
                .iter()
                .map(|n| n.frame.y)
                .fold(f64::INFINITY, f64::min);
            let max_r = result
                .nodes
                .iter()
                .map(|n| n.frame.right())
                .fold(f64::NEG_INFINITY, f64::max);
            let max_b = result
                .nodes
                .iter()
                .map(|n| n.frame.bottom())
                .fold(f64::NEG_INFINITY, f64::max);
            (max_r - min_x) * (max_b - min_y)
        };
        let single = area("single-layer");
        let compact = area("compact");
        assert!(
            compact + 1.0 < single,
            "compact should reduce area, single={single} compact={compact}"
        );
    }

    #[test]
    fn tree_aspect_ratio_pins_parent_to_corner_and_tracks_ratio() {
        let ids = ["root", "a", "b", "c", "d", "e", "f"];
        let graph = Graph {
            nodes: ids.iter().copied().map(node).collect(),
            edges: vec![
                edge("e0", "root", "a"),
                edge("e1", "root", "b"),
                edge("e2", "root", "c"),
                edge("e3", "root", "d"),
                edge("e4", "root", "e"),
                edge("e5", "root", "f"),
            ],
            groups: vec![],
            partition: None,
        };
        let layout = |ratio: f64| {
            run(&LayoutContract {
                layout: tree_options(&[
                    ("placer", AttrValue::Atom("aspect-ratio".into())),
                    ("preferred_aspect_ratio", AttrValue::Num(ratio)),
                ]),
                edge_routing: None,
                graph: graph.clone(),
                node_sizes: sizes(&ids),
            })
            .unwrap()
        };
        let wide = layout(2.0);
        let tall = layout(0.5);
        let frame =
            |res: &LayoutResult, id: &str| res.nodes.iter().find(|n| n.id == id).unwrap().frame;
        let min_child_x = ["a", "b", "c", "d", "e", "f"]
            .iter()
            .map(|id| frame(&wide, id).x)
            .fold(f64::INFINITY, f64::min);
        assert!(
            (frame(&wide, "root").x - min_child_x).abs() < 1e-6,
            "aspect-ratio root stays in the top-left corner"
        );
        let mid = (frame(&wide, "a").center().x + frame(&wide, "f").center().x) / 2.0;
        assert!(
            (frame(&wide, "root").center().x - mid).abs() > 1.0,
            "corner placement must not center the parent"
        );
        let span = |res: &LayoutResult| {
            let min_x = res
                .nodes
                .iter()
                .map(|n| n.frame.x)
                .fold(f64::INFINITY, f64::min);
            let min_y = res
                .nodes
                .iter()
                .map(|n| n.frame.y)
                .fold(f64::INFINITY, f64::min);
            let max_r = res
                .nodes
                .iter()
                .map(|n| n.frame.right())
                .fold(f64::NEG_INFINITY, f64::max);
            let max_b = res
                .nodes
                .iter()
                .map(|n| n.frame.bottom())
                .fold(f64::NEG_INFINITY, f64::max);
            (max_r - min_x, max_b - min_y)
        };
        let (ww, wh) = span(&wide);
        let (tw, th) = span(&tall);
        assert!(
            ww / wh > tw / th + 0.1,
            "ratio 2 should be wider than ratio 0.5, wide={ww}x{wh} tall={tw}x{th}"
        );
    }

    fn center_dist(a: &Rect, b: &Rect) -> f64 {
        let ca = a.center();
        let cb = b.center();
        ((ca.x - cb.x).powi(2) + (ca.y - cb.y).powi(2)).sqrt()
    }

    #[test]
    fn tree_radial_same_depth_shares_radius() {
        let graph = Graph {
            nodes: vec![node("root"), node("a"), node("a1"), node("b"), node("b1")],
            edges: vec![
                edge("e0", "root", "a"),
                edge("e1", "a", "a1"),
                edge("e2", "root", "b"),
                edge("e3", "b", "b1"),
            ],
            groups: vec![],
            partition: None,
        };
        let result = run(&LayoutContract {
            layout: tree_options(&[("placer", AttrValue::Atom("radial".into()))]),
            edge_routing: None,
            graph,
            node_sizes: sizes(&["root", "a", "a1", "b", "b1"]),
        })
        .unwrap();
        let frame = |id: &str| result.nodes.iter().find(|n| n.id == id).unwrap().frame;
        let d_a = center_dist(&frame("root"), &frame("a"));
        let d_b = center_dist(&frame("root"), &frame("b"));
        let d_a1 = center_dist(&frame("root"), &frame("a1"));
        let d_b1 = center_dist(&frame("root"), &frame("b1"));
        assert!(
            (d_a - d_b).abs() < 1e-6,
            "same-depth children should share a radius, a={d_a} b={d_b}"
        );
        assert!(
            (d_a1 - d_b1).abs() < 1e-6,
            "same-depth grandchildren should share a radius, a1={d_a1} b1={d_b1}"
        );
        assert!(
            d_a1 > d_a + 1.0,
            "grandchild should sit on a farther ring, a={d_a} a1={d_a1}"
        );
    }

    #[test]
    fn tree_balloon_packs_children_around_parent() {
        let graph = Graph {
            nodes: vec![
                node("root"),
                node("a"),
                node("a1"),
                node("a2"),
                node("a3"),
                node("b"),
                node("b1"),
            ],
            edges: vec![
                edge("e0", "root", "a"),
                edge("e1", "a", "a1"),
                edge("e2", "a", "a2"),
                edge("e3", "a", "a3"),
                edge("e4", "root", "b"),
                edge("e5", "b", "b1"),
            ],
            groups: vec![],
            partition: None,
        };
        let balloon = run(&LayoutContract {
            layout: tree_options(&[("placer", AttrValue::Atom("balloon".into()))]),
            edge_routing: None,
            graph: graph.clone(),
            node_sizes: sizes(&["root", "a", "a1", "a2", "a3", "b", "b1"]),
        })
        .unwrap();
        let radial = run(&LayoutContract {
            layout: tree_options(&[("placer", AttrValue::Atom("radial".into()))]),
            edge_routing: None,
            graph,
            node_sizes: sizes(&["root", "a", "a1", "a2", "a3", "b", "b1"]),
        })
        .unwrap();
        let frame =
            |res: &LayoutResult, id: &str| res.nodes.iter().find(|n| n.id == id).unwrap().frame;
        let ba = frame(&balloon, "a");
        let d_a1 = center_dist(&ba, &frame(&balloon, "a1"));
        let d_a2 = center_dist(&ba, &frame(&balloon, "a2"));
        let d_a3 = center_dist(&ba, &frame(&balloon, "a3"));
        assert!(
            (d_a1 - d_a2).abs() < 1e-6 && (d_a1 - d_a3).abs() < 1e-6,
            "balloon children should share a ring around their parent, {d_a1} {d_a2} {d_a3}"
        );
        assert!(
            (center_dist(&frame(&balloon, "root"), &ba)
                - center_dist(&frame(&balloon, "root"), &frame(&balloon, "b")))
            .abs()
                < 1e-6,
            "balloon siblings of the root sit on one ring"
        );
        let br = frame(&balloon, "root");
        let balloon_from_root = [
            center_dist(&br, &frame(&balloon, "a1")),
            center_dist(&br, &frame(&balloon, "a2")),
            center_dist(&br, &frame(&balloon, "a3")),
        ];
        let balloon_spread = balloon_from_root.iter().copied().fold(0.0, f64::max)
            - balloon_from_root
                .iter()
                .copied()
                .fold(f64::INFINITY, f64::min);
        assert!(
            balloon_spread > 1.0,
            "balloon grandchildren orbit their parent, not the global root, spread={balloon_spread}"
        );
        let rr = frame(&radial, "root");
        let radial_from_root = [
            center_dist(&rr, &frame(&radial, "a1")),
            center_dist(&rr, &frame(&radial, "a2")),
            center_dist(&rr, &frame(&radial, "a3")),
            center_dist(&rr, &frame(&radial, "b1")),
        ];
        let radial_spread = radial_from_root.iter().copied().fold(0.0, f64::max)
            - radial_from_root
                .iter()
                .copied()
                .fold(f64::INFINITY, f64::min);
        assert!(
            radial_spread < 1e-6,
            "radial same-depth nodes stay concentric, spread={radial_spread}"
        );
    }

    #[test]
    fn tree_mixed_subtree_placer_uses_left_right_locally() {
        let graph = Graph {
            nodes: vec![
                node("root"),
                node_attr("a", "subtree_placer", AttrValue::Atom("left-right".into())),
                node("a1"),
                node("a2"),
                node("a3"),
                node("b"),
                node("b1"),
                node("b2"),
                node("b3"),
            ],
            edges: vec![
                edge("e0", "root", "a"),
                edge("e1", "a", "a1"),
                edge("e2", "a", "a2"),
                edge("e3", "a", "a3"),
                edge("e4", "root", "b"),
                edge("e5", "b", "b1"),
                edge("e6", "b", "b2"),
                edge("e7", "b", "b3"),
            ],
            groups: vec![],
            partition: None,
        };
        let result = run(&LayoutContract {
            layout: tree_options(&[("placer", AttrValue::Atom("single-layer".into()))]),
            edge_routing: None,
            graph,
            node_sizes: sizes(&["root", "a", "a1", "a2", "a3", "b", "b1", "b2", "b3"]),
        })
        .unwrap();
        let frame = |id: &str| result.nodes.iter().find(|n| n.id == id).unwrap().frame;
        assert!(
            frame("a1").right() <= frame("a").center().x + 1e-6,
            "left-right first child sits left of the bus, a1={:?} a={:?}",
            frame("a1"),
            frame("a")
        );
        assert!(
            frame("a2").x + 1e-6 >= frame("a").center().x,
            "left-right second child sits right of the bus, a2={:?} a={:?}",
            frame("a2"),
            frame("a")
        );
        assert!(
            frame("a3").y + 1e-6 >= frame("a1").bottom(),
            "left-right stacks extra left-slot children, a3={:?} a1={:?}",
            frame("a3"),
            frame("a1")
        );
        assert!(
            (frame("b1").y - frame("b2").y).abs() < 1e-6
                && (frame("b1").y - frame("b3").y).abs() < 1e-6,
            "default single-layer keeps siblings on one row"
        );
        assert!(frame("b1").x < frame("b2").x && frame("b2").x < frame("b3").x);
    }

    #[test]
    fn tree_demand_raises_layer_gap_for_min_first_segment() {
        let graph = Graph {
            nodes: vec![node("root"), node("a")],
            edges: vec![edge("e0", "root", "a")],
            groups: vec![],
            partition: None,
        };
        let result = run(&LayoutContract {
            layout: tree_options(&[
                ("layer_gap", AttrValue::Num(20.0)),
                ("min_first_segment", AttrValue::Num(80.0)),
            ]),
            edge_routing: None,
            graph,
            node_sizes: sizes(&["root", "a"]),
        })
        .unwrap();
        let frame = |id: &str| result.nodes.iter().find(|n| n.id == id).unwrap().frame;
        let gap = frame("a").y - frame("root").bottom();
        assert!(
            gap + 1e-6 >= 80.0,
            "DemandBoard should raise layer_gap to min_first_segment, gap={gap}"
        );
    }

    #[test]
    fn circular_single_cycle_places_equal_nodes_on_a_ring() {
        let graph = Graph {
            nodes: vec![node("a"), node("b"), node("c"), node("d")],
            edges: vec![
                edge("e0", "a", "b"),
                edge("e1", "b", "c"),
                edge("e2", "c", "d"),
                edge("e3", "d", "a"),
            ],
            groups: vec![],
            partition: None,
        };
        let result = run(&LayoutContract {
            layout: circular_cycle(),
            edge_routing: None,
            graph,
            node_sizes: sizes(&["a", "b", "c", "d"]),
        })
        .unwrap();
        assert_eq!(result.nodes.len(), 4);
        assert_eq!(result.edges.len(), 4);
        let frame = |id: &str| result.nodes.iter().find(|n| n.id == id).unwrap().frame;
        let cx = ["a", "b", "c", "d"]
            .iter()
            .map(|id| frame(id).center().x)
            .sum::<f64>()
            / 4.0;
        let cy = ["a", "b", "c", "d"]
            .iter()
            .map(|id| frame(id).center().y)
            .sum::<f64>()
            / 4.0;
        let radii: Vec<f64> = ["a", "b", "c", "d"]
            .iter()
            .map(|id| {
                let c = frame(id).center();
                ((c.x - cx).powi(2) + (c.y - cy).powi(2)).sqrt()
            })
            .collect();
        let r0 = radii[0];
        for r in &radii {
            assert!(
                (r - r0).abs() < 1e-6,
                "equal nodes should share a radius, got {radii:?}"
            );
        }
        assert!(r0 > 1.0, "ring radius should be positive, got {r0}");
        for e in &result.edges {
            assert!(e.path.polyline_points().unwrap().len() >= 2);
        }
        let ids = ["a", "b", "c", "d"];
        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                let fa = frame(ids[i]);
                let fb = frame(ids[j]);
                let overlap_x = fa.right() > fb.x + 1e-6 && fb.right() > fa.x + 1e-6;
                let overlap_y = fa.bottom() > fb.y + 1e-6 && fb.bottom() > fa.y + 1e-6;
                assert!(
                    !(overlap_x && overlap_y),
                    "frames overlap {} / {}",
                    ids[i],
                    ids[j]
                );
            }
        }
    }

    #[test]
    fn circular_bfs_order_differs_from_declaration() {
        let graph = Graph {
            nodes: vec![node("a"), node("b"), node("c"), node("d")],
            edges: vec![
                edge("e0", "a", "c"),
                edge("e1", "c", "b"),
                edge("e2", "b", "d"),
            ],
            groups: vec![],
            partition: None,
        };
        let sizes = sizes(&["a", "b", "c", "d"]);
        let bfs = run(&LayoutContract {
            layout: circular_options(&[
                ("partitioning", AttrValue::Atom("single-cycle".into())),
                ("order", AttrValue::Atom("bfs".into())),
            ]),
            edge_routing: None,
            graph: graph.clone(),
            node_sizes: sizes.clone(),
        })
        .unwrap();
        let decl = run(&LayoutContract {
            layout: circular_options(&[
                ("partitioning", AttrValue::Atom("single-cycle".into())),
                ("order", AttrValue::Atom("declaration".into())),
            ]),
            edge_routing: None,
            graph,
            node_sizes: sizes,
        })
        .unwrap();
        let pos = |r: &LayoutResult, id: &str| {
            r.nodes.iter().find(|n| n.id == id).unwrap().frame.center()
        };
        let same = ["a", "b", "c", "d"].iter().all(|id| {
            let p = pos(&bfs, id);
            let q = pos(&decl, id);
            (p.x - q.x).abs() < 1e-6 && (p.y - q.y).abs() < 1e-6
        });
        assert!(!same, "bfs and declaration circle order should differ");
    }

    #[test]
    fn circular_two_components_pack_side_by_side() {
        let graph = Graph {
            nodes: vec![node("a"), node("b"), node("c"), node("d")],
            edges: vec![edge("e0", "a", "b"), edge("e1", "c", "d")],
            groups: vec![],
            partition: None,
        };
        let result = run(&LayoutContract {
            layout: circular_cycle(),
            edge_routing: None,
            graph,
            node_sizes: sizes(&["a", "b", "c", "d"]),
        })
        .unwrap();
        let frame = |id: &str| result.nodes.iter().find(|n| n.id == id).unwrap().frame;
        let left_max = frame("a").right().max(frame("b").right());
        let right_min = frame("c").x.min(frame("d").x);
        assert!(
            left_max <= right_min + 1e-6,
            "second component should sit to the right, left_max={left_max} right_min={right_min}"
        );
    }

    #[test]
    fn circular_named_unimplemented_is_unsupported() {
        let cases: &[(&[(&str, AttrValue)], &str)] = &[
            (
                &[
                    ("partitioning", AttrValue::Atom("single-cycle".into())),
                    ("order", AttrValue::Atom("bfs".into())),
                    ("partition_style", AttrValue::Atom("disk".into())),
                ],
                "disk",
            ),
            (
                &[("routing_policy", AttrValue::Atom("automatic".into()))],
                "automatic",
            ),
        ];
        for (opts, needle) in cases {
            let graph = Graph {
                nodes: vec![node("a"), node("b")],
                edges: vec![edge("e0", "a", "b")],
                groups: vec![],
                partition: None,
            };
            let err = run(&LayoutContract {
                layout: circular_options(opts),
                edge_routing: None,
                graph,
                node_sizes: sizes(&["a", "b"]),
            })
            .unwrap_err();
            assert!(
                matches!(err, LayoutError::Unsupported { .. }),
                "expected Unsupported for {needle}, got {err}"
            );
            let msg = err.to_string();
            assert!(
                msg.contains(needle),
                "Unsupported message should mention `{needle}`, got {msg}"
            );
        }
    }

    #[test]
    fn circular_default_two_triangles_are_two_rings() {
        let graph = Graph {
            nodes: vec![node("a"), node("b"), node("c"), node("d"), node("e")],
            edges: vec![
                edge("e0", "a", "b"),
                edge("e1", "b", "c"),
                edge("e2", "c", "a"),
                edge("e3", "a", "d"),
                edge("e4", "d", "e"),
                edge("e5", "e", "a"),
            ],
            groups: vec![],
            partition: None,
        };
        let result = run(&LayoutContract {
            layout: AlgorithmRef::new("circular"),
            edge_routing: None,
            graph,
            node_sizes: sizes(&["a", "b", "c", "d", "e"]),
        })
        .unwrap();
        assert_eq!(result.nodes.len(), 5);
        let frame = |id: &str| result.nodes.iter().find(|n| n.id == id).unwrap().frame;
        let p = |id: &str| frame(id).center();
        let cc = circumcenter3(p("a"), p("b"), p("c"));
        let r = dist(cc, p("a"));
        assert!(r > 1.0, "triangle abc should have a positive circumradius");
        assert!(
            (dist(cc, p("b")) - r).abs() < 1e-4 && (dist(cc, p("c")) - r).abs() < 1e-4,
            "a,b,c should lie on one partition circle"
        );
        assert!(
            (dist(cc, p("d")) - r).abs() > r * 0.25,
            "d should not sit on triangle abc's circle (got dist={}, r={r})",
            dist(cc, p("d"))
        );
        assert!(
            (dist(cc, p("e")) - r).abs() > r * 0.25,
            "e should not sit on triangle abc's circle (got dist={}, r={r})",
            dist(cc, p("e"))
        );
        let c_de = Point {
            x: (p("d").x + p("e").x) / 2.0,
            y: (p("d").y + p("e").y) / 2.0,
        };
        let split = dist(cc, c_de);
        assert!(
            split > r * 0.5,
            "the two BCC centers should be separated, dist={split} r={r}"
        );
    }

    #[test]
    fn circular_bcc_isolated_puts_cut_between_rings() {
        let graph = two_triangles();
        let compact = run(&LayoutContract {
            layout: AlgorithmRef::new("circular"),
            edge_routing: None,
            graph: graph.clone(),
            node_sizes: sizes(&["a", "b", "c", "d", "e"]),
        })
        .unwrap();
        let isolated = run(&LayoutContract {
            layout: circular_options(&[(
                "partitioning",
                AttrValue::Atom("bcc-isolated".into()),
            )]),
            edge_routing: None,
            graph,
            node_sizes: sizes(&["a", "b", "c", "d", "e"]),
        })
        .unwrap();
        let p = |r: &LayoutResult, id: &str| {
            r.nodes.iter().find(|n| n.id == id).unwrap().frame.center()
        };
        let mid = |u: Point, v: Point| Point {
            x: (u.x + v.x) / 2.0,
            y: (u.y + v.y) / 2.0,
        };
        let a = p(&isolated, "a");
        let m_bc = mid(p(&isolated, "b"), p(&isolated, "c"));
        let m_de = mid(p(&isolated, "d"), p(&isolated, "e"));
        let bc = dist(p(&isolated, "b"), p(&isolated, "c"));
        assert!(
            dist(a, m_bc) > bc,
            "isolated cut should sit off the {{b,c}} disk, dist={} bc={bc}",
            dist(a, m_bc)
        );
        let dot = (m_bc.x - a.x) * (m_de.x - a.x) + (m_bc.y - a.y) * (m_de.y - a.y);
        assert!(
            dot < 0.0,
            "cut should sit between the two rings, dot={dot}"
        );
        let a_c = p(&compact, "a");
        let m_bc_c = mid(p(&compact, "b"), p(&compact, "c"));
        let bc_c = dist(p(&compact, "b"), p(&compact, "c"));
        assert!(
            dist(a_c, m_bc_c) < bc_c,
            "compact should keep the cut on the {{b,c}} circle"
        );
        let same = ["a", "b", "c", "d", "e"].iter().all(|id| {
            let u = p(&compact, id);
            let v = p(&isolated, id);
            (u.x - v.x).abs() < 1e-6 && (u.y - v.y).abs() < 1e-6
        });
        assert!(!same, "bcc-isolated geometry must differ from bcc-compact");
    }

    #[test]
    fn circular_exterior_routes_non_adjacent_outside() {
        let graph = Graph {
            nodes: vec![node("a"), node("b"), node("c"), node("d")],
            edges: vec![
                edge("e0", "a", "b"),
                edge("e1", "b", "c"),
                edge("e2", "c", "d"),
                edge("e3", "d", "a"),
                edge("diag", "a", "c"),
            ],
            groups: vec![],
            partition: None,
        };
        let sizes = sizes(&["a", "b", "c", "d"]);
        let interior = run(&LayoutContract {
            layout: circular_options(&[
                ("partitioning", AttrValue::Atom("single-cycle".into())),
                ("order", AttrValue::Atom("declaration".into())),
                ("routing_policy", AttrValue::Atom("interior".into())),
            ]),
            edge_routing: None,
            graph: graph.clone(),
            node_sizes: sizes.clone(),
        })
        .unwrap();
        let exterior = run(&LayoutContract {
            layout: circular_options(&[
                ("partitioning", AttrValue::Atom("single-cycle".into())),
                ("order", AttrValue::Atom("declaration".into())),
                ("routing_policy", AttrValue::Atom("exterior".into())),
            ]),
            edge_routing: None,
            graph,
            node_sizes: sizes,
        })
        .unwrap();
        let frame = |r: &LayoutResult, id: &str| r.nodes.iter().find(|n| n.id == id).unwrap().frame;
        let cx = ["a", "b", "c", "d"]
            .iter()
            .map(|id| frame(&exterior, id).center().x)
            .sum::<f64>()
            / 4.0;
        let cy = ["a", "b", "c", "d"]
            .iter()
            .map(|id| frame(&exterior, id).center().y)
            .sum::<f64>()
            / 4.0;
        let center = Point { x: cx, y: cy };
        let r_node = dist(frame(&exterior, "a").center(), center);
        let ext = exterior.edges.iter().find(|e| e.id == "diag").unwrap();
        let pts = ext.path.polyline_points().unwrap();
        assert!(
            pts.len() > 2,
            "exterior diagonal should sample an arc, got {} points",
            pts.len()
        );
        let mid = pts[pts.len() / 2];
        let r_mid = dist(mid, center);
        assert!(
            r_mid > r_node + 8.0,
            "exterior arc midpoint should sit outside the node circle, r_mid={r_mid} r_node={r_node}"
        );
        let inn = interior.edges.iter().find(|e| e.id == "diag").unwrap();
        let ip = inn.path.polyline_points().unwrap();
        assert_eq!(ip.len(), 2, "interior diagonal should stay a chord");
        let imid = Point {
            x: (ip[0].x + ip[1].x) / 2.0,
            y: (ip[0].y + ip[1].y) / 2.0,
        };
        assert!(
            dist(imid, center) + 1e-6 < r_node,
            "interior chord midpoint should sit inside the node circle"
        );
    }

    #[test]
    fn circular_self_loop_is_short_arc() {
        let graph = Graph {
            nodes: vec![node("a"), node("b"), node("c")],
            edges: vec![
                edge("e0", "a", "b"),
                edge("e1", "b", "c"),
                edge("e2", "c", "a"),
                edge("loop", "a", "a"),
            ],
            groups: vec![],
            partition: None,
        };
        let result = run(&LayoutContract {
            layout: circular_options(&[
                ("partitioning", AttrValue::Atom("single-cycle".into())),
                ("order", AttrValue::Atom("declaration".into())),
            ]),
            edge_routing: None,
            graph,
            node_sizes: sizes(&["a", "b", "c"]),
        })
        .unwrap();
        let loop_e = result.edges.iter().find(|e| e.id == "loop").unwrap();
        let pts = loop_e.path.polyline_points().unwrap();
        assert!(
            pts.len() >= 4,
            "self-loop should be a short arc, got {} points",
            pts.len()
        );
        let ac = result
            .nodes
            .iter()
            .find(|n| n.id == "a")
            .unwrap()
            .frame
            .center();
        let r_max = pts.iter().map(|p| dist(*p, ac)).fold(0.0_f64, f64::max);
        assert!(
            r_max > 20.0,
            "self-loop arc should leave the node frame, r_max={r_max}"
        );
    }

    fn two_triangles() -> Graph {
        Graph {
            nodes: vec![node("a"), node("b"), node("c"), node("d"), node("e")],
            edges: vec![
                edge("e0", "a", "b"),
                edge("e1", "b", "c"),
                edge("e2", "c", "a"),
                edge("e3", "a", "d"),
                edge("e4", "d", "e"),
                edge("e5", "e", "a"),
            ],
            groups: vec![],
            partition: None,
        }
    }

    #[test]
    fn circular_two_runs_are_bit_identical() {
        let graph = two_triangles();
        let contract = LayoutContract {
            layout: AlgorithmRef::new("circular"),
            edge_routing: None,
            graph,
            node_sizes: sizes(&["a", "b", "c", "d", "e"]),
        };
        let a = run(&contract).unwrap();
        let b = run(&contract).unwrap();
        assert_eq!(a.diagnostics.params_hash, b.diagnostics.params_hash);
        for (na, nb) in a.nodes.iter().zip(b.nodes.iter()) {
            assert_eq!(na.id, nb.id);
            assert_eq!(na.frame.x, nb.frame.x);
            assert_eq!(na.frame.y, nb.frame.y);
        }
    }

    #[test]
    fn circular_single_cycle_differs_from_bcc_compact() {
        let graph = two_triangles();
        let sizes = sizes(&["a", "b", "c", "d", "e"]);
        let compact = run(&LayoutContract {
            layout: AlgorithmRef::new("circular"),
            edge_routing: None,
            graph: graph.clone(),
            node_sizes: sizes.clone(),
        })
        .unwrap();
        let cycle = run(&LayoutContract {
            layout: circular_options(&[
                ("partitioning", AttrValue::Atom("single-cycle".into())),
                ("order", AttrValue::Atom("declaration".into())),
            ]),
            edge_routing: None,
            graph,
            node_sizes: sizes,
        })
        .unwrap();
        let p = |r: &LayoutResult, id: &str| {
            r.nodes.iter().find(|n| n.id == id).unwrap().frame.center()
        };
        let same = ["a", "b", "c", "d", "e"].iter().all(|id| {
            let u = p(&compact, id);
            let v = p(&cycle, id);
            (u.x - v.x).abs() < 1e-6 && (u.y - v.y).abs() < 1e-6
        });
        assert!(!same, "single-cycle and bcc-compact must differ on the same topology");
        let cx = ["a", "b", "c", "d", "e"]
            .iter()
            .map(|id| p(&cycle, id).x)
            .sum::<f64>()
            / 5.0;
        let cy = ["a", "b", "c", "d", "e"]
            .iter()
            .map(|id| p(&cycle, id).y)
            .sum::<f64>()
            / 5.0;
        let center = Point { x: cx, y: cy };
        let radii: Vec<f64> = ["a", "b", "c", "d", "e"]
            .iter()
            .map(|id| dist(p(&cycle, id), center))
            .collect();
        let r0 = radii[0];
        for r in &radii {
            assert!(
                (r - r0).abs() < 1e-4,
                "single-cycle should keep all nodes on one ring, {radii:?}"
            );
        }
    }

    #[test]
    fn circular_custom_circle_splits_one_bcc() {
        let mut nodes = Vec::new();
        for id in ["n0", "n1", "n2", "n3", "n4", "n5"] {
            let atom = if matches!(id, "n0" | "n1" | "n2") {
                "left"
            } else {
                "right"
            };
            nodes.push(node_attr(id, "circle", AttrValue::Atom(atom.into())));
        }
        let graph = Graph {
            nodes,
            edges: vec![
                edge("e0", "n0", "n1"),
                edge("e1", "n1", "n2"),
                edge("e2", "n2", "n3"),
                edge("e3", "n3", "n4"),
                edge("e4", "n4", "n5"),
                edge("e5", "n5", "n0"),
            ],
            groups: vec![],
            partition: None,
        };
        let ids = ["n0", "n1", "n2", "n3", "n4", "n5"];
        let sizes = sizes(&ids);
        let custom = run(&LayoutContract {
            layout: AlgorithmRef::new("circular"),
            edge_routing: None,
            graph: graph.clone(),
            node_sizes: sizes.clone(),
        })
        .unwrap();
        let unmarked = {
            let plain = Graph {
                nodes: ids.iter().map(|id| node(id)).collect(),
                edges: graph.edges.clone(),
                groups: vec![],
                partition: None,
            };
            run(&LayoutContract {
                layout: AlgorithmRef::new("circular"),
                edge_routing: None,
                graph: plain,
                node_sizes: sizes,
            })
            .unwrap()
        };
        let p = |r: &LayoutResult, id: &str| {
            r.nodes.iter().find(|n| n.id == id).unwrap().frame.center()
        };
        let centroid = |r: &LayoutResult, who: &[&str]| {
            let n = who.len() as f64;
            Point {
                x: who.iter().map(|id| p(r, id).x).sum::<f64>() / n,
                y: who.iter().map(|id| p(r, id).y).sum::<f64>() / n,
            }
        };
        let left = centroid(&custom, &["n0", "n1", "n2"]);
        let right = centroid(&custom, &["n3", "n4", "n5"]);
        let r_left = dist(p(&custom, "n0"), left);
        assert!(
            dist(left, right) > r_left * 1.2,
            "custom groups should be two rings, not one, split={} r={r_left}",
            dist(left, right)
        );
        let gx = ids.iter().map(|id| p(&unmarked, id).x).sum::<f64>() / 6.0;
        let gy = ids.iter().map(|id| p(&unmarked, id).y).sum::<f64>() / 6.0;
        let g = Point { x: gx, y: gy };
        let r0 = dist(p(&unmarked, "n0"), g);
        for id in ids {
            assert!(
                (dist(p(&unmarked, id), g) - r0).abs() < 1e-4,
                "unmarked 6-cycle is one BCC ring"
            );
        }
    }

    #[test]
    fn circular_custom_cycle_warns_and_keeps_edges() {
        let graph = Graph {
            nodes: vec![
                node_attr("a0", "circle", AttrValue::Atom("g0".into())),
                node_attr("a1", "circle", AttrValue::Atom("g0".into())),
                node_attr("b0", "circle", AttrValue::Atom("g1".into())),
                node_attr("b1", "circle", AttrValue::Atom("g1".into())),
                node_attr("c0", "circle", AttrValue::Atom("g2".into())),
                node_attr("c1", "circle", AttrValue::Atom("g2".into())),
            ],
            edges: vec![
                edge("e0", "a0", "a1"),
                edge("e1", "b0", "b1"),
                edge("e2", "c0", "c1"),
                edge("e3", "a1", "b0"),
                edge("e4", "b1", "c0"),
                edge("e5", "c1", "a0"),
            ],
            groups: vec![],
            partition: None,
        };
        let result = run(&LayoutContract {
            layout: AlgorithmRef::new("circular"),
            edge_routing: None,
            graph,
            node_sizes: sizes(&["a0", "a1", "b0", "b1", "c0", "c1"]),
        })
        .unwrap();
        assert_eq!(result.edges.len(), 6);
        assert!(
            result
                .diagnostics
                .warnings
                .iter()
                .any(|w| w.message.contains("cycle")),
            "cycle in custom partition graph should warn, got {:?}",
            result.diagnostics.warnings
        );
    }

    #[test]
    fn circular_custom_invalid_circle_id() {
        let cases: &[(&str, AttrValue, &str)] = &[
            ("a", AttrValue::Atom(String::new()), "empty"),
            ("a", AttrValue::Atom("Left".into()), "node-id"),
            ("a", AttrValue::Num(1.0), "atom"),
        ];
        for (id, val, needle) in cases {
            let graph = Graph {
                nodes: vec![node_attr(id, "circle", val.clone()), node("b")],
                edges: vec![edge("e0", id, "b")],
                groups: vec![],
                partition: None,
            };
            let err = run(&LayoutContract {
                layout: AlgorithmRef::new("circular"),
                edge_routing: None,
                graph,
                node_sizes: sizes(&[id, "b"]),
            })
            .unwrap_err();
            assert!(
                matches!(err, LayoutError::InvalidInput { .. }),
                "expected InvalidInput for {needle}, got {err}"
            );
            let msg = err.to_string();
            assert!(
                msg.contains(needle),
                "InvalidInput should mention `{needle}`, got {msg}"
            );
        }
        let mut n = node("a");
        n.attrs
            .insert("circle".into(), AttrValue::Atom("one".into()));
        n.attrs
            .insert("partition".into(), AttrValue::Atom("two".into()));
        let graph = Graph {
            nodes: vec![n, node("b")],
            edges: vec![edge("e0", "a", "b")],
            groups: vec![],
            partition: None,
        };
        let err = run(&LayoutContract {
            layout: AlgorithmRef::new("circular"),
            edge_routing: None,
            graph,
            node_sizes: sizes(&["a", "b"]),
        })
        .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("conflicts"),
            "conflicting circle/partition should fail, got {msg}"
        );
    }

    fn dist(a: Point, b: Point) -> f64 {
        ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt()
    }

    fn circumcenter3(a: Point, b: Point, c: Point) -> Point {
        let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
        assert!(d.abs() > 1e-9, "triangle abc is collinear");
        let a2 = a.x * a.x + a.y * a.y;
        let b2 = b.x * b.x + b.y * b.y;
        let c2 = c.x * c.x + c.y * c.y;
        Point {
            x: (a2 * (b.y - c.y) + b2 * (c.y - a.y) + c2 * (a.y - b.y)) / d,
            y: (a2 * (c.x - b.x) + b2 * (a.x - c.x) + c2 * (b.x - a.x)) / d,
        }
    }
}
