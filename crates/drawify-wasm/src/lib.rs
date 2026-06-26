//! Drawify WASM Bindings
//!
//! 单格式渲染桥:每次调用 `render` / `render_with_options` 只产出一种格式,
//! 由调用方(playground)按页签切换分别调用。

use drawify_core::{
    ast::{Diagram, RawDiagram},
    diff2::{self, ChangeSet},
    error::DiagnosticError,
    layout::LayoutIntentOverlay,
    parser,
    pipeline::{parse_prepare_validate, render_output_with_report, RenderOutputWithReport},
    prepare::StyleRequest,
    render::{parse_graphic_style_id, RenderFormat, RenderOutput, RenderRequest},
    types::attr_schema,
};
use wasm_bindgen::prelude::*;

/// 单格式渲染结果。`text` 携带 SVG / ASCII / JSON 文本输出。
#[derive(serde::Serialize)]
pub struct RenderResult {
    pub success: bool,
    pub format: String,
    pub text: Option<String>,
    pub errors: Vec<DiagnosticError>,
    pub warnings: Vec<DiagnosticError>,
    /// 布局意图修正报告（仅当 `WasmRenderOptions.layout_intents` 为 `Some` 时存在）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refinement_report: Option<drawify_core::layout::RefinementReport>,
    /// drawio 导出降级报告（仅 drawio 格式时存在）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub export_report: Option<drawify_core::render::encode::ExportReport>,
}

#[derive(serde::Serialize)]
pub struct ValidationResult {
    pub valid: bool,
    pub errors: Vec<DiagnosticError>,
    pub warnings: Vec<DiagnosticError>,
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct WasmRenderOptions {
    pub theme_id: Option<String>,
    pub graphic_style: Option<String>,
    pub dark_mode: Option<bool>,
    pub transparent_background: Option<bool>,
    pub ascii: Option<drawify_core::render::encode::ascii::AsciiExportOptions>,
    /// 布局意图叠加层（可选）。
    ///
    /// 透传至 `RenderRequest::layout_overlay`，由布局算法与几何微调阶段消费。
    /// 为 `None` 时布局行为与无意图完全一致。
    pub layout_intents: Option<LayoutIntentOverlay>,
}

#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

#[wasm_bindgen]
pub fn layout_catalog() -> String {
    serde_json::to_string(&drawify_core::layout::layout_catalog())
        .unwrap_or_else(|_| "{}".to_string())
}

#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn style_request_from_options(options: Option<&WasmRenderOptions>) -> StyleRequest {
    StyleRequest {
        theme_id: options.and_then(|o| o.theme_id.clone()),
        dark_mode: options.and_then(|o| o.dark_mode).unwrap_or(false),
    }
}

fn normalize_option(value: Option<&str>) -> Option<&str> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "auto")
}

/// 将可选 options 应用到 RenderRequest(theme / graphic_style / dark_mode / transparent / ascii / layout_intents)。
/// 返回 Err 时携带错误消息(如未知 graphic_style)。
fn apply_options_to_request<'a>(
    request: &mut RenderRequest<'a>,
    options: Option<&'a WasmRenderOptions>,
) -> Result<(), String> {
    let Some(options) = options else { return Ok(()) };

    if let Some(theme_id) = normalize_option(options.theme_id.as_deref()) {
        request.explicit_theme_id = Some(theme_id);
    }
    if let Some(graphic_style_value) = normalize_option(options.graphic_style.as_deref()) {
        let graphic_style = parse_graphic_style_id(graphic_style_value)
            .ok_or_else(|| format!("unknown graphic style '{graphic_style_value}'"))?;
        request.explicit_graphic_style = Some(graphic_style);
    }
    request.dark_mode = options.dark_mode.unwrap_or(false);
    request.transparent_background = options.transparent_background.unwrap_or(false);
    if let Some(ascii) = &options.ascii {
        request.ascii_options = ascii.clone();
    }
    if let Some(intents) = &options.layout_intents {
        request.layout_overlay = Some(intents);
    }
    Ok(())
}

#[wasm_bindgen]
pub fn render(source: &str, format: &str) -> String {
    render_impl(source, format, None)
}

