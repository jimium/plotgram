//! merge：单层 `extends` 合并。
//!
//! Phase 1 实现。

use crate::error::Result;
use super::schema::{
    DiagramStyles, ElementDefaults, StyleBlock, StyleSheet, StyleTokens,
};

/// 合并子主题 overlay 到基座 base（单层 extends）。
///
/// - 对象 deep merge；数组整段替换。
/// - merge 不展开 token。
/// - `context_palettes` 按 palette id deep merge（entries 整段替换）。
/// - 返回的 sheet.id = overlay.id。
pub fn merge_style_sheets(base: &StyleSheet, overlay: &StyleSheet) -> Result<StyleSheet> {
    let mut merged = base.clone();
    merge_tokens(&mut merged.tokens, &overlay.tokens);
    merge_defaults(&mut merged.defaults, &overlay.defaults);
    merge_diagrams(&mut merged.diagrams, &overlay.diagrams);
    merged.id = overlay.id.clone();
    merged.name = overlay.name.clone();
    if overlay.meta != super::schema::StyleMeta::default() {
        merged.meta = overlay.meta.clone();
    }
    Ok(merged)
}

fn merge_tokens(base: &mut StyleTokens, overlay: &StyleTokens) {
    for (k, v) in &overlay.colors {
        base.colors.insert(k.clone(), v.clone());
    }
    for (k, v) in &overlay.typography {
        base.typography.insert(k.clone(), v.clone());
    }
    for (k, v) in &overlay.strokes {
        base.strokes.insert(k.clone(), v.clone());
    }
    for (k, v) in &overlay.radius {
        base.radius.insert(k.clone(), v.clone());
    }
    for (k, v) in &overlay.spacing {
        base.spacing.insert(k.clone(), v.clone());
    }
    for (k, v) in &overlay.effects {
        base.effects.insert(k.clone(), v.clone());
    }
    // palette：逐 role deep merge（overlay 的 role 字段覆盖 base 的 role 字段）
    for (role, ov_role) in &overlay.palette {
        let base_role = base.palette.entry(role.clone()).or_default();
        if ov_role.fill.is_some() {
            base_role.fill = ov_role.fill.clone();
        }
        if ov_role.stroke.is_some() {
            base_role.stroke = ov_role.stroke.clone();
        }
        if ov_role.text_fill.is_some() {
            base_role.text_fill = ov_role.text_fill.clone();
        }
        if ov_role.edge_stroke.is_some() {
            base_role.edge_stroke = ov_role.edge_stroke.clone();
        }
    }
}

fn merge_defaults(base: &mut ElementDefaults, overlay: &ElementDefaults) {
    merge_block(&mut base.canvas, &overlay.canvas);
    merge_block(&mut base.title, &overlay.title);
    merge_block(&mut base.node, &overlay.node);
    merge_block(&mut base.edge, &overlay.edge);
    merge_block(&mut base.group, &overlay.group);
}

fn merge_diagrams(
    base: &mut std::collections::BTreeMap<String, DiagramStyles>,
    overlay: &std::collections::BTreeMap<String, DiagramStyles>,
) {
    for (key, ov) in overlay {
        let bv = base.entry(key.clone()).or_default();
        // node / edge / group / title：Option<StyleBlock>，deep merge
        merge_option_block(&mut bv.node, &ov.node);
        merge_option_block(&mut bv.edge, &ov.edge);
        merge_option_block(&mut bv.group, &ov.group);
        merge_option_block(&mut bv.title, &ov.title);
        // entity_types / edge_kinds：逐 key deep merge
        for (k, v) in &ov.entity_types {
            let base_block = bv.entity_types.entry(k.clone()).or_default();
            merge_block(base_block, v);
        }
        for (k, v) in &ov.edge_kinds {
            let base_block = bv.edge_kinds.entry(k.clone()).or_default();
            merge_block(base_block, v);
        }
        // branch_palettes / edge_depth_stroke_width：整段替换
        if !ov.branch_palettes.is_empty() {
            bv.branch_palettes = ov.branch_palettes.clone();
        }
        if !ov.edge_depth_stroke_width.is_empty() {
            bv.edge_depth_stroke_width = ov.edge_depth_stroke_width.clone();
        }
        // context_palettes：按 palette id deep merge（entries 整段替换）
        for (pid, ov_palette) in &ov.context_palettes {
            let base_palette = bv.context_palettes.entry(pid.clone()).or_default();
            if !ov_palette.entries.is_empty() {
                base_palette.entries = ov_palette.entries.clone();
            }
            // index：overlay 的字段覆盖 base 的字段
            if !ov_palette.index.from.is_empty() {
                base_palette.index.from = ov_palette.index.from.clone();
            }
            if ov_palette.index.wrap {
                base_palette.index.wrap = true;
            }
            if ov_palette.index.cap.is_some() {
                base_palette.index.cap = ov_palette.index.cap;
            }
            if !ov_palette.bindings.is_empty() {
                base_palette.bindings = ov_palette.bindings.clone();
            }
        }
    }
}

/// deep merge Option<StyleBlock>：overlay 的字段 insert 覆盖 base 的字段。
fn merge_option_block(base: &mut Option<StyleBlock>, overlay: &Option<StyleBlock>) {
    if let Some(ov) = overlay {
        let base_block = base.get_or_insert_with(Default::default);
        merge_block(base_block, ov);
    }
}

/// deep merge StyleBlock：overlay 的字段 insert 覆盖 base 的字段。
fn merge_block(base: &mut StyleBlock, overlay: &StyleBlock) {
    for (k, v) in overlay.iter() {
        base.insert(k.clone(), v.clone());
    }
}
