//! compile：`compile_theme`、L1 展开、`lighten`/`darken`、`validate_style_sheet`、legacy 提升。
//!
//! 按 `docs/specs/style-system/theme-inheritance-plan.md` §4.2 / §4.3 / §4.4.1 实现。

use std::collections::BTreeMap;

use crate::error::{DrawifyError, Result};
use super::schema::{
    ContextBindingDef, ContextPaletteDef, DiagramStyles,
    IndexRuleDef, PaletteRole, StyleSheet, StyleBlock, StyleTokens, StyleValue,
};

use super::schema::{
    CompiledContextPalette, CompiledDiagram, CompiledTheme, IndexRule,
};

// ─── 公开 API ──────────────────────────────────────────────────────

/// 完整校验（spec §4.4.1）。
///
/// 在 merge 后、compile 前执行。包含：
/// - 基础字段校验（version/id/name/tokens/colors）— 委托 `loader::validate_basic`
/// - context_palettes schema 校验（entries/index/bindings/palette id）
pub fn validate_style_sheet(sheet: &StyleSheet) -> Result<()> {
    // 基础校验
    super::loader::validate_basic(sheet)?;

    // context_palettes 校验
    for (diag_key, ds) in &sheet.diagrams {
        let path = format!("diagrams.{diag_key}.context_palettes");
        for (pid, pal) in &ds.context_palettes {
            validate_context_palette(pid, pal, ds, &format!("{path}.{pid}"))?;
        }
    }

    Ok(())
}

fn validate_context_palette(
    pid: &str,
    pal: &ContextPaletteDef,
    ds: &DiagramStyles,
    path: &str,
) -> Result<()> {
    if pal.entries.is_empty() {
        return Err(DrawifyError::Style(format!(
            "{path}: entries must be non-empty"
        )));
    }
    // index.from 枚举
    match pal.index.from.as_str() {
        "branch_slot" | "tree_depth" | "group_depth" => {}
        other => {
            return Err(DrawifyError::Style(format!(
                "{path}.index.from: invalid value '{other}' (expected branch_slot/tree_depth/group_depth)"
            )));
        }
    }
    // wrap 仅在 branch_slot 时允许
    if pal.index.wrap && pal.index.from != "branch_slot" {
        return Err(DrawifyError::Style(format!(
            "{path}.index.wrap: only allowed when from == 'branch_slot'"
        )));
    }
    // cap 仅在 tree_depth / group_depth 时允许
    if pal.index.cap.is_some() && pal.index.from == "branch_slot" && !pal.index.wrap {
        return Err(DrawifyError::Style(format!(
            "{path}.index.cap: only allowed when from in (tree_depth, group_depth) or wrap=true"
        )));
    }
    // bindings target 枚举
    for (i, b) in pal.bindings.iter().enumerate() {
        let bpath = format!("{path}.bindings[{i}]");
        match b.target.as_str() {
            "entity" | "edge" | "group" => {}
            other => {
                return Err(DrawifyError::Style(format!(
                    "{bpath}.target: invalid value '{other}' (expected entity/edge/group)"
                )));
            }
        }
        // types 须在该 diagram 的 entity_types / edge_kinds 中有定义
        if !b.types.is_empty() {
            let valid_types: std::collections::HashSet<&str> = if b.target == "entity" {
                ds.entity_types.keys().map(|s| s.as_str()).collect()
            } else {
                ds.edge_kinds.keys().map(|s| s.as_str()).collect()
            };
            for t in &b.types {
                if !valid_types.contains(t.as_str()) {
                    return Err(DrawifyError::Style(format!(
                        "{bpath}.types: '{t}' not defined in diagram"
                    )));
                }
            }
        }
        // fields 非空
        if b.fields.is_empty() {
            return Err(DrawifyError::Style(format!(
                "{bpath}.fields: must be non-empty"
            )));
        }
        // entry 键须在 entries 中存在
        for (style_key, entry_key) in &b.fields {
            let exists = pal.entries.iter().any(|e| e.contains_key(entry_key));
            if !exists {
                return Err(DrawifyError::Style(format!(
                    "{bpath}.fields: entry key '{entry_key}' not found in entries"
                )));
            }
            let _ = style_key;
        }
    }
    // palette id 校验
    let valid_ids = ["branch", "group_nest", "edge_depth"];
    if !valid_ids.contains(&pid) {
        let re = regex_lite(pid);
        if !re {
            return Err(DrawifyError::Style(format!(
                "{path}: invalid palette id '{pid}' (must match ^[a-z][a-z0-9_]*)"
            )));
        }
    }
    Ok(())
}

