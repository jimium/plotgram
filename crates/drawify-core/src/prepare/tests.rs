//! prepare 模块的单元测试。
//!
//! 从 `prepare` 子模块迁出，保持主文件聚焦实现。

use crate::ast::{
    ArrowType, AttributeMap, AttributeValue, Diagram, Entity, Identifier, SourceInfo, Span,
    TextValue,
};
use crate::types::DiagramType;
use crate::prepare::{
    apply_profile_defaults, expand_structure, merge_style_block_into_attrs,
    attribute_to_style_value, style_value_to_attribute,
};
use crate::theme::StyleValue;
use crate::ast::StyleSource;

fn dummy_span() -> Span {
    Span::dummy()
}

fn make_entity(id: &str, label: &str, attrs: AttributeMap) -> Entity {
    Entity {
        id: Identifier::new_unchecked(id),
        label: label.to_string(),
        attributes: attrs,
        group_id: None,
        span: dummy_span(),
    }
}

#[test]
fn state_diagram_gets_default_type_state() {
    let diagram = Diagram {
        diagram_type: DiagramType::State,
        attributes: vec![],
        entities: vec![
            make_entity("s1", "待支付", AttributeMap::default()),
            make_entity("s2", "处理中", AttributeMap::default()),
        ],
        relations: vec![],
        groups: vec![],
        style_decls: vec![],
        source_info: SourceInfo {
            file: None,
            line_count: 5,
        },
            ..Default::default()
    };

    let result = apply_profile_defaults(diagram).unwrap();

    assert_eq!(
        result.entities[0].attributes.standard.get("type"),
        Some(&AttributeValue::String(TextValue::unquoted("state")))
    );
    assert_eq!(
        result.entities[1].attributes.standard.get("type"),
        Some(&AttributeValue::String(TextValue::unquoted("state")))
    );
}

#[test]
fn existing_type_not_overridden() {
    let mut attrs = AttributeMap::default();
    attrs
        .standard
        .insert("type".to_string(), AttributeValue::String(TextValue::unquoted("initial")));

    let diagram = Diagram {
        diagram_type: DiagramType::State,
        attributes: vec![],
        entities: vec![make_entity("init", "初始化", attrs)],
        relations: vec![],
        groups: vec![],
        style_decls: vec![],
        source_info: SourceInfo {
            file: None,
            line_count: 3,
        },
            ..Default::default()
    };

    let result = apply_profile_defaults(diagram).unwrap();

    assert_eq!(
        result.entities[0].attributes.standard.get("type"),
        Some(&AttributeValue::String(TextValue::unquoted("initial")))
    );
}

#[test]
fn er_diagram_no_default_type() {
    let diagram = Diagram {
        diagram_type: DiagramType::Er,
        attributes: vec![],
        entities: vec![make_entity("user", "User", AttributeMap::default())],
        relations: vec![],
        groups: vec![],
        style_decls: vec![],
        source_info: SourceInfo {
            file: None,
            line_count: 3,
        },
            ..Default::default()
    };

    let result = apply_profile_defaults(diagram).unwrap();

    assert!(result.entities[0].attributes.standard.get("type").is_none());
}

#[test]
fn flowchart_gets_default_type_process() {
    let diagram = Diagram {
        diagram_type: DiagramType::Flowchart,
        attributes: vec![],
        entities: vec![make_entity("step1", "步骤1", AttributeMap::default())],
        relations: vec![],
        groups: vec![],
        style_decls: vec![],
        source_info: SourceInfo {
            file: None,
            line_count: 3,
        },
            ..Default::default()
    };

    let result = apply_profile_defaults(diagram).unwrap();

    assert_eq!(
        result.entities[0].attributes.standard.get("type"),
        Some(&AttributeValue::String(TextValue::unquoted("process")))
    );
}

#[test]
fn sequence_gets_default_type_participant() {
    let diagram = Diagram {
        diagram_type: DiagramType::Sequence,
        attributes: vec![],
        entities: vec![make_entity("api", "API", AttributeMap::default())],
        relations: vec![],
        groups: vec![],
        style_decls: vec![],
        source_info: SourceInfo {
            file: None,
            line_count: 3,
        },
            ..Default::default()
    };

    let result = apply_profile_defaults(diagram).unwrap();

    assert_eq!(
        result.entities[0].attributes.standard.get("type"),
        Some(&AttributeValue::String(TextValue::unquoted("participant")))
    );
}

#[test]
fn architecture_gets_default_type_service() {
    let diagram = Diagram {
        diagram_type: DiagramType::Architecture,
        attributes: vec![],
        entities: vec![make_entity("svc", "服务", AttributeMap::default())],
        relations: vec![],
        groups: vec![],
        style_decls: vec![],
        source_info: SourceInfo {
            file: None,
            line_count: 3,
        },
            ..Default::default()
    };

    let result = apply_profile_defaults(diagram).unwrap();

    assert_eq!(
        result.entities[0].attributes.standard.get("type"),
        Some(&AttributeValue::String(TextValue::unquoted("service")))
    );
}

