//! 渲染编排:compute_layout → build_scene → encode。
//!
//! 本模块串联 [`crate::layout`]、[`crate::render::scene`]、[`crate::render::encode`] 三个服务,
//! 将 [`PreparedDiagram`] 转为 [`RenderOutput`]。
//!
//! ```text
//! PreparedDiagram
//!     ├─ EncodingPath::Diagram → encode_from_diagram() → RenderOutput
//!     └─ EncodingPath::Scene
//!         ↓ scene::compute_layout()    → LayoutResult
//!         ↓ scene::build_scene()       → ExportScene
//!         ↓ encode::encode_scene()     → RenderOutput
//! ```
//!
//! `EncodingPath::Diagram` 的编码器(ASCII / Markdown / OPML / FreeMind)跳过视觉物化,
//! 直接从 PreparedDiagram 编码。

use crate::ast::PreparedDiagram;
use crate::error::{PlotgramError, Result};
use crate::render::encode::encoder_for;
use crate::render::encode::drawio::ExportReport;
use crate::render::scene::export_scene;
use crate::render::{RenderFormat, RenderOutput, RenderRequest};

/// 渲染产物 + 导出报告。
///
/// `export_report` 为 `Some` 时携带编码器级别的导出降级报告（目前仅 drawio）。
pub struct RenderOutputWithReport {
    pub output: RenderOutput,
    pub export_report: Option<ExportReport>,
}

impl From<RenderOutput> for RenderOutputWithReport {
    fn from(output: RenderOutput) -> Self {
        Self {
            output,
            export_report: None,
        }
    }
}

/// 渲染流水线:布局 → 物化 → 编码。
///
/// `EncodingPath::Diagram` 的编码器跳过视觉物化,直接从 PreparedDiagram 编码;
/// `EncodingPath::Scene` 的编码器走完整流水线。
///
pub fn render_output(request: &RenderRequest<'_>) -> Result<RenderOutput> {
    render_output_with_report(request).map(|r| r.output)
}

/// 与 [`render_output`] 相同，但额外返回导出报告。
pub fn render_output_with_report(request: &RenderRequest<'_>) -> Result<RenderOutputWithReport> {
    let encoder = encoder_for(request.format)?;
    match encoder.encoding_path() {
        crate::render::encode::EncodingPath::Diagram => {
            let result = encoder.encode_from_diagram(request)?;
            Ok(RenderOutputWithReport {
                output: result.output,
                export_report: None,
            })
        }
        crate::render::encode::EncodingPath::Scene => {
            let scene = export_scene(request)?;
            let (output, export_report) = encoder.encode_scene_with_report(&scene)?;
            Ok(RenderOutputWithReport {
                output,
                export_report,
            })
        }
    }
}

/// 渲染为文本(SVG/ASCII/JSON)。
pub fn render_text(request: &RenderRequest<'_>) -> Result<String> {
    match render_output(request)? {
        RenderOutput::Text(text) => Ok(text),
        RenderOutput::Binary(_) => Err(PlotgramError::render_internal_msg(format!(
            "format '{}' produces binary output, use render_bytes instead",
            request.format
        ))),
    }
}

/// 渲染为字节(PNG/WebP)。
pub fn render_bytes(request: &RenderRequest<'_>) -> Result<Vec<u8>> {
    match render_output(request)? {
        RenderOutput::Binary(bytes) => Ok(bytes),
        RenderOutput::Text(_) => Err(PlotgramError::render_internal_msg(format!(
            "format '{}' produces text output, use render_text instead",
            request.format
        ))),
    }
}

/// 从 PreparedDiagram 直接渲染 JSON(便捷入口,供 diff/export 等场景使用)。
pub fn render_json(diagram: &PreparedDiagram) -> String {
    let request = RenderRequest::new(diagram, RenderFormat::Json);
    render_text(&request).unwrap_or_default()
}

/// 用预算好的 [`LayoutResult`](crate::layout::types::LayoutResult) 渲染 SVG，
/// 跳过布局阶段（Atlas Stage 0 交付 0.6：shadow 对拍为同图的两份布局各出一张 SVG）。
///
/// 与 [`render_output`] 的 Scene 路径保持一致（含 title band 调整），仅布局来源不同。
pub fn render_svg_with_layout(
    request: &RenderRequest<'_>,
    mut layout: crate::layout::types::LayoutResult,
) -> Result<String> {
    crate::render::scene::apply_title_band_layout_adjustment(&mut layout, request.show_title);
    let scene = crate::render::scene::build_scene(request, layout)?;
    let encoder = encoder_for(RenderFormat::Svg)?;
    match encoder.encode_scene(&scene)? {
        RenderOutput::Text(text) => Ok(text),
        RenderOutput::Binary(_) => Err(PlotgramError::render_internal_msg(
            "svg encoder unexpectedly produced binary output".to_string(),
        )),
    }
}

/// 使用内联 style JSON 渲染(供 CLI / Server 传入自定义主题)。
pub fn render_with_style_json(
    diagram: &PreparedDiagram,
    format: RenderFormat,
    style_json: &str,
) -> Result<RenderOutput> {
    let mut request = RenderRequest::new(diagram, format);
    request.explicit_style_json = Some(style_json);
    render_output(&request)
}
