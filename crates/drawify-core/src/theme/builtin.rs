//! theme builtin：内置主题加载 + `CompiledTheme` 进程缓存。
//!
//! 复用 `themes/*.json` 作为 JSON 真源（include_str!）。
//! 按 §4.3.4 加载顺序实现。

use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

use super::loader::parse_style_sheet_json;
use super::schema::StyleSheet;

use super::compile::{compile_theme, validate_style_sheet};
use super::merge::merge_style_sheets;
use super::schema::CompiledTheme;

// ─── 内置主题 ID 列表 ──────────────────────────────────────────────

/// 所有通用主题 ID（覆盖全部图表类型）。
pub const COMMON_THEME_IDS: &[&str] = &[
    "common.clean-light",
    "common.clean-dark",
    "common.blueprint",
    "common.presentation",
    "common.minimal-gray",
    "common.brand-vivid",
    "common.catppuccin-latte",
    "common.catppuccin-mocha",
    "common.dracula",
    "common.github-dark",
    "common.github-light",
    "common.gruvbox-dark",
    "common.monokai",
    "common.nord",
    "common.one-dark",
    "common.rose-pine",
    "common.solarized-light",
    "common.tokyo-night",
    "common.ibm-carbon",
    "common.okabe-ito",
    "common.tol-bright",
    "common.tol-high-contrast",
];

/// 所有 mindmap 专用主题 ID（仅覆盖 mindmap 图表类型）。
pub const MINDMAP_THEME_IDS: &[&str] = &[
    "mindmap.vivid-branches",
    "mindmap.pastel-soft",
    "mindmap.ink-dark",
];

/// 所有用户可见主题 ID（通用 + mindmap 专用，共 25 个）。
pub fn all_theme_ids() -> Vec<&'static str> {
    COMMON_THEME_IDS
        .iter()
        .chain(MINDMAP_THEME_IDS.iter())
        .copied()
        .collect()
}

/// 内部基座 ID（不在 all_theme_ids 中，仅作 extends 用）。
pub const INTERNAL_BASE_IDS: &[&str] = &["mindmap.base"];

// ─── 原始 StyleSheet 加载 ──────────────────────────────────────────

/// 按 ID 加载内置主题的原始 StyleSheet（含 extends 字段）。
fn builtin_style_sheet(id: &str) -> Option<StyleSheet> {
    let json = match id {
        // 用户可见主题（themes/*.json）
        "common.clean-light" => include_str!("themes/common.clean-light.json"),
        "common.clean-dark" => include_str!("themes/common.clean-dark.json"),
        "common.blueprint" => include_str!("themes/common.blueprint.json"),
        "common.presentation" => include_str!("themes/common.presentation.json"),
        "common.minimal-gray" => include_str!("themes/common.minimal-gray.json"),
        "common.brand-vivid" => include_str!("themes/common.brand-vivid.json"),
        "common.catppuccin-latte" => include_str!("themes/common.catppuccin-latte.json"),
        "common.catppuccin-mocha" => include_str!("themes/common.catppuccin-mocha.json"),
        "common.dracula" => include_str!("themes/common.dracula.json"),
        "common.github-dark" => include_str!("themes/common.github-dark.json"),
        "common.github-light" => include_str!("themes/common.github-light.json"),
        "common.gruvbox-dark" => include_str!("themes/common.gruvbox-dark.json"),
        "common.monokai" => include_str!("themes/common.monokai.json"),
        "common.nord" => include_str!("themes/common.nord.json"),
        "common.one-dark" => include_str!("themes/common.one-dark.json"),
        "common.rose-pine" => include_str!("themes/common.rose-pine.json"),
        "common.solarized-light" => include_str!("themes/common.solarized-light.json"),
        "common.tokyo-night" => include_str!("themes/common.tokyo-night.json"),
        "common.ibm-carbon" => include_str!("themes/common.ibm-carbon.json"),
        "common.okabe-ito" => include_str!("themes/common.okabe-ito.json"),
        "common.tol-bright" => include_str!("themes/common.tol-bright.json"),
        "common.tol-high-contrast" => include_str!("themes/common.tol-high-contrast.json"),
        "mindmap.vivid-branches" => include_str!("themes/mindmap.vivid-branches.json"),
        "mindmap.pastel-soft" => include_str!("themes/mindmap.pastel-soft.json"),
        "mindmap.ink-dark" => include_str!("themes/mindmap.ink-dark.json"),
        // 内部基座（mindmap.base，仅作 extends 用）
        "mindmap.base" => include_str!("themes/mindmap.base.json"),
        _ => return None,
    };
    match parse_style_sheet_json(json) {
        Ok(sheet) => Some(sheet),
        Err(err) => {
            eprintln!("[error] failed to load builtin style sheet '{id}': {err}");
            None
        }
    }
}

