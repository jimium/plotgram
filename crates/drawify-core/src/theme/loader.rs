//! StyleSheet v0.2 JSON 解析与校验。
//!
//! 按 spec §9 实现。

use crate::error::{DrawifyError, Result};

use super::schema::{
    ElementDefaults, StyleSheet, StyleTokens, StyleValue, SUPPORTED_VERSIONS, KNOWN_DIAGRAM_TYPES,
};

// ─── 验证错误类型 ──────────────────────────────────────────────────

/// 样式表验证错误。
#[derive(Debug, thiserror::Error)]
pub enum StyleValidationError {
    #[error("missing required field: {field}")]
    MissingRequiredField { field: String },

    #[error("invalid version '{got}', expected one of {expected:?}")]
    InvalidVersion { got: String, expected: Vec<String> },

    #[error("invalid color format at {field_path}: '{value}' (expected #RRGGBB or #RRGGBBAA)")]
    InvalidColorFormat { field_path: String, value: String },

    #[error("unknown diagram type: '{key}'")]
    UnknownDiagramType { key: String },

    #[error("unresolved token reference at {field_path}: '{reference}'")]
    UnresolvedTokenRef { field_path: String, reference: String },
}

impl From<StyleValidationError> for DrawifyError {
    fn from(err: StyleValidationError) -> Self {
        DrawifyError::Style(err.to_string())
    }
}

// ─── 公开 API ──────────────────────────────────────────────────────

/// 将 JSON 字符串解析为 StyleSheet，并执行基础字段校验。
///
/// 注意：此函数仅做 parse 期基础校验（version/id/name/tokens/colors）。
/// 完整校验（含 context_palettes schema）由 `compile::validate_style_sheet` 在 merge 后执行。
pub fn parse_style_sheet_json(input: &str) -> Result<StyleSheet> {
    let sheet: StyleSheet = serde_json::from_str(input)
        .map_err(|err| DrawifyError::Style(format!("invalid style sheet json: {err}")))?;

    validate_basic(&sheet)?;
    Ok(sheet)
}

/// 基础字段校验（spec §9 解析阶段）。
///
/// 校验 version/id/name/tokens/defaults 非空 + 颜色格式。
/// context_palettes 校验见 `compile::validate_style_sheet`。
pub(crate) fn validate_basic(sheet: &StyleSheet) -> Result<()> {
    // ── 必填字段检查 ────────────────────────────────────────
    if sheet.version.trim().is_empty() {
        return Err(StyleValidationError::MissingRequiredField {
            field: "version".into(),
        }
        .into());
    }
    if sheet.id.trim().is_empty() {
        return Err(StyleValidationError::MissingRequiredField {
            field: "id".into(),
        }
        .into());
    }
    if sheet.name.trim().is_empty() {
        return Err(StyleValidationError::MissingRequiredField {
            field: "name".into(),
        }
        .into());
    }

    // tokens 必须非空
    if is_tokens_empty(&sheet.tokens) {
        return Err(StyleValidationError::MissingRequiredField {
            field: "tokens".into(),
        }
        .into());
    }

    // defaults 必须非空（extends 子主题可省略，由基座提供）
    if sheet.extends.is_none() && is_defaults_empty(&sheet.defaults) {
        return Err(StyleValidationError::MissingRequiredField {
            field: "defaults".into(),
        }
        .into());
    }

    // ── 版本检查 ────────────────────────────────────────────
    if !SUPPORTED_VERSIONS.contains(&sheet.version.as_str()) {
        return Err(StyleValidationError::InvalidVersion {
            got: sheet.version.clone(),
            expected: SUPPORTED_VERSIONS.iter().map(|s| s.to_string()).collect(),
        }
        .into());
    }

    // ── 颜色格式验证（tokens.colors） ───────────────────────
    for (key, value) in &sheet.tokens.colors {
        if !is_valid_color(value) {
            return Err(StyleValidationError::InvalidColorFormat {
                field_path: format!("tokens.colors.{key}"),
                value: value.clone(),
            }
            .into());
        }
    }

    // ── defaults 中 fill/stroke 颜色验证 ────────────────────
    validate_colors_in_element_defaults(&sheet.defaults, "defaults")?;

    // ── diagrams 中 fill/stroke 颜色验证 + 图表类型警告 ─────
    for (diagram_key, diagram_styles) in &sheet.diagrams {
        let path_prefix = format!("diagrams.{diagram_key}");

        // 图表类型检查：仅警告，不报错（spec §9.2）
        if !KNOWN_DIAGRAM_TYPES.contains(&diagram_key.as_str()) {
            eprintln!(
                "[warn] unknown diagram type: '{diagram_key}' (known types: {:?})",
                KNOWN_DIAGRAM_TYPES
            );
        }

        // 第二层 node/edge/group/title
        if let Some(ref block) = diagram_styles.node {
            validate_colors_in_block(block, &format!("{path_prefix}.node"))?;
        }
        if let Some(ref block) = diagram_styles.edge {
            validate_colors_in_block(block, &format!("{path_prefix}.edge"))?;
        }
        if let Some(ref block) = diagram_styles.group {
            validate_colors_in_block(block, &format!("{path_prefix}.group"))?;
        }
        if let Some(ref block) = diagram_styles.title {
            validate_colors_in_block(block, &format!("{path_prefix}.title"))?;
        }

        // 第三层 entity_types
        for (type_key, type_block) in &diagram_styles.entity_types {
            validate_colors_in_block(type_block, &format!("{path_prefix}.entity_types.{type_key}"))?;
        }

        // edge_kinds
        for (kind_key, kind_block) in &diagram_styles.edge_kinds {
            validate_colors_in_block(kind_block, &format!("{path_prefix}.edge_kinds.{kind_key}"))?;
        }
    }

    Ok(())
}

