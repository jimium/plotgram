//! 笔触皮肤层（手绘、蓝图、卡通等）。
//!
//! 与 [`crate::theme`]（主题 token：颜色、字体）分工：
//! 本模块负责几何笔触装饰，作用于 [`crate::render::visual::NodeStyle`] /
//! [`crate::render::visual::EdgeStyle`]。

pub use crate::types::GraphicStyleId;
use crate::render::visual::{EdgeStyle, NodeShape, NodeStyle};
use self::excalidraw::FillMode;

mod blueprint;
pub(crate) mod common;
mod excalidraw;
mod neon_glow;
mod spatial_clarity;
mod standard;
mod stipple;

pub fn parse_graphic_style_id(value: &str) -> Option<GraphicStyleId> {
    match value {
        "standard" => Some(GraphicStyleId::Standard),
        "excalidraw" => Some(GraphicStyleId::Excalidraw),
        "cross-hatch" => Some(GraphicStyleId::CrossHatch),
        "blueprint" => Some(GraphicStyleId::Blueprint),
        "spatial-clarity" => Some(GraphicStyleId::SpatialClarity),
        "neon-glow" => Some(GraphicStyleId::NeonGlow),
        "stipple" => Some(GraphicStyleId::Stipple),
        _ => None,
    }
}

pub trait GraphicStylePainter: Sync {
    fn id(&self) -> GraphicStyleId;

    fn shared_svg_defs(&self) -> Option<String> {
        None
    }

    fn marker_defs(&self, active_stroke: &str, passive_stroke: &str) -> String {
        standard::standard_marker_defs(active_stroke, passive_stroke)
    }

    fn decorate_node_style(&self, style: &mut NodeStyle) {
        let _ = style;
    }

    fn decorate_edge_style(&self, style: &mut EdgeStyle) {
        let _ = style;
    }

    fn render_node_shape(
        &self,
        _shape: &NodeShape,
        _x: f64,
        _y: f64,
        _width: f64,
        _height: f64,
        _style: &NodeStyle,
    ) -> Option<String> {
        None
    }

    fn render_edge_line(
        &self,
        _sx: f64,
        _sy: f64,
        _ex: f64,
        _ey: f64,
        _stroke: &str,
        _style: &EdgeStyle,
        _marker_end: &str,
        _marker_start: &str,
    ) -> Option<String> {
        None
    }

    fn render_edge_path(
        &self,
        _path_data: &str,
        _stroke: &str,
        _style: &EdgeStyle,
        _marker_end: &str,
        _marker_start: &str,
    ) -> Option<String> {
        None
    }
}

pub fn painter_for(style_id: GraphicStyleId) -> &'static dyn GraphicStylePainter {
    match style_id {
        GraphicStyleId::Standard => &STANDARD_PAINTER,
        GraphicStyleId::Excalidraw => &EXCALIDRAW_PAINTER,
        GraphicStyleId::CrossHatch => &CROSS_HATCH_PAINTER,
        GraphicStyleId::Blueprint => &BLUEPRINT_PAINTER,
        GraphicStyleId::SpatialClarity => &SPATIAL_CLARITY_PAINTER,
        GraphicStyleId::NeonGlow => &NEON_GLOW_PAINTER,
        GraphicStyleId::Stipple => &STIPPLE_PAINTER,
    }
}

static STANDARD_PAINTER: standard::StandardGraphicStylePainter = standard::StandardGraphicStylePainter;
const EXCALIDRAW_PAINTER: excalidraw::ExcalidrawGraphicStylePainter = excalidraw::ExcalidrawGraphicStylePainter { fill_mode: FillMode::Hachure };
const CROSS_HATCH_PAINTER: excalidraw::ExcalidrawGraphicStylePainter = excalidraw::ExcalidrawGraphicStylePainter { fill_mode: FillMode::CrossHatch };
static BLUEPRINT_PAINTER: blueprint::BlueprintGraphicStylePainter = blueprint::BlueprintGraphicStylePainter;
static SPATIAL_CLARITY_PAINTER: spatial_clarity::SpatialClarityGraphicStylePainter =
    spatial_clarity::SpatialClarityGraphicStylePainter;
static NEON_GLOW_PAINTER: neon_glow::NeonGlowGraphicStylePainter =
    neon_glow::NeonGlowGraphicStylePainter;
static STIPPLE_PAINTER: stipple::StippleGraphicStylePainter =
    stipple::StippleGraphicStylePainter;

#[cfg(test)]
mod tests;
