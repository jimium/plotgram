use std::collections::HashSet;

use crate::types::standard_attr_keys::{diagram, entity, group};
use crate::types::attr_constants;
use crate::types::attr_schema;
use crate::ast::{is_valid_atom, AttributeValue, Diagram};
use crate::error::{DiagnosticError, ValidationResult};
use crate::layout::node::architecture_v2::{
    is_valid_group_layout_atom, is_valid_group_sizing_atom, VALID_GROUP_LAYOUTS,
    VALID_GROUP_SIZING,
};

use crate::types::style_attrs::{is_atom_like, is_boolean_like, is_number_like, is_string_like};

fn is_valid_option_value(value: &AttributeValue) -> bool {
    is_string_like(value)
        || is_number_like(value)
        || is_boolean_like(value)
        || is_atom_like(value)
}

pub fn validate_diagram_attributes(diagram: &Diagram, result: &mut ValidationResult) {
    let mut seen_keys = HashSet::new();

    for attr in &diagram.attributes {
        if !seen_keys.insert(&attr.key) {
            result.add_error(DiagnosticError::structure_violation(
                attr.span,
                format!("diagram 属性 '{}' 重复声明", attr.key),
            ));
            continue;
        }

        match attr.key.as_str() {
            diagram::DIRECTION
            | diagram::LAYOUT
            | diagram::EDGE_ROUTING
            | diagram::GROUP_FRAME
            | diagram::THEME
            | diagram::RENDER_STYLE
            | diagram::GROUP_SIZING
            | diagram::GROUP_ALIGN
            | diagram::GROUP_ARRANGEMENT => match &attr.value {
                AttributeValue::String(_) => {
                    if attr.key == diagram::GROUP_SIZING {
                        if let Some(v) = attr.value.as_str() {
                            if !is_valid_group_sizing_atom(v) {
                                result.add_error(DiagnosticError::invalid_enum_value(
                                    attr.span,
                                    diagram::GROUP_SIZING,
                                    v,
                                    VALID_GROUP_SIZING,
                                ));
                            }
                        }
                    } else if !is_atom_like(&attr.value) {
                        result.add_error(DiagnosticError::structure_violation(
                            attr.span,
                            format!("属性 '{}' 的值必须是 atom", attr.key),
                        ));
                    }
                    // 枚举值闭集校验（direction 等）：从 schema 查询 enum_values。
                    // group_sizing 已由上方 is_valid_group_sizing_atom 校验，跳过避免重复报错。
                    if attr.key != diagram::GROUP_SIZING {
                        if let Some(v) = attr.value.as_str() {
                            if let Some(valid_values) =
                                attr_schema::enum_values_for_key(&attr.key)
                            {
                                if !valid_values.contains(&v) {
                                    result.add_error(DiagnosticError::invalid_enum_value(
                                        attr.span,
                                        &attr.key,
                                        v,
                                        valid_values,
                                    ));
                                }
                            }
                        }
                    }
                }
                AttributeValue::Config { algo, options } => {
                    if !is_valid_atom(algo) {
                        result.add_error(DiagnosticError::structure_violation(
                            attr.span,
                            format!("算法名 '{algo}' 不是合法的 atom"),
                        ));
                    }
                    for (opt_key, opt_value) in options {
                        if !is_valid_option_value(opt_value) {
                            result.add_error(DiagnosticError::structure_violation(
                                attr.span,
                                format!(
                                    "属性 '{}' 的选项 '{opt_key}' 必须是 string、number、boolean 或 atom",
                                    attr.key
                                ),
                            ));
                        }
                    }
                }
                _ => {
                    result.add_error(DiagnosticError::structure_violation(
                        attr.span,
                        format!("属性 '{}' 的值必须是 atom 或算法配置块", attr.key),
                    ));
                }
            },
            diagram::SNAP => {
                if !matches!(attr.value, AttributeValue::Boolean(_)) {
                    result.add_error(DiagnosticError::structure_violation(
                        attr.span,
                        format!("属性 '{}' 的值必须是 boolean（true 或 false）", diagram::SNAP),
                    ));
                }
            }
            diagram::GROUP_GAP => {
                if !matches!(attr.value, AttributeValue::Number(_)) {
                    result.add_error(DiagnosticError::structure_violation(
                        attr.span,
                        format!("属性 '{}' 的值必须是数字", diagram::GROUP_GAP),
                    ));
                }
            }
            diagram::TITLE => {
                if !is_string_like(&attr.value) {
                    result.add_error(DiagnosticError::structure_violation(
                        attr.span,
                        format!("属性 '{}' 的值必须是 string", diagram::TITLE),
                    ));
                }
            }
            _ => {
                result.add_error(DiagnosticError::structure_violation(
                    attr.span,
                    format!("未知的 diagram 属性 '{}'", attr.key),
                ));
            }
        }
    }
}

