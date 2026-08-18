//! Plotgram → draw.io 格式导出器。
//!
//! 将 [`RenderInput`]（graph + layout + meta）编码为 diagrams.net /
//! draw.io 原生 `.drawio`（mxGraphModel XML）。颜色先经 theme/resolve
//! 物化，再映射为可编辑的 mxCell vertex / edge（非扁平 SVG）。
//!
//! 图层顺序与 SVG 后端一致：背景 → groups → edges → nodes。
//! 标题不导出（layout 未为其预留空间，与 `render_svg` 行为一致）。

mod encoder;
mod routing;
mod style;

use plotgram_model::render::RenderInput;

/// Render a complete diagram to draw.io (mxGraphModel) XML.
pub fn render_drawio(input: &RenderInput) -> String {
    let theme = crate::theme::load(input.meta.theme.as_deref());
    let resolved = crate::resolve::resolve_graph(&input.graph, &theme);
    encoder::DrawioEncoder::new(input, &theme, &resolved).encode()
}

#[cfg(test)]
mod tests {
    use super::*;
    use plotgram_model::attr::{AttrMap, AttrValue};
    use plotgram_model::geometry::{Point, Rect};
    use plotgram_model::graph::{Arrow, Edge, Graph, Group, Node, NodeRole};
    use plotgram_model::port::{AlongSpec, PortRef, Side};
    use plotgram_model::render::RenderMeta;
    use plotgram_model::result::{
        EdgePath, EdgePlacement, GroupPlacement, LabelOwner, LabelSlot, LayoutResult,
        NodePlacement,
    };

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

