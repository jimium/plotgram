//! Theme compilation: parse JSON, resolve token references, produce CompiledTheme.
//!
//! Supports `extends` inheritance: child themes override parent fields shallowly.

use std::collections::BTreeMap;

use super::schema::{StyleValue, ThemeFile};
use super::{
    CompiledDefaults, CompiledTheme, EdgeDefaults, GroupDefaults, KindStyle, Typography,
};

/// Compile a theme, resolving `extends` inheritance chain.
///
/// `resolver` maps a theme id to its JSON string (e.g. embedded_theme).
pub fn compile_theme(json: &str, resolver: &dyn Fn(&str) -> Option<&'static str>) -> CompiledTheme {
    let file: ThemeFile =
        serde_json::from_str(json).unwrap_or_else(|e| panic!("theme parse error: {e}"));

    // Resolve extends chain
    let merged = if let Some(parent_id) = &file.extends {
        let parent_json = resolver(parent_id)
            .unwrap_or_else(|| panic!("parent theme '{parent_id}' not found"));
        let parent: ThemeFile = serde_json::from_str(parent_json)
            .unwrap_or_else(|e| panic!("parent theme parse error: {e}"));
        // Recursively resolve parent's extends
        let resolved_parent = resolve_extends(parent, resolver);
        merge_theme_files(resolved_parent, file)
    } else {
        file
    };

    compile_resolved(merged)
}

/// Recursively resolve a ThemeFile's extends chain.
fn resolve_extends(
    file: ThemeFile,
    resolver: &dyn Fn(&str) -> Option<&'static str>,
) -> ThemeFile {
    if let Some(parent_id) = &file.extends {
        let parent_json = resolver(parent_id)
            .unwrap_or_else(|| panic!("parent theme '{parent_id}' not found"));
        let parent: ThemeFile = serde_json::from_str(parent_json)
            .unwrap_or_else(|e| panic!("parent theme parse error: {e}"));
        let resolved_parent = resolve_extends(parent, resolver);
        merge_theme_files(resolved_parent, file)
    } else {
        file
    }
}

/// Merge child on top of parent (child wins on conflicts).
fn merge_theme_files(parent: ThemeFile, child: ThemeFile) -> ThemeFile {
    ThemeFile {
        id: child.id,
        name: child.name,
        extends: None, // resolved
        tokens: super::schema::Tokens {
            colors: merge_maps(parent.tokens.colors, child.tokens.colors),
            palette: merge_nested_maps(parent.tokens.palette, child.tokens.palette),
            typography: merge_maps(parent.tokens.typography, child.tokens.typography),
            strokes: merge_maps(parent.tokens.strokes, child.tokens.strokes),
            radius: merge_maps(parent.tokens.radius, child.tokens.radius),
            spacing: merge_maps(parent.tokens.spacing, child.tokens.spacing),
        },
        defaults: super::schema::Defaults {
            canvas: merge_maps(parent.defaults.canvas, child.defaults.canvas),
            title: merge_maps(parent.defaults.title, child.defaults.title),
            node: merge_maps(parent.defaults.node, child.defaults.node),
            edge: merge_maps(parent.defaults.edge, child.defaults.edge),
            group: merge_maps(parent.defaults.group, child.defaults.group),
        },
        kind_styles: merge_nested_maps(parent.kind_styles, child.kind_styles),
    }
}

fn merge_maps(
    mut parent: BTreeMap<String, StyleValue>,
    child: BTreeMap<String, StyleValue>,
) -> BTreeMap<String, StyleValue> {
    for (k, v) in child {
        parent.insert(k, v);
    }
    parent
}

fn merge_nested_maps(
    mut parent: BTreeMap<String, BTreeMap<String, StyleValue>>,
    child: BTreeMap<String, BTreeMap<String, StyleValue>>,
) -> BTreeMap<String, BTreeMap<String, StyleValue>> {
    for (k, v) in child {
        match parent.get_mut(&k) {
            Some(existing) => {
                for (ik, iv) in v {
                    existing.insert(ik, iv);
                }
            }
            None => {
                parent.insert(k, v);
            }
        }
    }
    parent
}

/// Compile a fully-resolved ThemeFile (no extends) into CompiledTheme.
fn compile_resolved(file: ThemeFile) -> CompiledTheme {
    // Build flat token lookup: "colors.canvas" → "#F7F7F8"
    let token_map = build_token_map(&file);

    // Resolve defaults
    let defaults = compile_defaults(&file, &token_map);

    // Resolve kind_styles
    let kind_styles = file
        .kind_styles
        .iter()
        .map(|(kind, props)| (kind.clone(), compile_kind_style(props, &token_map, &defaults.node)))
        .collect();

    CompiledTheme {
        id: file.id,
        name: file.name,
        defaults,
        kind_styles,
        tokens: file.tokens,
    }
}

/// Compile a theme from its JSON string representation (no extends resolution).
pub fn compile_theme_str(json: &str) -> CompiledTheme {
    let file: ThemeFile =
        serde_json::from_str(json).unwrap_or_else(|e| panic!("theme parse error: {e}"));
    compile_resolved(file)
}

