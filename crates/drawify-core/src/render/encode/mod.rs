//! 格式编码层:将 [`super::scene::ExportScene`] 编码为 SVG / JSON / ASCII / PNG / WebP 等输出。
//!
//! 编码器只负责 `ExportScene → RenderOutput`,不参与布局计算或视觉物化。
//! 流水线编排(布局 → 物化 → 编码)由 [`crate::pipeline::render`] 统一调度。

use crate::ast::PreparedDiagram;
use crate::error::Result;
use crate::layout::RefinementReport;

use super::scene::ExportScene;
use super::{RenderFormat, RenderOutput, RenderRequest};

pub mod ascii;
pub mod drawio;
pub mod json;
pub mod svg;
#[cfg(feature = "raster")]
pub mod font;
#[cfg(feature = "raster")]
pub mod png;
#[cfg(feature = "raster")]
pub mod rasterize;
#[cfg(feature = "raster")]
pub mod webp;

pub use ascii::AsciiRenderer;
pub use drawio::{DrawioRenderer, ExportReport, ExportWarning, DegradeTier};
pub use json::JsonRenderer;
pub use svg::SvgRenderer;
#[cfg(feature = "raster")]
pub use font::{
    build_usvg_options, default_fonts_dir, fonts_dir, fonts_dir_from_env, set_fonts_dir,
    FONTS_DIR_ENV_VAR,
};
#[cfg(feature = "raster")]
pub use png::PngRenderer;
#[cfg(feature = "raster")]
pub use webp::WebpRenderer;

/// 编码路径：声明编码器需要哪条管线分支。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodingPath {
    /// 需要 layout → build_scene → encode_scene（SVG / PNG / JSON / Draw.io 等）
    Scene,
    /// 直接从 PreparedDiagram 编码，跳过 layout 和 scene（ASCII / Markdown / OPML / FreeMind 等）
    Diagram,
}

/// `EncodingPath::Diagram` 编码器的输出。
pub struct DiagramEncodeOutput {
    pub output: RenderOutput,
    pub report: Option<RefinementReport>,
}

/// 格式编码器 trait:将 [`ExportScene`] 编码为 [`RenderOutput`]。
///
/// 编码器只做"场景 → 字节/文本"的格式转换,不负责布局或样式物化。
/// 布局与物化由上层 [`crate::render::render_output`] 编排。
pub trait FormatEncoder {
    fn format(&self) -> RenderFormat;

    fn name(&self) -> &str;

    fn description(&self) -> &str;

    /// 将已物化的场景编码为输出格式。
    fn encode_scene(&self, scene: &ExportScene<'_>) -> Result<RenderOutput>;

    /// 将已物化的场景编码为输出格式，同时返回导出报告（降级警告等）。
    ///
    /// 默认实现调用 `encode_scene` 并返回 `None` 报告。
    /// drawio 编码器覆写此方法以返回 `ExportReport`。
    fn encode_scene_with_report(
        &self,
        scene: &ExportScene<'_>,
    ) -> Result<(RenderOutput, Option<ExportReport>)> {
        let output = self.encode_scene(scene)?;
        Ok((output, None))
    }

    /// 便捷入口:从 PreparedDiagram 走完整流水线(布局 → 物化 → 编码)。
    ///
    /// 等价于 `pipeline::render::render_output(&RenderRequest::new(diagram, self.format()))`。
    /// 推荐调用方直接使用 `pipeline::render_output` 以复用布局结果。
    fn render_diagram(&self, diagram: &PreparedDiagram) -> Result<RenderOutput> {
        let request = RenderRequest::new(diagram, self.format());
        crate::pipeline::render::render_output(&request)
    }

    fn file_extension(&self) -> &str;

    /// 声明编码路径。默认 `Scene`。
    fn encoding_path(&self) -> EncodingPath {
        EncodingPath::Scene
    }

    /// 从 PreparedDiagram 直接编码（`EncodingPath::Diagram` 的编码器必须实现）。
    /// `EncodingPath::Scene` 的编码器无需覆写，默认返回 unsupported 错误。
    fn encode_from_diagram(
        &self,
        _diagram: &PreparedDiagram,
        _layout_overlay: Option<&crate::layout::LayoutIntentOverlay>,
    ) -> crate::error::Result<DiagramEncodeOutput> {
        Err(crate::error::DrawifyError::render_internal_msg(
            "format does not support direct diagram encoding",
        ))
    }
}

use crate::interchange::mindmap::export::freemind::FreemindEncoder;
use crate::interchange::mindmap::export::markdown::MdOutlineEncoder;
use crate::interchange::mindmap::export::opml::OpmlEncoder;

static SVG_RENDERER: SvgRenderer = SvgRenderer;
static ASCII_RENDERER: AsciiRenderer = AsciiRenderer;
static JSON_RENDERER: JsonRenderer = JsonRenderer;
static DRAWIO_RENDERER: DrawioRenderer = DrawioRenderer;
#[cfg(feature = "raster")]
static PNG_RENDERER: PngRenderer = PngRenderer;
#[cfg(feature = "raster")]
static WEBP_RENDERER: WebpRenderer = WebpRenderer;
static MD_OUTLINE_RENDERER: MdOutlineEncoder = MdOutlineEncoder;
static OPML_RENDERER: OpmlEncoder = OpmlEncoder;
static FREEMIND_RENDERER: FreemindEncoder = FreemindEncoder;

/// 根据渲染格式选择具体编码器。
pub fn encoder_for(format: RenderFormat) -> Result<&'static dyn FormatEncoder> {
    match format {
        RenderFormat::Svg => Ok(&SVG_RENDERER),
        RenderFormat::Ascii => Ok(&ASCII_RENDERER),
        RenderFormat::Json => Ok(&JSON_RENDERER),
        RenderFormat::Drawio => Ok(&DRAWIO_RENDERER),
        RenderFormat::Png => {
            #[cfg(feature = "raster")]
            {
                Ok(&PNG_RENDERER)
            }
            #[cfg(not(feature = "raster"))]
            {
                Err(crate::error::DrawifyError::render_internal_msg(
                    "format 'png' requires the 'raster' feature",
                ))
            }
        }
        RenderFormat::Webp => {
            #[cfg(feature = "raster")]
            {
                Ok(&WEBP_RENDERER)
            }
            #[cfg(not(feature = "raster"))]
            {
                Err(crate::error::DrawifyError::render_internal_msg(
                    "format 'webp' requires the 'raster' feature",
                ))
            }
        }
        RenderFormat::MdOutline => Ok(&MD_OUTLINE_RENDERER),
        RenderFormat::Opml => Ok(&OPML_RENDERER),
        RenderFormat::Freemind => Ok(&FREEMIND_RENDERER),
    }
}

/// 向后兼容的 JSON 快捷入口。
pub fn render_json(diagram: &PreparedDiagram) -> String {
    encoder_for(RenderFormat::Json)
        .ok()
        .and_then(|encoder| encoder.render_diagram(diagram).ok())
        .and_then(RenderOutput::into_text)
        .unwrap_or_default()
}
