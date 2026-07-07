//! materialize：`materialize_diagram_styles` 入口。
//!
//! 镜像 `prepare/styles.rs::materialize_styles` 职责，但使用
//! `CompiledTheme` + `InstanceContext` 实现物化。
//!
//! 与旧路径的差异：
//! - group 样式在 prepare 物化（旧路径在 render 用 `group_style_by_depth` 派生）
//! - `{branch.*}` 由 context_palettes 结构化 overlay 替代
//! - `edge_depth_stroke_width` 由 context_palettes.edge_depth 替代
//!
//! Phase 2：仅用于门禁测试，不接入生产 prepare 管线（Phase 4 切换）。

use std::collections::{HashMap, HashSet};

use crate::ast::{AttributeValue, Diagram, StyleSource};
use crate::error::Result;

use super::context_palette::{
    derive_edge_context, materialize_edge, materialize_group, materialize_node,
};
use super::schema::{InstanceContext, ThemeContext};
use super::schema::StyleValue;

/// 从 `AttributeValue` 读取 usize（仅 `Number` 类型，负数/NaN 返回 None）。
fn attr_as_usize(v: &AttributeValue) -> Option<usize> {
    match v {
        AttributeValue::Number(n) if *n >= 0.0 && n.is_finite() => Some(*n as usize),
        _ => None,
    }
}

/// 将物化结果写入 `attributes.style`，使用 `or_insert` 语义。
///
/// 复用 `prepare::bridge::merge_style_block_into_attrs` 以保持与旧路径一致的写入语义。
fn merge_block_into_attrs(
    target: &mut crate::ast::StyleMap,
    source: &super::schema::StyleBlock,
    source_kind: StyleSource,
) {
    crate::prepare::bridge::merge_style_block_into_attrs(target, source, source_kind);
}