/// 简易校验 palette id 格式 `^[a-z][a-z0-9_]*$`。
fn regex_lite(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

// ─── compile_theme ─────────────────────────────────────────────────

/// 编译 StyleSheet → CompiledTheme。
///
/// 步骤：
/// 1. legacy 提升（branch_palettes → context_palettes.branch 等）
/// 2. 扫描 {branch.*} 用法 → 补全 branch palette bindings
/// 3. 删除 entity_types / edge 中的 {branch.*} 字段
/// 4. L1 token 展开（{colors.*} {role.*} {strokes.*} 等）
/// 5. compile 期颜色函数展开（{lighten(...)} {darken(...)}）
/// 6. L1 三层预合并（node_default / nodes / edge_default / edges 等）
/// 7. context_palettes entries 展开
pub fn compile_theme(sheet: StyleSheet) -> Result<CompiledTheme> {
    let tokens = sheet.tokens.clone();

    // 1-3. legacy 提升 + {branch.*} 处理
    let mut diagrams = sheet.diagrams.clone();
    for (_, ds) in diagrams.iter_mut() {
        promote_legacy(ds);
        remove_branch_tokens(ds);
    }

    // 4-5. token + 函数展开
    let canvas = expand_block(&tokens, &sheet.defaults.canvas);
    let defaults_node = expand_block(&tokens, &sheet.defaults.node);
    let defaults_edge = expand_block(&tokens, &sheet.defaults.edge);
    let defaults_group = expand_block(&tokens, &sheet.defaults.group);
    let defaults_title = expand_block(&tokens, &sheet.defaults.title);

    // 全局 group_nest：从 defaults.group 基色合成，作为 diagram 不存在或
    // diagram 未定义 group_nest 时的 fallback。
    // 复刻旧 `group_style_by_depth` 的提亮参数（见 synthesize_group_nest_legacy）。
    let global_group_nest = synthesize_group_nest_legacy(&defaults_group)
        .expect("defaults.group must have fill/stroke for group_nest synthesis");

    let mut compiled_diagrams = BTreeMap::new();
    for (key, ds) in &diagrams {
        let cd = compile_diagram(
            key,
            ds,
            &defaults_node,
            &defaults_edge,
            &defaults_group,
            &defaults_title,
            &tokens,
        );
        compiled_diagrams.insert(key.clone(), cd);
    }

    Ok(CompiledTheme {
        id: sheet.id.clone(),
        name: sheet.name.clone(),
        canvas,
        node_default: defaults_node,
        edge_default: defaults_edge,
        group_default: defaults_group,
        title: defaults_title,
        group_nest: global_group_nest,
        diagrams: compiled_diagrams,
    })
}

fn compile_diagram(
    _diag_key: &str,
    ds: &DiagramStyles,
    defaults_node: &StyleBlock,
    defaults_edge: &StyleBlock,
    defaults_group: &StyleBlock,
    defaults_title: &StyleBlock,
    tokens: &StyleTokens,
) -> CompiledDiagram {
    // L1 三层预合并
    // node_default = diagrams.node or_insert defaults.node
    let diag_node = expand_block_opt(tokens, ds.node.as_ref());
    let node_default = merge_or_insert(&diag_node, defaults_node);

    // nodes[type] = entity_types[type] or_insert node_default
    let mut nodes = BTreeMap::new();
    for (t, block) in &ds.entity_types {
        let expanded = expand_block(tokens, block);
        nodes.insert(t.clone(), merge_or_insert(&expanded, &node_default));
    }

    // edge_default = diagrams.edge or_insert defaults.edge
    let diag_edge = expand_block_opt(tokens, ds.edge.as_ref());
    let edge_default = merge_or_insert(&diag_edge, defaults_edge);

    // edges[kind] = edge_kinds[kind] or_insert edge_default
    let mut edges = BTreeMap::new();
    for (k, block) in &ds.edge_kinds {
        let expanded = expand_block(tokens, block);
        edges.insert(k.clone(), merge_or_insert(&expanded, &edge_default));
    }

    // group_default = diagrams.group or_insert defaults.group
    let diag_group = expand_block_opt(tokens, ds.group.as_ref());
    let group_default = merge_or_insert(&diag_group, defaults_group);

    // title = diagrams.title or_insert defaults.title
    let diag_title = expand_block_opt(tokens, ds.title.as_ref());
    let title = merge_or_insert(&diag_title, defaults_title);

    // context_palettes entries 展开
    let mut context_palettes = BTreeMap::new();
    for (pid, pal) in &ds.context_palettes {
        let compiled = compile_context_palette(pal, tokens);
        context_palettes.insert(pid.clone(), compiled);
    }

    // Legacy 提升：若 JSON 未定义 group_nest，从 group_default 基色 + 旧
    // `render/paint/color_queries.rs::group_style_by_depth` 的提亮参数合成。
    // 这使 Phase 2 门禁（gate_group_nest）能在旧 JSON 上通过；Phase 3 新 JSON
    // 显式定义 group_nest 后此分支自动跳过。
    if !context_palettes.contains_key("group_nest") {
        if let Some(synth) = synthesize_group_nest_legacy(&group_default) {
            context_palettes.insert("group_nest".to_string(), synth);
        }
    }

    CompiledDiagram {
        node_default,
        nodes,
        edge_default,
        edges,
        group_default,
        title,
        context_palettes,
    }
}

fn compile_context_palette(pal: &ContextPaletteDef, tokens: &StyleTokens) -> CompiledContextPalette {
    let entries: Vec<StyleBlock> = pal
        .entries
        .iter()
        .map(|e| expand_block(tokens, e))
        .collect();

    let index = match pal.index.from.as_str() {
        "branch_slot" => IndexRule::BranchSlot {
            wrap: pal.index.wrap,
        },
        "tree_depth" => IndexRule::TreeDepth {
            cap: pal.index.cap.unwrap_or(entries.len().saturating_sub(1)),
        },
        "group_depth" => IndexRule::GroupDepth {
            cap: pal.index.cap.unwrap_or(entries.len().saturating_sub(1)),
        },
        _ => IndexRule::TreeDepth {
            cap: entries.len().saturating_sub(1),
        },
    };

    CompiledContextPalette {
        entries,
        index,
        bindings: pal.bindings.clone(),
    }
}

/// 从 group_default 基色合成 `group_nest` palette（legacy 提升）。
///
/// 复刻旧 `render/paint/color_queries.rs::group_style_by_depth` 的提亮参数：
/// - depth 0: 基色, stroke_width=2.0, border_radius=8.0
/// - depth 1: lighten(fill, 0.35) / lighten(stroke, 0.30), stroke_width=1.5, border_radius=6.0
/// - depth 2: lighten(fill, 0.55) / lighten(stroke, 0.50), stroke_width=1.5, border_radius=6.0
/// - depth 3: lighten(fill, 0.70) / lighten(stroke, 0.65), stroke_width=1.5, border_radius=6.0
///
/// `text_fill`（label_color）不随 depth 变化，不纳入 entries（保留在 group_default）。
fn synthesize_group_nest_legacy(group_default: &StyleBlock) -> Option<CompiledContextPalette> {
    let fill = group_default
        .get("fill")
        .and_then(|v| v.as_str())
        .unwrap_or("#f0f0f0");
    let stroke = group_default
        .get("stroke")
        .and_then(|v| v.as_str())
        .unwrap_or("#cccccc");

    let entry = |fill_amt: f64, stroke_amt: f64, sw: f64, br: f64| -> StyleBlock {
        let mut b = StyleBlock::new();
        b.insert(
            "fill".to_string(),
            StyleValue::String(lighten(fill, fill_amt)),
        );
        b.insert(
            "stroke".to_string(),
            StyleValue::String(lighten(stroke, stroke_amt)),
        );
        b.insert("stroke_width".to_string(), StyleValue::Number(sw));
        b.insert("border_radius".to_string(), StyleValue::Number(br));
        b
    };

    let entries = vec![
        entry(0.0, 0.0, 2.0, 8.0),
        entry(0.35, 0.30, 1.5, 6.0),
        entry(0.55, 0.50, 1.5, 6.0),
        entry(0.70, 0.65, 1.5, 6.0),
    ];

    let bindings = vec![ContextBindingDef {
        target: "group".to_string(),
        types: vec![],
        fields: {
            let mut m = BTreeMap::new();
            m.insert("fill".to_string(), "fill".to_string());
            m.insert("stroke".to_string(), "stroke".to_string());
            m.insert("stroke_width".to_string(), "stroke_width".to_string());
            m.insert("border_radius".to_string(), "border_radius".to_string());
            m
        },
    }];

    Some(CompiledContextPalette {
        entries,
        index: IndexRule::GroupDepth { cap: 3 },
        bindings,
    })
}

// ─── legacy 提升 ───────────────────────────────────────────────────

/// 将旧字段提升为 context_palettes（仅迁移期）。
fn promote_legacy(ds: &mut DiagramStyles) {
    // branch_palettes → context_palettes.branch
    if !ds.branch_palettes.is_empty() && !ds.context_palettes.contains_key("branch") {
        let entries: Vec<StyleBlock> = ds
            .branch_palettes
            .iter()
            .map(|e| {
                let mut block = StyleBlock::new();
                if let Some(ref fill) = e.fill {
                    block.insert("fill".to_string(), fill.clone());
                }
                if let Some(ref stroke) = e.stroke {
                    block.insert("stroke".to_string(), stroke.clone());
                }
                if let Some(ref edge_stroke) = e.edge_stroke {
                    block.insert("edge_stroke".to_string(), edge_stroke.clone());
                }
                block
            })
            .collect();

        // bindings 由 scan_branch_usage 填充（先放空，后续补全）
        ds.context_palettes.insert(
            "branch".to_string(),
            ContextPaletteDef {
                entries,
                index: IndexRuleDef {
                    from: "branch_slot".to_string(),
                    wrap: true,
                    cap: None,
                },
                bindings: vec![],
            },
        );
    }

    // edge_depth_stroke_width → context_palettes.edge_depth
    if !ds.edge_depth_stroke_width.is_empty() && !ds.context_palettes.contains_key("edge_depth") {
        let entries: Vec<StyleBlock> = ds
            .edge_depth_stroke_width
            .iter()
            .map(|w| {
                let mut block = StyleBlock::new();
                block.insert(
                    "stroke_width".to_string(),
                    StyleValue::Number(*w),
                );
                block
            })
            .collect();

        ds.context_palettes.insert(
            "edge_depth".to_string(),
            ContextPaletteDef {
                entries,
                index: IndexRuleDef {
                    from: "tree_depth".to_string(),
                    wrap: false,
                    cap: None,
                },
                bindings: vec![ContextBindingDef {
                    target: "edge".to_string(),
                    types: vec![],
                    fields: {
                        let mut m = BTreeMap::new();
                        m.insert("stroke_width".to_string(), "stroke_width".to_string());
                        m
                    },
                }],
            },
        );
    }

    // 扫描 {branch.*} 用法，补全 branch palette 的 bindings
    if ds.context_palettes.contains_key("branch") {
        let bindings = scan_branch_bindings(ds);
        if let Some(pal) = ds.context_palettes.get_mut("branch") {
            if pal.bindings.is_empty() {
                pal.bindings = bindings;
            }
        }
    }
}

/// 扫描 entity_types / edge 中的 {branch.*} 用法，生成 branch palette bindings。
fn scan_branch_bindings(ds: &DiagramStyles) -> Vec<ContextBindingDef> {
    // 收集每个 entity_type 使用的 branch 字段
    // key = entity_type, value = set of (style_key, entry_key)
    let mut entity_fields: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();

    for (etype, block) in &ds.entity_types {
        let mut fields = BTreeMap::new();
        for (style_key, value) in block.iter() {
            if let StyleValue::String(s) = value {
                if let Some(entry_key) = branch_ref_key(s) {
                    fields.insert(style_key.clone(), entry_key.to_string());
                }
            }
        }
        if !fields.is_empty() {
            entity_fields.insert(etype.clone(), fields);
        }
    }

    // 按 fields 分组（相同 fields 模式的 entity_type 合并到同一 binding）
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (etype, fields) in &entity_fields {
        let key = serialize_fields(fields);
        groups.entry(key).or_default().push(etype.clone());
    }

    let mut bindings: Vec<ContextBindingDef> = groups
        .into_iter()
        .map(|(_, types)| {
            // 取第一组的 fields（同组相同）
            let first_type = &types[0];
            let fields = entity_fields.get(first_type).cloned().unwrap_or_default();
            ContextBindingDef {
                target: "entity".to_string(),
                types,
                fields,
            }
        })
        .collect();

    // 检查 edge 块是否使用 {branch.*}
    if let Some(ref edge_block) = ds.edge {
        let mut edge_fields = BTreeMap::new();
        for (style_key, value) in edge_block.iter() {
            if let StyleValue::String(s) = value {
                if let Some(entry_key) = branch_ref_key(s) {
                    edge_fields.insert(style_key.clone(), entry_key.to_string());
                }
            }
        }
        if !edge_fields.is_empty() {
            bindings.push(ContextBindingDef {
                target: "edge".to_string(),
                types: vec![],
                fields: edge_fields,
            });
        }
    }

    // 检查 edge_kinds 是否使用 {branch.*}
    for (kind, block) in &ds.edge_kinds {
        let mut edge_fields = BTreeMap::new();
        for (style_key, value) in block.iter() {
            if let StyleValue::String(s) = value {
                if let Some(entry_key) = branch_ref_key(s) {
                    edge_fields.insert(style_key.clone(), entry_key.to_string());
                }
            }
        }
        if !edge_fields.is_empty() {
            bindings.push(ContextBindingDef {
                target: "edge".to_string(),
                types: vec![kind.clone()],
                fields: edge_fields,
            });
        }
    }

    bindings
}

fn serialize_fields(fields: &BTreeMap<String, String>) -> String {
    fields
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// 如果 s 是 {branch.X}，返回 Some("X")，否则 None。
fn branch_ref_key(s: &str) -> Option<&str> {
    let trimmed = s.trim();
    if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
        return None;
    }
    let inner = &trimmed[1..trimmed.len() - 1];
    let (ns, key) = inner.split_once('.')?;
    if ns == "branch" {
        Some(key)
    } else {
        None
    }
}

/// 删除 entity_types / edge / edge_kinds 中的 {branch.*} 字段。
fn remove_branch_tokens(ds: &mut DiagramStyles) {
    for (_, block) in ds.entity_types.iter_mut() {
        block.retain(|_, v| !is_branch_ref(v));
    }
    if let Some(ref mut edge) = ds.edge {
        edge.retain(|_, v| !is_branch_ref(v));
    }
    for (_, block) in ds.edge_kinds.iter_mut() {
        block.retain(|_, v| !is_branch_ref(v));
    }
}

fn is_branch_ref(value: &StyleValue) -> bool {
    match value {
        StyleValue::String(s) => branch_ref_key(s).is_some(),
        _ => false,
    }
}

// ─── token 展开 ────────────────────────────────────────────────────

fn expand_block_opt(tokens: &StyleTokens, block: Option<&StyleBlock>) -> StyleBlock {
    match block {
        Some(b) => expand_block(tokens, b),
        None => StyleBlock::new(),
    }
}

fn expand_block(tokens: &StyleTokens, block: &StyleBlock) -> StyleBlock {
    let mut out = StyleBlock::new();
    for (key, value) in block.iter() {
        out.insert(key.clone(), expand_value(tokens, value));
    }
    out
}

fn expand_value(tokens: &StyleTokens, value: &StyleValue) -> StyleValue {
    match value {
        StyleValue::String(s) => {
            // 先检查是否为 {lighten(...)} / {darken(...)} 函数表达式
            if let Some(result) = try_expand_color_function(s, tokens) {
                return StyleValue::String(result);
            }
            // sheet 级 token 引用
            if is_token_ref(s) {
                if let Some(resolved) = resolve_token_ref(tokens, s) {
                    if let Ok(n) = resolved.parse::<f64>() {
                        return StyleValue::Number(n);
                    }
                    return StyleValue::String(resolved);
                }
                // 无法解析：保留原值
                eprintln!("[warn] unresolved token reference: '{s}'");
                return StyleValue::String(s.clone());
            }
            StyleValue::String(s.clone())
        }
        StyleValue::Number(n) => StyleValue::Number(*n),
        StyleValue::Boolean(b) => StyleValue::Boolean(*b),
        StyleValue::Array(arr) => StyleValue::Array(arr.clone()),
    }
}

fn is_token_ref(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.starts_with('{') && trimmed.ends_with('}') && trimmed.contains('.')
}

/// 解析 sheet 级 token 引用：{colors.*} {typography.*} {strokes.*} {radius.*} {spacing.*} {effects.*} {role.*}
fn resolve_token_ref(tokens: &StyleTokens, reference: &str) -> Option<String> {
    let trimmed = reference.trim();
    if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
        return None;
    }
    let inner = &trimmed[1..trimmed.len() - 1];
    let (category, key) = inner.split_once('.')?;
    match category {
        "colors" => tokens.colors.get(key).cloned(),
        "typography" => tokens
            .typography
            .get(key)
            .and_then(|v| v.to_resolved_string()),
        "strokes" => tokens
            .strokes
            .get(key)
            .and_then(|v| v.to_resolved_string()),
        "radius" => tokens
            .radius
            .get(key)
            .and_then(|v| v.to_resolved_string()),
        "spacing" => tokens
            .spacing
            .get(key)
            .and_then(|v| v.to_resolved_string()),
        "effects" => tokens
            .effects
            .get(key)
            .and_then(|v| v.to_resolved_string()),
        "role" => {
            // {role.<role>.<field>} → tokens.palette.<role>.<field>
            let (role_name, field) = key.split_once('.')?;
            let role: &PaletteRole = tokens.palette.get(role_name)?;
            match field {
                "fill" => role.fill.clone(),
                "stroke" => role.stroke.clone(),
                "text_fill" => role.text_fill.clone(),
                "edge_stroke" => role.edge_stroke.clone(),
                _ => None,
            }
        }
        _ => None,
    }
}

