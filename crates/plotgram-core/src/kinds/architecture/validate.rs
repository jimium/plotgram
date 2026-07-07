//! Architecture 专属验证规则。

use crate::ast::Diagram;
use crate::error::ValidationResult;
use crate::validation::common::validate_self_loop;

pub fn validate(diagram: &Diagram, result: &mut ValidationResult) {
    validate_self_loop(diagram, &[], "架构图中通常无意义", result);
}

#[cfg(test)]
mod tests {
    use super::validate;
    use crate::ast::{
        AttributeMap, AttributeValue, Diagram, Entity, Identifier, Position, Relation, SourceInfo,
        Span, TextValue,
    };
    use crate::error::ValidationResult;
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

    fn make_diagram(entities: Vec<Entity>, relations: Vec<Relation>) -> Diagram {
        Diagram {
            diagram_type: DiagramType::Architecture,
            attributes: vec![],
            entities,
            relations,
            groups: vec![],
            style_decls: vec![],
            source_info: SourceInfo {
                file: None,
                line_count: 1,
            },
            ..Default::default()
        }
    }

    #[test]
    fn accepts_canonical_entity_types() {
        for entity_type in [
            "frontend",
            "backend",
            "service",
            "database",
            "gateway",
            "cache",
            "queue",
            "storage",
            "external",
        ] {
            let mut result = ValidationResult::new();
            validate(
                &make_diagram(vec![make_entity("node", entity_type)], vec![]),
                &mut result,
            );
            assert!(
                result.errors.is_empty(),
                "type '{entity_type}' should be accepted"
            );
        }
    }

    #[test]
    fn rejects_unknown_entity_type() {
        let diagram = make_diagram(vec![make_entity("node", "server")], vec![]);
        let result = crate::validation::validate(&crate::ast::PreparedDiagram::new(diagram));
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn rejects_flowchart_type_alias() {
        let diagram = make_diagram(vec![make_entity("node", "person")], vec![]);
        let result = crate::validation::validate(&crate::ast::PreparedDiagram::new(diagram));
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn self_loop_emits_error_for_non_exempt_type() {
        let relation = Relation {
            from: Identifier::new("api").unwrap(),
            to: Identifier::new("api").unwrap(),
            arrow: crate::ast::ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span: span(),
        };
        let mut result = ValidationResult::new();
        validate(
            &make_diagram(vec![make_entity("api", "service")], vec![relation]),
            &mut result,
        );
        // spec: 非豁免类型（非 decision）的自环为 E013 error
        assert!(!result.errors.is_empty());
    }
}