#[wasm_bindgen]
pub fn render_with_options(source: &str, format: &str, options_json: &str) -> String {
    if options_json.trim().is_empty() {
        return render_impl(source, format, None);
    }

    match serde_json::from_str::<WasmRenderOptions>(options_json) {
        Ok(options) => render_impl(source, format, Some(options)),
        Err(err) => {
            let result = RenderResult {
                success: false,
                format: format.to_string(),
                text: None,
                errors: vec![DiagnosticError::render_internal(
                    drawify_core::ast::Span::dummy(),
                    format!("invalid render options json: {err}"),
                )],
                warnings: vec![],
                refinement_report: None,
                export_report: None,
            };
            serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string())
        }
    }
}

fn render_impl(source: &str, format_str: &str, options: Option<WasmRenderOptions>) -> String {
    let format = match RenderFormat::from_str(format_str) {
        Some(f) => f,
        None => {
            let result = RenderResult {
                success: false,
                format: format_str.to_string(),
                text: None,
                errors: vec![DiagnosticError::render_internal(
                    drawify_core::ast::Span::dummy(),
                    format!(
                        "unsupported format '{format_str}'; supported: svg, ascii, png, webp, json, drawio, md-outline, opml, freemind"
                    ),
                )],
                warnings: vec![],
                refinement_report: None,
                export_report: None,
            };
            return serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string());
        }
    };

    let style_request = style_request_from_options(options.as_ref());
    let output = parse_prepare_validate(source, &style_request);
    let mut errors = output.errors;
    let warnings = output.warnings;

    if let Some(prepared) = output.diagram {
        if errors.is_empty() {
            let mut request = RenderRequest::new(&prepared, format);
            if let Err(msg) = apply_options_to_request(&mut request, options.as_ref()) {
                errors.push(DiagnosticError::render_internal(
                    drawify_core::ast::Span::dummy(),
                    msg,
                ));
            } else {
                match render_output_with_report(&request) {
                    Ok(RenderOutputWithReport { output: RenderOutput::Text(text), report, export_report }) => {
                        let result = RenderResult {
                            success: true,
                            format: format_str.to_string(),
                            text: Some(text),
                            errors,
                            warnings,
                            refinement_report: report,
                            export_report,
                        };
                        return serde_json::to_string(&result)
                            .unwrap_or_else(|_| "{}".to_string());
                    }
                    Ok(RenderOutputWithReport { output: RenderOutput::Binary(_), .. }) => {
                        errors.push(DiagnosticError::render_internal(
                            drawify_core::ast::Span::dummy(),
                            format!(
                                "format '{format_str}' produces binary output, not supported in WASM text bridge"
                            ),
                        ));
                    }
                    Err(e) => errors.extend(e.into_diagnostics()),
                }
            }
        }
    }

    let result = RenderResult {
        success: false,
        format: format_str.to_string(),
        text: None,
        errors,
        warnings,
        refinement_report: None,
        export_report: None,
    };
    serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string())
}

#[derive(serde::Serialize)]
struct ParseResult {
    pub diagram: Option<Diagram>,
    pub errors: Vec<DiagnosticError>,
    pub warnings: Vec<DiagnosticError>,
}

#[wasm_bindgen]
pub fn parse_to_json(source: &str) -> String {
    let (diagram_opt, parse_errors, parse_warnings) = parser::parse_with_diagnostics(source);

    let result = ParseResult {
        diagram: diagram_opt,
        errors: parse_errors,
        warnings: parse_warnings,
    };

    serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string())
}

#[wasm_bindgen]
pub fn validate(source: &str) -> String {
    let output = parse_prepare_validate(source, &StyleRequest::default());
    let result = ValidationResult {
        valid: output.is_valid(),
        errors: output.errors,
        warnings: output.warnings,
    };
    serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string())
}

/// 获取指定 scope 下的属性 schema 列表，供 playground 前端编辑器做自动补全。
///
/// `scope`: `"diagram"` | `"entity"` | `"group"` | `"relation"`
///
/// 返回 JSON 字符串，结构为 `[{ "key": "direction", "scope": "diagram", "value_type": "atom", "enum_values": ["top-to-bottom", ...] }, ...]`。
#[wasm_bindgen]
pub fn get_attr_schema(scope: &str) -> String {
    match attr_schema::schema_for_scope(scope) {
        Some(schema) => serde_json::to_string(schema).unwrap_or_else(|_| "[]".to_string()),
        None => "[]".to_string(),
    }
}