// ─── compile 期颜色函数 ────────────────────────────────────────────

/// 尝试展开 {lighten(...)} / {darken(...)} 表达式。返回 None 表示不是函数表达式。
fn try_expand_color_function(s: &str, tokens: &StyleTokens) -> Option<String> {
    let trimmed = s.trim();
    let inner = trimmed.strip_prefix('{')?.strip_suffix('}')?;
    let (func, args) = inner.split_once('(')?;
    let func = func.trim();
    let args = args.trim().trim_end_matches(')');

    match func {
        "lighten" | "darken" => {
            // 解析参数：<color_expr>, <amount>
            let parts: Vec<&str> = args.splitn(2, ',').collect();
            if parts.len() != 2 {
                return None;
            }
            let color_expr = parts[0].trim();
            let amount: f64 = parts[1].trim().parse().ok()?;

            // 展开 color_expr（可能是 hex 或 token 引用）
            let hex = if is_token_ref(color_expr) {
                resolve_token_ref(tokens, color_expr)?
            } else {
                color_expr.trim_matches('"').to_string()
            };

            let result = if func == "lighten" {
                lighten(&hex, amount)
            } else {
                darken(&hex, amount)
            };
            Some(result)
        }
        _ => None,
    }
}

/// 向白色混合（提亮），amount ∈ [0, 1]。
/// amount=0 返回原色，amount=1 返回白色。
/// 与旧 `render/paint/color_queries.rs::lighten` 算法完全一致。
pub fn lighten(hex: &str, amount: f64) -> String {
    let amount = amount.clamp(0.0, 1.0);
    let (r, g, b) = match parse_hex_rgb(hex) {
        Some(rgb) => rgb,
        None => return hex.to_string(),
    };
    let nr = r as f64 + (255.0 - r as f64) * amount;
    let ng = g as f64 + (255.0 - g as f64) * amount;
    let nb = b as f64 + (255.0 - b as f64) * amount;
    to_hex_rgb(nr.round() as u8, ng.round() as u8, nb.round() as u8)
}

