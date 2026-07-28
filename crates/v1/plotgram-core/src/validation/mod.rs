//! Plotgram 语义验证模型。
//!
//! 验证拆成三层：
//! - `common`: 与图表类型无关的公共结构规则
//! - `attrs`: 基于 profile 的属性和值域校验（属性键见 [`crate::profile::STANDARD_ENTITY_ATTRS`]）
//! - `kinds`: 图表类型专属规则（经 [`crate::kinds::validate_diagram_type`] 分派）

pub mod attrs;
pub mod common;

use crate::ast::{Diagram, PreparedDiagram};
use crate::error::ValidationResult;

pub fn validate(diagram: &PreparedDiagram) -> ValidationResult {
    let mut result = ValidationResult::new();

    common::validate_diagram_attributes(diagram, &mut result);
    attrs::validate_entity_attributes(diagram, &mut result);
    crate::icons::validate_entity_semantic_icon(diagram.inner(), &mut result);
    attrs::validate_relation_attributes(diagram, &mut result);
    common::validate_relations(diagram, &mut result);
    common::validate_constraints(diagram, &mut result);
    common::validate_groups(diagram, &mut result);
    common::check_orphan_entities(diagram, &mut result);
    validate_diagram_specific(diagram, &mut result);
    crate::layout::algorithm_config::validate_algorithm_config_warnings(diagram.inner(), &mut result);
    crate::layout::validate_layout_plan_warnings(
        diagram.inner(),
        diagram.layout_plan(),
        &mut result,
    );

    result
}

fn validate_diagram_specific(diagram: &Diagram, result: &mut ValidationResult) {
    crate::kinds::validate_diagram_type(&diagram.diagram_type, diagram, result);
}

#[cfg(test)]
mod tests {
    use super::validate;
    use crate::ast::{
        AttributeMap, AttributeValue, Diagram, Entity, Identifier, Position, PreparedDiagram,
        Relation, SourceInfo, Span, TextValue,
    };
    use crate::error::ErrorCode;
    use crate::types::DiagramType;

    fn span() -> Span {
        Span::new(Position::new(1, 1), Position::new(1, 1))
    }

    fn make_entity(id: &str, entity_type: &str) -> Entity {
        let mut attributes = AttributeMap::default();
        attributes.standard.insert(
            "type".to_string(),
            AttributeValue::String(TextValue::unquoted(entity_type.to_string())),
        );

        Entity {
            id: Identifier::new(id).unwrap(),
            label: id.to_string(),
            attributes,
            group_id: None,
            span: span(),
        }
    }

    fn make_diagram(diagram_type: DiagramType, entities: Vec<Entity>, relations: Vec<Relation>) -> Diagram {
        Diagram {
            diagram_type,
            attributes: vec![],
            entities,
            relations,
            groups: vec![],
            constraints: vec![],
            style_decls: vec![],
            source_info: SourceInfo {
                file: None,
                line_count: 1,
            },
            ..Default::default()
        }
    }

    fn make_prepared(diagram: Diagram) -> PreparedDiagram {
        PreparedDiagram::new(diagram)
        }

    #[test]
    fn legacy_validator_path_still_works() {
        let diagram = make_diagram(DiagramType::Sequence, vec![make_entity("node", "actor")], vec![]);
        let result = validate(&make_prepared(diagram));

        assert_eq!(result.errors.len(), 0);
    }

    #[test]
    fn profile_based_entity_type_validation_is_preserved() {
        let diagram = make_diagram(DiagramType::State, vec![make_entity("node", "actor")], vec![]);
        let result = validate(&make_prepared(diagram));

        assert!(!result.errors.is_empty());
    }