/// 加载基座 StyleSheet（用于 extends merge）。
fn base_style_sheet(theme_id: &str) -> Option<StyleSheet> {
    builtin_style_sheet(theme_id)
}

// ─── compile + 缓存 ────────────────────────────────────────────────

/// 进程内缓存：`theme_id` → `CompiledTheme`。
static COMPILED_BUILTIN_CACHE: LazyLock<RwLock<HashMap<String, CompiledTheme>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// 按 ID 返回编译后的内置主题。
///
/// 加载顺序（§4.3.4）：
/// 1. load overlay
/// 2. if extends: merge(base, overlay) else sheet = overlay
/// 3. sheet.id = overlay.id
/// 4. validate_style_sheet(sheet)
/// 5. compile → CompiledTheme
/// 6. cache
pub fn compiled_builtin_theme(id: &str) -> Option<CompiledTheme> {
    // 快路径：读缓存
    if let Ok(cache) = COMPILED_BUILTIN_CACHE.read() {
        if let Some(compiled) = cache.get(id) {
            return Some(compiled.clone());
        }
    }

    // 慢路径：load + merge + validate + compile
    let overlay = builtin_style_sheet(id)?;

    let sheet = if let Some(ref base_id) = overlay.extends {
        let base = base_style_sheet(base_id)?;
        // extends 单层校验（§4.4.1）：基座不得有 extends
        if base.extends.is_some() {
            eprintln!(
                "[error] theme '{id}' extends '{base_id}' which itself has extends (chain not allowed)"
            );
            return None;
        }
        match merge_style_sheets(&base, &overlay) {
            Ok(merged) => merged,
            Err(err) => {
                eprintln!("[error] failed to merge '{id}' extends '{base_id}': {err}");
                return None;
            }
        }
    } else {
        overlay
    };

    // 完整校验（§4.4.1）：基础字段 + context_palettes schema
    if let Err(err) = validate_style_sheet(&sheet) {
        eprintln!("[error] validation failed for builtin theme '{id}': {err}");
        return None;
    }

    match compile_theme(sheet) {
        Ok(compiled) => {
            if let Ok(mut cache) = COMPILED_BUILTIN_CACHE.write() {
                cache
                    .entry(id.to_string())
                    .or_insert_with(|| compiled.clone());
            }
            Some(compiled)
        }
        Err(err) => {
            eprintln!("[error] failed to compile builtin theme '{id}': {err}");
            None
        }
    }
}

// ─── 测试 ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_builtin_themes_compile() {
        for id in all_theme_ids() {
            let compiled = compiled_builtin_theme(id)
                .unwrap_or_else(|| panic!("failed to compile builtin theme '{id}'"));
            assert_eq!(compiled.id, id);
            assert!(!compiled.diagrams.is_empty(), "{id} has no diagrams");
        }
    }

    #[test]
    fn compiled_cache_is_idempotent() {
        for id in ["common.clean-light", "common.nord", "mindmap.vivid-branches"] {
            let first = compiled_builtin_theme(id).unwrap();
            let second = compiled_builtin_theme(id).unwrap();
            assert_eq!(first.id, second.id);
        }
    }
}
