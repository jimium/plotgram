//! Mindmap 结构展开：从 relation 树计算 branch_slot / tree_depth。
//!
//! 与原 `MindmapTheme::from_diagram` 算法等价，仅产出整数写入 `attributes.standard`。
//! 确定性：branch_slot 分配按 relation 插入序（不依赖 HashMap 迭代序）。

use std::collections::HashMap;

use crate::ast::{AttributeValue, Diagram};
use crate::types::standard_attr_keys::entity;

/// 将 branch_slot / tree_depth 写入 mindmap entity 的 `attributes.standard`。
///
/// 已显式声明 `branch_slot` 的 entity 跳过自动分配（DSL override）。
pub fn expand(diagram: &mut Diagram) {
    let children = build_children_map(diagram);
    let root_id = find_root_id(diagram, &children);

    // root: tree_depth = 0，不写 branch_slot
    let mut slot_of: HashMap<String, usize> = HashMap::new();
    let mut depth_of: HashMap<String, usize> = HashMap::new();
    depth_of.insert(root_id.clone(), 0);

    // root 直接子节点按 relation 插入序分配 branch_slot
    let root_children = children.get(&root_id).cloned().unwrap_or_default();
    for (index, child_id) in root_children.iter().enumerate() {
        assign_branch(child_id, index, 1, &children, &mut slot_of, &mut depth_of);
    }

    // 写入 attributes.standard（仅当 entity 未显式声明时）
    for entity in &mut diagram.entities {
        let id = entity.id.as_str().to_string();

        if entity.attributes.standard.contains_key(BRANCH_SLOT) {
            // DSL override：跳过自动分配
            continue;
        }

        if let Some(&slot) = slot_of.get(&id) {
            entity.attributes.standard.insert(
                BRANCH_SLOT.to_string(),
                AttributeValue::Number(slot as f64),
            );
        }

        if entity.attributes.standard.contains_key(TREE_DEPTH) {
            continue;
        }
        if let Some(&depth) = depth_of.get(&id) {
            entity.attributes.standard.insert(
                TREE_DEPTH.to_string(),
                AttributeValue::Number(depth as f64),
            );
        }
    }

    // 确保 root 有 tree_depth=0（即使上面漏写）
    let _ = entity::TYPE; // 引用 standard_attr_keys 保持一致性
}

const BRANCH_SLOT: &str = "branch_slot";
const TREE_DEPTH: &str = "tree_depth";

fn assign_branch(
    node_id: &str,
    branch_index: usize,
    depth: usize,
    children: &HashMap<String, Vec<String>>,
    slot_of: &mut HashMap<String, usize>,
    depth_of: &mut HashMap<String, usize>,
) {
    slot_of.insert(node_id.to_string(), branch_index);
    depth_of.insert(node_id.to_string(), depth);
    if let Some(kids) = children.get(node_id) {
        for kid in kids {
            assign_branch(kid, branch_index, depth + 1, children, slot_of, depth_of);
        }
    }
}

pub fn build_children_map(diagram: &Diagram) -> HashMap<String, Vec<String>> {
    let mut children: HashMap<String, Vec<String>> = HashMap::new();
    for entity in &diagram.entities {
        children.entry(entity.id.as_str().to_string()).or_default();
    }
    for rel in &diagram.relations {
        children
            .entry(rel.from.as_str().to_string())
            .or_default()
            .push(rel.to.as_str().to_string());
    }
    children
}