/// 获取指定属性 key 的合法枚举值列表（跨所有 scope 查找）。
///
/// 返回 `null` 表示该属性无闭集枚举约束。
#[wasm_bindgen]
pub fn get_enum_values(key: &str) -> Option<Vec<String>> {
    attr_schema::enum_values_for_key(key).map(|vals| vals.iter().map(|s| s.to_string()).collect())
}

// ─── Diff / Patch / Format API ─────────────────────────────────────

#[derive(serde::Serialize)]
pub struct DiffResult {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changes: Option<ChangeSet>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
}

#[derive(serde::Serialize)]
pub struct PatchResultJson {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    pub applied: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
}

#[derive(serde::Serialize)]
pub struct FormatResult {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
}

fn parse_raw(source: &str) -> Result<RawDiagram, Vec<String>> {
    match parser::parse(source) {
        Ok(diagram) => Ok(RawDiagram(diagram)),
        Err(e) => Err(vec![e.to_string()]),
    }
}

/// 比较两份 DSL 源码的语义差异，返回结构化 ChangeSet。
#[wasm_bindgen]
pub fn diff_sources(source_a: &str, source_b: &str) -> String {
    let mut errors = Vec::new();

    let raw_a = match parse_raw(source_a) {
        Ok(d) => d,
        Err(errs) => {
            errors.extend(errs);
            errors.push("source_a 解析失败".into());
            return serde_json::to_string(&DiffResult {
                success: false,
                changes: None,
                errors,
            })
            .unwrap_or_else(|_| "{}".to_string());
        }
    };

    let raw_b = match parse_raw(source_b) {
        Ok(d) => d,
        Err(errs) => {
            errors.extend(errs);
            errors.push("source_b 解析失败".into());
            return serde_json::to_string(&DiffResult {
                success: false,
                changes: None,
                errors,
            })
            .unwrap_or_else(|_| "{}".to_string());
        }
    };

    let changes = diff2::diff(&raw_a, &raw_b);
    serde_json::to_string(&DiffResult {
        success: true,
        changes: Some(changes),
        errors,
    })
    .unwrap_or_else(|_| "{}".to_string())
}

/// 将 ChangeSet（JSON 字符串）应用到 DSL 源码，返回 patch 后重新格式化的 DSL。
#[wasm_bindgen]
pub fn apply_patch(source: &str, patch_json: &str) -> String {
    let base = match parse_raw(source) {
        Ok(d) => d,
        Err(errs) => {
            return serde_json::to_string(&PatchResultJson {
                success: false,
                text: None,
                applied: 0,
                errors: errs,
            })
            .unwrap_or_else(|_| "{}".to_string());
        }
    };

    let changes: ChangeSet = match serde_json::from_str(patch_json) {
        Ok(cs) => cs,
        Err(e) => {
            return serde_json::to_string(&PatchResultJson {
                success: false,
                text: None,
                applied: 0,
                errors: vec![format!("patch_json 解析失败: {e}")],
            })
            .unwrap_or_else(|_| "{}".to_string());
        }
    };

    let result = diff2::patch(&base, &changes);
    let text = diff2::format(&result.diagram);

    serde_json::to_string(&PatchResultJson {
        success: result.errors.is_empty(),
        text: Some(text),
        applied: result.applied,
        errors: result.errors,
    })
    .unwrap_or_else(|_| "{}".to_string())
}

/// 将 DSL 源码解析后重新规范化输出（确定性排序 / 格式化）。
#[wasm_bindgen]
pub fn format_source(source: &str) -> String {
    match parse_raw(source) {
        Ok(raw) => {
            let text = diff2::format(&raw);
            serde_json::to_string(&FormatResult {
                success: true,
                text: Some(text),
                errors: vec![],
            })
            .unwrap_or_else(|_| "{}".to_string())
        }
        Err(errs) => serde_json::to_string(&FormatResult {
            success: false,
            text: None,
            errors: errs,
        })
        .unwrap_or_else(|_| "{}".to_string()),
    }
}
