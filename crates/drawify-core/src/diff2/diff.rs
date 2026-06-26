//! Intent Diff 比较逻辑
//!
//! 比较两份 RawDiagram 的语义差异，输出结构化 [`ChangeSet`]。
//!
//! 比较范围：
//! - `diagram_type`
//! - diagram 级属性（`attributes`）
//! - `style_decls`（node_style / edge_style 声明）
//! - entities（按 id 索引；label / group_id / standard / style / meta 分别比较）
//! - relations（按 `(from, to, label)` 三元组索引；arrow / standard / style / meta 比较）
//! - groups（按 id 索引；label / parent_id / standard / meta 比较；
//!   `entity_ids` / `child_group_ids` / `depth` 为派生字段，不直接比较）
//!
//! 不比较的内容（非语义）：
//! - `Span` / `SourceInfo`
//! - `StyleSource`（RawDiagram 中均为 Inline）
//! - `group.entity_ids` / `group.child_group_ids` / `group.depth`（从 `entity.group_id`
//!   和 `group.parent_id` 派生）

use crate::ast::*;
use crate::diff2::types::*;
use std::collections::{HashMap, HashSet};

/// 比较两份 RawDiagram，返回语义差异变更集。
pub fn diff(old: &RawDiagram, new: &RawDiagram) -> ChangeSet {
    let mut changes = Vec::new();
    let old = old.inner();
    let new = new.inner();

    // diagram_type
    if old.diagram_type != new.diagram_type {
        changes.push(Change::modify(
            ChangePath::diagram_attr("diagram_type"),
            serde_json::to_value(&old.diagram_type).unwrap_or(serde_json::Value::Null),
            serde_json::to_value(&new.diagram_type).unwrap_or(serde_json::Value::Null),
        ));
    }

    diff_diagram_attributes(old, new, &mut changes);
    diff_style_decls(old, new, &mut changes);
    diff_entities(old, new, &mut changes);
    diff_relations(old, new, &mut changes);
    diff_groups(old, new, &mut changes);

    ChangeSet::new(changes)
}

// ─── Relation key ──────────────────────────────────────────────────

/// 生成 relation 的唯一键。
///
/// 格式：
/// - 无 label：`"from->to"`
/// - 有 label：`"from->to::label"`
///
/// 注意：label 是身份的一部分，label 变更会表现为 remove + add。
pub(super) fn relation_key(relation: &Relation) -> String {
    match &relation.label {
        None => format!("{}->{}", relation.from.as_str(), relation.to.as_str()),
        Some(label) => format!(
            "{}->{}::{}",
            relation.from.as_str(),
            relation.to.as_str(),
            label
        ),
    }
}

/// 生成 style_decl 的唯一键：`"node_style/target"` 或 `"edge_style/target"`。
pub(super) fn style_decl_key(decl: &StyleDecl) -> String {
    let kind = match decl.kind {
        StyleDeclKind::Node => "node_style",
        StyleDeclKind::Edge => "edge_style",
    };
    format!("{}/{}", kind, decl.target)
}

// ─── Diagram attributes ────────────────────────────────────────────

fn diff_diagram_attributes(old: &Diagram, new: &Diagram, changes: &mut Vec<Change>) {
    let old_map: HashMap<&str, &AttributeValue> = old
        .attributes
        .iter()
        .map(|a| (a.key.as_str(), &a.value))
        .collect();
    let new_map: HashMap<&str, &AttributeValue> = new
        .attributes
        .iter()
        .map(|a| (a.key.as_str(), &a.value))
        .collect();

    let old_keys: HashSet<&str> = old_map.keys().copied().collect();
    let new_keys: HashSet<&str> = new_map.keys().copied().collect();

    for key in new_keys.difference(&old_keys) {
        changes.push(Change::add(
            ChangePath::diagram_attr(*key),
            attribute_value_to_json(new_map[key]),
        ));
    }
    for key in old_keys.difference(&new_keys) {
        changes.push(Change::remove(
            ChangePath::diagram_attr(*key),
            attribute_value_to_json(old_map[key]),
        ));
    }
    for key in old_keys.intersection(&new_keys) {
        if old_map[key] != new_map[key] {
            changes.push(Change::modify(
                ChangePath::diagram_attr(*key),
                attribute_value_to_json(old_map[key]),
                attribute_value_to_json(new_map[key]),
            ));
        }
    }
}

// ─── StyleDecls ────────────────────────────────────────────────────