// ─── 内部辅助函数 ──────────────────────────────────────────────────

fn is_tokens_empty(tokens: &StyleTokens) -> bool {
    tokens.colors.is_empty()
        && tokens.typography.is_empty()
        && tokens.strokes.is_empty()
        && tokens.radius.is_empty()
        && tokens.spacing.is_empty()
        && tokens.effects.is_empty()
}

fn is_defaults_empty(defaults: &ElementDefaults) -> bool {
    defaults.canvas.is_empty()
        && defaults.title.is_empty()
        && defaults.node.is_empty()
        && defaults.edge.is_empty()
        && defaults.group.is_empty()
}

/// 验证 ElementDefaults 中 fill/stroke 的颜色格式。
fn validate_colors_in_element_defaults(defaults: &ElementDefaults, path_prefix: &str) -> Result<()> {
    validate_colors_in_block(&defaults.canvas, &format!("{path_prefix}.canvas"))?;
    validate_colors_in_block(&defaults.title, &format!("{path_prefix}.title"))?;
    validate_colors_in_block(&defaults.node, &format!("{path_prefix}.node"))?;
    validate_colors_in_block(&defaults.edge, &format!("{path_prefix}.edge"))?;
    validate_colors_in_block(&defaults.group, &format!("{path_prefix}.group"))?;
    Ok(())
}

/// 验证 StyleBlock 中 fill/stroke 字段的颜色格式。
///
/// 仅检查非 token 引用的字符串值（token 引用在 resolve 阶段验证）。
fn validate_colors_in_block(block: &super::schema::StyleBlock, path_prefix: &str) -> Result<()> {
    for (key, value) in block.iter() {
        if key == "fill" || key == "stroke" || key == "text_fill" {
            if let StyleValue::String(s) = value {
                // 跳过 token 引用（在 resolve 阶段验证）
                if !StyleTokens::is_token_ref(s) && !is_valid_color(s) {
                    return Err(StyleValidationError::InvalidColorFormat {
                        field_path: format!("{path_prefix}.{key}"),
                        value: s.clone(),
                    }
                    .into());
                }
            }
        }
    }
    Ok(())
}

/// 检查颜色值是否符合 #RRGGBB 或 #RRGGBBAA 格式。
pub fn is_valid_color(value: &str) -> bool {
    let value = value.trim();
    if !value.starts_with('#') {
        return false;
    }
    let hex = &value[1..];
    (hex.len() == 6 || hex.len() == 8) && hex.chars().all(|c| c.is_ascii_hexdigit())
}

