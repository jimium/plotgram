//! Parent-scoped declaration index helpers for soft layout bias.
//!
//! Priority for soft ordering: edges/constrain (hard) > semantic >
//! **declaration order (soft)** > id lexicographic (fallback).
//!
//! Scope is parent-scoped local declaration order (siblings under the same
//! parent), not the global entities array alone.

use crate::ast::Diagram;
use std::cmp::Ordering;
use std::collections::HashMap;

/// Build map: entity_id → index among siblings under the same parent group
/// (or among top-level ungrouped entities using `diagram.entities` order
/// filtered by `group_id.is_none()`).
///
/// For entities inside a group: use that group's `entity_ids` order.
/// If `entity_ids` is empty, fall back to `diagram.entities` filtered by
/// `group_id` (declaration order).
pub fn entity_sibling_decl_index(diagram: &Diagram) -> HashMap<String, usize> {
    let mut map = HashMap::new();

    for group in &diagram.groups {
        if !group.entity_ids.is_empty() {
            for (idx, eid) in group.entity_ids.iter().enumerate() {
                map.insert(eid.as_str().to_string(), idx);
            }
        } else {
            let mut idx = 0usize;
            for entity in &diagram.entities {
                if entity
                    .group_id
                    .as_ref()
                    .map(|g| g.as_str() == group.id.as_str())
                    .unwrap_or(false)
                {
                    map.insert(entity.id.as_str().to_string(), idx);
                    idx += 1;
                }
            }
        }
    }

    // Top-level ungrouped entities: order among siblings with no group.
    let mut ungrouped_idx = 0usize;
    for entity in &diagram.entities {
        if entity.group_id.is_none() {
            map.entry(entity.id.as_str().to_string())
                .or_insert_with(|| {
                    let i = ungrouped_idx;
                    ungrouped_idx += 1;
                    i
                });
        }
    }

    map
}

/// Build map: group_id → index among siblings under the same parent
/// (top-level: order in `diagram.groups` where `parent_id.is_none()`;
/// nested: parent's `child_group_ids`).
pub fn group_sibling_decl_index(diagram: &Diagram) -> HashMap<String, usize> {
    let mut map = HashMap::new();

    // Nested: parent's child_group_ids order.
    for group in &diagram.groups {
        for (idx, cid) in group.child_group_ids.iter().enumerate() {
            map.insert(cid.as_str().to_string(), idx);
        }
    }

    // Top-level: declaration order among groups with no parent.
    let mut top_idx = 0usize;
    for group in &diagram.groups {
        if group.parent_id.is_none() {
            map.entry(group.id.as_str().to_string())
                .or_insert_with(|| {
                    let i = top_idx;
                    top_idx += 1;
                    i
                });
        }
    }

    map
}

/// Global entity declaration index (`diagram.entities` order) — for cases
/// that need a global fallback.
pub fn entity_global_decl_index(diagram: &Diagram) -> HashMap<String, usize> {
    diagram
        .entities
        .iter()
        .enumerate()
        .map(|(i, e)| (e.id.as_str().to_string(), i))
        .collect()
}

/// Compare by declaration index, then by id lexicographic as fallback.
pub fn cmp_by_decl_then_id(decl: &HashMap<String, usize>, a: &str, b: &str) -> Ordering {
    decl.get(a).cmp(&decl.get(b)).then_with(|| a.cmp(b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Entity, Group, Identifier, Span};

    fn entity(id: &str, group: Option<&str>) -> Entity {
        Entity {
            id: Identifier::new_unchecked(id),
            label: id.to_string(),
            attributes: crate::ast::AttributeMap::default(),
            group_id: group.map(Identifier::new_unchecked),
            span: Span::dummy(),
        }
    }

    fn group(id: &str, parent: Option<&str>, entities: &[&str], children: &[&str]) -> Group {
        Group {
            id: Identifier::new_unchecked(id),
            label: id.to_string(),
            attributes: crate::ast::AttributeMap::default(),
            parent_id: parent.map(Identifier::new_unchecked),
            depth: 0,
            entity_ids: entities
                .iter()
                .map(|e| Identifier::new_unchecked(e))
                .collect(),
            child_group_ids: children
                .iter()
                .map(|c| Identifier::new_unchecked(c))
                .collect(),
            span: Span::dummy(),
        }
    }

    #[test]
    fn sibling_decl_index_prefers_ast_order_over_id_lex() {
        // Declared as group a then group z, but "z" < "a" lexicographically.
        let diagram = Diagram {
            entities: vec![
                entity("n1", Some("a")),
                entity("n2", Some("z")),
            ],
            groups: vec![
                group("a", None, &["n1"], &[]),
                group("z", None, &["n2"], &[]),
            ],
            ..Default::default()
        };

        let group_decl = group_sibling_decl_index(&diagram);
        assert_eq!(group_decl.get("a"), Some(&0));
        assert_eq!(group_decl.get("z"), Some(&1));
        assert_eq!(
            cmp_by_decl_then_id(&group_decl, "a", "z"),
            Ordering::Less,
            "declaration order a before z, not id lex"
        );

        let entity_decl = entity_sibling_decl_index(&diagram);
        assert_eq!(entity_decl.get("n1"), Some(&0));
        assert_eq!(entity_decl.get("n2"), Some(&0));
    }

    #[test]
    fn nested_group_sibling_uses_parent_child_group_ids() {
        let diagram = Diagram {
            entities: vec![],
            groups: vec![
                group("parent", None, &[], &["z", "a"]),
                group("z", Some("parent"), &[], &[]),
                group("a", Some("parent"), &[], &[]),
            ],
            ..Default::default()
        };

        let group_decl = group_sibling_decl_index(&diagram);
        assert_eq!(group_decl.get("z"), Some(&0));
        assert_eq!(group_decl.get("a"), Some(&1));
        assert_eq!(cmp_by_decl_then_id(&group_decl, "z", "a"), Ordering::Less);
    }
}