/// Build a flat token reference map from nested token groups.
fn build_token_map(file: &ThemeFile) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();

    for (k, v) in &file.tokens.colors {
        map.insert(format!("colors.{k}"), v.to_svg_string());
    }
    for (group, entries) in &file.tokens.palette {
        for (k, v) in entries {
            map.insert(format!("palette.{group}.{k}"), v.to_svg_string());
            // Also register as "role.{group}.{k}" for V1 compat references
            map.insert(format!("role.{group}.{k}"), v.to_svg_string());
        }
    }
    for (k, v) in &file.tokens.typography {
        map.insert(format!("typography.{k}"), v.to_svg_string());
    }
    for (k, v) in &file.tokens.strokes {
        map.insert(format!("strokes.{k}"), v.to_svg_string());
    }
    for (k, v) in &file.tokens.radius {
        map.insert(format!("radius.{k}"), v.to_svg_string());
    }
    for (k, v) in &file.tokens.spacing {
        map.insert(format!("spacing.{k}"), v.to_svg_string());
    }

    map
}

/// Resolve a StyleValue's token references (e.g. `{colors.canvas}` → actual value).
fn resolve_value(v: &StyleValue, tokens: &BTreeMap<String, String>) -> String {
    let raw = v.to_svg_string();
    resolve_ref(&raw, tokens)
}

/// Resolve `{token.path}` references in a string.
///
/// Unresolvable references are kept verbatim and skipped (never re-scanned),
/// so the cursor always advances and the loop is guaranteed to terminate.
fn resolve_ref(s: &str, tokens: &BTreeMap<String, String>) -> String {
    if !s.contains('{') {
        return s.to_string();
    }
    let mut result = s.to_string();
    let mut search_from = 0;
    while let Some(rel_start) = result[search_from..].find('{') {
        let start = search_from + rel_start;
        let Some(rel_end) = result[start..].find('}') else {
            break; // no closing brace: leave rest as-is
        };
        let key = &result[start + 1..start + rel_end];
        match tokens.get(key) {
            Some(replacement) => {
                let replacement = replacement.clone();
                result = format!(
                    "{}{}{}",
                    &result[..start],
                    replacement,
                    &result[start + rel_end + 1..]
                );
                // Continue after the substituted value (token values contain no refs)
                search_from = start + replacement.len();
            }
            None => {
                // Keep `{key}` verbatim, skip past it
                search_from = start + rel_end + 1;
            }
        }
    }
    result
}

fn compile_defaults(file: &ThemeFile, tokens: &BTreeMap<String, String>) -> CompiledDefaults {
    let d = &file.defaults;

    let canvas_background = get_resolved(&d.canvas, "background", tokens)
        .unwrap_or_else(|| "#FFFFFF".to_string());

    let title_fill = get_resolved(&d.title, "fill", tokens)
        .unwrap_or_else(|| "#18181B".to_string());
    let title_font_size = get_f64(&d.title, "font_size", tokens).unwrap_or(21.0);

    let node = compile_kind_style(&d.node, tokens, &default_node_style());

    let edge = EdgeDefaults {
        stroke: get_resolved(&d.edge, "stroke", tokens).unwrap_or_else(|| "#9C9CA6".to_string()),
        stroke_width: get_f64(&d.edge, "stroke_width", tokens).unwrap_or(1.25),
        text_fill: get_resolved(&d.edge, "text_fill", tokens).unwrap_or_else(|| "#71717A".to_string()),
        font_size: get_f64(&d.edge, "font_size", tokens).unwrap_or(14.0),
        arrow_fill: get_resolved(&d.edge, "arrow_fill", tokens).unwrap_or_else(|| "#0F766E".to_string()),
        arrow_style: get_resolved(&d.edge, "arrow_style", tokens).unwrap_or_else(|| "normal".to_string()),
        response_dasharray: get_resolved(&d.edge, "response_dasharray", tokens)
            .unwrap_or_else(|| "6,4".to_string()),
        stroke_linecap: get_resolved(&d.edge, "stroke_linecap", tokens),
        stroke_linejoin: get_resolved(&d.edge, "stroke_linejoin", tokens),
        stroke_opacity: get_f64(&d.edge, "stroke_opacity", tokens),
        label_bg: get_resolved(&d.edge, "label_bg", tokens),
        label_bg_opacity: get_f64(&d.edge, "label_bg_opacity", tokens).unwrap_or(1.0),
    };

    let group = GroupDefaults {
        fill: get_resolved(&d.group, "fill", tokens).unwrap_or_else(|| "#ECECEF".to_string()),
        stroke: get_resolved(&d.group, "stroke", tokens).unwrap_or_else(|| "#B0B0B8".to_string()),
        stroke_width: get_f64(&d.group, "stroke_width", tokens).unwrap_or(1.25),
        text_fill: get_resolved(&d.group, "text_fill", tokens).unwrap_or_else(|| "#71717A".to_string()),
        radius: get_f64(&d.group, "radius", tokens).unwrap_or(10.0),
        stroke_dasharray: get_resolved(&d.group, "stroke_dasharray", tokens)
            .filter(|s| s != "none"),
    };

    let typography = Typography {
        font_family: get_resolved(&file.tokens.typography.iter().map(|(k, v)| (k.clone(), v.clone())).collect(), "font_family", tokens)
            .unwrap_or_else(|| "sans-serif".to_string()),
        title_size: title_font_size,
        label_size: get_f64(&file.tokens.typography.iter().map(|(k, v)| (k.clone(), v.clone())).collect(), "label_size", tokens).unwrap_or(17.0),
        small_size: get_f64(&file.tokens.typography.iter().map(|(k, v)| (k.clone(), v.clone())).collect(), "small_size", tokens).unwrap_or(14.0),
    };

    CompiledDefaults {
        canvas_background,
        title_fill,
        title_font_size,
        node,
        edge,
        group,
        typography,
    }
}

