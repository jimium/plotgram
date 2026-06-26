//! 通用边 SVG 绘制。

use crate::ast::Relation;
use crate::layout::EdgeLayout;
use crate::render::paint::svg_utils::{
    arrow_style, render_bundled_edge_path, render_edge_labels, render_edge_path, BundleRenderInfo,
};
use crate::render::color_queries::muted_text_color;
use crate::render::visual::{ArrowStyle, EdgeStyle};
use crate::render::CompiledRenderContext;
use crate::types::DiagramType;

/// 标准有箭头边（flowchart / state / architecture）。
///
/// `bundle` 为 `Some` 时启用 P6 渲染增强（trunk 加粗 + 透明度叠加）。
#[allow(clippy::too_many_arguments)]
pub fn paint_arrowed_edge(
    diagram_type: &DiagramType,
    relation: &Relation,
    layout: &EdgeLayout,
    style: &EdgeStyle,
    render_labels: bool,
    context: &CompiledRenderContext,
    bundle: Option<&BundleRenderInfo<'_>>,
    svg: &mut String,
) {
    let passive_stroke = muted_text_color(diagram_type, context, "#999");
    let (stroke, dash, marker_end, marker_start) = arrow_style(relation, style, &passive_stroke);
    if let Some(bundle) = bundle {
        render_bundled_edge_path(
            layout, context, style, stroke, Some(dash), marker_end, marker_start, bundle, svg,
        );
    } else {
        render_edge_path(
            layout, context, style, stroke, Some(dash), marker_end, marker_start, svg,
        );
    }

    if render_labels {
        render_edge_labels(layout, &style.label_style, context, diagram_type, svg);
    }
}

/// 无箭头边（mindmap 等）。
///
/// `bundle` 为 `Some` 时启用 P6 渲染增强（trunk 加粗 + 透明度叠加）。
pub fn paint_plain_edge(
    layout: &EdgeLayout,
    style: &EdgeStyle,
    context: &CompiledRenderContext,
    bundle: Option<&BundleRenderInfo<'_>>,
    svg: &mut String,
) {
    if let Some(bundle) = bundle {
        render_bundled_edge_path(layout, context, style, &style.stroke, None, "", "", bundle, svg);
    } else {
        render_edge_path(layout, context, style, &style.stroke, None, "", "", svg);
    }
}

/// 是否使用箭头绘制。
pub fn uses_arrows(arrow_style: &ArrowStyle) -> bool {
    *arrow_style != ArrowStyle::None
}
