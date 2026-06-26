//! Intent Patch 应用逻辑
//!
//! 将 [`ChangeSet`] 应用到基础 [`RawDiagram`]，产出更新后的 [`RawDiagram`]。
//!
//! 应用流程：
//! 1. 克隆基础 Diagram
//! 2. 逐条应用 Change（Add / Remove / Modify）
//! 3. 重建派生字段：`group.entity_ids`、`group.child_group_ids`、`group.depth`
//!    （从 `entity.group_id` 和 `group.parent_id` 推导）
//!
//! 即使部分变更失败，也返回已应用部分的结果。调用方应检查 [`PatchResult::is_ok`]。

use crate::ast::*;
use crate::diff2::diff::{relation_key, style_decl_key};
use crate::diff2::types::*;
use crate::types::DiagramType;
use std::collections::HashMap;

/// 将变更集应用到基础 RawDiagram。
pub fn patch(base: &RawDiagram, changes: &ChangeSet) -> PatchResult {
    let mut diagram = base.inner().clone();
    let mut applied = 0;
    let mut errors = Vec::new();

    for change in &changes.changes {
        match apply_change(&mut diagram, change) {
            Ok(_) => applied += 1,
            Err(e) => errors.push(e),
        }
    }

    // 重建派生字段
    rebuild_group_membership(&mut diagram);

    PatchResult {
        diagram: RawDiagram(diagram),
        applied,
        errors,
    }
}

// ─── Dispatch ──────────────────────────────────────────────────────

fn apply_change(diagram: &mut Diagram, change: &Change) -> Result<(), String> {
    match change.op {
        ChangeOp::Add => apply_add(diagram, change),
        ChangeOp::Remove => apply_remove(diagram, change),
        ChangeOp::Modify => apply_modify(diagram, change),
    }
}

// ─── Add ───────────────────────────────────────────────────────────

fn apply_add(diagram: &mut Diagram, change: &Change) -> Result<(), String> {
    let value = change
        .new_value
        .as_ref()
        .ok_or("Add 操作缺少 new_value")?;

    // 子属性 Add（非 Diagram 目标且有 attr_key）等价于 Modify（插入新值）
    if change.path.target != ChangeTarget::Diagram && change.path.attr_key.is_some() {
        return apply_modify(diagram, change);
    }

    match change.path.target {
        ChangeTarget::Diagram => {
            let key = change
                .path
                .attr_key
                .as_ref()
                .ok_or("Diagram Add 缺少 attr_key")?;
            if key == "diagram_type" {
                return Err("diagram_type 应使用 Modify 修改，不能用 Add".into());
            }
            let av = json_to_attribute_value(value)?;
            if diagram.attributes.iter().any(|a| a.key == *key) {
                return Err(format!("diagram 属性 '{}' 已存在", key));
            }
            diagram.attributes.push(DiagramAttribute {
                key: key.clone(),
                value: av,
                span: Span::dummy(),
            });
            Ok(())
        }
        ChangeTarget::Entity => {
            let id_str = value
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or("Entity Add 缺少 id 字段")?;
            if diagram.entities.iter().any(|e| e.id.as_str() == id_str) {
                return Err(format!("entity '{}' 已存在", id_str));
            }
            let entity = json_to_entity(value)?;
            diagram.entities.push(entity);
            Ok(())
        }
        ChangeTarget::Relation => {
            let relation = json_to_relation(value)?;
            let key = relation_key(&relation);
            if diagram.relations.iter().any(|r| relation_key(r) == key) {
                return Err(format!("relation '{}' 已存在", key));
            }
            diagram.relations.push(relation);
            Ok(())
        }
        ChangeTarget::Group => {
            let id_str = value
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or("Group Add 缺少 id 字段")?;
            if diagram.groups.iter().any(|g| g.id.as_str() == id_str) {
                return Err(format!("group '{}' 已存在", id_str));
            }
            let group = json_to_group(value)?;
            diagram.groups.push(group);
            Ok(())
        }
        ChangeTarget::StyleDecl => {
            let decl = json_to_style_decl(value)?;
            let key = style_decl_key(&decl);
            if diagram.style_decls.iter().any(|d| style_decl_key(d) == key) {
                return Err(format!("style_decl '{}' 已存在", key));
            }
            diagram.style_decls.push(decl);
            Ok(())
        }
    }
}

