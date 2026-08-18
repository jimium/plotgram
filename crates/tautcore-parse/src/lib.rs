//! Tautcore DSL parser (`.taut` → model).
//!
//! Pipeline:
//!
//! ```text
//! source
//!   → lex / parse          → DslAst
//!   → lower                → Graph (+ diagram attrs)
//!   → expand @group sugar  → group_anchor nodes
//!   → lift structural attrs
//!   → archetype expand     → fill shape / variant / icon (fill-only)
//!   → profile expand       → default layout / edge_routing
//!   → ParseOutput          → ready for measure → LayoutContract
//! ```
//!
//! This crate must **not** depend on `tautcore-engine` / layout crates.
//! Theme / MeasureParams stay in the orchestrator (cli); parse may later
//! call `tautcore-content` only for node `content:` MD strings.

#![forbid(unsafe_code)]

pub mod ast;
pub mod error;
pub mod expand;
pub mod lexer;
pub mod lower;
pub mod parser;

pub use error::{ParseError, ParseWarning};
pub use expand::DiagramMeta;

use tautcore_model::contract::{AlgorithmRef, LayoutContract};
use tautcore_model::graph::Graph;
use tautcore_model::profile::DiagramType;
use tautcore_model::render::RenderMeta;
use tautcore_model::sizes::NodeSizes;

/// Successful parse + profile expansion (sizes still empty — measure fills them).
#[derive(Debug, Clone)]
pub struct ParseOutput {
    /// Structural graph (anchors expanded, structural attrs lifted).
    pub graph: Graph,
    /// Resolved layout algorithm (from `layout:` or `profile:` defaults).
    pub layout: AlgorithmRef,
    /// Independent router, if any (`None` = layout built-in edges).
    pub edge_routing: Option<AlgorithmRef>,
    /// Profile id after resolve (`None` if author only set `layout:`).
    pub profile: Option<DiagramType>,
    /// Chrome for render (title / theme / render_style).
    pub meta: RenderMeta,
    /// Non-fatal diagnostics collected during parse (e.g. unknown diagram keys).
    pub warnings: Vec<ParseWarning>,
}

impl ParseOutput {
    /// Build a [`LayoutContract`] once preferred sizes are known.
    pub fn into_contract(self, node_sizes: NodeSizes) -> LayoutContract {
        LayoutContract {
            layout: self.layout,
            edge_routing: self.edge_routing,
            graph: self.graph,
            node_sizes,
        }
    }
}

