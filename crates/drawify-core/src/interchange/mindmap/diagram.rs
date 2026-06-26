//! Diagram ↔ MindmapTree 双向转换。

use std::collections::HashMap;
use std::collections::HashSet;

use crate::ast::{
    ArrowType, AttributeMap, AttributeValue, Diagram, DiagramAttribute, Entity, Identifier,
    Relation, SourceInfo, Span, TextValue,
};
use crate::prepare::structure::mindmap::{build_children_map, find_root_id};
use crate::types::DiagramType;

use super::tree::{
    MindmapTree, MindmapTreeNode, RootTitleMode, TreeValidationError, TreeValidationResult,
};

/// 从 PreparedDiagram 构建 MindmapTree 的选项。
#[derive(Debug, Clone)]
pub struct BuildTreeOptions {
    pub root_title_mode: RootTitleMode,
    pub strict_tree: bool,
}

impl Default for BuildTreeOptions {
    fn default() -> Self {
        Self {
            root_title_mode: RootTitleMode::Separate,
            strict_tree: true,
        }
    }
}

/// 从 Diagram 构建 MindmapTree。
///
/// 输入：已通过 `prepare/structure/mindmap::expand` 的 Diagram。
/// 算法与 `prepare/structure/mindmap.rs` 中的 `build_children_map` + `find_root_id` 一致。
pub fn build_mindmap_tree(
    diagram: &Diagram,
    options: &BuildTreeOptions,
) -> Result<MindmapTree, Vec<TreeValidationError>> {
    let children_map = build_children_map(diagram);
    let root_id = find_root_id(diagram, &children_map);

    // 提取 diagram title
    let title = diagram.title().map(|s| s.to_string());

    // 构建实体索引：id → (label, entity_type, branch_slot, tree_depth)
    let entity_index: HashMap<String, (String, Option<String>, Option<usize>, Option<usize>)> =
        diagram
            .entities
            .iter()
            .map(|e| {
                let entity_type = e
                    .attributes
                    .standard
                    .get("type")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let branch_slot = e.attributes.standard.get("branch_slot").and_then(|v| match v {
                    AttributeValue::Number(n) => Some(*n as usize),
                    _ => None,
                });
                let tree_depth = e.attributes.standard.get("tree_depth").and_then(|v| match v {
                    AttributeValue::Number(n) => Some(*n as usize),
                    _ => None,
                });
                (
                    e.id.as_str().to_string(),
                    (e.label.clone(), entity_type, branch_slot, tree_depth),
                )
            })
            .collect();

    // 树合法性检查
    let validation = validate_tree(diagram, &children_map, &root_id, options);
    if options.strict_tree && !validation.errors.is_empty() {
        return Err(validation.errors);
    }

    // DFS 递归构建 MindmapTreeNode
    let root_node = build_node(&root_id, &children_map, &entity_index);

    // 收集孤立节点
    let mut orphans = Vec::new();
    for orphan_id in &validation.orphan_ids {
        if let Some(node) = try_build_node(orphan_id, &children_map, &entity_index) {
            orphans.push(node);
        }
    }

    Ok(MindmapTree {
        title,
        root: root_node,
        orphans,
    })
}

fn build_node(
    id: &str,
    children_map: &HashMap<String, Vec<String>>,
    entity_index: &HashMap<String, (String, Option<String>, Option<usize>, Option<usize>)>,
) -> MindmapTreeNode {
    let (label, entity_type, branch_slot, tree_depth) = entity_index
        .get(id)
        .cloned()
        .unwrap_or_else(|| (id.to_string(), None, None, None));

    let child_ids = children_map.get(id).cloned().unwrap_or_default();
    let children: Vec<MindmapTreeNode> = child_ids
        .iter()
        .map(|child_id| build_node(child_id, children_map, entity_index))
        .collect();

    MindmapTreeNode {
        entity_id: id.to_string(),
        label,
        entity_type,
        branch_slot,
        tree_depth,
        children,
    }
}

fn try_build_node(
    id: &str,
    children_map: &HashMap<String, Vec<String>>,
    entity_index: &HashMap<String, (String, Option<String>, Option<usize>, Option<usize>)>,
) -> Option<MindmapTreeNode> {
    if !entity_index.contains_key(id) {
        return None;
    }
    Some(build_node(id, children_map, entity_index))
}