// ─── Remove ────────────────────────────────────────────────────────

fn apply_remove(diagram: &mut Diagram, change: &Change) -> Result<(), String> {
    // 子属性 Remove（非 Diagram 目标且有 attr_key）— 构造 null Modify 委托
    if change.path.target != ChangeTarget::Diagram && change.path.attr_key.is_some() {
        let null_change = Change {
            op: ChangeOp::Modify,
            path: change.path.clone(),
            old_value: change.old_value.clone(),
            new_value: Some(serde_json::Value::Null),
        };
        return apply_modify(diagram, &null_change);
    }

    match change.path.target {
        ChangeTarget::Diagram => {
            let key = change
                .path
                .attr_key
                .as_ref()
                .ok_or("Diagram Remove 缺少 attr_key")?;
            let len = diagram.attributes.len();
            diagram.attributes.retain(|a| a.key != *key);
            if diagram.attributes.len() == len {
                return Err(format!("diagram 属性 '{}' 不存在", key));
            }
            Ok(())
        }
        ChangeTarget::Entity => {
            let id = change.path.id.as_ref().ok_or("Entity Remove 缺少 id")?;
            let len = diagram.entities.len();
            diagram.entities.retain(|e| e.id.as_str() != id);
            if diagram.entities.len() == len {
                return Err(format!("entity '{}' 不存在", id));
            }
            Ok(())
        }
        ChangeTarget::Relation => {
            let id = change.path.id.as_ref().ok_or("Relation Remove 缺少 id")?;
            let len = diagram.relations.len();
            diagram.relations.retain(|r| relation_key(r) != *id);
            if diagram.relations.len() == len {
                return Err(format!("relation '{}' 不存在", id));
            }
            Ok(())
        }
        ChangeTarget::Group => {
            let id = change.path.id.as_ref().ok_or("Group Remove 缺少 id")?;
            let len = diagram.groups.len();
            diagram.groups.retain(|g| g.id.as_str() != id);
            if diagram.groups.len() == len {
                return Err(format!("group '{}' 不存在", id));
            }
            Ok(())
        }
        ChangeTarget::StyleDecl => {
            let id = change.path.id.as_ref().ok_or("StyleDecl Remove 缺少 id")?;
            let (kind, target) = id
                .split_once('/')
                .ok_or("style_decl id 格式错误，应为 'node_style/target' 或 'edge_style/target'")?;
            let kind = parse_style_decl_kind(kind)?;
            let len = diagram.style_decls.len();
            diagram
                .style_decls
                .retain(|d| !(d.kind == kind && d.target == target));
            if diagram.style_decls.len() == len {
                return Err(format!("style_decl '{}' 不存在", id));
            }
            Ok(())
        }
    }
}

// ─── Modify ────────────────────────────────────────────────────────

fn apply_modify(diagram: &mut Diagram, change: &Change) -> Result<(), String> {
    match change.path.target {
        ChangeTarget::Diagram => apply_diagram_modify(diagram, change),
        ChangeTarget::Entity => {
            let id = change.path.id.as_deref().unwrap_or("");
            let entity = diagram
                .entities
                .iter_mut()
                .find(|e| e.id.as_str() == id)
                .ok_or_else(|| format!("entity '{}' 不存在", id))?;
            apply_entity_modify(entity, change)
        }
        ChangeTarget::Relation => {
            let id = change.path.id.as_deref().unwrap_or("");
            let relation = diagram
                .relations
                .iter_mut()
                .find(|r| relation_key(r) == id)
                .ok_or_else(|| format!("relation '{}' 不存在", id))?;
            apply_relation_modify(relation, change)
        }
        ChangeTarget::Group => {
            let id = change.path.id.as_deref().unwrap_or("");
            let group = diagram
                .groups
                .iter_mut()
                .find(|g| g.id.as_str() == id)
                .ok_or_else(|| format!("group '{}' 不存在", id))?;
            apply_group_modify(group, change)
        }
        ChangeTarget::StyleDecl => {
            let id = change.path.id.as_deref().unwrap_or("");
            let (kind_str, target) = id
                .split_once('/')
                .ok_or("style_decl id 格式错误")?;
            let kind = parse_style_decl_kind(kind_str)?;
            let decl = diagram
                .style_decls
                .iter_mut()
                .find(|d| d.kind == kind && d.target == target)
                .ok_or_else(|| format!("style_decl '{}' 不存在", id))?;
            apply_style_decl_modify(decl, change)
        }
    }
}