fn diff_style_decls(old: &Diagram, new: &Diagram, changes: &mut Vec<Change>) {
    let old_map: HashMap<String, &StyleDecl> = old
        .style_decls
        .iter()
        .map(|d| (style_decl_key(d), d))
        .collect();
    let new_map: HashMap<String, &StyleDecl> = new
        .style_decls
        .iter()
        .map(|d| (style_decl_key(d), d))
        .collect();

    let old_keys: HashSet<String> = old_map.keys().cloned().collect();
    let new_keys: HashSet<String> = new_map.keys().cloned().collect();

    for key in new_keys.difference(&old_keys) {
        changes.push(Change::add(
            ChangePath::style_decl(key),
            style_decl_to_json(new_map[key]),
        ));
    }
    for key in old_keys.difference(&new_keys) {
        changes.push(Change::remove(
            ChangePath::style_decl(key),
            style_decl_to_json(old_map[key]),
        ));
    }
    for key in old_keys.intersection(&new_keys) {
        diff_single_style_decl(key, old_map[key], new_map[key], changes);
    }
}

fn diff_single_style_decl(key: &str, old: &StyleDecl, new: &StyleDecl, changes: &mut Vec<Change>) {
    // kind 和 target 是身份的一部分，不在此处比较
    let old_keys: HashSet<&String> = old.style.keys().collect();
    let new_keys: HashSet<&String> = new.style.keys().collect();
    let all_keys: HashSet<&String> = old_keys.union(&new_keys).copied().collect();

    for prop_key in all_keys {
        let path = ChangePath::style_decl_attr(key, format!("style/{}", prop_key));
        match (old.style.get(prop_key), new.style.get(prop_key)) {
            (Some(o), Some(n)) if o != n => {
                changes.push(Change::modify(
                    path,
                    attribute_value_to_json(o),
                    attribute_value_to_json(n),
                ));
            }
            (None, Some(n)) => {
                changes.push(Change::add(path, attribute_value_to_json(n)));
            }
            (Some(o), None) => {
                changes.push(Change::remove(path, attribute_value_to_json(o)));
            }
            _ => {}
        }
    }
}

// ─── Entities ──────────────────────────────────────────────────────

fn diff_entities(old: &Diagram, new: &Diagram, changes: &mut Vec<Change>) {
    let old_map: HashMap<&str, &Entity> = old.entities.iter().map(|e| (e.id.as_str(), e)).collect();
    let new_map: HashMap<&str, &Entity> = new.entities.iter().map(|e| (e.id.as_str(), e)).collect();

    let old_ids: HashSet<&str> = old_map.keys().copied().collect();
    let new_ids: HashSet<&str> = new_map.keys().copied().collect();

    for id in new_ids.difference(&old_ids) {
        changes.push(Change::add(
            ChangePath::entity(*id),
            entity_to_json(new_map[id]),
        ));
    }
    for id in old_ids.difference(&new_ids) {
        changes.push(Change::remove(
            ChangePath::entity(*id),
            entity_to_json(old_map[id]),
        ));
    }
    for id in old_ids.intersection(&new_ids) {
        diff_single_entity(old_map[id], new_map[id], changes);
    }
}

fn diff_single_entity(old: &Entity, new: &Entity, changes: &mut Vec<Change>) {
    let id = old.id.as_str();

    if old.label != new.label {
        changes.push(Change::modify(
            ChangePath::entity_attr(id, "label"),
            serde_json::json!(old.label),
            serde_json::json!(new.label),
        ));
    }

    if old.group_id != new.group_id {
        changes.push(Change::modify(
            ChangePath::entity_attr(id, "group_id"),
            serde_json::json!(old.group_id.as_ref().map(|g| g.as_str().to_string())),
            serde_json::json!(new.group_id.as_ref().map(|g| g.as_str().to_string())),
        ));
    }

    diff_attr_map(
        &old.attributes.standard,
        &new.attributes.standard,
        changes,
        |k| ChangePath::entity_attr(id, format!("standard/{}", k)),
    );
    diff_style_map(
        &old.attributes.style,
        &new.attributes.style,
        changes,
        |k| ChangePath::entity_attr(id, format!("style/{}", k)),
    );
    diff_attr_map(
        &old.attributes.meta,
        &new.attributes.meta,
        changes,
        |k| ChangePath::entity_attr(id, format!("meta/{}", k)),
    );
}

// ─── Relations ─────────────────────────────────────────────────────

