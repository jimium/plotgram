//! Integration tests for error paths and invalid DSL inputs.
//!
//! Each test asserts the error variant and key diagnostic info.

use plotgram_parse::{parse, ParseError};

// ─── Helpers ────────────────────────────────────────────────────────────────

fn expect_err(src: &str) -> ParseError {
    match parse(src) {
        Ok(_) => panic!("expected error but parse succeeded for:\n{src}"),
        Err(e) => e,
    }
}

// ─── Duplicate IDs ──────────────────────────────────────────────────────────

#[test]
fn duplicate_node_id() {
    let e = expect_err("diagram { node a {} node a {} }");
    assert!(matches!(e, ParseError::DuplicateId { .. }), "got: {e:?}");
}

#[test]
fn duplicate_group_id() {
    let e = expect_err("diagram { group g { node x {} } group g { node y {} } }");
    assert!(matches!(e, ParseError::DuplicateId { .. }), "got: {e:?}");
}

#[test]
fn duplicate_node_group_id_collision() {
    let e = expect_err("diagram { node x {} group x { node y {} } }");
    assert!(matches!(e, ParseError::DuplicateId { .. }), "got: {e:?}");
}

#[test]
fn duplicate_id_across_nesting() {
    // Node in group collides with top-level node
    let e = expect_err("diagram { node a {} group g { node a {} } }");
    assert!(matches!(e, ParseError::DuplicateId { .. }), "got: {e:?}");
}

// ─── Unknown profile ────────────────────────────────────────────────────────

#[test]
fn unknown_profile() {
    let e = expect_err("diagram { profile: bogus node a {} }");
    assert!(
        matches!(&e, ParseError::UnknownProfile { value, .. } if value == "bogus"),
        "got: {e:?}"
    );
}

// ─── Unknown edge endpoint ──────────────────────────────────────────────────

#[test]
fn edge_unknown_source() {
    let e = expect_err("diagram { node b {} ghost -> b }");
    assert!(
        matches!(&e, ParseError::Semantic(msg) if msg.contains("ghost")),
        "got: {e:?}"
    );
}

#[test]
fn edge_unknown_target() {
    let e = expect_err("diagram { node a {} a -> ghost }");
    assert!(
        matches!(&e, ParseError::Semantic(msg) if msg.contains("ghost")),
        "got: {e:?}"
    );
}

#[test]
fn edge_unknown_group_frame() {
    let e = expect_err("diagram { node a {} a -> @nonexist { to_side: north } }");
    assert!(
        matches!(&e, ParseError::Semantic(msg) if msg.contains("nonexist")),
        "got: {e:?}"
    );
}

// ─── Port constraint errors ─────────────────────────────────────────────────

#[test]
fn removed_edge_port_key_rejected() {
    let e = expect_err("diagram { node a {} node b {} a -> b { from_slot: 0 } }");
    assert!(matches!(e, ParseError::Port(_)), "got: {e:?}");
    assert!(e.to_string().contains("unsupported"), "got: {e}");
}

#[test]
fn invalid_side_value() {
    let e = expect_err("diagram { node a {} node b {} a -> b { from_side: diagonal } }");
    // Should fail: "diagonal" is not a valid Side
    assert!(
        matches!(&e, ParseError::Port(_) | ParseError::Semantic(_)),
        "got: {e:?}"
    );
}

// ─── @group frame edge missing side ─────────────────────────────────────────

#[test]
fn group_frame_missing_from_side() {
    let e = expect_err(
        r#"diagram {
        group g { node x {} }
        node a {}
        @g -> a { to_side: north }
    }"#,
    );
    assert!(
        matches!(&e, ParseError::Semantic(msg) if msg.contains("from_side")),
        "got: {e:?}"
    );
}

#[test]
fn group_frame_missing_to_side() {
    let e = expect_err(
        r#"diagram {
        group g { node x {} }
        node a {}
        a -> @g { from_side: south }
    }"#,
    );
    assert!(
        matches!(&e, ParseError::Semantic(msg) if msg.contains("to_side")),
        "got: {e:?}"
    );
}

#[test]
fn group_frame_both_group_missing_side() {
    let e = expect_err(
        r#"diagram {
        group g1 { node x {} }
        group g2 { node y {} }
        @g1 -> @g2 { from_side: east }
    }"#,
    );
    // g2 endpoint missing to_side
    assert!(
        matches!(&e, ParseError::Semantic(msg) if msg.contains("to_side")),
        "got: {e:?}"
    );
}

// ─── Syntax errors ──────────────────────────────────────────────────────────

#[test]
fn missing_diagram_keyword() {
    let e = expect_err("node a {}");
    assert!(matches!(e, ParseError::Syntax { .. }), "got: {e:?}");
}