fn apply_diagram_modify(diagram: &mut Diagram, change: &Change) -> Result<(), String> {
    let key = change
        .path
        .attr_key
        .as_deref()
        .ok_or("Diagram Modify 缺少 attr_key")?;

    if key == "diagram_type" {
        let new_val = change.new_value.as_ref().ok_or("缺少 new_value")?;
        diagram.diagram_type = serde_json::from_value::<DiagramType>(new_val.clone())
            .map_err(|e| format!("无法解析 diagram_type: {e}"))?;
        return Ok(());
    }

    let attr = diagram
        .attributes
        .iter_mut()
        .find(|a| a.key == key)
        .ok_or_else(|| format!("diagram 属性 '{}' 不存在", key))?;
    let new_val = change.new_value.as_ref().ok_or("缺少 new_value")?;
    attr.value = json_to_attribute_value(new_val)?;
    Ok(())
}

fn apply_entity_modify(entity: &mut Entity, change: &Change) -> Result<(), String> {
    let key = change
        .path
        .attr_key
        .as_deref()
        .ok_or("Entity Modify 缺少 attr_key")?;
    let new_val = change.new_value.as_ref().ok_or("缺少 new_value")?;

    match key {
        "label" => {
            entity.label = new_val
                .as_str()
                .ok_or("label 应为字符串")?
                .to_string();
        }
        "group_id" => {
            entity.group_id = if new_val.is_null() {
                None
            } else {
                Some(Identifier::new_unchecked(
                    new_val
                        .as_str()
                        .ok_or("group_id 应为字符串或 null")?,
                ))
            };
        }
        _ if key.starts_with("standard/") => {
            let sub_key = &key["standard/".len()..];
            apply_attr_map_modify(&mut entity.attributes.standard, sub_key, new_val)?;
        }
        _ if key.starts_with("style/") => {
            let sub_key = &key["style/".len()..];
            apply_style_map_modify(&mut entity.attributes.style, sub_key, new_val)?;
        }
        _ if key.starts_with("meta/") => {
            let sub_key = &key["meta/".len()..];
            apply_attr_map_modify(&mut entity.attributes.meta, sub_key, new_val)?;
        }
        _ => return Err(format!("未知 entity attr_key: '{}'", key)),
    }
    Ok(())
}

fn apply_relation_modify(relation: &mut Relation, change: &Change) -> Result<(), String> {
    let key = change
        .path
        .attr_key
        .as_deref()
        .ok_or("Relation Modify 缺少 attr_key")?;
    let new_val = change.new_value.as_ref().ok_or("缺少 new_value")?;

    match key {
        "arrow" => {
            relation.arrow = serde_json::from_value::<ArrowType>(new_val.clone())
                .map_err(|e| format!("无法解析 arrow: {e}"))?;
        }
        _ if key.starts_with("standard/") => {
            let sub_key = &key["standard/".len()..];
            apply_attr_map_modify(&mut relation.attributes.standard, sub_key, new_val)?;
        }
        _ if key.starts_with("style/") => {
            let sub_key = &key["style/".len()..];
            apply_style_map_modify(&mut relation.attributes.style, sub_key, new_val)?;
        }
        _ if key.starts_with("meta/") => {
            let sub_key = &key["meta/".len()..];
            apply_attr_map_modify(&mut relation.attributes.meta, sub_key, new_val)?;
        }
        _ => return Err(format!("未知 relation attr_key: '{}'", key)),
    }
    Ok(())
}