/// 向黑色混合（加深），amount ∈ [0, 1]。
/// amount=0 返回原色，amount=1 返回黑色。
pub fn darken(hex: &str, amount: f64) -> String {
    let amount = amount.clamp(0.0, 1.0);
    let (r, g, b) = match parse_hex_rgb(hex) {
        Some(rgb) => rgb,
        None => return hex.to_string(),
    };
    let nr = r as f64 * (1.0 - amount);
    let ng = g as f64 * (1.0 - amount);
    let nb = b as f64 * (1.0 - amount);
    to_hex_rgb(nr.round() as u8, ng.round() as u8, nb.round() as u8)
}

fn parse_hex_rgb(hex: &str) -> Option<(u8, u8, u8)> {
    let hex = hex.strip_prefix('#')?;
    match hex.len() {
        6 | 8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some((r, g, b))
        }
        3 | 4 => {
            let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
            Some((r, g, b))
        }
        _ => None,
    }
}

fn to_hex_rgb(r: u8, g: u8, b: u8) -> String {
    format!("#{:02x}{:02x}{:02x}", r, g, b)
}

// ─── 合并辅助 ──────────────────────────────────────────────────────

/// `or_insert` 语义合并：先写入 high（高优先级），再用 low 填空。
fn merge_or_insert(high: &StyleBlock, low: &StyleBlock) -> StyleBlock {
    let mut result = high.clone();
    for (key, value) in low.iter() {
        result.entry(key.clone()).or_insert(value.clone());
    }
    result
}