/// 树合法性检查。
fn validate_tree(
    diagram: &Diagram,
    children_map: &HashMap<String, Vec<String>>,
    root_id: &str,
    options: &BuildTreeOptions,
) -> TreeValidationResult {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut orphan_ids = Vec::new();

    // 1. 检查多父节点
    let mut parent_count: HashMap<&str, usize> = HashMap::new();
    for rel in &diagram.relations {
        *parent_count.entry(rel.to.as_str()).or_insert(0) += 1;
    }
    for (id, count) in &parent_count {
        if *count > 1 {
            let err = TreeValidationError::MultiParent {
                entity_id: id.to_string(),
                parent_count: *count,
            };
            if options.strict_tree {
                errors.push(err);
            } else {
                warnings.push(err);
            }
        }
    }

    // 2. 检查环（DFS from root）
    let mut visited = HashSet::new();
    let mut on_stack = HashSet::new();
    let has_cycle = dfs_cycle(root_id, children_map, &mut visited, &mut on_stack);
    if has_cycle {
        let err = TreeValidationError::Cycle {
            entity_id: root_id.to_string(),
        };
        if options.strict_tree {
            errors.push(err);
        } else {
            warnings.push(err);
        }
    }

    // 3. 检查不可达节点
    let all_entity_ids: HashSet<String> = diagram
        .entities
        .iter()
        .map(|e| e.id.as_str().to_string())
        .collect();
    let unreachable_ids: Vec<String> = all_entity_ids.difference(&visited).cloned().collect();
    if !unreachable_ids.is_empty() {
        orphan_ids = unreachable_ids.clone();
        let err = TreeValidationError::Unreachable {
            entity_ids: unreachable_ids,
        };
        if options.strict_tree {
            errors.push(err);
        } else {
            warnings.push(err);
        }
    }

    TreeValidationResult {
        errors,
        warnings,
        orphan_ids,
    }
}

fn dfs_cycle(
    id: &str,
    children_map: &HashMap<String, Vec<String>>,
    visited: &mut HashSet<String>,
    on_stack: &mut HashSet<String>,
) -> bool {
    visited.insert(id.to_string());
    on_stack.insert(id.to_string());
    if let Some(kids) = children_map.get(id) {
        for kid in kids {
            if on_stack.contains(kid.as_str()) {
                return true;
            }
            if !visited.contains(kid.as_str()) {
                if dfs_cycle(kid, children_map, visited, on_stack) {
                    return true;
                }
            }
        }
    }
    on_stack.remove(id);
    false
}

// ─── MindmapTree → Diagram ────────────────────────────────────────

/// 从 MindmapTree 构建 Diagram 的选项。
#[derive(Debug, Clone)]
pub struct DiagramBuildOptions {
    pub diagram_type: DiagramType,
    pub infer_entity_types: bool,
    pub layout: Option<String>,
    pub theme: Option<String>,
    pub graphic_style: Option<String>,
}

impl Default for DiagramBuildOptions {
    fn default() -> Self {
        Self {
            diagram_type: DiagramType::Mindmap,
            infer_entity_types: true,
            layout: None,
            theme: None,
            graphic_style: None,
        }
    }
}

/// 从 MindmapTree 构建 Diagram AST。
///
/// 按 DFS 序创建 Entity 列表与 Relation 列表。
/// 不在此阶段调用 expand_structure；由后续 prepare() 写入 branch_slot / tree_depth。
pub fn mindmap_tree_to_diagram(tree: &MindmapTree, opts: &DiagramBuildOptions) -> Diagram {
    let mut entities = Vec::new();
    let mut relations = Vec::new();

    // 递归构建 entities 和 relations
    build_diagram_recursive(
        &tree.root,
        None,
        opts.infer_entity_types,
        &mut entities,
        &mut relations,
    );

    // 处理孤立节点
    for orphan in &tree.orphans {
        let entity = make_entity(&orphan.entity_id, &orphan.label, None);
        entities.push(entity);
    }

    // 构建 diagram attributes（title 存储为 DiagramAttribute）
    let mut attributes = Vec::new();
    if let Some(ref title) = tree.title {
        attributes.push(DiagramAttribute {
            key: "title".to_string(),
            value: AttributeValue::String(TextValue::quoted(title.clone())),
            span: Span::dummy(),
        });
    }

    Diagram {
        diagram_type: opts.diagram_type.clone(),
        attributes,
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

fn build_diagram_recursive(
    node: &MindmapTreeNode,
    parent_id: Option<&str>,
    infer_types: bool,
    entities: &mut Vec<Entity>,
    relations: &mut Vec<Relation>,
) {
    let entity_type = if infer_types {
        infer_entity_type(node, parent_id.is_some())
    } else {
        node.entity_type.clone()
    };

    entities.push(make_entity(&node.entity_id, &node.label, entity_type.as_deref()));

    if let Some(pid) = parent_id {
        relations.push(make_relation(pid, &node.entity_id));
    }

    for child in &node.children {
        build_diagram_recursive(child, Some(&node.entity_id), infer_types, entities, relations);
    }
}

/// 根据树位置推断 entity type。
fn infer_entity_type(node: &MindmapTreeNode, has_parent: bool) -> Option<String> {
    if !has_parent {
        Some("root".to_string())
    } else if node.children.is_empty() {
        Some("leaf".to_string())
    } else if node.tree_depth == Some(1) {
        Some("main".to_string())
    } else {
        Some("branch".to_string())
    }
}

fn make_entity(id: &str, label: &str, entity_type: Option<&str>) -> Entity {
    let mut attrs = AttributeMap::default();
    if let Some(ty) = entity_type {
        attrs.standard.insert(
            "type".to_string(),
            AttributeValue::String(TextValue::unquoted(ty.to_string())),
        );
    }
    Entity {
        id: Identifier::new_unchecked(id),
        label: label.to_string(),
        attributes: attrs,
        group_id: None,
        span: Span::dummy(),
    }
}

fn make_relation(from: &str, to: &str) -> Relation {
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
