//! Comprehensive integration tests for DSL syntax sugar combinations.
//!
//! Table-driven where possible (AGENTS.md §4). Asserts observable output only.

use plotgram_model::attr::AttrValue;
use plotgram_model::graph::{Arrow, NodeRole};
use plotgram_model::port::Side;
use plotgram_parse::{parse, ParseOutput};

// ─── Helpers ────────────────────────────────────────────────────────────────

fn p(src: &str) -> ParseOutput {
    parse(src).unwrap_or_else(|e| panic!("parse failed: {e}\nsource: {src}"))
}

fn node<'a>(out: &'a ParseOutput, id: &str) -> &'a plotgram_model::graph::Node {
    out.graph
        .find_node(id)
        .unwrap_or_else(|| panic!("node `{id}` not found"))
}

fn edge_at(out: &ParseOutput, idx: usize) -> &plotgram_model::graph::Edge {
    &out.graph.edges[idx]
}

// ─── §5.5 Node positional sugar ─────────────────────────────────────────────

#[test]
fn node_sugar_combinations() {
    // Table: (source, expected_label, expected_archetype, expected_icon)
    let cases: &[(&str, Option<&str>, Option<&str>, Option<&str>)] = &[
        // No sugar at all
        ("diagram { node a }", None, None, None),
        ("diagram { node a {} }", None, None, None),
        // Label only
        (r#"diagram { node a "Hello" }"#, Some("Hello"), None, None),
        // Label + archetype
        (
            r#"diagram { node a "DB" database }"#,
            Some("DB"),
            Some("database"),
            None,
        ),
        // Label + archetype + icon
        (
            r#"diagram { node a "Cache" cache redis }"#,
            Some("Cache"),
            Some("cache"),
            Some("redis"),
        ),
        // Empty string → no label
        (r#"diagram { node a "" database }"#, None, Some("database"), None),
        // Empty string + archetype + icon
        (
            r#"diagram { node a "" service aws }"#,
            None,
            Some("service"),
            Some("aws"),
        ),
    ];

    for (src, label, archetype, icon) in cases {
        let out = p(src);
        let n = node(&out, "a");
        assert_eq!(n.label.as_deref(), *label, "label mismatch for: {src}");
        assert_eq!(
            n.attrs.get("archetype").and_then(|v| v.as_str()),
            *archetype,
            "archetype mismatch for: {src}"
        );
        assert_eq!(
            n.attrs.get("icon").and_then(|v| v.as_str()),
            *icon,
            "icon mismatch for: {src}"
        );
    }
}

#[test]
fn node_sugar_plus_block_merge() {
    // Positional sugar + block attrs (non-conflicting) → merged
    let out = p(r#"diagram { node a "Label" database { status: healthy } }"#);
    let n = node(&out, "a");
    assert_eq!(n.label.as_deref(), Some("Label"));
    assert_eq!(n.attrs.get("archetype").and_then(|v| v.as_str()), Some("database"));
    assert_eq!(n.attrs.get("status").and_then(|v| v.as_str()), Some("healthy"));
}

#[test]
fn node_sugar_conflict_label() {
    // Positional label + block label → DuplicateAttr error
    let err = parse(r#"diagram { node a "X" { label: "Y" } }"#).unwrap_err();
    assert!(
        matches!(&err, plotgram_parse::ParseError::DuplicateAttr { key, .. } if key == "label"),
        "expected DuplicateAttr(label), got: {err:?}"
    );
}

#[test]
fn node_sugar_conflict_archetype() {
    let err = parse(r#"diagram { node a "X" database { archetype: service } }"#).unwrap_err();
    assert!(
        matches!(&err, plotgram_parse::ParseError::DuplicateAttr { key, .. } if key == "archetype"),
        "expected DuplicateAttr(archetype), got: {err:?}"
    );
}

#[test]
fn node_sugar_conflict_icon() {
    let err = parse(r#"diagram { node a "X" database myicon { icon: other } }"#).unwrap_err();
    assert!(
        matches!(&err, plotgram_parse::ParseError::DuplicateAttr { key, .. } if key == "icon"),
        "expected DuplicateAttr(icon), got: {err:?}"
    );
}

#[test]
fn node_block_only_attrs() {
    let out = p(r##"diagram { node a { label: "Block" variant: primary style.fill: "#FFF" } }"##);
    let n = node(&out, "a");
    assert_eq!(n.label.as_deref(), Some("Block"));
    assert_eq!(n.attrs.get("variant").and_then(|v| v.as_str()), Some("primary"));
    assert_eq!(n.attrs.get("style.fill").and_then(|v| v.as_str()), Some("#FFF"));
}

// ─── §5.5 + archetype expansion interplay ───────────────────────────────────

#[test]
fn node_sugar_archetype_expands_axes() {
    // Positional archetype triggers expansion in pipeline
    let out = p(r#"diagram { node db "Users" database }"#);
    let n = node(&out, "db");
    assert_eq!(n.shape.as_deref(), Some("cylinder"));
    assert_eq!(n.attrs.get("variant").and_then(|v| v.as_str()), Some("info"));
    // database has no icon → not filled
    assert!(!n.attrs.contains_key("icon"));
}

#[test]
fn node_sugar_icon_blocks_archetype_icon() {
    // Positional icon (3rd atom) counts as "explicit" → archetype won't override
    let out = p(r#"diagram { node svc "S" service custom_icon }"#);
    let n = node(&out, "svc");
    assert_eq!(n.attrs.get("icon").and_then(|v| v.as_str()), Some("custom_icon"));
    // shape/variant still filled from archetype
    assert_eq!(n.shape.as_deref(), Some("rounded_rect"));
    assert_eq!(n.attrs.get("variant").and_then(|v| v.as_str()), Some("default"));
}

#[test]
fn node_explicit_shape_in_block_overrides_archetype() {
    let out = p(r#"diagram { node db "DB" database { shape: rounded_rect } }"#);
    let n = node(&out, "db");
    assert_eq!(n.shape.as_deref(), Some("rounded_rect"));
    // variant still filled
    assert_eq!(n.attrs.get("variant").and_then(|v| v.as_str()), Some("info"));
}

// ─── §6.5 Group positional label sugar ──────────────────────────────────────

#[test]
fn group_sugar_combinations() {
    let cases: &[(&str, Option<&str>)] = &[
        ("diagram { group g { node a {} } }", None),
        (r#"diagram { group g "My Group" { node a {} } }"#, Some("My Group")),
        (r#"diagram { group g "" { node a {} } }"#, None), // empty string → no label
    ];

    for (src, label) in cases {
        let out = p(src);
        let g = &out.graph.groups[0];
        assert_eq!(g.label.as_deref(), *label, "group label mismatch for: {src}");
    }
}

#[test]
fn group_sugar_plus_block_label_conflict() {
    let err = parse(r#"diagram { group g "A" { label: "B" node a {} } }"#).unwrap_err();
    assert!(
        matches!(&err, plotgram_parse::ParseError::DuplicateAttr { key, .. } if key == "label"),
        "expected DuplicateAttr(label), got: {err:?}"
    );
}

#[test]
fn group_block_attrs() {
    let out = p(r#"diagram { group g { label: "G" variant: secondary node a {} } }"#);
    let g = &out.graph.groups[0];
    assert_eq!(g.label.as_deref(), Some("G"));
    assert_eq!(g.attrs.get("variant").and_then(|v| v.as_str()), Some("secondary"));
}

#[test]
fn group_nested() {
    let out = p(r#"diagram {
        group outer "Outer" {
            group inner "Inner" {
                node x {}
            }
            node y {}
        }
    }"#);
    let outer = &out.graph.groups[0];
    assert_eq!(outer.label.as_deref(), Some("Outer"));
    assert_eq!(outer.nodes.len(), 1); // y
    assert_eq!(outer.groups.len(), 1); // inner
    let inner = &outer.groups[0];
    assert_eq!(inner.label.as_deref(), Some("Inner"));
    assert_eq!(inner.nodes.len(), 1); // x
}

// ─── §7.5 Edge positional label sugar ───────────────────────────────────────

#[test]
fn edge_sugar_combinations() {
    // Table: (source, expected_label, expected_arrow)
    let cases: &[(&str, Option<&str>, Arrow)] = &[
        ("diagram { node a {} node b {} a -> b }", None, Arrow::Forward),
        (
            r#"diagram { node a {} node b {} a -> b "Request" }"#,
            Some("Request"),
            Arrow::Forward,
        ),
        (
            "diagram { node a {} node b {} a --> b }",
            None,
            Arrow::Response,
        ),
        (
            r#"diagram { node a {} node b {} a --> b "Response" }"#,
            Some("Response"),
            Arrow::Response,
        ),
        (
            "diagram { node a {} node b {} a <-> b }",
            None,
            Arrow::Bidirectional,
        ),
        (
            r#"diagram { node a {} node b {} a <-> b "Sync" }"#,
            Some("Sync"),
            Arrow::Bidirectional,
        ),
        // Empty string → no label
        (r#"diagram { node a {} node b {} a -> b "" }"#, None, Arrow::Forward),
    ];

    for (src, label, arrow) in cases {
        let out = p(src);
        let e = edge_at(&out, 0);
        assert_eq!(e.label.as_deref(), *label, "edge label mismatch for: {src}");
        assert_eq!(e.arrow, *arrow, "arrow mismatch for: {src}");
    }
}

#[test]
fn edge_sugar_plus_block() {
    let out = p(r#"diagram { node a {} node b {} a -> b "Call" { from_side: east } }"#);
    let e = edge_at(&out, 0);
    assert_eq!(e.label.as_deref(), Some("Call"));
    assert_eq!(e.from_port.unwrap().side, Side::East);
}

#[test]
fn edge_sugar_label_conflict() {
    let err = parse(r#"diagram { node a {} node b {} a -> b "X" { label: "Y" } }"#).unwrap_err();
    assert!(
        matches!(&err, plotgram_parse::ParseError::DuplicateAttr { key, .. } if key == "label"),
        "expected DuplicateAttr(label), got: {err:?}"
    );
}

#[test]
fn edge_block_only() {
    let out = p(r#"diagram { node a {} node b {} a -> b { label: "Block" head_label: "H" tail_label: "T" } }"#);
    let e = edge_at(&out, 0);
    assert_eq!(e.label.as_deref(), Some("Block"));
    assert_eq!(e.head_label.as_deref(), Some("H"));
    assert_eq!(e.tail_label.as_deref(), Some("T"));
}

// ─── §7.4 Edge port attrs + lift ────────────────────────────────────────────

#[test]
fn edge_port_lift_combinations() {
    // Table: (attrs_src, expect_from_side, expect_from_slot, expect_to_side, expect_to_slot)
    let cases: &[(&str, Option<Side>, Option<u32>, Option<Side>, Option<u32>)] = &[
        ("from_side: north", Some(Side::North), None, None, None),
        ("to_side: south", None, None, Some(Side::South), None),
        ("from_side: east to_side: west", Some(Side::East), None, Some(Side::West), None),
        ("from_side: north from_slot: 2", Some(Side::North), Some(2), None, None),
        (
            "from_side: south from_slot: 0 to_side: north to_slot: 1",
            Some(Side::South),
            Some(0),
            Some(Side::North),
            Some(1),
        ),
    ];

    for (attrs, fs, fsl, ts, tsl) in cases {
        let src = format!("diagram {{ node a {{}} node b {{}} a -> b {{ {attrs} }} }}");
        let out = p(&src);
        let e = edge_at(&out, 0);
        let fp = e.from_port;
        let tp = e.to_port;
        assert_eq!(fp.map(|p| p.side), *fs, "from_side for: {attrs}");
        assert_eq!(fp.and_then(|p| p.slot), *fsl, "from_slot for: {attrs}");
        assert_eq!(tp.map(|p| p.side), *ts, "to_side for: {attrs}");
        assert_eq!(tp.and_then(|p| p.slot), *tsl, "to_slot for: {attrs}");
    }
}

#[test]
fn edge_group_lift() {
    let out = p("diagram { node a {} node b {} a -> b { edge_group: bus1 } }");
    let e = edge_at(&out, 0);
    assert_eq!(e.edge_group.as_deref(), Some("bus1"));
    assert!(!e.attrs.contains_key("edge_group"));
}

// ─── §7.6 @group frame edges ────────────────────────────────────────────────

#[test]
fn group_frame_edge_basic() {
    let out = p(r#"diagram {
        group fe { node web {} }
        group be { node api {} }
        @fe -> @be { from_side: east to_side: west label: "HTTP" }
    }"#);
    assert_eq!(out.graph.edges.len(), 1);
    let e = edge_at(&out, 0);
    assert!(e.source.starts_with("ga_fe_east"));
    assert!(e.target.starts_with("ga_be_west"));
    assert_eq!(e.label.as_deref(), Some("HTTP"));
}

#[test]
fn group_frame_edge_mixed_endpoints() {
    let out = p(r#"diagram {
        group be { node api {} }
        node user {}
        user -> @be { to_side: north }
        @be -> user { from_side: south }
    }"#);
    assert_eq!(out.graph.edges.len(), 2);
    let e0 = edge_at(&out, 0);
    assert_eq!(e0.source, "user");
    assert!(e0.target.starts_with("ga_be_north"));
    let e1 = edge_at(&out, 1);
    assert!(e1.source.starts_with("ga_be_south"));
    assert_eq!(e1.target, "user");
}

#[test]
fn group_frame_anchor_reuse() {
    let out = p(r#"diagram {
        group g { node x {} }
        node a {} node b {}
        a -> @g { to_side: west }
        b -> @g { to_side: west }
    }"#);
    // Same (group, side, slot=None) → same anchor
    let e0 = edge_at(&out, 0);
    let e1 = edge_at(&out, 1);
    assert_eq!(e0.target, e1.target, "anchors should be reused");
}

#[test]
fn group_frame_anchor_different_slots() {
    let out = p(r#"diagram {
        group g { node x {} }
        node a {} node b {}
        a -> @g { to_side: west to_slot: 0 }
        b -> @g { to_side: west to_slot: 1 }
    }"#);
    let e0 = edge_at(&out, 0);
    let e1 = edge_at(&out, 1);
    assert_ne!(e0.target, e1.target, "different slots → different anchors");
}

#[test]
fn group_frame_with_sugar_label() {
    let out = p(r#"diagram {
        group g { node x {} }
        node a {}
        a -> @g "调用" { to_side: east }
    }"#);
    let e = edge_at(&out, 0);
    assert_eq!(e.label.as_deref(), Some("调用"));
    assert!(e.target.starts_with("ga_g_east"));
}

// ─── Diagram-level attrs ────────────────────────────────────────────────────

#[test]
fn diagram_layout_with_options() {
    let out = p("diagram { layout: hierarchical { direction: left-to-right spacing: 20 } node a {} }");
    assert_eq!(out.layout.name, "hierarchical");
    assert_eq!(
        out.layout.options.get("direction"),
        Some(&AttrValue::Atom("left-to-right".into()))
    );
    assert_eq!(
        out.layout.options.get("spacing"),
        Some(&AttrValue::Num(20.0))
    );
}

#[test]
fn diagram_profile_plus_layout_override() {
    let out = p("diagram { profile: sequence layout: custom node a {} }");
    assert_eq!(out.profile, Some(plotgram_model::profile::DiagramType::Sequence));
    assert_eq!(out.layout.name, "custom"); // explicit overrides profile default
}

#[test]
fn diagram_meta_fields() {
    let out = p(r#"diagram {
        title: "My Diagram"
        theme: dark
        render_style: sketch
        node a {}
    }"#);
    assert_eq!(out.meta.title.as_deref(), Some("My Diagram"));
    assert_eq!(out.meta.theme.as_deref(), Some("dark"));
    assert_eq!(out.meta.render_style.as_deref(), Some("sketch"));
}

#[test]
fn diagram_meta_namespace_attrs() {
    // §14.1: meta.<identifier> namespace — arbitrary metadata stored in extra.
    let out = p(r#"diagram {
        meta.author: "jane"
        meta.version: 2
        node a {}
    }"#);
    assert_eq!(
        out.meta.extra.get("meta.author"),
        Some(&plotgram_model::attr::AttrValue::Str("jane".into()))
    );
    assert!(out.meta.extra.contains_key("meta.version"));
}

#[test]
fn multiple_edges_same_pair() {
    // §7.8: parallel edges between the same pair are allowed.
    let out = p("diagram { node a {} node b {} a -> b a -> b { label: \"second\" } }");
    assert_eq!(out.graph.edges.len(), 2);
    assert_eq!(out.graph.edges[0].source, "a");
    assert_eq!(out.graph.edges[0].target, "b");
    assert_eq!(out.graph.edges[1].source, "a");
    assert_eq!(out.graph.edges[1].target, "b");
    // Distinct sequential ids
    assert_ne!(out.graph.edges[0].id, out.graph.edges[1].id);
}

#[test]
fn attribute_block_commas() {
    let out = p(r#"diagram { node a { label: "开始", archetype: start } }"#);
    let n = node(&out, "a");
    assert_eq!(n.label.as_deref(), Some("开始"));
    assert_eq!(n.attrs.get("archetype").and_then(|v| v.as_str()), Some("start"));
}

#[test]
fn unknown_diagram_key_warns() {
    let out = p(r#"diagram { unknown_key: foo node a {} }"#);
    assert_eq!(out.warnings.len(), 1);
    assert!(out.warnings[0].message.contains("unknown_key"));
    assert_eq!(
        out.meta.extra.get("unknown_key").and_then(|v| v.as_str()),
        Some("foo")
    );
}

#[test]
fn meta_diagram_key_no_warning() {
    let out = p(r#"diagram { meta.author: "jane" node a {} }"#);
    assert!(out.warnings.is_empty());
}

// ─── Node structural lift (role / host_group / anchor) ──────────────────────

#[test]
fn node_role_lift() {
    let out = p(r#"diagram {
        group g {
            node x {}
            node a { role: group_anchor host_group: g side: north }
        }
    }"#);
    let g = &out.graph.groups[0];
    let n = g.nodes.iter().find(|n| n.id == "a").unwrap();
    assert_eq!(n.role, NodeRole::GroupAnchor);
    assert_eq!(n.host_group.as_deref(), Some("g"));
    assert_eq!(n.anchor.unwrap().side, Side::North);
    // Structural keys removed from attrs
    assert!(!n.attrs.contains_key("role"));
    assert!(!n.attrs.contains_key("host_group"));
    assert!(!n.attrs.contains_key("side"));
}

// ─── Multiple edges + declaration order ─────────────────────────────────────

#[test]
fn edge_declaration_order_preserved() {
    let out = p(r#"diagram {
        node a {} node b {} node c {}
        a -> b
        b -> c
        c -> a
        a <-> b
    }"#);
    let ids: Vec<&str> = out.graph.edges_in_declaration_order()
        .iter()
        .map(|e| e.id.as_str())
        .collect();
    assert_eq!(ids, ["e0", "e1", "e2", "e3"]);
}

#[test]
fn edges_inside_group_declaration_order() {
    let out = p(r#"diagram {
        node x {}
        group g {
            node a {} node b {}
            a -> b
            b -> a
        }
        x -> a
    }"#);
    // edges_in_declaration_order: top-level vector first, then groups depth-first.
    // Group edges a->b (e0), b->a (e1); top-level x->a (e2) → [e2, e0, e1].
    let ids: Vec<&str> = out.graph.edges_in_declaration_order()
        .iter()
        .map(|e| e.id.as_str())
        .collect();
    assert_eq!(ids, ["e2", "e0", "e1"]);
}

#[test]
fn group_frame_edges_keep_declaration_order() {
    // §7.6.2 #6 / §8.1: @group 展开不得改变边声明序（sequence 时间轴依赖）
    let out = p(r#"diagram {
        group g { node x {} }
        node a {} node b {}
        a -> @g { to_side: west }
        a -> b
        b -> @g { to_side: east }
    }"#);
    let ids: Vec<&str> = out.graph.edges_in_declaration_order()
        .iter()
        .map(|e| e.id.as_str())
        .collect();
    assert_eq!(ids, ["e0", "e1", "e2"]);
}

#[test]
fn group_frame_edge_ports_lifted() {
    // @group 展开后，from_side/to_side 仍须提升为端口约束
    let out = p(r#"diagram {
        group fe { node ui {} }
        group be { node api {} }
        @fe -> @be { from_side: east, to_side: west }
    }"#);
    let e = &out.graph.edges[0];
    assert_eq!(e.from_port.as_ref().map(|p| p.side), Some(Side::East));
    assert_eq!(e.to_port.as_ref().map(|p| p.side), Some(Side::West));
}

#[test]
fn anchor_id_collision_gets_edge_suffix() {
    // §7.6.2 #3: 合成锚点 id 与用户声明 id 冲突时加边 id 后缀
    let out = p(r#"diagram {
        node ga_g_west {}
        group g { node x {} }
        ga_g_west -> @g { to_side: west }
    }"#);
    let e = &out.graph.edges[0];
    assert_ne!(e.target, "ga_g_west");
    assert_eq!(e.target, "ga_g_west_e0");
    // 用户节点未被覆盖，锚点注入 group 内
    assert!(out.graph.nodes.iter().any(|n| n.id == "ga_g_west"));
    let g = &out.graph.groups[0];
    assert!(g.nodes.iter().any(|n| n.id == "ga_g_west_e0"));
}

// ─── Doc comments ───────────────────────────────────────────────────────────

#[test]
fn file_leading_doc_comment() {
    // Only file-leading doc comments are extracted (stored in AST, not per-node)
    let out = p("// File description\ndiagram { node a {} }");
    // doc_comment is on the AST level, not propagated to node attrs
    // (per-node /// is treated as regular comment and skipped)
    assert_eq!(out.graph.nodes.len(), 1);
}

// ─── Archetype expansion: full matrix ───────────────────────────────────────

#[test]
fn archetype_expansion_all_builtin() {
    use plotgram_model::archetype::ARCHETYPES;

    for def in ARCHETYPES {
        let src = format!(
            r#"diagram {{ node n {{ archetype: {} }} }}"#,
            def.id
        );
        let out = p(&src);
        let n = node(&out, "n");

        // Shape filled from archetype
        assert_eq!(
            n.shape.as_deref(),
            def.shape,
            "shape for archetype `{}`",
            def.id
        );
        // Variant filled
        assert_eq!(
            n.attrs.get("variant").and_then(|v| v.as_str()),
            def.variant,
            "variant for archetype `{}`",
            def.id
        );
        // Icon filled only if def provides one
        match def.icon {
            Some(icon) => assert_eq!(
                n.attrs.get("icon").and_then(|v| v.as_str()),
                Some(icon),
                "icon for archetype `{}`",
                def.id
            ),
            None => assert!(
                !n.attrs.contains_key("icon"),
                "icon should not be set for archetype `{}`",
                def.id
            ),
        }
    }
}

#[test]
fn archetype_fill_only_never_overrides() {
    // All three axes explicitly set → archetype changes nothing
    let out = p(r#"diagram { node n { archetype: service shape: circle variant: primary icon: custom } }"#);
    let n = node(&out, "n");
    assert_eq!(n.shape.as_deref(), Some("circle"));
    assert_eq!(n.attrs.get("variant").and_then(|v| v.as_str()), Some("primary"));
    assert_eq!(n.attrs.get("icon").and_then(|v| v.as_str()), Some("custom"));
}

// ─── Combined scenario: real-world diagram ──────────────────────────────────

#[test]
fn combined_real_world_flowchart() {
    let out = p(r##"diagram {
    profile: flowchart
    title: "订单处理"
    layout: hierarchical { direction: top-to-bottom }

    /// 开始节点
    node start "开始" start
    node check "库存检查" decision
    node ok "确认订单" service
    node fail "缺货通知" external
    node db "订单库" database

    group payment "支付模块" {
        node pay "支付网关" gateway
        node bank "银行" external
        pay -> bank "扣款"
        bank --> pay "结果"
    }

    start -> check
    check -> ok "有货"
    check -> fail "无货"
    ok -> pay { from_side: south to_side: north }
    pay --> ok "支付成功"
    ok -> db { label: "写入" }
}"##);

    // Profile & layout
    assert_eq!(out.layout.name, "hierarchical");
    assert_eq!(out.meta.title.as_deref(), Some("订单处理"));

    // Archetype expansion checks
    assert_eq!(node(&out, "start").shape.as_deref(), Some("circle"));
    assert_eq!(node(&out, "check").shape.as_deref(), Some("diamond"));
    assert_eq!(node(&out, "db").shape.as_deref(), Some("cylinder"));
    assert_eq!(node(&out, "pay").shape.as_deref(), Some("diamond"));

    // Group structure
    assert_eq!(out.graph.groups.len(), 1);
    let payment = &out.graph.groups[0];
    assert_eq!(payment.label.as_deref(), Some("支付模块"));
    assert_eq!(payment.nodes.len(), 2);
    assert_eq!(payment.edges.len(), 2);

    // Top-level edges: start->check, check->ok, check->fail, ok->pay, pay-->ok, ok->db
    assert_eq!(out.graph.edges.len(), 6);

    // Port lift on ok->pay
    let ok_pay = out.graph.edges.iter().find(|e| e.source == "ok" && e.target == "pay").unwrap();
    assert_eq!(ok_pay.from_port.unwrap().side, Side::South);
    assert_eq!(ok_pay.to_port.unwrap().side, Side::North);
}

// ─── edge_routing 声明（§10.2）─────────────────────────────────────────────────────────

#[test]
fn edge_routing_explicit_with_options() {
    let out = p(r#"diagram {
        edge_routing: orthogonal { spacing: 8 }
        node a {} node b {}
        a -> b
    }"#);
    let routing = out.edge_routing.expect("edge_routing should be set");
    assert_eq!(routing.name, "orthogonal");
    assert_eq!(
        routing.options.get("spacing"),
        Some(&AttrValue::Num(8.0))
    );
}

#[test]
fn edge_routing_overrides_profile_default() {
    // 显式 edge_routing 覆盖 profile 默认（作者覆盖 > profile 默认）
    let out = p(r#"diagram {
        profile: flowchart
        edge_routing: straight
        node a {} node b {}
        a -> b
    }"#);
    assert_eq!(out.edge_routing.map(|r| r.name), Some("straight".to_string()));
}

// ─── 位置糖同行约束（§5.5.2）────────────────────────────────────────────────

#[test]
fn node_sugar_atom_must_be_on_same_line() {
    // 换行终止位置糖：下一行的裸标识符是新语句（边源），不是 archetype
    let out = p("diagram {\n  node x {}\n  node y {}\n  node a \"A\"\n  x -> y\n}");
    let a = node(&out, "a");
    assert_eq!(a.label.as_deref(), Some("A"));
    assert!(a.attrs.get("archetype").is_none());
    assert!(a.shape.is_none());
    assert_eq!(out.graph.edges.len(), 1);
    assert_eq!(out.graph.edges[0].source, "x");
}

// ─── profile 自环规则（§4.2 / §7.8）─────────────────────────────────────────────

#[test]
fn profile_state_allows_self_loop() {
    let out = p("diagram { profile: state node a {} a -> a }");
    assert_eq!(out.graph.edges.len(), 1);
    assert_eq!(out.graph.edges[0].source, out.graph.edges[0].target);
}

// ─── profile 默认 layout 展开（§8 表）─────────────────────────────────────────

#[test]
fn profile_default_layout_all_six() {
    // 封闭集全覆盖：profile → 默认 layout（与 model Profile::for_type 对账）
    let cases: &[(&str, &str)] = &[
        ("flowchart", "hierarchical"),
        ("architecture", "hierarchical"),
        ("state", "hierarchical"),
        ("sequence", "sequence"),
        ("mindmap", "tree"),
        ("er", "circular"),
    ];
    for (profile, expected_layout) in cases {
        let out = p(&format!("diagram {{ profile: {profile} node a {{}} }}"));
        assert_eq!(
            out.layout.name, *expected_layout,
            "profile `{profile}` should default to layout `{expected_layout}`"
        );
    }
}