#[test]
fn missing_diagram_brace() {
    let e = expect_err("diagram node a {}");
    assert!(matches!(e, ParseError::Syntax { .. }), "got: {e:?}");
}

#[test]
fn unclosed_brace() {
    let e = expect_err("diagram { node a {}");
    assert!(matches!(e, ParseError::Syntax { .. }), "got: {e:?}");
}

#[test]
fn edge_missing_arrow() {
    let e = expect_err("diagram { node a {} node b {} a b }");
    assert!(matches!(e, ParseError::Syntax { .. }), "got: {e:?}");
}

#[test]
fn node_missing_id() {
    let e = expect_err("diagram { node }");
    assert!(matches!(e, ParseError::Syntax { .. }), "got: {e:?}");
}

#[test]
fn group_missing_body() {
    let e = expect_err("diagram { group g }");
    assert!(matches!(e, ParseError::Syntax { .. }), "got: {e:?}");
}

// ─── Duplicate attrs in block ───────────────────────────────────────────────

#[test]
fn duplicate_attr_in_node_block() {
    let e = expect_err(r#"diagram { node a { label: "X", label: "Y" } }"#);
    assert!(matches!(e, ParseError::DuplicateAttr { .. }), "got: {e:?}");
}

#[test]
fn duplicate_attr_in_edge_block() {
    let e =
        expect_err("diagram { node a {} node b {} a -> b { from_side: north, from_side: south } }");
    assert!(matches!(e, ParseError::DuplicateAttr { .. }), "got: {e:?}");
}

// ─── Self-loop rejection ────────────────────────────────────────────────────

#[test]
fn self_loop_rejected() {
    let e = expect_err("diagram { node a {} a -> a }");
    assert!(
        matches!(&e, ParseError::Semantic(msg) if msg.contains("self")),
        "got: {e:?}"
    );
}

// ─── Node structural validation ─────────────────────────────────────────────

#[test]
fn entity_with_host_group_rejected() {
    // Entity node (default role) cannot have host_group
    let e = expect_err("diagram { node a { host_group: g } }");
    assert!(
        matches!(&e, ParseError::NodeStructural(_) | ParseError::Semantic(_)),
        "got: {e:?}"
    );
}

#[test]
fn group_anchor_missing_side() {
    let e = expect_err(
        "diagram { group g { node x {} } node a { role: group_anchor, host_group: g } }",
    );
    assert!(
        matches!(&e, ParseError::NodeStructural(_) | ParseError::Semantic(_)),
        "got: {e:?}"
    );
}

// ─── Empty source ───────────────────────────────────────────────────────────

#[test]
fn empty_source() {
    let e = expect_err("");
    assert!(matches!(e, ParseError::Syntax { .. }), "got: {e:?}");
}

#[test]
fn only_comments() {
    let e = expect_err("// just a comment\n// another");
    assert!(matches!(e, ParseError::Syntax { .. }), "got: {e:?}");
}

// ─── P1: Group edge scope violation (§12 #5) ────────────────────────────────

#[test]
fn group_edge_referencing_external_node() {
    // Edge inside group references a top-level node → error
    let e = expect_err(
        r#"diagram {
        node external {}
        group g {
            node x {}
            x -> external
        }
    }"#,
    );
    assert!(
        matches!(&e, ParseError::Semantic(msg) if msg.contains("external")),
        "got: {e:?}"
    );
}

#[test]
fn group_edge_referencing_other_group_node() {
    let e = expect_err(
        r#"diagram {
        group g1 { node a {} }
        group g2 {
            node b {}
            b -> a
        }
    }"#,
    );
    assert!(
        matches!(&e, ParseError::Semantic(msg) if msg.contains("a")),
        "got: {e:?}"
    );
}

#[test]
fn group_edge_to_nested_child_ok() {
    // Edge in parent group referencing node in child group → OK (descendant)
    let out = parse(
        r#"diagram {
        group outer {
            group inner { node x {} }
            node y {}
            y -> x
        }
    }"#,
    );
    assert!(out.is_ok(), "should accept edge to descendant node");
}

// ─── P2a: Reserved words (§10) ──────────────────────────────────────────────

#[test]
fn reserved_word_as_node_id() {
    let cases = &[
        "flowchart",
        "sequence",
        "architecture",
        "state",
        "er",
        "mindmap",
    ];
    for word in cases {
        let src = format!("diagram {{ node {word} {{}} }}");
        let e = expect_err(&src);
        assert!(
            matches!(&e, ParseError::Syntax { message, .. } if message.contains("reserved")),
            "for `{word}`: got {e:?}"
        );
    }
}