pub fn validate_relations(diagram: &Diagram, result: &mut ValidationResult) {
    let entity_ids: HashSet<&str> = diagram.entities.iter().map(|e| e.id.as_str()).collect();
    let group_ids: HashSet<&str> = diagram.groups.iter().map(|g| g.id.as_str()).collect();
    let all_ids: HashSet<&str> = entity_ids.union(&group_ids).copied().collect();
    let available: Vec<String> = diagram
        .entities
        .iter()
        .map(|e| e.id.as_str().to_string())
        .collect();

    for relation in &diagram.relations {
        let from_str = relation.from.as_str();
        let to_str = relation.to.as_str();

        if !all_ids.contains(from_str) {
            result.add_error(DiagnosticError::undefined_reference(
                relation.span,
                from_str,
                &available,
            ));
        } else if group_ids.contains(from_str) {
            result.add_error(DiagnosticError::group_relation(relation.span, from_str));
        }

        if !all_ids.contains(to_str) {
            result.add_error(DiagnosticError::undefined_reference(
                relation.span,
                to_str,
                &available,
            ));
        } else if group_ids.contains(to_str) {
            result.add_error(DiagnosticError::group_relation(relation.span, to_str));
        }
    }
}

pub fn validate_groups(diagram: &Diagram, result: &mut ValidationResult) {
    for group in &diagram.groups {
        for (key, value) in &group.attributes.standard {
            match key.as_str() {
                group::BORDER_STYLE => {
                    if let Some(v) = value.as_str() {
                        if !attr_constants::group_border_style::ALL.contains(&v) {
                            result.add_error(DiagnosticError::invalid_enum_value(
                                group.span,
                                group::BORDER_STYLE,
                                v,
                                attr_constants::group_border_style::ALL,
                            ));
                        }
                    } else if !is_atom_like(value) {
                        result.add_error(DiagnosticError::structure_violation(
                            group.span,
                            format!("group '{}' 的 style 属性必须是 atom", group.id),
                        ));
                    }
                }
                group::COLOR => {
                    if !matches!(value, AttributeValue::String(_)) {
                        result.add_error(DiagnosticError::structure_violation(
                            group.span,
                            format!("group '{}' 的 color 属性必须是字符串", group.id),
                        ));
                    }
                }
                group::LAYOUT => {
                    if let Some(v) = value.as_str() {
                        if !is_valid_group_layout_atom(v) {
                            result.add_error(DiagnosticError::invalid_enum_value(
                                group.span,
                                group::LAYOUT,
                                v,
                                VALID_GROUP_LAYOUTS,
                            ));
                        }
                    } else if !is_atom_like(value) {
                        result.add_error(DiagnosticError::structure_violation(
                            group.span,
                            format!("group '{}' 的 layout 属性必须是 atom", group.id),
                        ));
                    }
                }
                _ => {
                    result.add_error(DiagnosticError::structure_violation(
                        group.span,
                        format!("group '{}' 的未知属性 '{}'", group.id, key),
                    ));
                }
            }
        }

        if group.entity_ids.is_empty() && group.child_group_ids.is_empty() {
            result.add_warning(DiagnosticError::unused_group(group.span, group.id.as_str()));
        }
    }
}

pub fn check_orphan_entities(diagram: &Diagram, result: &mut ValidationResult) {
    let connected: HashSet<&str> = diagram
        .relations
        .iter()
        .flat_map(|r| [r.from.as_str(), r.to.as_str()])
        .collect();

    for entity in &diagram.entities {
        if !connected.contains(entity.id.as_str()) {
            result.add_warning(DiagnosticError::orphan_entity(
                entity.span,
                entity.id.as_str(),
                &entity.label,
            ));
        }
    }
}

/// 校验某规范类型在图表中最多出现 `max_count` 次，超出时对第 `max_count+1` 个及之后的实体报错。
///
/// 泛化了 state（initial 最多 1 个）和 mindmap（root 最多 1 个）的唯一性规则。
/// 错误 span 指向违规实体本身，而非 `entities.first()`。
pub fn validate_unique_canonical_type(
    diagram: &Diagram,
    canonical_type: &str,
    max_count: usize,
    error_message: &str,
    result: &mut ValidationResult,
) {
    let mut seen_count = 0;
    for entity in &diagram.entities {
        let is_match = entity
            .attributes
            .standard
            .get(entity::TYPE)
            .and_then(|v| v.as_str())
            .is_some_and(|raw| raw == canonical_type);
        if is_match {
            seen_count += 1;
            if seen_count > max_count {
                result.add_error(DiagnosticError::structure_violation(
                    entity.span,
                    error_message,
                ));
            }
        }
    }
}

/// 校验自环关系。
///
/// - `exempt_types` 中的实体类型允许自环，但发出 W003 警告（如 flowchart 的 decision）。
/// - 不在 `exempt_types` 中的自环为 E013 错误。
pub fn validate_self_loop(
    diagram: &Diagram,
    exempt_types: &[&str],
    _warning_message: &str,
    result: &mut ValidationResult,
) {
    let entity_type_map: std::collections::HashMap<&str, &str> = diagram
        .entities
        .iter()
        .filter_map(|e| {
            e.attributes
                .standard
                .get(entity::TYPE)
                .and_then(|v| v.as_str())
                .map(|t| (e.id.as_str(), t))
        })
        .collect();

    for relation in &diagram.relations {
        if relation.from == relation.to {
            let entity_type = entity_type_map.get(relation.from.as_str()).copied();
            if exempt_types.contains(&entity_type.unwrap_or("")) {
                // 豁免类型（如 decision）：允许自环，但发出 W003 警告
                result.add_warning(DiagnosticError::self_loop_warning(
                    relation.span,
                    relation.from.as_str(),
                ));
            } else {
                // 非豁免类型：E013 错误
                result.add_error(DiagnosticError::self_loop_error(
                    relation.span,
                    relation.from.as_str(),
                ));
            }
        }
    }
}
