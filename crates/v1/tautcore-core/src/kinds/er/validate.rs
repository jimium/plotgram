//! ER 专属验证规则。

use crate::ast::{AttributeValue, Diagram};
use crate::error::{DiagnosticError, ValidationResult};
use super::semantics::{is_valid_cardinality, relation_cardinality};

pub fn validate(diagram: &Diagram, result: &mut ValidationResult) {
    validate_entity_columns(diagram, result);
    validate_relation_cardinality(diagram, result);
}

fn validate_entity_columns(diagram: &Diagram, result: &mut ValidationResult) {
    for entity in &diagram.entities {
        if let Some(AttributeValue::String(s)) = entity.attributes.standard.get("columns") {
            for col in s.split(',') {
                if col.trim().is_empty() {
                    result.add_warning(DiagnosticError::orphan_entity(
                        entity.span,
                        entity.id.as_str(),
                        "ER 列定义不能为空字符串",
                    ));
                }
            }
        }
    }
}

fn validate_relation_cardinality(diagram: &Diagram, result: &mut ValidationResult) {
    for relation in &diagram.relations {
        if let Some(AttributeValue::String(cardinality)) =
            relation.attributes.standard.get("cardinality")
        {
            if !is_valid_cardinality(cardinality) {
                result.add_warning(DiagnosticError::structure_violation(
                    relation.span,
                    format!(
                        "ER 图 relation.cardinality 建议使用 '1:N'、'N:M' 等格式，当前为 '{cardinality}'"
                    ),
                ));
            }
        } else if relation_cardinality(relation).is_none() {
            if let Some(label) = &relation.label {
                if label.trim().is_empty() {
                    result.add_warning(DiagnosticError::structure_violation(
                        relation.span,
                        "ER 图建议在 relation 上声明 cardinality 属性或含 '1:N' 前缀的 label",
                    ));
                }
            } else {
                result.add_warning(DiagnosticError::structure_violation(
                    relation.span,
                    "ER 图建议为 relation 标注 cardinality（如 cardinality: \"1:N\"）",
                ));
            }
        }
    }
}