    fn edge(id: &str, arrow: Arrow) -> Edge {
        Edge {
            id: id.to_string(),
            source: "a".to_string(),
            target: "b".to_string(),
            arrow,
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

    fn input(graph: Graph, layout: LayoutResult) -> RenderInput {
        RenderInput {
            graph,
            layout,
            meta: RenderMeta {
                title: None,
                theme: None,
                render_style: None,
                extra: AttrMap::new(),
            },
        }
    }

    fn two_node_layout() -> LayoutResult {
        LayoutResult {
            nodes: vec![
                NodePlacement {
                    id: "a".to_string(),
                    frame: Rect::new(10.0, 10.0, 80.0, 40.0),
                },
                NodePlacement {
                    id: "b".to_string(),
                    frame: Rect::new(10.0, 110.0, 80.0, 40.0),
                },
            ],
            edges: vec![EdgePlacement {
                id: "e1".to_string(),
                source: "a".to_string(),
                target: "b".to_string(),
                path: EdgePath::polyline(vec![
                    Point { x: 50.0, y: 50.0 },
                    Point { x: 50.0, y: 110.0 },
                ]),
                from_port: Some(PortRef {
                    side: Side::South,
                    along: AlongSpec::Ordered { order: 0, count: 1 },
                }),
                to_port: Some(PortRef {
                    side: Side::North,
                    along: AlongSpec::Ordered { order: 0, count: 1 },
                }),
            }],
            groups: vec![],
            labels: vec![],
            canvas_width: 200.0,
            canvas_height: 200.0,
            diagnostics: Default::default(),
            decorations: vec![],
        }
    }

    #[test]
    fn minimal_export_is_valid_mxfile() {
        let graph = Graph {
            nodes: vec![node("a"), node("b")],
            edges: vec![edge("e1", Arrow::Forward)],
            groups: vec![],
            partition: None,
        };
        let xml = render_drawio(&input(graph, two_node_layout()));

        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"), "{xml}");
        assert!(xml.contains("<mxfile host=\"Plotgram/"), "{xml}");
        assert!(xml.contains(r#"<mxCell id="0" />"#), "{xml}");
        assert!(xml.contains(r#"<mxCell id="1" parent="0" />"#), "{xml}");
        // 节点 vertex（页坐标 = 画布坐标 + 20 padding）
        assert!(
            xml.contains(r#"<mxCell id="drawio-node-a" value="a""#),
            "{xml}"
        );
        assert!(
            xml.contains(r#"<mxGeometry x="30" y="30" width="80" height="40""#),
            "{xml}"
        );
        // 边始终绑定 source/target
        assert!(
            xml.contains(r#"edge="1" parent="1" source="drawio-node-a" target="drawio-node-b""#),
            "{xml}"
        );
        // 端口沿边偏移（下边界中心出口 / 上边界中心入口）
        assert!(xml.contains("exitX=0.5000"), "{xml}");
        assert!(xml.contains("exitY=1.0000"), "{xml}");
        assert!(xml.contains("entryX=0.5000"), "{xml}");
        assert!(xml.contains("entryY=0.0000"), "{xml}");
        // 背景矩形
        assert!(xml.contains(r#"id="drawio-bg""#), "{xml}");
    }

    #[test]
    fn render_is_deterministic() {
        let graph = Graph {
            nodes: vec![node("a"), node("b")],
            edges: vec![edge("e1", Arrow::Forward)],
            groups: vec![],
            partition: None,
        };
        let input = input(graph, two_node_layout());
        let first = render_drawio(&input);
        for _ in 0..2 {
            assert_eq!(render_drawio(&input), first, "must be byte-identical");
        }
    }

    #[test]
    fn arrow_semantics_map_to_drawio_style() {
        // Response → dashed（来自 resolve 的 response_dasharray）；Bidirectional → startArrow
        let cases = [
            (Arrow::Forward, "endArrow=block", false),
            (Arrow::Response, "endArrow=block", true),
            (Arrow::Bidirectional, "startArrow=block", false),
        ];
        for (arrow, marker, expect_dashed) in cases {
            let graph = Graph {
                nodes: vec![node("a"), node("b")],
                edges: vec![edge("e1", arrow)],
                groups: vec![],
                partition: None,
            };
            let xml = render_drawio(&input(graph, two_node_layout()));
            assert!(xml.contains(marker), "arrow={arrow:?}: {xml}");
            assert_eq!(xml.contains("dashed=1"), expect_dashed, "arrow={arrow:?}");
        }
    }

    #[test]
    fn group_and_nested_relative_coords() {
        let graph = Graph {
            nodes: vec![],
            edges: vec![],
            groups: vec![Group {
                id: "outer".to_string(),
                label: Some("Outer".to_string()),
                attrs: AttrMap::new(),
                nodes: vec![node("a"), node("b")],
                edges: vec![],
                groups: vec![Group {
                    id: "inner".to_string(),
                    label: None,
                    attrs: AttrMap::new(),
                    nodes: vec![],
                    edges: vec![],
                    groups: vec![],
                }],
            }],
            partition: None,
        };
        let mut layout = two_node_layout();
        layout.groups = vec![
            // finalize post-order: inner before outer
            GroupPlacement {
                id: "inner".to_string(),
                frame: Rect::new(20.0, 20.0, 60.0, 50.0),
            },
            GroupPlacement {
                id: "outer".to_string(),
                frame: Rect::new(5.0, 5.0, 90.0, 150.0),
            },
        ];
        let xml = render_drawio(&input(graph, layout));

        // 有标签 → swimlane；无标签 → 虚线容器
        let outer_pos = xml.find(r#"id="drawio-group-outer""#).expect("outer cell");
        let inner_pos = xml.find(r#"id="drawio-group-inner""#).expect("inner cell");
        assert!(outer_pos < inner_pos, "outer must be written first:\n{xml}");
        assert!(
            xml[outer_pos..inner_pos].contains("swimlane"),
            "labeled group → swimlane:\n{xml}"
        );
        assert!(
            xml[inner_pos..].contains("dashed=1;container=1"),
            "unlabeled group → dashed container:\n{xml}"
        );
        // inner 挂在 outer 下，坐标相对 outer（(20,20) - (5,5) = (15,15)）
        let inner_cell = &xml[inner_pos..];
        assert!(
            inner_cell.contains(r#"parent="drawio-group-outer""#),
            "{xml}"
        );
        assert!(
            inner_cell.contains(r#"<mxGeometry x="15" y="15""#),
            "inner relative coords:\n{xml}"
        );
        // 节点挂 inner? 不 — a/b 属于 outer；相对 outer 坐标 (10+20-25, …)
        let node_a = xml.find(r#"id="drawio-node-a""#).expect("node a");
        assert!(
            xml[node_a..].contains(r#"parent="drawio-group-outer""#),
            "node parent binding:\n{xml}"
        );
    }

    #[test]
    fn polyline_corners_drive_routing_style() {
        // 2+ 拐点 → segmentEdgeStyle + waypoints（含 padding）
        let graph = Graph {
            nodes: vec![node("a"), node("b")],
            edges: vec![edge("e1", Arrow::Forward)],
            groups: vec![],
            partition: None,
        };
        let mut layout = two_node_layout();
        layout.edges[0].path = EdgePath::polyline(vec![
            Point { x: 10.0, y: 30.0 },
            Point { x: 10.0, y: 70.0 },
            Point { x: 90.0, y: 70.0 },
            Point { x: 90.0, y: 110.0 },
            Point { x: 50.0, y: 110.0 },
        ]);
        let xml = render_drawio(&input(graph, layout));
        assert!(xml.contains("edgeStyle=segmentEdgeStyle"), "{xml}");
        assert!(
            xml.contains(
                r#"<Array as="points"><mxPoint x="30" y="90" /><mxPoint x="110" y="90" /><mxPoint x="110" y="130" /></Array>"#
            ),
            "waypoints carry page padding:\n{xml}"
        );
    }

    #[test]
    fn group_anchor_endpoints_become_floating_edges() {
        // 挂到 GroupAnchor 的端不绑定 cell，用 sourcePoint 锚定
        let mut anchor = node("ga");
        anchor.role = NodeRole::GroupAnchor;
        anchor.host_group = Some("g".to_string());
        let mut graph = Graph {
            nodes: vec![node("a"), anchor],
            edges: vec![],
            groups: vec![Group {
                id: "g".to_string(),
                label: Some("G".to_string()),
                attrs: AttrMap::new(),
                nodes: vec![],
                edges: vec![],
                groups: vec![],
            }],
            partition: None,
        };
        let mut e = edge("e1", Arrow::Forward);
        e.source = "ga".to_string();
        graph.edges = vec![e];

        let mut layout = two_node_layout();
        // ga 无 vertex；a 保留；边 ga → a
        layout.nodes = vec![
            NodePlacement {
                id: "a".to_string(),
                frame: Rect::new(10.0, 10.0, 80.0, 40.0),
            },
            NodePlacement {
                id: "ga".to_string(),
                frame: Rect::new(60.0, 0.0, 0.0, 0.0),
            },
        ];
        layout.groups = vec![GroupPlacement {
            id: "g".to_string(),
            frame: Rect::new(5.0, 5.0, 90.0, 50.0),
        }];
        layout.edges[0].source = "ga".to_string();
        layout.edges[0].target = "a".to_string();
        layout.edges[0].path = EdgePath::polyline(vec![
            Point { x: 60.0, y: 30.0 },
            Point { x: 50.0, y: 30.0 },
        ]);

        let xml = render_drawio(&input(graph.clone(), layout));
        assert!(
            !xml.contains(r#"id="drawio-node-ga""#),
            "anchor must not export a vertex:\n{xml}"
        );
        assert!(
            !xml.contains(r#" source="drawio-node-ga""#),
            "{xml}"
        );
        assert!(
            xml.contains(r#" target="drawio-node-a""#),
            "bound end stays bound:\n{xml}"
        );
        assert!(
            xml.contains(r#"<mxPoint x="80" y="50" as="sourcePoint" />"#),
            "floating end anchored by sourcePoint (pad applied):\n{xml}"
        );
    }

    #[test]
    fn edge_label_becomes_value_with_relative_geometry() {
        let graph = Graph {
            nodes: vec![node("a"), node("b")],
            edges: vec![edge("e1", Arrow::Forward)],
            groups: vec![],
            partition: None,
        };
        let mut layout = two_node_layout();
        layout.labels = vec![LabelSlot {
            owner: LabelOwner::Edge("e1".to_string()),
            role: Some("mid".to_string()),
            text: "go".to_string(),
            frame: Rect::new(42.0, 72.0, 16.0, 12.0),
        }];
        let xml = render_drawio(&input(graph, layout));
        assert!(
            xml.contains(r#" value="go""#),
            "label text on edge cell:\n{xml}"
        );
        assert!(
            xml.contains(r#"<mxGeometry x="-0.0667" y="0.00""#),
            "label relative geometry:\n{xml}"
        );
        assert!(!xml.contains("-0.00"), "no negative zero:\n{xml}");
        assert!(xml.contains(r#"as="offset""#), "{xml}");
    }

    #[test]
    fn inline_styles_and_sketch_round_trip() {
        let mut a = node("a");
        a.attrs.insert(
            "style.fill".to_string(),
            AttrValue::Str("#123456".to_string()),
        );
        let mut e = edge("e1", Arrow::Forward);
        e.attrs.insert(
            "style.stroke".to_string(),
            AttrValue::Str("#C62828".to_string()),
        );
        e.attrs.insert("style.dashed".to_string(), AttrValue::Bool(true));
        let graph = Graph {
            nodes: vec![a, node("b")],
            edges: vec![e],
            groups: vec![],
            partition: None,
        };
        let mut i = input(graph, two_node_layout());
        i.meta.render_style = Some("sketch".to_string());
        let xml = render_drawio(&i);

        assert!(xml.contains("fillColor=#123456"), "{xml}");
        assert!(xml.contains("strokeColor=#C62828"), "{xml}");
        assert!(xml.matches("dashed=1").count() >= 1, "{xml}");
        assert!(xml.matches("sketch=1").count() >= 3, "bg? no — nodes+edges+…:\n{xml}");
    }

    #[test]
    fn value_text_is_xml_escaped() {
        let mut a = node("a");
        a.label = Some(r#"a<b>&"c"#.to_string());
        let graph = Graph {
            nodes: vec![a, node("b")],
            edges: vec![edge("e1", Arrow::Forward)],
            groups: vec![],
            partition: None,
        };
        let xml = render_drawio(&input(graph, two_node_layout()));
        assert!(
            xml.contains(r#"value="a&lt;b&gt;&amp;&quot;c""#),
            "{xml}"
        );
    }

    #[test]
    fn minimal_drawio_snapshot() {
        let graph = Graph {
            nodes: vec![node("a"), node("b")],
            edges: vec![edge("e1", Arrow::Response)],
            groups: vec![],
            partition: None,
        };
        let xml = render_drawio(&input(graph, two_node_layout()));
        insta::assert_snapshot!(xml);
    }
}
