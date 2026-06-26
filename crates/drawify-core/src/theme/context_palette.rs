//! context_palette：`InstanceContext`、`apply_context_palettes`、`materialize_*`。
//!
//! 按 §4.2.3 物化算法实现。Phase 2 核心。

use super::schema::{ContextBindingDef, StyleBlock};

use super::schema::{
    CompiledContextPalette, CompiledDiagram, CompiledTheme, IndexRule, InstanceContext,
};

// ─── L2 物化 API ───────────────────────────────────────────────────

/// 物化节点样式：L1 类型级块 + L2 context_palettes overlay。
pub fn materialize_node(
    compiled: &CompiledTheme,
    diagram: &str,
    entity_type: Option<&str>,
    ctx: &InstanceContext,
) -> StyleBlock {
    let base = compiled.node_block(diagram, entity_type).clone();
    apply_context_palettes(compiled, diagram, "entity", entity_type, ctx, base)
}

/// 物化边样式：L1 类型级块 + L2 context_palettes overlay。
pub fn materialize_edge(
    compiled: &CompiledTheme,
    diagram: &str,
    edge_kind: Option<&str>,
    ctx: &InstanceContext,
) -> StyleBlock {
    let base = compiled.edge_block(diagram, edge_kind).clone();
    apply_context_palettes(compiled, diagram, "edge", edge_kind, ctx, base)
}

/// 物化 group 样式：L1 group_default + L2 context_palettes overlay（group_nest）。
pub fn materialize_group(
    compiled: &CompiledTheme,
    diagram: &str,
    ctx: &InstanceContext,
) -> StyleBlock {
    let base = compiled.group_block(diagram).clone();
    apply_context_palettes(compiled, diagram, "group", None, ctx, base)
}

/// 对 base 应用所有 matching context_palettes（按 palette id 字典序）。
///
/// 物化算法（§4.2.3）：
/// 1. for each context_palette on this diagram (按 palette id 字典序):
///      for each binding matching (target, type):
///        if binding applies to this instance:
///          entry = palette.entries[index(ctx)]
///          overlay = project(entry, binding.fields)
///          base = merge_overlay(base, overlay)   // palette 强制覆盖指定字段
fn apply_context_palettes(
    compiled: &CompiledTheme,
    diagram: &str,
    target: &str,
    type_filter: Option<&str>,
    ctx: &InstanceContext,
    mut base: StyleBlock,
) -> StyleBlock {
    let diag: Option<&CompiledDiagram> = compiled.diagrams.get(diagram);

    // 按 palette id 字典序（BTreeMap 天然有序）应用 per-diagram palettes
    if let Some(diag) = diag {
        for (pid, pal) in &diag.context_palettes {
            let _ = pid;
            apply_single_palette(&mut base, pal, target, type_filter, ctx);
        }
    }

    // 全局 fallback：group_nest
    // 当 diagram 不存在或 diagram 未定义 group_nest 时，使用全局 group_nest
    // （从 defaults.group 合成），保证旧 group_style_by_depth 行为在所有
    // diagram type 上等价。
    let diag_has_group_nest = diag
        .map(|d| d.context_palettes.contains_key("group_nest"))
        .unwrap_or(false);
    if !diag_has_group_nest && target == "group" {
        apply_single_palette(&mut base, &compiled.group_nest, target, type_filter, ctx);
    }

    base
}

/// 对单个 palette 应用 matching bindings 到 base。
fn apply_single_palette(
    base: &mut StyleBlock,
    pal: &CompiledContextPalette,
    target: &str,
    type_filter: Option<&str>,
    ctx: &InstanceContext,
) {
    for binding in &pal.bindings {
        if !binding_matches(binding, target, type_filter) {
            continue;
        }
        let Some(entry_index) = compute_index(pal, ctx) else {
            continue;
        };
        let Some(entry) = pal.entries.get(entry_index) else {
            continue;
        };
        // overlay：对 fields 列出的键使用 insert（强制覆盖）
        for (style_key, entry_key) in &binding.fields {
            if let Some(value) = entry.get(entry_key) {
                base.insert(style_key.clone(), value.clone());
            }
        }
    }
}

fn binding_matches(binding: &ContextBindingDef, target: &str, type_filter: Option<&str>) -> bool {
    if binding.target != target {
        return false;
    }
    if binding.types.is_empty() {
        return true;
    }
    match type_filter {
        Some(t) => binding.types.iter().any(|bt| bt == t),
        None => false,
    }
}

/// 按 IndexRule 计算下标。
fn compute_index(pal: &CompiledContextPalette, ctx: &InstanceContext) -> Option<usize> {
    if pal.entries.is_empty() {
        return None;
    }
    let len = pal.entries.len();
    match &pal.index {
        IndexRule::BranchSlot { wrap } => {
            let raw = ctx.branch_slot.unwrap_or(0);
            if *wrap {
                Some(raw % len)
            } else {
                Some(raw.min(len.saturating_sub(1)))
            }
        }
        IndexRule::TreeDepth { cap } => {
            let raw = ctx.tree_depth.unwrap_or(0);
            Some(raw.min(*cap))
        }
        IndexRule::GroupDepth { cap } => {
            let raw = ctx.group_depth.unwrap_or(0);
            Some(raw.min(*cap))
        }
    }
}

// ─── 边的 InstanceContext 派生 ─────────────────────────────────────

/// 派生边的 `InstanceContext`。
///
/// 按 §4.2.3：
/// - `branch_slot`：from 为 root → to.slot（回退 from.slot）；其他 → from.slot（回退 to.slot）
/// - `tree_depth`：始终取 to.depth（回退 from.depth）
pub fn derive_edge_context(
    from_slot: Option<usize>,
    from_depth: Option<usize>,
    to_slot: Option<usize>,
    to_depth: Option<usize>,
    from_is_root: bool,
) -> InstanceContext {
    let branch_slot = if from_is_root {
        to_slot.or(from_slot)
    } else {
        from_slot.or(to_slot)
    };
    let tree_depth = to_depth.or(from_depth);
    InstanceContext {
        branch_slot,
        tree_depth,
        group_depth: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_context_root_uses_to_slot() {
        let ctx = derive_edge_context(Some(0), Some(0), Some(2), Some(1), true);
        assert_eq!(ctx.branch_slot, Some(2));
        assert_eq!(ctx.tree_depth, Some(1));
    }

    #[test]
    fn edge_context_non_root_uses_from_slot() {
        let ctx = derive_edge_context(Some(3), Some(0), Some(2), Some(1), false);
        assert_eq!(ctx.branch_slot, Some(3));
        assert_eq!(ctx.tree_depth, Some(1));
    }

    #[test]
    fn edge_context_tree_depth_always_to() {
        // 即使 from 是 root，tree_depth 仍取 to
        let ctx = derive_edge_context(Some(0), Some(5), Some(2), Some(1), true);
        assert_eq!(ctx.tree_depth, Some(1));
    }
}
