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
    use plotgram_model::geometry::Size;
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

    fn seq_options(pairs: &[(&str, AttrValue)]) -> AlgorithmRef {
        AlgorithmRef::with_options(
            "sequence",
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        )
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
        assert_eq!(result.edges.len(), 2);
        assert!(result.edges[0].path.polyline_points().unwrap().len() >= 2);
    }
}