fn apply_group_modify(group: &mut Group, change: &Change) -> Result<(), String> {
    let key = change
        .path
        .attr_key
        .as_deref()
        .ok_or("Group Modify 缺少 attr_key")?;
    let new_val = change.new_value.as_ref().ok_or("缺少 new_value")?;

    match key {
        "label" => {
            group.label = new_val
                .as_str()
                .ok_or("label 应为字符串")?
                .to_string();
        }
        "parent_id" => {
            group.parent_id = if new_val.is_null() {
                None
            } else {
                Some(Identifier::new_unchecked(
                    new_val
                        .as_str()
                        .ok_or("parent_id 应为字符串或 null")?,
                ))
            };
        }
        _ if key.starts_with("standard/") => {
            let sub_key = &key["standard/".len()..];
            apply_attr_map_modify(&mut group.attributes.standard, sub_key, new_val)?;
        }
        _ if key.starts_with("meta/") => {
            let sub_key = &key["meta/".len()..];
            apply_attr_map_modify(&mut group.attributes.meta, sub_key, new_val)?;
        }
        _ => return Err(format!("未知 group attr_key: '{}'", key)),
    }
    Ok(())
}

fn apply_style_decl_modify(decl: &mut StyleDecl, change: &Change) -> Result<(), String> {
    let key = change
        .path
        .attr_key
        .as_deref()
        .ok_or("StyleDecl Modify 缺少 attr_key")?;

    if let Some(sub_key) = key.strip_prefix("style/") {
        let new_val = change.new_value.as_ref().ok_or("缺少 new_value")?;
        if new_val.is_null() {
            decl.style.remove(sub_key);
        } else {
            let av = json_to_attribute_value(new_val)?;
            decl.style.insert(sub_key.to_string(), av);
        }
        return Ok(());
    }

    Err(format!("未知 style_decl attr_key: '{}'", key))
}

// ─── Attr map modify helpers ───────────────────────────────────────

fn apply_attr_map_modify(
    map: &mut HashMap<String, AttributeValue>,
    key: &str,
    new_val: &serde_json::Value,
) -> Result<(), String> {
    if new_val.is_null() {
        map.remove(key);
    } else {
        let av = json_to_attribute_value(new_val)?;
        map.insert(key.to_string(), av);
    }
    Ok(())
}

fn apply_style_map_modify(
    style: &mut StyleMap,
    key: &str,
    new_val: &serde_json::Value,
) -> Result<(), String> {
    if new_val.is_null() {
        style.remove(key);
    } else {
        let av = json_to_attribute_value(new_val)?;
        style.insert_with_source(key.to_string(), av, StyleSource::Inline);
    }
    Ok(())
}

// ─── JSON → AST converters ─────────────────────────────────────────