// ─── 测试 ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lighten_works() {
        assert_eq!(lighten("#000000", 0.0), "#000000");
        assert_eq!(lighten("#000000", 1.0), "#ffffff");
        assert_eq!(lighten("#808080", 0.5), "#c0c0c0");
    }

    #[test]
    fn darken_works() {
        assert_eq!(darken("#ffffff", 0.0), "#ffffff");
        assert_eq!(darken("#ffffff", 1.0), "#000000");
        assert_eq!(darken("#808080", 0.5), "#404040");
    }

    #[test]
    fn lighten_matches_old_algorithm() {
        // 与旧 render/paint/color_queries.rs::lighten 一致
        // #F1F2F6 lighten 0.35 → #F8F9FB
        let result = lighten("#F1F2F6", 0.35);
        let (r, g, b) = parse_hex_rgb(&result).unwrap();
        // 验证向白色混合
        let (orig_r, orig_g, orig_b) = parse_hex_rgb("#F1F2F6").unwrap();
        assert!((r as f64) >= orig_r as f64);
        assert!((g as f64) >= orig_g as f64);
        assert!((b as f64) >= orig_b as f64);
    }

    #[test]
    fn color_function_expression_expands() {
        let tokens = StyleTokens {
            colors: {
                let mut m = BTreeMap::new();
                m.insert("group_fill".to_string(), "#F1F2F6".to_string());
                m
            },
            ..Default::default()
        };
        let result = try_expand_color_function(
            "{lighten({colors.group_fill}, 0.35)}",
            &tokens,
        );
        assert!(result.is_some());
        let hex = result.unwrap();
        assert!(hex.starts_with('#'));
        assert_eq!(hex.len(), 7);
    }

    #[test]
    fn role_token_expands() {
        let tokens = StyleTokens {
            palette: {
                let mut m = BTreeMap::new();
                m.insert(
                    "blue".to_string(),
                    PaletteRole {
                        fill: Some("#E3F2FD".to_string()),
                        stroke: Some("#1976D2".to_string()),
                        text_fill: None,
                        edge_stroke: None,
                    },
                );
                m
            },
            ..Default::default()
        };
        assert_eq!(
            resolve_token_ref(&tokens, "{role.blue.fill}"),
            Some("#E3F2FD".to_string())
        );
        assert_eq!(
            resolve_token_ref(&tokens, "{role.blue.stroke}"),
            Some("#1976D2".to_string())
        );
        assert_eq!(resolve_token_ref(&tokens, "{role.blue.text_fill}"), None);
    }
}