fn diff_relations(old: &Diagram, new: &Diagram, changes: &mut Vec<Change>) {
    let old_map: HashMap<String, &Relation> =
        old.relations.iter().map(|r| (relation_key(r), r)).collect();
    let new_map: HashMap<String, &Relation> =
        new.relations.iter().map(|r| (relation_key(r), r)).collect();

    let old_keys: HashSet<String> = old_map.keys().cloned().collect();
    let new_keys: HashSet<String> = new_map.keys().cloned().collect();

    for key in new_keys.difference(&old_keys) {
        changes.push(Change::add(
            ChangePath::relation(key),
            relation_to_json(new_map[key]),
        ));
    }
    for key in old_keys.difference(&new_keys) {
        changes.push(Change::remove(
            ChangePath::relation(key),
            relation_to_json(old_map[key]),
        ));
    }
    for key in old_keys.intersection(&new_keys) {
        diff_single_relation(key, old_map[key], new_map[key], changes);
    }
}

fn diff_single_relation(key: &str, old: &Relation, new: &Relation, changes: &mut Vec<Change>) {
    // label 是身份的一部分（在 relation_key 中），不在此处比较

    if old.arrow != new.arrow {
        changes.push(Change::modify(
            ChangePath::relation_attr(key, "arrow"),
            serde_json::to_value(&old.arrow).unwrap_or(serde_json::Value::Null),
            serde_json::to_value(&new.arrow).unwrap_or(serde_json::Value::Null),
        ));
    }

    diff_attr_map(
        &old.attributes.standard,
        &new.attributes.standard,
        changes,
        |k| ChangePath::relation_attr(key, format!("standard/{}", k)),
    );
    diff_style_map(
        &old.attributes.style,
        &new.attributes.style,
        changes,
        |k| ChangePath::relation_attr(key, format!("style/{}", k)),
    );
    diff_attr_map(
        &old.attributes.meta,
        &new.attributes.meta,
        changes,
        |k| ChangePath::relation_attr(key, format!("meta/{}", k)),
    );
}

// ─── Groups ────────────────────────────────────────────────────────

fn diff_groups(old: &Diagram, new: &Diagram, changes: &mut Vec<Change>) {
    let old_map: HashMap<&str, &Group> = old.groups.iter().map(|g| (g.id.as_str(), g)).collect();
    let new_map: HashMap<&str, &Group> = new.groups.iter().map(|g| (g.id.as_str(), g)).collect();

    let old_ids: HashSet<&str> = old_map.keys().copied().collect();
    let new_ids: HashSet<&str> = new_map.keys().copied().collect();

    for id in new_ids.difference(&old_ids) {
        changes.push(Change::add(
            ChangePath::group(*id),
            group_to_json(new_map[id]),
        ));
    }
    for id in old_ids.difference(&new_ids) {
        changes.push(Change::remove(
            ChangePath::group(*id),
            group_to_json(old_map[id]),
        ));
    }
    for id in old_ids.intersection(&new_ids) {
        diff_single_group(old_map[id], new_map[id], changes);
    }
}

fn diff_single_group(old: &Group, new: &Group, changes: &mut Vec<Change>) {
    let id = old.id.as_str();

    if old.label != new.label {
        changes.push(Change::modify(
            ChangePath::group_attr(id, "label"),
            serde_json::json!(old.label),
            serde_json::json!(new.label),
        ));
    }

    if old.parent_id != new.parent_id {
        changes.push(Change::modify(
            ChangePath::group_attr(id, "parent_id"),
            serde_json::json!(old.parent_id.as_ref().map(|g| g.as_str().to_string())),
            serde_json::json!(new.parent_id.as_ref().map(|g| g.as_str().to_string())),
        ));
    }

    diff_attr_map(
        &old.attributes.standard,
        &new.attributes.standard,
        changes,
        |k| ChangePath::group_attr(id, format!("standard/{}", k)),
    );
    diff_attr_map(
        &old.attributes.meta,
        &new.attributes.meta,
        changes,
        |k| ChangePath::group_attr(id, format!("meta/{}", k)),
    );

    // entity_ids / child_group_ids / depth 为派生字段，不直接比较
}

// ─── Generic attr map diff ─────────────────────────────────────────

fn diff_attr_map(
    old: &HashMap<String, AttributeValue>,
    new: &HashMap<String, AttributeValue>,
    changes: &mut Vec<Change>,
    path_fn: impl Fn(&str) -> ChangePath,
) {
    let keys: HashSet<&String> = old.keys().chain(new.keys()).collect();
    for key in keys {
        let path = path_fn(key);
        match (old.get(key), new.get(key)) {
            (Some(o), Some(n)) if o != n => {
                changes.push(Change::modify(
                    path,
                    attribute_value_to_json(o),
                    attribute_value_to_json(n),
                ));
            }
            (None, Some(n)) => {
                changes.push(Change::add(path, attribute_value_to_json(n)));
            }
            (Some(o), None) => {
                changes.push(Change::remove(path, attribute_value_to_json(o)));
            }
            _ => {}
        }
    }
}