// ─── 测试 ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_v02_style_sheet() {
        let json = r##"{
            "version": "0.2",
            "id": "custom.clean",
            "name": "Custom Clean",
            "tokens": {
                "colors": { "canvas": "#ffffff" }
            },
            "defaults": {
                "node": { "fill": "#1a1a2e" }
            }
        }"##;

        let sheet = parse_style_sheet_json(json).unwrap();
        assert_eq!(sheet.id, "custom.clean");
        assert_eq!(sheet.version, "0.2");
    }

    #[test]
    fn parse_full_v02_structure() {
        let json = r##"{
            "version": "0.2",
            "id": "test.full",
            "name": "Full Test",
            "tokens": {
                "colors": { "primary": "#1976D2", "canvas": "#FAFAFA" },
                "typography": { "label_size": 13 },
                "strokes": { "normal": 1.5, "dashed": [6, 3] },
                "radius": { "md": 8 },
                "spacing": { "node_padding_x": 12 },
                "effects": { "shadow": false }
            },
            "defaults": {
                "canvas": { "background": "{colors.canvas}" },
                "node": { "fill": "{colors.primary}", "shape": "rounded_rect" }
            },
            "diagrams": {
                "flowchart": {
                    "entity_types": {
                        "service": { "fill": "#E3F2FD", "stroke": "#1976D2", "shape": "rounded_rect" }
                    }
                }
            }
        }"##;

        let sheet = parse_style_sheet_json(json).unwrap();
        assert_eq!(sheet.id, "test.full");
        assert!(sheet.diagrams.contains_key("flowchart"));
        let fc = sheet.diagrams.get("flowchart").unwrap();
        assert!(fc.entity_types.contains_key("service"));
    }

    #[test]
    fn reject_empty_version() {
        let json = r##"{
            "version": "",
            "id": "test",
            "name": "Test",
            "tokens": { "colors": { "a": "#000000" } },
            "defaults": { "node": {} }
        }"##;
        let err = parse_style_sheet_json(json).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("missing required field"), "got: {msg}");
    }

    #[test]
    fn reject_invalid_version() {
        let json = r##"{
            "version": "0.1",
            "id": "test",
            "name": "Test",
            "tokens": { "colors": { "a": "#000000" } },
            "defaults": { "node": { "fill": "#000000" } }
        }"##;
        let err = parse_style_sheet_json(json).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("invalid version"), "got: {msg}");
    }

    #[test]
    fn reject_invalid_color_in_tokens() {
        let json = r##"{
            "version": "0.2",
            "id": "test",
            "name": "Test",
            "tokens": { "colors": { "primary": "red" } },
            "defaults": { "node": { "fill": "#000000" } }
        }"##;
        let err = parse_style_sheet_json(json).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("invalid color format"), "got: {msg}");
    }

    #[test]
    fn accept_token_ref_in_fill() {
        let json = r##"{
            "version": "0.2",
            "id": "test",
            "name": "Test",
            "tokens": { "colors": { "primary": "#1976D2" } },
            "defaults": { "node": { "fill": "{colors.primary}" } }
        }"##;
        assert!(parse_style_sheet_json(json).is_ok());
    }

    #[test]
    fn accept_rrggbbaa_color() {
        let json = r##"{
            "version": "0.2",
            "id": "test",
            "name": "Test",
            "tokens": { "colors": { "bg": "#ffffffcc" } },
            "defaults": { "node": { "fill": "#1a1a2e80" } }
        }"##;
        assert!(parse_style_sheet_json(json).is_ok());
    }

    #[test]
    fn reject_empty_tokens() {
        let json = r##"{
            "version": "0.2",
            "id": "test",
            "name": "Test",
            "tokens": {},
            "defaults": { "node": { "fill": "#000000" } }
        }"##;
        let err = parse_style_sheet_json(json).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("missing required field"), "got: {msg}");
    }

    #[test]
    fn reject_empty_defaults() {
        let json = r##"{
            "version": "0.2",
            "id": "test",
            "name": "Test",
            "tokens": { "colors": { "a": "#000000" } },
            "defaults": {}
        }"##;
        let err = parse_style_sheet_json(json).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("missing required field"), "got: {msg}");
    }

    #[test]
    fn is_valid_color_checks() {
        assert!(is_valid_color("#000000"));
        assert!(is_valid_color("#ffffff"));
        assert!(is_valid_color("#1a2B3C"));
        assert!(is_valid_color("#000000ff"));
        assert!(is_valid_color("#ffffff00"));
        assert!(!is_valid_color("red"));
        assert!(!is_valid_color("#fff"));
        assert!(!is_valid_color("#12345"));
        assert!(!is_valid_color("#1234567"));
        assert!(!is_valid_color("#123456789"));
        assert!(!is_valid_color("#gggggg"));
    }
}