    #[test]
    fn sequence_rejects_flowchart_type_alias() {
        let diagram = make_diagram(DiagramType::Sequence, vec![make_entity("user", "person")], vec![]);
        let result = validate(&make_prepared(diagram));
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn state_rejects_flowchart_type_alias() {
        let diagram = make_diagram(
            DiagramType::State,
            vec![make_entity("start", "start"), make_entity("end", "end")],
            vec![],
        );
        let result = validate(&make_prepared(diagram));
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn mindmap_rejects_flowchart_type_alias() {
        let diagram = make_diagram(
            DiagramType::Mindmap,
            vec![make_entity("root", "start"), make_entity("branch", "process")],
            vec![],
        );
        let result = validate(&make_prepared(diagram));
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn architecture_rejects_flowchart_type_alias() {
        let diagram = make_diagram(
            DiagramType::Architecture,
            vec![make_entity("client", "client"), make_entity("user", "person")],
            vec![],
        );
        let result = validate(&make_prepared(diagram));
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn canonical_sequence_types_are_accepted() {
        let diagram = make_diagram(
            DiagramType::Sequence,
            vec![make_entity("user", "actor"), make_entity("svc", "control")],
            vec![],
        );
        let result = validate(&make_prepared(diagram));
        assert!(result.errors.is_empty());
    }

    #[test]
    fn algorithm_config_block_is_accepted() {
        use crate::ast::{DiagramAttribute, AttributeValue};
        use std::collections::HashMap;

        let mut options = HashMap::new();
        options.insert("tension".to_string(), AttributeValue::Number(0.6));
        let mut diagram = make_diagram(DiagramType::Flowchart, vec![make_entity("a", "process")], vec![]);
        diagram.attributes.push(DiagramAttribute {
            key: "edge_routing".to_string(),
            value: AttributeValue::Config {
                algo: "bezier".to_string(),
                options,
            },
            span: span(),
        });

        let result = validate(&make_prepared(diagram));
        assert!(result.errors.is_empty(), "{:?}", result.errors);
    }

    #[test]
    fn atom_layout_algo_is_accepted() {
        use crate::ast::{DiagramAttribute, AttributeValue};

        let mut diagram = make_diagram(DiagramType::Flowchart, vec![make_entity("a", "process")], vec![]);
        diagram.attributes.push(DiagramAttribute {
            key: "layout".to_string(),
            value: AttributeValue::String(TextValue::unquoted("flowchart".to_string())),
            span: span(),
        });

        let result = validate(&make_prepared(diagram));
        assert!(result.errors.is_empty());
    }

    #[test]
    fn flowchart_specific_rule_is_dispatched() {
        let relation = Relation {
            from: Identifier::new("decision").unwrap(),
            to: Identifier::new("decision").unwrap(),
            arrow: crate::ast::ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span: span(),
        };
        let diagram = make_diagram(
            DiagramType::Flowchart,
            vec![make_entity("decision", "process")],
            vec![relation],
        );

        let result = validate(&make_prepared(diagram));
        // 非豁免类型（process）的自环为 E013 错误
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn state_specific_rule_is_dispatched() {
        let diagram = make_diagram(
            DiagramType::State,
            vec![make_entity("a", "initial"), make_entity("b", "initial")],
            vec![],
        );

        let result = validate(&make_prepared(diagram));
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn er_entity_type_database_is_accepted() {
        let diagram = make_diagram(DiagramType::Er, vec![make_entity("users", "database")], vec![]);
        let result = validate(&make_prepared(diagram));

        assert!(result.errors.is_empty(), "{:?}", result.errors);
    }

    #[test]
    fn expanded_default_entity_type_passes_validation() {
        use crate::prepare::apply_profile_defaults;

        let mut entity = make_entity("step", "process");
        entity.attributes.standard.remove("type");
        let diagram = make_diagram(DiagramType::Flowchart, vec![entity], vec![]);
        let expanded = apply_profile_defaults(diagram).expect("apply profile defaults");
        let result = validate(&make_prepared(expanded));

        assert!(result.errors.is_empty(), "{:?}", result.errors);
    }

    #[test]
    fn mindmap_specific_rule_is_dispatched() {
        let diagram = make_diagram(
            DiagramType::Mindmap,
            vec![make_entity("a", "root"), make_entity("b", "root")],
            vec![],
        );

        let result = validate(&make_prepared(diagram));
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn unknown_semantic_emits_warning() {
        let mut entity = make_entity("svc", "service");
        entity.attributes.standard.insert(
            "semantic".to_string(),
            AttributeValue::String(TextValue::unquoted("not_a_real_semantic".to_string())),
        );
        let diagram = make_diagram(DiagramType::Flowchart, vec![entity], vec![]);
        let result = validate(&make_prepared(diagram));

        assert!(result.errors.is_empty());
        assert!(result.warnings.iter().any(|w| w.code == ErrorCode::W008));
    }

    #[test]
    fn unknown_icon_emits_warning() {
        let mut entity = make_entity("svc", "service");
        entity.attributes.standard.insert(
            "icon".to_string(),
            AttributeValue::String(TextValue::unquoted("not_a_real_icon".to_string())),
        );
        let diagram = make_diagram(DiagramType::Flowchart, vec![entity], vec![]);
        let result = validate(&make_prepared(diagram));

        assert!(result.errors.is_empty());
        assert!(result.warnings.iter().any(|w| w.code == ErrorCode::W009));
    }

    #[test]
    fn icon_none_does_not_warn() {
        let mut entity = make_entity("svc", "service");
        entity.attributes.standard.insert(
            "icon".to_string(),
            AttributeValue::String(TextValue::unquoted("none".to_string())),
        );
        let diagram = make_diagram(DiagramType::Flowchart, vec![entity], vec![]);
        let result = validate(&make_prepared(diagram));

        assert!(!result.warnings.iter().any(|w| w.code == ErrorCode::W009));
    }

    #[test]
    fn direction_invalid_value_is_rejected() {
        use crate::ast::DiagramAttribute;

        let mut diagram = make_diagram(DiagramType::Flowchart, vec![make_entity("a", "process")], vec![]);
        diagram.attributes.push(DiagramAttribute {
            key: "direction".to_string(),
            value: AttributeValue::String(TextValue::unquoted("invalid-direction".to_string())),
            span: span(),
        });

        let result = validate(&make_prepared(diagram));
        assert!(
            result.errors.iter().any(|e| e.code == ErrorCode::E011),
            "expected invalid_enum_value error, got {:?}",
            result.errors
        );
    }

    #[test]
    fn direction_valid_value_is_accepted() {
        use crate::ast::DiagramAttribute;

        let mut diagram = make_diagram(DiagramType::Flowchart, vec![make_entity("a", "process")], vec![]);
        diagram.attributes.push(DiagramAttribute {
            key: "direction".to_string(),
            value: AttributeValue::String(TextValue::unquoted("left-to-right".to_string())),
            span: span(),
        });

        let result = validate(&make_prepared(diagram));
        assert!(result.errors.is_empty(), "{:?}", result.errors);
    }

    fn make_constraint(from: &str, to: &str) -> crate::ast::Constraint {
        crate::ast::Constraint {
            from: Identifier::new(from).unwrap(),
            to: Identifier::new(to).unwrap(),
            span: span(),
        }
    }

    fn make_relation(from: &str, to: &str) -> Relation {
        Relation {
            from: Identifier::new(from).unwrap(),
            to: Identifier::new(to).unwrap(),
            arrow: crate::ast::ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span: span(),
        }
    }

    #[test]
    fn constrain_self_loop_is_error() {
        let mut diagram = make_diagram(
            DiagramType::Flowchart,
            vec![make_entity("a", "process")],
            vec![],
        );
        diagram.constraints.push(make_constraint("a", "a"));
        let result = validate(&make_prepared(diagram));
        assert!(
            result.errors.iter().any(|e| e.message.contains("自环")),
            "{:?}",
            result.errors
        );
    }

    #[test]
    fn constrain_unsatisfiable_with_relation_is_error() {
        // relation a->b + constrain b->a：FAS 保留 a->b，加约束后成环
        let mut diagram = make_diagram(
            DiagramType::Flowchart,
            vec![make_entity("a", "process"), make_entity("b", "process")],
            vec![make_relation("a", "b")],
        );
        diagram.constraints.push(make_constraint("b", "a"));
        let result = validate(&make_prepared(diagram));
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.message.contains("不可满足") || e.message.contains("环")),
            "{:?}",
            result.errors
        );
    }

    #[test]
    fn constrain_satisfiable_with_cyclic_relations() {
        // a->b, b->a 关系环可由 FAS 打破；constrain a->c 可满足
        let mut diagram = make_diagram(
            DiagramType::Flowchart,
            vec![
                make_entity("a", "process"),
                make_entity("b", "process"),
                make_entity("c", "process"),
            ],
            vec![make_relation("a", "b"), make_relation("b", "a")],
        );
        diagram.constraints.push(make_constraint("a", "c"));
        let result = validate(&make_prepared(diagram));
        assert!(
            !result
                .errors
                .iter()
                .any(|e| e.message.contains("不可满足") || e.message.contains("自环")),
            "{:?}",
            result.errors
        );
    }

    #[test]
    fn constrain_with_flowchart_groups_is_allowed() {
        use crate::ast::Group;

        let mut diagram = make_diagram(
            DiagramType::Flowchart,
            vec![
                Entity {
                    id: Identifier::new("a").unwrap(),
                    label: "A".into(),
                    attributes: {
                        let mut m = AttributeMap::default();
                        m.standard.insert(
                            "type".into(),
                            AttributeValue::String(TextValue::unquoted("process")),
                        );
                        m
                    },
                    group_id: Some(Identifier::new("g").unwrap()),
                    span: span(),
                },
                make_entity("b", "process"),
            ],
            vec![],
        );
        diagram.groups.push(Group {
            id: Identifier::new("g").unwrap(),
            label: "G".into(),
            attributes: AttributeMap::default(),
            parent_id: None,
            depth: 0,
            entity_ids: vec![Identifier::new("a").unwrap()],
            child_group_ids: vec![],
            span: span(),
        });
        diagram.constraints.push(make_constraint("a", "b"));
        let result = validate(&make_prepared(diagram));
        assert!(
            !result
                .errors
                .iter()
                .any(|e| e.message.contains("group divide") || e.message.contains("cross-group")),
            "{:?}",
            result.errors
        );
    }
}
