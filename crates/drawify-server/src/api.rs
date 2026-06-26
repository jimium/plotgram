//! Drawify Server HTTP API handlers

use axum::{
    body::Body,
    http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use drawify_core::{
    ast::{PreparedDiagram, Span},
    error::DiagnosticError,
    layout::LayoutIntentOverlay,
    pipeline::{render_output_with_report, RenderOutputWithReport},
    prepare::StyleRequest,
    render::{parse_graphic_style_id, RenderFormat, RenderOutput, RenderRequest},
};
use serde::{Deserialize, Serialize};

static HEADER_FORMAT: HeaderName = HeaderName::from_static("x-drawify-format");
static HEADER_VALID: HeaderName = HeaderName::from_static("x-drawify-valid");
static HEADER_WARNINGS: HeaderName = HeaderName::from_static("x-drawify-warnings");
static HEADER_REFINEMENT_REPORT: HeaderName =
    HeaderName::from_static("x-drawify-refinement-report");

#[derive(Debug, Serialize)]
pub struct CheckResult {
    pub passed: bool,
    pub errors: Vec<DiagnosticError>,
    pub warnings: Vec<DiagnosticError>,
}

#[derive(Debug, Deserialize)]
pub struct ValidateRequest {
    pub source: String,
}

#[derive(Debug, Serialize)]
pub struct ValidateResponse {
    pub valid: bool,
    pub check: CheckResult,
}

#[derive(Debug, Deserialize)]
pub struct RenderRequestBody {
    pub source: String,
    #[serde(default = "default_format")]
    pub format: String,
    pub theme_id: Option<String>,
    pub graphic_style: Option<String>,
    pub dark_mode: Option<bool>,
    /// 布局意图叠加层（可选）。
    ///
    /// 透传至 `RenderRequest::layout_overlay`，由布局算法与几何微调阶段消费。
    /// 为 `None` 时布局行为与无意图完全一致。
    pub layout_intents: Option<LayoutIntentOverlay>,
}

fn default_format() -> String {
    "svg".to_string()
}

/// 渲染失败时返回 JSON（成功时直接返回渲染产物，不套 JSON 外壳）。
#[derive(Debug, Serialize)]
pub struct RenderErrorResponse {
    pub valid: bool,
    pub format: String,
    pub check: CheckResult,
}

fn request_error(message: impl Into<String>) -> DiagnosticError {
    DiagnosticError::syntax_error(Span::dummy(), message)
}

fn normalize_option(value: Option<&str>) -> Option<&str> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "auto")
}

/// 解析 + prepare + 校验 Drawify 源码，返回 PreparedDiagram 与结构化诊断结果。
fn check_source(source: &str) -> (Option<PreparedDiagram>, CheckResult) {
    let output = drawify_core::pipeline::parse_prepare_validate(source, &StyleRequest::default());
    let check = CheckResult {
        passed: output.is_valid(),
        errors: output.errors,
        warnings: output.warnings,
    };
    (output.diagram, check)
}

fn attach_render_meta(headers: &mut HeaderMap, format: RenderFormat, check: &CheckResult) {
    headers.insert(
        HEADER_FORMAT.clone(),
        HeaderValue::from_str(&format.to_string()).expect("format header"),
    );
    headers.insert(
        HEADER_VALID.clone(),
        if check.passed {
            HeaderValue::from_static("true")
        } else {
            HeaderValue::from_static("false")
        },
    );
    if !check.warnings.is_empty() {
        if let Ok(json) = serde_json::to_string(&check.warnings) {
            if let Ok(value) = HeaderValue::from_str(&json) {
                headers.insert(HEADER_WARNINGS.clone(), value);
            }
        }
    }
}

fn render_success(
    format: RenderFormat,
    output: RenderOutput,
    check: &CheckResult,
    refinement_report: Option<&drawify_core::layout::RefinementReport>,
) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&content_type_for(format)).expect("content-type"),
    );
    attach_render_meta(&mut headers, format, check);

    // 仅当存在报告时设置 X-Drawify-Refinement-Report 头
    if let Some(report) = refinement_report {
        if let Ok(json) = serde_json::to_string(report) {
            if let Ok(value) = HeaderValue::from_str(&json) {
                headers.insert(HEADER_REFINEMENT_REPORT.clone(), value);
            }
        }
    }

    let body = match output {
        RenderOutput::Text(text) => Body::from(text),
        RenderOutput::Binary(bytes) => Body::from(bytes),
    };

    let mut response = Response::new(body);
    *response.status_mut() = StatusCode::OK;
    *response.headers_mut() = headers;
    response
}