/// 将 JSON 值还原为 [`AttributeValue`]。
///
/// `AttributeValue` 使用 `#[serde(untagged)]` + `serialize_with`，但无对应的
/// `deserialize_with`，因此 `serde_json::from_value::<AttributeValue>` 无法还原
/// `Atom` / `Enum` / `Config` 变体（它们序列化为 `{"$atom": ...}` 等带标记对象）。
/// 本函数手动处理这些标记格式，保证 diff→patch 闭环。
fn json_to_attribute_value(value: &serde_json::Value) -> Result<AttributeValue, String> {
    match value {
        serde_json::Value::String(s) => Ok(AttributeValue::String(TextValue::quoted(s.clone()))),
        serde_json::Value::Number(n) => {
            let f = n.as_f64().ok_or_else(|| format!("无效数字: {n}"))?;
            Ok(AttributeValue::Number(f))
        }
        serde_json::Value::Bool(b) => Ok(AttributeValue::Boolean(*b)),
        serde_json::Value::Object(obj) => {
            if let Some(v) = obj.get("$atom").and_then(|v| v.as_str()) {
                Ok(AttributeValue::String(TextValue::unquoted(v.to_string())))
            } else if let Some(v) = obj.get("$enum").and_then(|v| v.as_str()) {
                Ok(AttributeValue::String(TextValue::unquoted(v.to_string())))
            } else if let Some(config) = obj.get("$config").and_then(|v| v.as_object()) {
                let algo = config
                    .get("algo")
                    .and_then(|v| v.as_str())
                    .ok_or("$config 缺少 algo 字段")?
                    .to_string();
                let mut options = HashMap::new();
                if let Some(opts) = config.get("options").and_then(|v| v.as_object()) {
                    for (k, v) in opts {
                        let av = json_to_attribute_value(v)
                            .map_err(|e| format!("$config options '{}': {e}", k))?;
                        options.insert(k.clone(), av);
                    }
                }
                Ok(AttributeValue::Config { algo, options })
            } else {
                Err(format!("无法识别的属性值对象: {value}"))
            }
        }
        _ => Err(format!("无法解析属性值: {value}")),
    }
}

fn json_to_entity(value: &serde_json::Value) -> Result<Entity, String> {
    let id_str = value
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or("Entity 缺少 id 字段")?;
    let label = value
        .get("label")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let group_id = value
        .get("group_id")
        .and_then(|v| v.as_str())
        .map(Identifier::new_unchecked);
    let attributes = value
        .get("attributes")
        .map(json_to_attributes)
        .unwrap_or_else(|| Ok(AttributeMap::default()))?;

    Ok(Entity {
        id: Identifier::new_unchecked(id_str),
        label,
        attributes,
        group_id,
        span: Span::dummy(),
    })
}

fn json_to_relation(value: &serde_json::Value) -> Result<Relation, String> {
    let from_str = value
        .get("from")
        .and_then(|v| v.as_str())
        .ok_or("Relation 缺少 from 字段")?;
    let to_str = value
        .get("to")
        .and_then(|v| v.as_str())
        .ok_or("Relation 缺少 to 字段")?;
    let arrow = value
        .get("arrow")
        .and_then(|v| serde_json::from_value::<ArrowType>(v.clone()).ok())
        .unwrap_or(ArrowType::Active);
    let label = value
        .get("label")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let attributes = value
        .get("attributes")
        .map(json_to_attributes)
        .unwrap_or_else(|| Ok(AttributeMap::default()))?;

    Ok(Relation {
        from: Identifier::new_unchecked(from_str),
        to: Identifier::new_unchecked(to_str),
        arrow,
        label,
        head_label: None,
        tail_label: None,
        attributes,
        span: Span::dummy(),
    })
}

fn json_to_group(value: &serde_json::Value) -> Result<Group, String> {
    let id_str = value
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or("Group 缺少 id 字段")?;
    let label = value
        .get("label")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let parent_id = value
        .get("parent_id")
        .and_then(|v| v.as_str())
        .map(Identifier::new_unchecked);
    let attributes = value
        .get("attributes")
        .map(json_to_attributes)
        .unwrap_or_else(|| Ok(AttributeMap::default()))?;

    Ok(Group {
        id: Identifier::new_unchecked(id_str),
        label,
        attributes,
        parent_id,
        depth: 0, // 由 rebuild_group_membership 重算
        entity_ids: Vec::new(),
        child_group_ids: Vec::new(),
        span: Span::dummy(),
    })
}

fn json_to_style_decl(value: &serde_json::Value) -> Result<StyleDecl, String> {
    let kind = value
        .get("kind")
        .and_then(|v| serde_json::from_value::<StyleDeclKind>(v.clone()).ok())
        .ok_or("StyleDecl 缺少 kind 字段")?;
    let target = value
        .get("target")
        .and_then(|v| v.as_str())
        .ok_or("StyleDecl 缺少 target 字段")?
        .to_string();

    let mut style = HashMap::new();
    if let Some(style_obj) = value.get("style").and_then(|v| v.as_object()) {
        for (k, v) in style_obj {
            let av = json_to_attribute_value(v)
                .map_err(|e| format!("无法解析 style 属性 '{}': {e}", k))?;
            style.insert(k.clone(), av);
        }
    }

    Ok(StyleDecl {
        kind,
        target,
        style,
        span: Span::dummy(),
    })
}