#[test]
fn reserved_word_as_group_id() {
    let e = expect_err("diagram { group state { node x {} } }");
    assert!(
        matches!(&e, ParseError::Syntax { message, .. } if message.contains("reserved")),
        "got: {e:?}"
    );
}

#[test]
fn reserved_word_as_attr_key() {
    let e = expect_err("diagram { node a { state: healthy } }");
    assert!(
        matches!(&e, ParseError::Syntax { message, .. } if message.contains("reserved")),
        "got: {e:?}"
    );
}

#[test]
fn reserved_word_as_atom_value_ok() {
    // Reserved words ARE valid as atom values (e.g. profile: flowchart)
    let out = parse("diagram { profile: flowchart node a {} }");
    assert!(out.is_ok());
}

#[test]
fn keyword_as_id_rejected() {
    // 关键字 node/group/diagram 不能作 node/group id（词法层即拒绝）
    let cases = [
        "diagram { node node {} }",
        "diagram { node diagram {} }",
        "diagram { node group {} }",
        "diagram { group node { node x {} } }",
        "diagram { group diagram { node x {} } }",
    ];
    for src in cases {
        let e = expect_err(src);
        assert!(
            matches!(e, ParseError::Syntax { .. }),
            "expected syntax error for {src}, got: {e:?}"
        );
    }
}

// ─── P2b: Bare group id endpoint diagnostic (§7.6.3) ────────────────────────

#[test]
fn bare_group_id_as_edge_source() {
    let e = expect_err(
        r#"diagram {
        group g { node x {} }
        node a {}
        g -> a
    }"#,
    );
    assert!(
        matches!(&e, ParseError::Semantic(msg) if msg.contains("group id") && msg.contains("@")),
        "got: {e:?}"
    );
}

#[test]
fn bare_group_id_as_edge_target() {
    let e = expect_err(
        r#"diagram {
        group g { node x {} }
        node a {}
        a -> g
    }"#,
    );
    assert!(
        matches!(&e, ParseError::Semantic(msg) if msg.contains("group id") && msg.contains("@")),
        "got: {e:?}"
    );
}

// ─── P2c: Group-level algorithm_config (§6.2) ───────────────────────────────

#[test]
fn group_layout_with_options_parses() {
    let out = parse(
        r#"diagram {
        group g {
            layout: hierarchical { direction: left-to-right }
            node a {} node b {}
            a -> b
        }
    }"#,
    );
    assert!(
        out.is_ok(),
        "group layout with options should parse: {out:?}"
    );
}

// ─── P3a: Lexical constraints (§2.1–2.3) ────────────────────────────────────

#[test]
fn identifier_underscore_start_rejected() {
    let e = expect_err("diagram { node _bad {} }");
    assert!(matches!(e, ParseError::Lex { .. }), "got: {e:?}");
}

#[test]
fn identifier_too_long_rejected() {
    let long_id = "a".repeat(65);
    let src = format!("diagram {{ node {long_id} {{}} }}");
    let e = expect_err(&src);
    assert!(matches!(e, ParseError::Lex { .. }), "got: {e:?}");
}

#[test]
fn string_too_long_rejected() {
    let long_str = "x".repeat(257);
    let src = format!(r#"diagram {{ node a {{ label: "{long_str}" }} }}"#);
    let e = expect_err(&src);
    assert!(matches!(e, ParseError::Lex { .. }), "got: {e:?}");
}

// ─── P3b: label/shape type enforcement (§14) ────────────────────────────────

#[test]
fn label_numeric_rejected() {
    let e = expect_err("diagram { node a { label: 42 } }");
    assert!(
        matches!(&e, ParseError::Semantic(msg) if msg.contains("label")),
        "got: {e:?}"
    );
}

#[test]
fn shape_numeric_rejected() {
    let e = expect_err("diagram { node a { shape: 42 } }");
    assert!(
        matches!(&e, ParseError::Semantic(msg) if msg.contains("shape")),
        "got: {e:?}"
    );
}

#[test]
fn shape_boolean_rejected() {
    let e = expect_err("diagram { node a { shape: true } }");
    assert!(
        matches!(&e, ParseError::Semantic(msg) if msg.contains("shape")),
        "got: {e:?}"
    );
}

#[test]
fn shape_unknown_atom_rejected() {
    let e = expect_err("diagram { node a { shape: triangle } }");
    assert!(
        matches!(&e, ParseError::Semantic(msg) if msg.contains("unknown shape") && msg.contains("triangle")),
        "got: {e:?}"
    );
}

// ─── P3d: node bare archetype sugar (§5.5.3) ────────────────────────────────

#[test]
fn node_bare_archetype_atom_rejected() {
    let e = expect_err("diagram { node db database }");
    assert!(
        matches!(&e, ParseError::Syntax { message, .. } if message.contains("bare atom")),
        "got: {e:?}"
    );
}

// ─── P3e: group_anchor placement (§5.7.1) ───────────────────────────────────

#[test]
fn group_anchor_unknown_host_group() {
    let e =
        expect_err("diagram { node a { role: group_anchor, host_group: missing, side: north } }");
    assert!(
        matches!(&e, ParseError::Semantic(msg) if msg.contains("missing")),
        "got: {e:?}"
    );
}

#[test]
fn group_anchor_not_in_host_group() {
    let e = expect_err(
        r#"diagram {
        group g { node x {} }
        node a { role: group_anchor, host_group: g, side: north }
    }"#,
    );
    assert!(
        matches!(&e, ParseError::Semantic(msg) if msg.contains("direct member")),
        "got: {e:?}"
    );
}