#[test]
fn idempotent() {
    let diagram = Diagram {
        diagram_type: DiagramType::State,
        attributes: vec![],
        entities: vec![make_entity("s1", "状态1", AttributeMap::default())],
        relations: vec![],
        groups: vec![],
        style_decls: vec![],
        source_info: SourceInfo {
            file: None,
            line_count: 3,
        },
            ..Default::default()
    };

    let first = apply_profile_defaults(diagram).unwrap();
    let second = apply_profile_defaults(first.clone()).unwrap();

    assert_eq!(first.entities[0].attributes.standard, second.entities[0].attributes.standard);
}

// ── StyleValue ↔ AttributeValue 桥接测试 ──────────────────────

#[test]
fn style_value_to_attribute_string() {
    let sv = StyleValue::String("#E3F2FD".to_string());
    let av = style_value_to_attribute(&sv);
    assert_eq!(av, AttributeValue::String(TextValue::quoted("#E3F2FD")));
}

#[test]
fn style_value_to_attribute_number() {
    let sv = StyleValue::Number(2.0);
    let av = style_value_to_attribute(&sv);
    assert_eq!(av, AttributeValue::Number(2.0));
}

#[test]
fn style_value_to_attribute_boolean() {
    let sv = StyleValue::Boolean(true);
    let av = style_value_to_attribute(&sv);
    assert_eq!(av, AttributeValue::Boolean(true));
}

#[test]
fn style_value_to_attribute_array() {
    let sv = StyleValue::Array(vec![5.0, 3.0]);
    let av = style_value_to_attribute(&sv);
    assert_eq!(av, AttributeValue::String(TextValue::quoted("5,3")));
}

#[test]
fn attribute_to_style_value_roundtrip() {
    let original = StyleValue::String("#E3F2FD".to_string());
    let av = style_value_to_attribute(&original);
    let sv = attribute_to_style_value(&av).unwrap();
    assert_eq!(sv, original);
}

#[test]
fn merge_style_block_or_insert_semantics() {
    use crate::ast::StyleMap;
    use crate::theme::StyleBlock;

    let mut target = StyleMap::default();
    target.insert("fill".to_string(), AttributeValue::String(TextValue::quoted("#FF0000")));

    let mut source = StyleBlock::new();
    source.insert("fill".to_string(), StyleValue::String("#E3F2FD".to_string()));
    source.insert("stroke".to_string(), StyleValue::String("#1976D2".to_string()));

    merge_style_block_into_attrs(
        &mut target,
        &source,
        StyleSource::Token {
            key: "test".to_string(),
        },
    );

    // fill 不被覆盖（or_insert 语义）
    assert_eq!(
        target.get("fill"),
        Some(&AttributeValue::String(TextValue::quoted("#FF0000")))
    );
    // stroke 被填入
    assert_eq!(
        target.get("stroke"),
        Some(&AttributeValue::String(TextValue::quoted("#1976D2")))
    );
}

// ── expand_structure 测试 ──────────────────────────────────────────

#[test]
fn expand_structure_derives_branch_slot_for_mindmap() {
    use crate::ast::Relation;

    let diagram = Diagram {
        diagram_type: DiagramType::Mindmap,
        attributes: vec![],
        entities: vec![
            make_entity("root", "中心", {
                let mut a = AttributeMap::default();
                a.standard
                    .insert("type".to_string(), AttributeValue::String(TextValue::unquoted("root")));
                a
            }),
            make_entity("a", "分支A", {
                let mut a = AttributeMap::default();
                a.standard
                    .insert("type".to_string(), AttributeValue::String(TextValue::unquoted("main")));
                a
            }),
        ],
        relations: vec![Relation {
            from: Identifier::new_unchecked("root"),
            to: Identifier::new_unchecked("a"),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span: Span::dummy(),
        }],
        groups: vec![],
        style_decls: vec![],
        source_info: SourceInfo { file: None, line_count: 5 },
        ..Default::default()
    };

    let mut diagram = diagram;
    expand_structure(&mut diagram);

    // root 不写 branch_slot，仅 tree_depth=0
    assert_eq!(
        diagram.entities[0].attributes.standard.get("branch_slot"),
        None
    );
    assert_eq!(
        diagram.entities[0].attributes.standard.get("tree_depth"),
        Some(&AttributeValue::Number(0.0))
    );
    // a 是 root 第一个子节点 → branch_slot 0, tree_depth 1
    assert_eq!(
        diagram.entities[1].attributes.standard.get("branch_slot"),
        Some(&AttributeValue::Number(0.0))
    );
    assert_eq!(
        diagram.entities[1].attributes.standard.get("tree_depth"),
        Some(&AttributeValue::Number(1.0))
    );
}