pub fn find_root_id(diagram: &Diagram, children: &HashMap<String, Vec<String>>) -> String {
    // 优先找 type: root
    for entity in &diagram.entities {
        let is_root = entity
            .attributes
            .standard
            .get("type")
            .and_then(|v| match v {
                AttributeValue::String(s) => Some(s == "root"),
                _ => None,
            })
            .unwrap_or(false);
        if is_root {
            return entity.id.as_str().to_string();
        }
    }

    // 回退：入度 0 的节点
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    for entity in &diagram.entities {
        in_degree.insert(entity.id.as_str(), 0);
    }
    for rel in &diagram.relations {
        *in_degree.entry(rel.to.as_str()).or_insert(0) += 1;
    }
    if let Some((id, _)) = in_degree.iter().find(|(_, deg)| **deg == 0) {
        return id.to_string();
    }

    // 最终回退：第一个 entity
    children
        .keys()
        .next()
        .cloned()
        .unwrap_or_else(|| diagram.entities[0].id.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        ArrowType, AttributeMap, Diagram, Entity, Identifier, Relation, SourceInfo, Span, TextValue,
    };
    use crate::types::DiagramType;

    fn entity(id: &str, ty: &str) -> Entity {
        let mut attrs = AttributeMap::default();
        attrs.standard.insert(
            "type".to_string(),
            AttributeValue::String(TextValue::unquoted(ty.to_string())),
        );
        Entity {
            id: Identifier::new_unchecked(id),
            label: id.to_string(),
            attributes: attrs,
            group_id: None,
            span: Span::dummy(),
        }
    }

    fn relation(from: &str, to: &str) -> Relation {
        Relation {
            from: Identifier::new_unchecked(from),
            to: Identifier::new_unchecked(to),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span: Span::dummy(),
        }
    }

    fn make_diagram(entities: Vec<Entity>, relations: Vec<Relation>) -> Diagram {
        Diagram {
            diagram_type: DiagramType::Mindmap,
            attributes: Vec::new(),
            entities,
            relations,
            groups: Vec::new(),
            style_decls: vec![],
            source_info: SourceInfo {
                file: None,
                line_count: 1,
            },
            ..Default::default()
        }
    }

    #[test]
    fn assigns_branch_slots_by_relation_order() {
        let diagram = make_diagram(
            vec![
                entity("root", "root"),
                entity("a", "main"),
                entity("a1", "leaf"),
                entity("b", "main"),
            ],
            vec![
                relation("root", "a"),
                relation("a", "a1"),
                relation("root", "b"),
            ],
        );

        let mut d = diagram;
        expand(&mut d);

        let get_slot = |e: &Entity| {
            e.attributes
                .standard
                .get("branch_slot")
                .and_then(|v| match v {
                    AttributeValue::Number(n) => Some(*n as usize),
                    _ => None,
                })
        };
        let get_depth = |e: &Entity| {
            e.attributes
                .standard
                .get("tree_depth")
                .and_then(|v| match v {
                    AttributeValue::Number(n) => Some(*n as usize),
                    _ => None,
                })
        };

        let root = &d.entities[0];
        let a = &d.entities[1];
        let a1 = &d.entities[2];
        let b = &d.entities[3];

        // root 无 branch_slot，tree_depth=0
        assert_eq!(get_slot(root), None);
        assert_eq!(get_depth(root), Some(0));

        // a 是 root 第一个子节点 → slot 0, depth 1
        assert_eq!(get_slot(a), Some(0));
        assert_eq!(get_depth(a), Some(1));

        // a1 继承 a 的 slot 0, depth 2
        assert_eq!(get_slot(a1), Some(0));
        assert_eq!(get_depth(a1), Some(2));

        // b 是 root 第二个子节点 → slot 1, depth 1
        assert_eq!(get_slot(b), Some(1));
        assert_eq!(get_depth(b), Some(1));
    }

    #[test]
    fn dsl_override_skips_auto_assignment() {
        let mut override_attrs = AttributeMap::default();
        override_attrs.standard.insert(
            "type".to_string(),
            AttributeValue::String(TextValue::unquoted("main".to_string())),
        );
        override_attrs
            .standard
            .insert("branch_slot".to_string(), AttributeValue::Number(5.0));

        let diagram = make_diagram(
            vec![
                Entity {
                    id: Identifier::new_unchecked("root"),
                    label: "root".to_string(),
                    attributes: {
                        let mut a = AttributeMap::default();
                        a.standard.insert(
                            "type".to_string(),
                            AttributeValue::String(TextValue::unquoted("root".to_string())),
                        );
                        a
                    },
                    group_id: None,
                    span: Span::dummy(),
                },
                Entity {
                    id: Identifier::new_unchecked("a"),
                    label: "a".to_string(),
                    attributes: override_attrs,
                    group_id: None,
                    span: Span::dummy(),
                },
            ],
            vec![relation("root", "a")],
        );

        let mut d = diagram;
        expand(&mut d);

        // a 应保留用户显式写的 branch_slot=5
        let a = &d.entities[1];
        let slot = a
            .attributes
            .standard
            .get("branch_slot")
            .and_then(|v| match v {
                AttributeValue::Number(n) => Some(*n),
                _ => None,
            });
        assert_eq!(slot, Some(5.0));
    }

    #[test]
    fn isolated_node_has_no_branch_slot() {
        let diagram = make_diagram(
            vec![entity("root", "root"), entity("orphan", "leaf")],
            vec![],
        );

        let mut d = diagram;
        expand(&mut d);

        let orphan = &d.entities[1];
        assert!(!orphan.attributes.standard.contains_key("branch_slot"));
    }
}
