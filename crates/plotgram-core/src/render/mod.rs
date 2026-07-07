//! 渲染服务层:提供 scene / paint / encode 等服务模块。
//!
//! 本模块只做 re-export,不包含编排逻辑。
//! 渲染编排(compute_layout → build_scene → encode)位于 [`crate::pipeline::render`]。
//!
//! - [`scene`]:布局 + 主题/样式物化 → [`ExportScene`]
//! - [`paint`]:SVG 绘制原语(路径、颜色、样式映射)
//! - [`encode`]:格式编码(SVG / JSON / ASCII / PNG / WebP)
//!
//! 图表类型差异见 [`crate::kinds`],笔触皮肤见 [`crate::graphic_style`]。

pub mod encode;
pub mod output;
pub mod paint;
pub mod request;
pub mod scene;
pub mod visual;

use std::fmt;

pub use crate::graphic_style::parse_graphic_style_id;
pub use encode::{
    encoder_for, AsciiRenderer, DiagramEncodeOutput, DrawioRenderer, EncodingPath, FormatEncoder,
    JsonRenderer, SvgRenderer,
};
#[cfg(feature = "raster")]
pub use encode::{PngRenderer, WebpRenderer};
#[cfg(feature = "raster")]
pub use encode::{
    build_usvg_options, default_fonts_dir, fonts_dir, fonts_dir_from_env, set_fonts_dir,
    FONTS_DIR_ENV_VAR,
};
pub use output::RenderOutput;
pub use paint::{color_queries, style_mapping, svg_utils};
pub use request::RenderRequest;
pub use crate::theme::CompiledRenderContext;
pub use scene::{
    build_scene, compute_layout, export_scene, ExportCanvas, ExportEdge, ExportGroup, ExportNode,
    ExportScene,
};
pub use visual::{ArrowStyle, EdgeLabelStyle, EdgeStyle, LabelAnchor, NodeShape, NodeStyle};

/// 渲染格式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderFormat {
    Svg,
    Ascii,
    Png,
    Webp,
    Json,
    Drawio,
    MdOutline,
    Opml,
    Freemind,
}

impl fmt::Display for RenderFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RenderFormat::Svg => write!(f, "svg"),
            RenderFormat::Ascii => write!(f, "ascii"),
            RenderFormat::Png => write!(f, "png"),
            RenderFormat::Webp => write!(f, "webp"),
            RenderFormat::Json => write!(f, "json"),
            RenderFormat::Drawio => write!(f, "drawio"),
            RenderFormat::MdOutline => write!(f, "md-outline"),
            RenderFormat::Opml => write!(f, "opml"),
            RenderFormat::Freemind => write!(f, "freemind"),
        }
    }
}

impl RenderFormat {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "svg" => Some(RenderFormat::Svg),
            "ascii" | "text" => Some(RenderFormat::Ascii),
            "png" => Some(RenderFormat::Png),
            "webp" => Some(RenderFormat::Webp),
            "json" => Some(RenderFormat::Json),
            "drawio" => Some(RenderFormat::Drawio),
            "md-outline" | "md" => Some(RenderFormat::MdOutline),
            "opml" => Some(RenderFormat::Opml),
            "freemind" | "mm" => Some(RenderFormat::Freemind),
            _ => None,
        }
    }

    pub fn file_extension(self) -> &'static str {
        match self {
            RenderFormat::Svg => "svg",
            RenderFormat::Ascii => "txt",
            RenderFormat::Png => "png",
            RenderFormat::Webp => "webp",
            RenderFormat::Json => "json",
            RenderFormat::Drawio => "drawio",
            RenderFormat::MdOutline => "md",
            RenderFormat::Opml => "opml",
            RenderFormat::Freemind => "mm",
        }
    }
}
