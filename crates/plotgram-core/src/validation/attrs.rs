use std::collections::HashSet;

use crate::ast::{AttributeValue, Diagram};
use crate::error::{DiagnosticError, ValidationResult};
use crate::profile::profile_for;
use crate::types::attr_constants;
use crate::types::standard_attr_keys::{entity, relation};
use crate::types::style_attrs::{
    is_atom_like, is_text_like, validate_style_property, StylePropError,
    VALID_ENTITY_STYLE_ATTRS, VALID_RELATION_STYLE_ATTRS,
};

pub fn validate_entity_attributes(diagram: &Diagram, result: &mut ValidationResult) {
    let profile = profile_for(&diagram.diagram_type);

    for entity in &diagram.entities {
        let mut seen_keys = HashSet::new();

        // 校验 standard 属性
        for (key, value) in &entity.attributes.standard {
            if !seen_keys.insert(key.as_str()) {
                result.add_error(DiagnosticError::structure_violation(
                    entity.span,
                    format!("实体 '{}' 的属性 '{}' 重复声明", entity.id, key),
                ));
                continue;
            }

            if !profile.standard_entity_attrs().contains(&key.as_str()) {
                result.add_error(DiagnosticError::invalid_attribute(
                    entity.span,
                    key,
                    entity.id.as_str(),
                    profile.standard_entity_attrs(),
                ));
                continue;
            }

            match key.as_str() {
                entity::TYPE => {
                    if let Some(v) = value.as_str() {
                        if profile.restricts_entity_type_values()
                            && !profile.supports_entity_type(v)
                        {
                            result.add_error(DiagnosticError::invalid_enum_value(
                                entity.span,
                                entity::TYPE,
                                v,
                                profile.entity_types,
                            ));
                        }
                    } else if !is_atom_like(value) {
                        result.add_error(DiagnosticError::structure_violation(
                            entity.span,
                            format!("实体 '{}' 的属性 '{}' 必须是 atom", entity.id, entity::TYPE),
                        ));
                    }
                }
                entity::STATUS => {
                    if let Some(v) = value.as_str() {
                        if !attr_constants::status::ALL.contains(&v) {
                            result.add_error(DiagnosticError::invalid_enum_value(
                                entity.span,
                                entity::STATUS,
                                v,
                                attr_constants::status::ALL,
                            ));
                        }
                    } else if !is_atom_like(value) {
                        result.add_error(DiagnosticError::structure_violation(
                            entity.span,
                            format!("实体 '{}' 的属性 '{}' 必须是 atom", entity.id, entity::STATUS),
                        ));
                    }
                }
                entity::SEMANTIC | entity::ICON => {
                    if !is_atom_like(value) {
                        result.add_error(DiagnosticError::structure_violation(
                            entity.span,
                            format!("实体 '{}' 的属性 '{}' 必须是 atom", entity.id, key),
                        ));
                    }
                }
                entity::OWNER | entity::DESCRIPTION => {
                    if !matches!(value, AttributeValue::String(_)) {
                        result.add_error(DiagnosticError::structure_violation(
                            entity.span,
                            format!("实体 '{}' 的属性 '{}' 必须是字符串类型", entity.id, key),
                        ));
                    }
                }
                _ => {}
            }
        }

        // 校验 style 属性
        validate_entity_style_attributes(entity, result);
    }
}

/// 校验 entity 的 `attributes.style` 中的样式属性。
fn validate_entity_style_attributes(entity: &crate::ast::Entity, result: &mut ValidationResult) {
    let context = format!("实体 '{}'", entity.id);
    for (key, value) in entity.attributes.style.iter_values() {
        if let Some(err) = validate_style_property(key, value, VALID_ENTITY_STYLE_ATTRS, &context) {
            push_style_error(err, entity.span, &context, result);
        }
    }
}

/// 校验 relation 的 `attributes.standard` 和 `attributes.style`。
pub fn validate_relation_attributes(diagram: &Diagram, result: &mut ValidationResult) {
    let profile = profile_for(&diagram.diagram_type);

    for relation in &diagram.relations {
        // 校验 standard 属性
        for (key, value) in &relation.attributes.standard {
            if !profile.standard_relation_attrs().contains(&key.as_str()) {
                result.add_error(DiagnosticError::invalid_attribute(
                    relation.span,
                    key,
                    &format!("{} -> {}", relation.from, relation.to),
                    profile.standard_relation_attrs(),
                ));
                continue;
            }

            match key.as_str() {
                relation::STATUS => {
                    if let Some(v) = value.as_str() {
                        if !attr_constants::status::ALL.contains(&v) {
                            result.add_error(DiagnosticError::invalid_enum_value(
                                relation.span,
                                relation::STATUS,
                                v,
                                attr_constants::status::ALL,
                            ));
                        }
                    } else if !is_atom_like(value) {
                        result.add_error(DiagnosticError::structure_violation(
                            relation.span,
                            format!(
                                "关系 '{} -> {}' 的属性 '{}' 必须是 atom",
                                relation.from, relation.to, relation::STATUS
                            ),
                        ));
                    }
                }
                relation::LINE_STYLE => {
                    if !is_atom_like(value) {
                        result.add_error(DiagnosticError::structure_violation(
                            relation.span,
                            format!(
                                "关系 '{} -> {}' 的属性 '{}' 必须是 atom",
                                relation.from, relation.to, relation::LINE_STYLE
                            ),
                        ));
                    }
                }
                relation::CARDINALITY => {
                    if !is_text_like(value) {
                        result.add_error(DiagnosticError::structure_violation(
                            relation.span,
                            format!(
                                "关系 '{} -> {}' 的属性 '{}' 必须是字符串类型",
                                relation.from, relation.to, relation::CARDINALITY
                            ),
                        ));
                    }
                }
                _ => {}
            }
        }

        // 校验 style 属性（统一走 validate_style_property）
        let context = format!("关系 '{} -> {}'", relation.from, relation.to);
        for (key, value) in relation.attributes.style.iter_values() {
            if let Some(err) = validate_style_property(key, value, VALID_RELATION_STYLE_ATTRS, &context) {
                push_style_error(err, relation.span, &context, result);
            }
        }
    }
}

/// 将 StylePropError 转换为 DiagnosticError 并加入 result。
fn push_style_error(err: StylePropError, span: crate::ast::Span, context: &str, result: &mut ValidationResult) {
    match err {
        StylePropError::UnknownKey { key, allowed } => {
            result.add_error(DiagnosticError::invalid_attribute(
                span,
                &key,
                context,
                &allowed.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            ));
        }
        StylePropError::TypeMismatch { key, expected } => {
            result.add_error(DiagnosticError::style_type_mismatch(
                span,
                &key,
                expected,
                "mismatched",
            ));
        }
    }
}