// ─── P3f: unknown diagram key warnings (§4.2) ───────────────────────────────

#[test]
fn unknown_diagram_key_warning_not_error() {
    let out = parse("diagram { foo: bar node a {} }").unwrap();
    assert_eq!(out.warnings.len(), 1);
    assert!(out.warnings[0].message.contains("foo"));
}

// ─── P3g: attribute block commas ────────────────────────────────────────────

#[test]
fn comma_separated_attrs_in_block() {
    let out = parse(r#"diagram { node a { label: "X", variant: primary } }"#).unwrap();
    let n = &out.graph.nodes[0];
    assert_eq!(n.label.as_deref(), Some("X"));
    assert_eq!(
        n.attrs.get("variant").and_then(|v| v.as_str()),
        Some("primary")
    );
}

// ─── P3h: atom dot rules (§2.2) ─────────────────────────────────────────────

#[test]
fn atom_consecutive_dots_rejected() {
    let e = expect_err("diagram { node a { variant: foo..bar } }");
    assert!(matches!(e, ParseError::Lex { .. }), "got: {e:?}");
}

#[test]
fn atom_trailing_dot_rejected() {
    let e = expect_err("diagram { node a { theme: common. } }");
    assert!(matches!(e, ParseError::Lex { .. }), "got: {e:?}");
}

// ─── P3c: removed edge port keys (§7.4) ─────────────────────────────────────

#[test]
fn removed_slot_key_rejected_even_with_side() {
    let e = expect_err("diagram { node a {} node b {} a -> b { from_side: north, from_slot: 1 } }");
    assert!(matches!(e, ParseError::Port(_)), "got: {e:?}");
}

#[test]
fn removed_ratio_key_rejected() {
    let e = expect_err("diagram { node a {} node b {} a -> b { from_ratio: 0.5 } }");
    assert!(matches!(e, ParseError::Port(_)), "got: {e:?}");
}

// ─── §7.5.2: arrow / endpoints are syntax-only ─────────────────────────────────

#[test]
fn edge_block_forbidden_keys_rejected() {
    let cases = [
        r#"diagram { node a {} node b {} a -> b { arrow: solid } }"#,
        r#"diagram { node a {} node b {} a -> b { source: a } }"#,
        r#"diagram { node a {} node b {} a -> b { target: b } }"#,
    ];
    for src in cases {
        let e = expect_err(src);
        assert!(
            matches!(&e, ParseError::Semantic(msg) if msg.contains("§7.5.2")),
            "expected §7.5.2 rejection for {src}, got: {e:?}"
        );
    }
}

// ─── §2.7: diagram-level duplicate attributes ──────────────────────────────────────

#[test]
fn diagram_duplicate_attrs_rejected() {
    let cases = [
        r#"diagram { title: "A", title: "B" node a {} }"#,
        r#"diagram { layout: hierarchical, layout: organic node a {} }"#,
        r#"diagram { edge_routing: orthogonal, edge_routing: straight node a {} }"#,
    ];
    for src in cases {
        let e = expect_err(src);
        assert!(
            matches!(e, ParseError::DuplicateAttr { .. }),
            "expected DuplicateAttr for {src}, got: {e:?}"
        );
    }
}

// ─── §14: label type checks unified across node / group / edge ───────────────────

#[test]
fn non_string_label_rejected_everywhere() {
    let cases = [
        "diagram { node a { label: 42 } }",
        "diagram { group g { label: 42 node x {} } }",
        "diagram { node a {} node b {} a -> b { label: 42 } }",
        "diagram { node a {} node b {} a -> b { head_label: true } }",
        "diagram { group g { node x {} } node a {} a -> @g { to_side: west, label: 42 } }",
    ];
    for src in cases {
        let e = expect_err(src);
        assert!(
            matches!(&e, ParseError::Semantic(msg) if msg.contains("must be a string")),
            "expected string-type rejection for {src}, got: {e:?}"
        );
    }
}