fn diff_style_map(
    old: &StyleMap,
    new: &StyleMap,
    changes: &mut Vec<Change>,
    path_fn: impl Fn(&str) -> ChangePath,
) {
    let old_map: HashMap<String, AttributeValue> =
        old.iter_values().map(|(k, v)| (k.clone(), v.clone())).collect();
    let new_map: HashMap<String, AttributeValue> =
        new.iter_values().map(|(k, v)| (k.clone(), v.clone())).collect();
    diff_attr_map(&old_map, &new_map, changes, path_fn);
}

// ─── JSON serialization helpers ────────────────────────────────────

fn attribute_value_to_json(value: &AttributeValue) -> serde_json::Value {
    serde_json::to_value(value).unwrap_or(serde_json::Value::Null)
}

fn entity_to_json(entity: &Entity) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    obj.insert("id".into(), serde_json::json!(entity.id.as_str()));
    obj.insert("label".into(), serde_json::json!(entity.label));
    if let Some(gid) = &entity.group_id {
        obj.insert("group_id".into(), serde_json::json!(gid.as_str()));
    }
    let attrs = attributes_to_json(&entity.attributes);
    if !attrs.is_empty() {
        obj.insert("attributes".into(), serde_json::Value::Object(attrs));
    }
    serde_json::Value::Object(obj)
}

fn relation_to_json(relation: &Relation) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    obj.insert("from".into(), serde_json::json!(relation.from.as_str()));
    obj.insert("to".into(), serde_json::json!(relation.to.as_str()));
    obj.insert(
        "arrow".into(),
        serde_json::to_value(&relation.arrow).unwrap_or(serde_json::Value::Null),
    );
    if let Some(label) = &relation.label {
        obj.insert("label".into(), serde_json::json!(label));
    }
    let attrs = attributes_to_json(&relation.attributes);
    if !attrs.is_empty() {
        obj.insert("attributes".into(), serde_json::Value::Object(attrs));
    }
    serde_json::Value::Object(obj)
}

fn group_to_json(group: &Group) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    obj.insert("id".into(), serde_json::json!(group.id.as_str()));
    obj.insert("label".into(), serde_json::json!(group.label));
    if let Some(pid) = &group.parent_id {
        obj.insert("parent_id".into(), serde_json::json!(pid.as_str()));
    }
    let attrs = attributes_to_json(&group.attributes);
    if !attrs.is_empty() {
        obj.insert("attributes".into(), serde_json::Value::Object(attrs));
    }
    // entity_ids / child_group_ids / depth 为派生字段，不序列化
    serde_json::Value::Object(obj)
}

fn style_decl_to_json(decl: &StyleDecl) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    obj.insert(
        "kind".into(),
        serde_json::to_value(&decl.kind).unwrap_or(serde_json::Value::Null),
    );
    obj.insert("target".into(), serde_json::json!(decl.target));
    let style: serde_json::Map<String, serde_json::Value> = decl
        .style
        .iter()
        .map(|(k, v)| (k.clone(), attribute_value_to_json(v)))
        .collect();
    obj.insert("style".into(), serde_json::Value::Object(style));
    serde_json::Value::Object(obj)
}

fn attributes_to_json(attrs: &AttributeMap) -> serde_json::Map<String, serde_json::Value> {
    let mut obj = serde_json::Map::new();
    if !attrs.standard.is_empty() {
        let m: serde_json::Map<String, serde_json::Value> = attrs
            .standard
            .iter()
            .map(|(k, v)| (k.clone(), attribute_value_to_json(v)))
            .collect();
        obj.insert("standard".into(), serde_json::Value::Object(m));
    }
    if !attrs.style.is_empty() {
        let m: serde_json::Map<String, serde_json::Value> = attrs
            .style
            .iter_values()
            .map(|(k, v)| (k.clone(), attribute_value_to_json(v)))
            .collect();
        obj.insert("style".into(), serde_json::Value::Object(m));
    }
    if !attrs.meta.is_empty() {
        let m: serde_json::Map<String, serde_json::Value> = attrs
            .meta
            .iter()
            .map(|(k, v)| (k.clone(), attribute_value_to_json(v)))
            .collect();
        obj.insert("meta".into(), serde_json::Value::Object(m));
    }
    obj
}