/// Parse a `.taut` source string into [`ParseOutput`].
///
/// Full pipeline: lex → parse → lower → expand @group → lift → profile expand.
pub fn parse(source: &str) -> Result<ParseOutput, ParseError> {
    // 1. lex + parse → AST
    let ast = parser::parse_file(source)?;

    // 2. lower → Graph + DiagramMeta + pending group edges
    let lowered = lower::lower(&ast)?;
    let mut graph = lowered.graph;

    // 3. expand @group sugar (inject group_anchor nodes, rewrite edges)
    expand::expand_group_frame_sugar(&mut graph, &lowered.pending_group_edges)?;

    // 4. lift structural attrs (role/host_group/side/slot on nodes; ports/critical on edges)
    expand::lift_structural(&mut graph)?;

    // 5. archetype expand (fill shape/variant/icon from named packs; fill-only)
    expand::expand_archetypes(&mut graph);

    // 6. profile expand → resolve layout / edge_routing
    let (layout, edge_routing, profile) = lowered.meta.resolve_algorithms();

    // 7. assemble output
    Ok(ParseOutput {
        graph,
        layout,
        edge_routing,
        profile,
        meta: lowered.meta.to_render_meta(),
        warnings: lowered.meta.warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tautcore_model::graph::NodeRole;
    use tautcore_model::port::Side;

    #[test]
    fn end_to_end_minimal() {
        let out = parse("diagram { node a {} node b {} a -> b }").unwrap();
        assert_eq!(out.graph.nodes.len(), 2);
        assert_eq!(out.graph.edges.len(), 1);
        assert_eq!(out.layout.name, "hierarchical");
        assert_eq!(out.profile, Some(DiagramType::Flowchart));
    }

    #[test]
    fn end_to_end_full_example() {
        let source = r#"
// 登录认证示意

diagram {
    profile: flowchart,
    title: "用户登录",
    layout: hierarchical { direction: top-to-bottom },
    theme: common.clean-light,
    render_style: standard

    node start { label: "开始", archetype: start }
    node login { label: "登录" }
    node ok { label: "成功？", archetype: decision }

    group auth {
        label: "认证服务",
        variant: secondary

        node api { label: "API", archetype: service, status: healthy }
        node db { label: "用户库", archetype: database }
        api -> db { label: "查询" }
    }

    start -> login
    login -> api { label: "提交" }
    api --> login { label: "结果" }
    login -> ok
    ok -> start "重试"
}
"#;
        let out = parse(source).unwrap();

        // Layout from explicit override
        assert_eq!(out.layout.name, "hierarchical");
        assert_eq!(
            out.layout.options.get("direction"),
            Some(&tautcore_model::attr::AttrValue::Atom(
                "top-to-bottom".into()
            ))
        );
        assert_eq!(out.profile, Some(DiagramType::Flowchart));
        assert_eq!(out.meta.title.as_deref(), Some("用户登录"));
        assert_eq!(out.meta.theme.as_deref(), Some("common.clean-light"));
        assert_eq!(out.meta.render_style.as_deref(), Some("standard"));

        // 3 top-level nodes + 1 group
        assert_eq!(out.graph.nodes.len(), 3);
        assert_eq!(out.graph.groups.len(), 1);

        // Group has 2 nodes + 1 internal edge
        let auth = &out.graph.groups[0];
        assert_eq!(auth.id, "auth");
        assert_eq!(auth.label.as_deref(), Some("认证服务"));
        assert_eq!(auth.nodes.len(), 2);
        assert_eq!(auth.edges.len(), 1);

        // Top-level edges: start->login, login->api, api-->login, login->ok, ok->start
        assert_eq!(out.graph.edges.len(), 5);
    }

    #[test]
    fn end_to_end_group_frame_edge() {
        let source = r#"diagram {
    group frontend {
        label: "前端"
        node web { label: "Web" }
    }
    group backend {
        label: "后端"
        node api { label: "API" }
    }

    @frontend -> @backend { label: "调用", from_side: east, to_side: west }
    web -> @backend { label: "直连框", to_side: north }
}"#;
        let out = parse(source).unwrap();

        // 2 expanded edges at top level
        assert_eq!(out.graph.edges.len(), 2);

        // Anchors injected
        let fe = out
            .graph
            .groups
            .iter()
            .find(|g| g.id == "frontend")
            .unwrap();
        let fe_anchors: Vec<_> = fe.nodes.iter().filter(|n| n.is_group_anchor()).collect();
        assert_eq!(fe_anchors.len(), 1);
        assert_eq!(fe_anchors[0].anchor_side(), Some(Side::East));
        assert_eq!(fe_anchors[0].role, NodeRole::GroupAnchor);

        let be = out.graph.groups.iter().find(|g| g.id == "backend").unwrap();
        let be_anchors: Vec<_> = be.nodes.iter().filter(|n| n.is_group_anchor()).collect();
        // west + north = 2 anchors
        assert_eq!(be_anchors.len(), 2);
    }

    #[test]
    fn end_to_end_port_lift() {
        let source = r#"diagram {
    node a { label: "A" }
    node b { label: "B" }
    a -> b { from_side: south, to_side: north }
}"#;
        let out = parse(source).unwrap();
        let edge = &out.graph.edges[0];
        assert_eq!(
            edge.from_port,
            Some(tautcore_model::port::PortConstraint::FixedSide { side: Side::South })
        );
        assert_eq!(
            edge.to_port,
            Some(tautcore_model::port::PortConstraint::FixedSide { side: Side::North })
        );
        assert!(!edge.attrs.contains_key("from_side"));
        assert!(!edge.attrs.contains_key("to_side"));
    }

    #[test]
    fn end_to_end_removed_port_keys_are_errors() {
        for src in [
            "diagram { node a {} node b {} a -> b { from_slot: 0 } }",
            "diagram { node a {} node b {} a -> b { from_ratio: 0.5 } }",
            "diagram { node a {} node b {} a -> b { from_x: 1.0, from_y: 0.0 } }",
            "diagram { node a {} node b {} a -> b { to_sides: \"south, east\" } }",
        ] {
            let err = parse(src).unwrap_err();
            assert!(matches!(err, ParseError::Port(_)), "src={src} got {err:?}");
        }
    }

    #[test]
    fn end_to_end_edge_group_is_rejected() {
        let err = parse(
            r#"diagram {
    node a {} node hub {}
    a -> hub { edge_group: bus_auth }
}"#,
        )
        .unwrap_err();
        assert!(matches!(err, ParseError::Port(_)));
        assert!(err.to_string().contains("edge_group"), "unexpected: {err}");
    }

    #[test]
    fn end_to_end_sequence_profile() {
        let source = r#"diagram {
    profile: sequence
    node alice {} node bob {}
    alice -> bob { label: "hello" }
    bob --> alice { label: "hi" }
}"#;
        let out = parse(source).unwrap();
        assert_eq!(out.layout.name, "sequence");
        assert_eq!(out.profile, Some(DiagramType::Sequence));
        // Edge declaration order = time axis
        let ids: Vec<_> = out
            .graph
            .edges_in_declaration_order()
            .iter()
            .map(|e| e.id.as_str())
            .collect();
        assert_eq!(ids, ["e0", "e1"]);
    }

    #[test]
    fn error_unknown_profile() {
        let err = parse("diagram { profile: bogus node a {} }").unwrap_err();
        assert!(matches!(err, ParseError::UnknownProfile { value, .. } if value == "bogus"));
    }

    #[test]
    fn error_duplicate_id() {
        let err = parse("diagram { node x {} node x {} }").unwrap_err();
        assert!(matches!(err, ParseError::DuplicateId { .. }));
    }

    #[test]
    fn error_removed_edge_port_key() {
        let err = parse("diagram { node a {} node b {} a -> b { from_slot: 0 } }").unwrap_err();
        assert!(matches!(err, ParseError::Port(_)));
        assert!(err.to_string().contains("unsupported"));
    }

    #[test]
    fn node_positional_sugar_full() {
        let out = parse(r#"diagram { node db "用户库" database mysql }"#).unwrap();
        let n = &out.graph.nodes[0];
        assert_eq!(n.label.as_deref(), Some("用户库"));
        assert_eq!(
            n.attrs.get("archetype").and_then(|v| v.as_str()),
            Some("database")
        );
        assert_eq!(n.attrs.get("icon").and_then(|v| v.as_str()), Some("mysql"));
    }

    #[test]
    fn node_no_block_sugar() {
        let out = parse("diagram { node lonely }").unwrap();
        assert_eq!(out.graph.nodes.len(), 1);
        assert_eq!(out.graph.nodes[0].id, "lonely");
        assert!(out.graph.nodes[0].label.is_none());
    }

    #[test]
    fn diagram_no_profile_no_layout_defaults_flowchart() {
        let out = parse("diagram { node a {} }").unwrap();
        assert_eq!(out.profile, Some(DiagramType::Flowchart));
        assert_eq!(out.layout.name, "hierarchical");
    }

    #[test]
    fn diagram_explicit_layout_no_profile() {
        let out = parse("diagram { layout: tree node a {} }").unwrap();
        assert_eq!(out.profile, None);
        assert_eq!(out.layout.name, "tree");
    }

    #[test]
    fn end_to_end_archetype_expansion() {
        let source = r#"diagram {
    node db { label: "DB", archetype: database }
    node gw { label: "GW", archetype: gateway, icon: none }
    node svc "Service" service
    node custom { archetype: database, shape: rounded_rect, variant: primary }
    node unknown_arch { archetype: foobar }
}"#;
        let out = parse(source).unwrap();

        // db: archetype fills shape + variant, no icon (database has icon: None)
        let db = &out.graph.nodes[0];
        assert_eq!(db.shape, Some(tautcore_model::NodeShape::Cylinder));
        assert_eq!(
            db.attrs.get("variant").and_then(|v| v.as_str()),
            Some("info")
        );
        assert!(!db.attrs.contains_key("icon"));

        // gw: explicit `icon: none` blocks archetype icon fill (gateway has no icon anyway)
        let gw = &out.graph.nodes[1];
        assert_eq!(gw.shape, Some(tautcore_model::NodeShape::Diamond));
        assert_eq!(gw.attrs.get("icon").and_then(|v| v.as_str()), Some("none"));

        // svc: positional sugar → archetype: service → fills shape/variant/icon
        let svc = &out.graph.nodes[2];
        assert_eq!(svc.label.as_deref(), Some("Service"));
        assert_eq!(svc.shape, Some(tautcore_model::NodeShape::RoundedRect));
        assert_eq!(
            svc.attrs.get("variant").and_then(|v| v.as_str()),
            Some("default")
        );
        assert_eq!(
            svc.attrs.get("icon").and_then(|v| v.as_str()),
            Some("service")
        );

        // custom: explicit shape + variant override archetype defaults
        let custom = &out.graph.nodes[3];
        assert_eq!(custom.shape, Some(tautcore_model::NodeShape::RoundedRect));
        assert_eq!(
            custom.attrs.get("variant").and_then(|v| v.as_str()),
            Some("primary")
        );

        // unknown_arch: no expansion, no error
        let unk = &out.graph.nodes[4];
        assert!(unk.shape.is_none());
        assert!(!unk.attrs.contains_key("variant"));
    }

    #[test]
    fn end_to_end_partition_swimlane() {
        let source = r#"diagram {
    profile: flowchart,
    layout: hierarchical { direction: top-to-bottom }

    partition {
        column customer { label: "客户" }
        column sales { label: "销售" }
        column warehouse { label: "仓库" }
    }

    node order { label: "下单", cell_col: customer }
    node confirm { label: "确认", cell_col: sales }
    node ship { label: "发货", cell_col: warehouse }

    order -> confirm
    confirm -> ship
}"#;
        let out = parse(source).unwrap();

        // Graph.partition filled
        let grid = out
            .graph
            .partition
            .as_ref()
            .expect("partition should be set");
        assert_eq!(grid.columns.len(), 3);
        assert!(grid.rows.is_empty());
        assert_eq!(grid.columns[0].id, "customer");
        assert_eq!(grid.columns[0].label.as_deref(), Some("客户"));
        assert_eq!(grid.columns[1].id, "sales");
        assert_eq!(grid.columns[2].id, "warehouse");
        assert_eq!(grid.columns[2].label.as_deref(), Some("仓库"));

        // cell_col lifted on nodes
        let order = &out.graph.nodes[0];
        assert_eq!(
            order
                .partition_cell
                .as_ref()
                .and_then(|c| c.column.as_deref()),
            Some("customer")
        );
        let ship = &out.graph.nodes[2];
        assert_eq!(
            ship.partition_cell
                .as_ref()
                .and_then(|c| c.column.as_deref()),
            Some("warehouse")
        );
    }

    #[test]
    fn end_to_end_partition_matrix() {
        let source = r#"diagram {
    partition {
        column col_a { label: "A" }
        row row_x { label: "X" }
    }
    node n1 { cell_col: col_a, cell_row: row_x }
}"#;
        let out = parse(source).unwrap();
        let grid = out.graph.partition.as_ref().unwrap();
        assert_eq!(grid.columns.len(), 1);
        assert_eq!(grid.rows.len(), 1);
        assert_eq!(grid.rows[0].id, "row_x");

        let n1 = &out.graph.nodes[0];
        let cell = n1.partition_cell.as_ref().unwrap();
        assert_eq!(cell.column.as_deref(), Some("col_a"));
        assert_eq!(cell.row.as_deref(), Some("row_x"));
    }
}