/// 将 theme cascade + context_palettes 物化到每个 entity / relation / group 的
/// `attributes.style`。
///
/// 优先级（从低到高）：
/// 1. L1 类型级（CompiledTheme 查表，`or_insert` 写入）
/// 2. L2 实例级（context_palettes overlay，`insert` 强制覆盖指定字段）
/// 3. DSL 声明 `node_style` / `edge_style`（跳过内联键）
/// 4. 内联 `style.*`（最高，全程 `or_insert` 不覆盖）
///
/// 与旧 `materialize_styles` 的差异：新增 group 物化（从 `context_palettes.group_nest`
/// 读取 depth 递进样式，写入 `group.attributes.style`）。
pub fn materialize_diagram_styles(
    mut diagram: Diagram,
    ctx: &ThemeContext<'_>,
) -> Result<Diagram> {
    let diagram_type_key = diagram.diagram_type.style_key();
    let compiled = ctx.compiled;

    // 记录内联 style key（Parser 写入的），用于 DSL 声明跳过
    let inline_node_keys: HashSet<(String, String)> = diagram
        .entities
        .iter()
        .flat_map(|e| {
            e.attributes
                .style
                .keys()
                .map(|k| (e.id.as_str().to_string(), k.clone()))
                .collect::<Vec<_>>()
        })
        .collect();
    let inline_edge_keys: HashSet<(usize, String)> = diagram
        .relations
        .iter()
        .enumerate()
        .flat_map(|(i, r)| {
            r.attributes
                .style
                .keys()
                .map(|k| (i, k.clone()))
                .collect::<Vec<_>>()
        })
        .collect();

    // 预建 entity_id → (branch_slot, tree_depth) 查找表（边物化用）
    let entity_context: HashMap<String, (Option<usize>, Option<usize>)> = diagram
        .entities
        .iter()
        .map(|e| {
            let slot = e
                .attributes
                .standard
                .get("branch_slot")
                .and_then(attr_as_usize);
            let depth = e
                .attributes
                .standard
                .get("tree_depth")
                .and_then(attr_as_usize);
            (e.id.as_str().to_string(), (slot, depth))
        })
        .collect();

    // root_id：优先用 ThemeContext 提供的；否则从 entity type == "root" 推导
    let root_id_str: Option<String> = ctx
        .root_id
        .map(|id| id.as_str().to_string())
        .or_else(|| {
            diagram.entities.iter().find_map(|e| {
                let is_root = e
                    .attributes
                    .standard
                    .get("type")
                    .and_then(|v| match v {
                        AttributeValue::String(s) => Some(s == "root"),
                        _ => None,
                    })
                    .unwrap_or(false);
                if is_root {
                    Some(e.id.as_str().to_string())
                } else {
                    None
                }
            })
        });

    // ── 物化 entity 样式 ──
    for entity in &mut diagram.entities {
        let entity_type = entity
            .attributes
            .standard
            .get("type")
            .and_then(|v| match v {
                AttributeValue::String(s) => Some(s.as_str()),
                _ => None,
            });

        let branch_slot = entity
            .attributes
            .standard
            .get("branch_slot")
            .and_then(attr_as_usize);
        let tree_depth = entity
            .attributes
            .standard
            .get("tree_depth")
            .and_then(attr_as_usize);

        let inst_ctx = InstanceContext {
            branch_slot,
            tree_depth,
            group_depth: None,
        };

        let block = materialize_node(compiled, diagram_type_key, entity_type, &inst_ctx);

        let palette_source = StyleSource::Palette {
            style_sheet_id: compiled.id.clone(),
            entity_type: entity_type.unwrap_or("default").to_string(),
            branch_slot,
        };
        merge_block_into_attrs(&mut entity.attributes.style, &block, palette_source);
    }

    // ── 物化 relation 样式 ──
    for relation in diagram.relations.iter_mut() {
        let edge_kind = relation
            .attributes
            .standard
            .get("line_style")
            .and_then(|v| match v {
                AttributeValue::String(s) => Some(s.as_str()),
                _ => None,
            });

        let from_id = relation.from.as_str();
        let to_id = relation.to.as_str();
        let (from_slot, from_depth) = entity_context
            .get(from_id)
            .copied()
            .unwrap_or((None, None));
        let (to_slot, to_depth) = entity_context.get(to_id).copied().unwrap_or((None, None));
        let from_is_root = Some(from_id) == root_id_str.as_deref();

        let inst_ctx =
            derive_edge_context(from_slot, from_depth, to_slot, to_depth, from_is_root);

        let block = materialize_edge(compiled, diagram_type_key, edge_kind, &inst_ctx);

        let token_source = StyleSource::Token {
            key: edge_kind.unwrap_or("default").to_string(),
        };
        merge_block_into_attrs(&mut relation.attributes.style, &block, token_source);
    }

    // ── 物化 group 样式（新增，旧路径在 render 才算）──
    for group in &mut diagram.groups {
        let inst_ctx = InstanceContext {
            branch_slot: None,
            tree_depth: None,
            group_depth: Some(group.depth as usize),
        };

        let block = materialize_group(compiled, diagram_type_key, &inst_ctx);

        let group_source = StyleSource::Palette {
            style_sheet_id: compiled.id.clone(),
            entity_type: "group".to_string(),
            branch_slot: Some(group.depth as usize),
        };
        merge_block_into_attrs(&mut group.attributes.style, &block, group_source);
    }

    // ── DSL 声明覆盖（node_style / edge_style）──
    crate::prepare::styles::apply_style_decls(&mut diagram, &inline_node_keys, &inline_edge_keys);

    // 声明式规则已物化，清空以满足 PreparedDiagram 不变量 I1
    diagram.style_decls.clear();

    Ok(diagram)
}

// ─── 辅助：从 StyleBlock 读取值（供门禁测试用） ─────────────────────

/// 读取 StyleBlock 的字符串值。
pub fn block_string(block: &super::schema::StyleBlock, key: &str) -> Option<String> {
    block.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

/// 读取 StyleBlock 的数值。
pub fn block_number(block: &super::schema::StyleBlock, key: &str) -> Option<f64> {
    block.get(key).and_then(|v| v.as_number())
}

/// 读取 StyleMap（已物化）的字符串值。
pub fn attr_string(map: &crate::ast::StyleMap, key: &str) -> Option<String> {
    map.get(key).and_then(|v| match v {
        AttributeValue::String(s) => Some(s.to_string()),
        _ => None,
    })
}

/// 读取 StyleMap（已物化）的数值。
pub fn attr_number(map: &crate::ast::StyleMap, key: &str) -> Option<f64> {
    map.get(key).and_then(|v| match v {
        AttributeValue::Number(n) => Some(*n),
        _ => None,
    })
}

/// 将 StyleValue 转为可比较的字符串形式（用于 diff）。
pub fn style_value_to_cmp_string(v: &StyleValue) -> String {
    match v {
        StyleValue::String(s) => s.clone(),
        StyleValue::Number(n) => {
            if *n == (*n as i64) as f64 {
                (*n as i64).to_string()
            } else {
                format!("{n:.2}")
            }
        }
        StyleValue::Boolean(b) => b.to_string(),
        StyleValue::Array(arr) => arr
            .iter()
            .map(|n| {
                if *n == (*n as i64) as f64 {
                    (*n as i64).to_string()
                } else {
                    format!("{n:.2}")
                }
            })
            .collect::<Vec<_>>()
            .join(","),
    }
}
