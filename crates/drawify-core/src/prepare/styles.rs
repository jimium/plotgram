//! Style materialization: write theme cascade into `attributes.style`.

use std::collections::HashSet;

use crate::ast::{AttributeValue, Diagram, StyleDeclKind, StyleSource};
use crate::error::DiagnosticError;
use crate::profile::profile_for;
use crate::types::style_attrs::{allowed_keys_for_decl, collect_style_map_errors};

/// prepare 前校验 `style_decls` 与 `edge_style` 引用。
///
/// 返回 `Ok(warnings)` 或 `Err(errors)`（阻断 prepare）。
pub fn validate_style_decls(diagram: &Diagram) -> std::result::Result<Vec<DiagnosticError>, Vec<DiagnosticError>> {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let profile = profile_for(&diagram.diagram_type);

    let mut seen_node_targets = HashSet::new();
    let mut seen_edge_targets = HashSet::new();
    let mut edge_decl_names = HashSet::new();

    for decl in &diagram.style_decls {
        let allowed = allowed_keys_for_decl(decl.kind.clone());
        let context = match &decl.kind {
            StyleDeclKind::Node => format!("node_style {}", decl.target),
            StyleDeclKind::Edge => format!("edge_style {}", decl.target),
        };
        collect_style_map_errors(&decl.style, allowed, &context, decl.span, &mut errors);

        match &decl.kind {
            StyleDeclKind::Node => {
                if !seen_node_targets.insert(decl.target.clone()) {
                    errors.push(DiagnosticError::duplicate_style_decl(
                        decl.span,
                        "node_style",
                        &decl.target,
                        0, // first line not tracked here; use 0 as placeholder
                    ));
                }
                if !profile.entity_types.is_empty()
                    && !profile.supports_entity_type(&decl.target)
                {
                    warnings.push(DiagnosticError::unknown_style_selector(
                        decl.span,
                        &decl.target,
                    ));
                }
            }
            StyleDeclKind::Edge => {
                if !seen_edge_targets.insert(decl.target.clone()) {
                    errors.push(DiagnosticError::duplicate_style_decl(
                        decl.span,
                        "edge_style",
                        &decl.target,
                        0,
                    ));
                }
                edge_decl_names.insert(decl.target.clone());
            }
        }
    }

    for relation in &diagram.relations {
        let Some(AttributeValue::String(name)) = relation.attributes.standard.get("line_style") else {
            continue;
        };
        if !edge_decl_names.contains(name.as_str()) {
            warnings.push(DiagnosticError::unresolved_edge_style(
                relation.span,
                &format!("{} -> {}", relation.from, relation.to),
                name,
            ));
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(warnings)
}

/// 应用 DSL 声明式样式（第 4 层）。
///
/// `node_style service { fill: "#xxx" }` 会覆盖 entity_type == "service" 的所有 entity 的 fill，
/// 但不覆盖内联 `style.fill`（手动跳过内联键后用 insert_with_source 覆盖）。
///
/// 实现策略：在 `materialize_diagram_styles` 中，theme cascade 先用 `or_insert` 写入（保护内联），
/// 然后 DSL 声明用 `insert_with_source` 覆盖 palette 写入的值，但手动跳过内联 key。
/// 通过在 materialize_diagram_styles 入口处记录内联 key 集合来实现。
pub fn apply_style_decls(diagram: &mut Diagram, inline_node_keys: &std::collections::HashSet<(String, String)>, inline_edge_keys: &std::collections::HashSet<(usize, String)>) {
    for decl in &diagram.style_decls {
        match decl.kind {
            StyleDeclKind::Node => {
                for (_entity_idx, entity) in diagram.entities.iter_mut().enumerate() {
                    let raw_type = entity
                        .attributes
                        .standard
                        .get("type")
                        .and_then(|v| match v {
                            AttributeValue::String(s) => Some(s.as_str()),
                            _ => None,
                        });
                    let entity_type = raw_type;
                    if entity_type == Some(decl.target.as_str()) {
                        for (key, value) in &decl.style {
                            // 跳过内联 key（第 5 层优先级最高）
                            if inline_node_keys.contains(&(entity.id.as_str().to_string(), key.clone())) {
                                continue;
                            }
                            entity.attributes.style.insert_with_source(
                                key.clone(),
                                value.clone(),
                                StyleSource::Expanded {
                                    decl_target: decl.target.clone(),
                                },
                            );
                        }
                    }
                }
            }
            StyleDeclKind::Edge => {
                for (rel_idx, relation) in diagram.relations.iter_mut().enumerate() {
                    let edge_kind = relation
                        .attributes
                        .standard
                        .get("line_style")
                        .and_then(|v| match v {
                            AttributeValue::String(s) => Some(s.as_str()),
                            _ => None,
                        });
                    if edge_kind == Some(decl.target.as_str()) {
                        for (key, value) in &decl.style {
                            // 跳过内联 key
                            if inline_edge_keys.contains(&(rel_idx, key.clone())) {
                                continue;
                            }
                            relation.attributes.style.insert_with_source(
                                key.clone(),
                                value.clone(),
                                StyleSource::Expanded {
                                    decl_target: decl.target.clone(),
                                },
                            );
                        }
                    }
                }
            }
        }
    }
}
