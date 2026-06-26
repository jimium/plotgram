//! 通用节点 SVG 绘制（形状 + 图标/标签）。

use std::fmt::Write;

use crate::ast::Entity;
use crate::icons::render_entity_content;
use crate::layout::NodeLayout;
use crate::render::paint::svg_utils::{label_weight, FONT_SIZE};
use crate::render::color_queries::{entity_label_font_size, entity_text_fill};
use crate::render::visual::{NodeShape, NodeStyle};
use crate::render::CompiledRenderContext;
use crate::types::DiagramType;

/// 绘制带标签/图标的节点（使用 scene 已物化的样式）。
pub fn paint_labeled_node(
    diagram_type: &DiagramType,
    entity: &Entity,
    layout: &NodeLayout,
    style: &NodeStyle,
    label_weight_default: &str,
    context: &CompiledRenderContext,
    svg: &mut String,
) {
    let text_color = entity_text_fill(entity, diagram_type, context, "#333");
    let font_size = entity_label_font_size(entity, diagram_type, context, FONT_SIZE);

    let shape_svg = style.shape.render_with_context(
        layout.x,
        layout.y,
        layout.width,
        layout.height,
        style,
        context,
    );
    writeln!(svg, "{shape_svg}").unwrap();

    let content = render_entity_content(
        entity,
        layout.x,
        layout.y,
        layout.width,
        layout.height,
        style.shape.clone(),
        &text_color,
        font_size,
        label_weight(style, label_weight_default),
        &context.icon_resolve,
    );
    writeln!(svg, "{content}").unwrap();
}

/// 绘制矩形参与者头 + 生命线（时序图）。
pub fn paint_rect_header(
    entity: &Entity,
    node_layout: &NodeLayout,
    style: &NodeStyle,
    diagram_type: &DiagramType,
    context: &CompiledRenderContext,
    svg: &mut String,
) {
    let shape = NodeShape::Rect;
    let shape_svg = shape.render_with_context(
        node_layout.x,
        node_layout.y,
        node_layout.width,
        node_layout.height,
        style,
        context,
    );
    writeln!(svg, "{shape_svg}").unwrap();

    let text_color = entity_text_fill(entity, diagram_type, context, "#333");
    let font_size = entity_label_font_size(entity, diagram_type, context, 12.0);
    let content = render_entity_content(
        entity,
        node_layout.x,
        node_layout.y,
        node_layout.width,
        node_layout.height,
        shape,
        &text_color,
        font_size,
        label_weight(style, "500"),
        &context.icon_resolve,
    );
    writeln!(svg, "{content}").unwrap();
}