fn render_error(format: RenderFormat, check: CheckResult) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    attach_render_meta(&mut headers, format, &check);

    let body = RenderErrorResponse {
        valid: false,
        format: format.to_string(),
        check,
    };

    (StatusCode::BAD_REQUEST, headers, Json(body)).into_response()
}

fn render_error_raw(format_str: &str, check: CheckResult) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    if let Ok(value) = HeaderValue::from_str(format_str) {
        headers.insert(HEADER_FORMAT.clone(), value);
    }
    headers.insert(HEADER_VALID.clone(), HeaderValue::from_static("false"));

    let body = RenderErrorResponse {
        valid: false,
        format: format_str.to_string(),
        check,
    };

    (StatusCode::BAD_REQUEST, headers, Json(body)).into_response()
}

pub async fn validate_handler(Json(body): Json<ValidateRequest>) -> Json<ValidateResponse> {
    let (_, check) = check_source(&body.source);

    Json(ValidateResponse {
        valid: check.passed,
        check,
    })
}

pub async fn render_handler(Json(body): Json<RenderRequestBody>) -> Response {
    let format_str = body.format.trim();
    let format = match RenderFormat::from_str(format_str) {
        Some(format) => format,
        None => {
            let check = CheckResult {
                passed: false,
                errors: vec![request_error(format!(
                    "unsupported format '{format_str}'; supported: svg, ascii, png, webp, json"
                ))],
                warnings: vec![],
            };
            return render_error_raw(format_str, check);
        }
    };

    let (prepared_opt, check) = check_source(&body.source);

    if !check.passed {
        return render_error(format, check);
    }

    let Some(prepared) = prepared_opt else {
        let check = CheckResult {
            passed: false,
            errors: vec![request_error("failed to parse diagram")],
            warnings: check.warnings,
        };
        return render_error(format, check);
    };

    let mut request = RenderRequest::new(&prepared, format);

    if let Some(theme_id) = normalize_option(body.theme_id.as_deref()) {
        request.explicit_theme_id = Some(theme_id);
    }

    if let Some(graphic_style_value) = normalize_option(body.graphic_style.as_deref()) {
        match parse_graphic_style_id(graphic_style_value) {
            Some(graphic_style) => request.explicit_graphic_style = Some(graphic_style),
            None => {
                let check = CheckResult {
                    passed: false,
                    errors: vec![request_error(format!(
                        "unknown graphic style '{graphic_style_value}'"
                    ))],
                    warnings: check.warnings,
                };
                return render_error(format, check);
            }
        }
    }

    request.dark_mode = body.dark_mode.unwrap_or(false);

    // 透传布局意图叠加层
    if let Some(intents) = &body.layout_intents {
        request.layout_overlay = Some(intents);
    }

    match render_output_with_report(&request) {
        Ok(RenderOutputWithReport { output, report, .. }) => {
            render_success(format, output, &check, report.as_ref())
        }
        Err(err) => {
            let check = CheckResult {
                passed: false,
                errors: vec![request_error(err.to_string())],
                warnings: check.warnings,
            };
            render_error(format, check)
        }
    }
}

fn content_type_for(format: RenderFormat) -> String {
    match format {
        RenderFormat::Svg => "image/svg+xml".to_string(),
        RenderFormat::Ascii => "text/plain; charset=utf-8".to_string(),
        RenderFormat::Png => "image/png".to_string(),
        RenderFormat::Webp => "image/webp".to_string(),
        RenderFormat::Json => "application/json".to_string(),
        RenderFormat::Drawio => "application/xml; charset=utf-8".to_string(),
        RenderFormat::MdOutline => "text/markdown; charset=utf-8".to_string(),
        RenderFormat::Opml => "text/x-opml; charset=utf-8".to_string(),
        RenderFormat::Freemind => "application/x-freemind; charset=utf-8".to_string(),
    }
}