fn compile_kind_style(
    props: &BTreeMap<String, StyleValue>,
    tokens: &BTreeMap<String, String>,
    base: &KindStyle,
) -> KindStyle {
    KindStyle {
        fill: get_resolved(props, "fill", tokens).unwrap_or_else(|| base.fill.clone()),
        stroke: get_resolved(props, "stroke", tokens).unwrap_or_else(|| base.stroke.clone()),
        stroke_width: get_f64(props, "stroke_width", tokens).unwrap_or(base.stroke_width),
        text_fill: get_resolved(props, "text_fill", tokens).unwrap_or_else(|| base.text_fill.clone()),
        font_size: get_f64(props, "font_size", tokens).unwrap_or(base.font_size),
        font_weight: get_resolved(props, "font_weight", tokens).or_else(|| base.font_weight.clone()),
        shape: get_resolved(props, "shape", tokens).or_else(|| base.shape.clone()),
        radius: get_f64(props, "radius", tokens).or(base.radius),
        stroke_dasharray: get_resolved(props, "stroke_dasharray", tokens)
            .filter(|s| s != "none")
            .or_else(|| base.stroke_dasharray.clone()),
        stroke_linecap: get_resolved(props, "stroke_linecap", tokens).or_else(|| base.stroke_linecap.clone()),
        stroke_linejoin: get_resolved(props, "stroke_linejoin", tokens).or_else(|| base.stroke_linejoin.clone()),
        fill_opacity: get_f64(props, "fill_opacity", tokens).or(base.fill_opacity),
        stroke_opacity: get_f64(props, "stroke_opacity", tokens).or(base.stroke_opacity),
    }
}

fn default_node_style() -> KindStyle {
    KindStyle {
        fill: "#FFFFFF".to_string(),
        stroke: "#C9C9C9".to_string(),
        stroke_width: 1.0,
        text_fill: "#18181B".to_string(),
        font_size: 17.0,
        font_weight: None,
        shape: Some("rounded_rect".to_string()),
        radius: Some(10.0),
        stroke_dasharray: None,
        stroke_linecap: Some("round".to_string()),
        stroke_linejoin: Some("round".to_string()),
        fill_opacity: None,
        stroke_opacity: None,
    }
}

// ─── Helpers ──────────────────────────────────────────────────────

fn get_resolved(
    map: &BTreeMap<String, StyleValue>,
    key: &str,
    tokens: &BTreeMap<String, String>,
) -> Option<String> {
    map.get(key).map(|v| resolve_value(v, tokens))
}

fn get_f64(
    map: &BTreeMap<String, StyleValue>,
    key: &str,
    tokens: &BTreeMap<String, String>,
) -> Option<f64> {
    map.get(key).and_then(|v| {
        // Try direct number first
        if let Some(n) = v.as_f64() {
            return Some(n);
        }
        // Try resolving token reference then parsing
        let resolved = resolve_value(v, tokens);
        resolved.parse().ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens() -> BTreeMap<String, String> {
        let mut t = BTreeMap::new();
        t.insert("colors.canvas".to_string(), "#FFFFFF".to_string());
        t.insert("strokes.normal".to_string(), "1.5".to_string());
        t
    }

    #[test]
    fn resolve_ref_replaces_known_tokens() {
        assert_eq!(resolve_ref("{colors.canvas}", &tokens()), "#FFFFFF");
        assert_eq!(
            resolve_ref("{colors.canvas} / {strokes.normal}", &tokens()),
            "#FFFFFF / 1.5"
        );
    }

    #[test]
    fn resolve_ref_keeps_unknown_token_verbatim_and_terminates() {
        // Regression: unresolvable refs used to livelock the loop
        assert_eq!(
            resolve_ref("{colors.nonexistent}", &tokens()),
            "{colors.nonexistent}"
        );
        // Mixed known + unknown: known resolved, unknown kept, no hang
        assert_eq!(
            resolve_ref("{colors.nonexistent}-{colors.canvas}", &tokens()),
            "{colors.nonexistent}-#FFFFFF"
        );
    }

    #[test]
    fn resolve_ref_handles_unclosed_brace() {
        assert_eq!(resolve_ref("{colors.canvas", &tokens()), "{colors.canvas");
    }
}
