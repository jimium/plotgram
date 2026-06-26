//! semantic / icon 属性校验（warning 级别）。

use crate::ast::{AttributeValue, Diagram};
use crate::error::{DiagnosticError, ValidationResult};
use crate::types::standard_attr_keys::entity;

use super::catalog;
use super::registry;

/// 校验实体 `semantic` / `icon` 属性值是否在封闭词表内。
pub fn validate_entity_semantic_icon(diagram: &Diagram, result: &mut ValidationResult) {
    for entity in &diagram.entities {
        validate_semantic(entity.span, entity.id.as_str(), &entity.attributes.standard, result);
        validate_icon(entity.span, entity.id.as_str(), &entity.attributes.standard, result);
    }
}

fn validate_semantic(
    span: crate::ast::Span,
    entity_id: &str,
    standard: &std::collections::HashMap<String, AttributeValue>,
    result: &mut ValidationResult,
) {
    let Some(value) = standard.get(entity::SEMANTIC) else {
        return;
    };
    let Some(atom) = atom_value(value) else {
        return;
    };
    if registry::is_known_semantic(atom) {
        return;
    }
    result.add_warning(DiagnosticError::unknown_semantic(span, entity_id, atom));
}

fn validate_icon(
    span: crate::ast::Span,
    entity_id: &str,
    standard: &std::collections::HashMap<String, AttributeValue>,
    result: &mut ValidationResult,
) {
    let Some(value) = standard.get(entity::ICON) else {
        return;
    };
    let Some(atom) = atom_value(value) else {
        return;
    };
    if catalog::normalize_key(atom) == "none" {
        return;
    }
    if catalog::icon_by_key(atom).is_some() {
        return;
    }
    result.add_warning(DiagnosticError::unknown_icon(span, entity_id, atom));
}

fn atom_value(value: &AttributeValue) -> Option<&str> {
    match value {
        AttributeValue::String(s) => Some(s.as_str()),
        _ => None,
    }
}