fn json_to_attributes(value: &serde_json::Value) -> Result<AttributeMap, String> {
    let mut attrs = AttributeMap::default();
    let Some(obj) = value.as_object() else {
        return Ok(attrs);
    };

    if let Some(std_obj) = obj.get("standard").and_then(|v| v.as_object()) {
        for (k, v) in std_obj {
            let av = json_to_attribute_value(v)
                .map_err(|e| format!("无法解析 standard 属性 '{}': {e}", k))?;
            attrs.standard.insert(k.clone(), av);
        }
    }
    if let Some(style_obj) = obj.get("style").and_then(|v| v.as_object()) {
        for (k, v) in style_obj {
            let av = json_to_attribute_value(v)
                .map_err(|e| format!("无法解析 style 属性 '{}': {e}", k))?;
            attrs
                .style
                .insert_with_source(k.clone(), av, StyleSource::Inline);
        }
    }
    if let Some(meta_obj) = obj.get("meta").and_then(|v| v.as_object()) {
        for (k, v) in meta_obj {
            let av = json_to_attribute_value(v)
                .map_err(|e| format!("无法解析 meta 属性 '{}': {e}", k))?;
            attrs.meta.insert(k.clone(), av);
        }
    }
    Ok(attrs)
}

fn parse_style_decl_kind(s: &str) -> Result<StyleDeclKind, String> {
    match s {
        "node_style" => Ok(StyleDeclKind::Node),
        "edge_style" => Ok(StyleDeclKind::Edge),
        _ => Err(format!("未知 style_decl kind: '{}'", s)),
    }
}

// ─── Rebuild derived group fields ──────────────────────────────────

/// 从 `entity.group_id` 和 `group.parent_id` 重建派生字段：
/// - `group.entity_ids`
/// - `group.child_group_ids`
/// - `group.depth`
fn rebuild_group_membership(diagram: &mut Diagram) {
    // 清空派生字段
    for group in &mut diagram.groups {
        group.entity_ids.clear();
        group.child_group_ids.clear();
    }

    // 从 entity.group_id 重建 entity_ids
    for entity in &diagram.entities {
        if let Some(gid) = &entity.group_id {
            if let Some(group) = diagram.groups.iter_mut().find(|g| &g.id == gid) {
                group.entity_ids.push(entity.id.clone());
            }
        }
    }

    // 从 group.parent_id 重建 child_group_ids
    let group_parents: Vec<(Identifier, Option<Identifier>)> = diagram
        .groups
        .iter()
        .map(|g| (g.id.clone(), g.parent_id.clone()))
        .collect();
    for (gid, parent_id) in &group_parents {
        if let Some(pid) = parent_id {
            if let Some(parent) = diagram.groups.iter_mut().find(|g| &g.id == pid) {
                parent.child_group_ids.push(gid.clone());
            }
        }
    }

    // 重算 depth
    let groups_snapshot: Vec<(Identifier, Option<Identifier>)> = diagram
        .groups
        .iter()
        .map(|g| (g.id.clone(), g.parent_id.clone()))
        .collect();
    for group in &mut diagram.groups {
        group.depth = compute_depth(&groups_snapshot, &group.id);
    }
}

fn compute_depth(
    groups: &[(Identifier, Option<Identifier>)],
    id: &Identifier,
) -> u8 {
    let mut depth = 0u8;
    let mut current = id.clone();
    while let Some((_, parent_id)) = groups.iter().find(|(gid, _)| gid == &current) {
        if let Some(pid) = parent_id {
            depth += 1;
            current = pid.clone();
        } else {
            break;
        }
    }
    depth
}
