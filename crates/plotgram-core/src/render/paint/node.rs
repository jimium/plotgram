//! 通用节点 SVG 绘制（形状 + 图标/标签）。

use std::fmt::Write;

use crate::ast::Entity;
use crate::icons::render_entity_content;
use crate::layout::NodeLayout;
use crate::render::paint::svg_utils::{label_weight, FONT_SIZE};
use crate::render::color_queries::{entity_label_font_size, entity_text_fill};
use crate::render::visual::{NodeShape, NodeStyle};
use crate::render::CompiledRenderContext;
use crate::theme::compile::darken;
use crate::types::DiagramType;

/// 图标颜色加深比例：取节点边框色后向黑色混合 15%。
const ICON_DARKEN_AMOUNT: f64 = 0.15;

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
    let icon_color = darken(&style.stroke, ICON_DARKEN_AMOUNT);
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

    // 状态图 initial：伪状态实心圆，圆内不画字（28px 放不下标签会遮挡）。
    // 非空 label 画在圆右侧；外置文字用标题色（深色），不用圆内的浅色 text_fill。
    if is_state_initial(diagram_type, entity) {
        if !entity.label.trim().is_empty() {
            let tx = layout.x + layout.width + 6.0;
            let ty = layout.y + layout.height / 2.0 + font_size / 3.0;
            let external_color = context
                .compiled
                .title_block(diagram_type.style_key())
                .get("fill")
                .and_then(|v| v.as_str())
                .unwrap_or("#18181B");
            writeln!(
                svg,
                r##"<text x="{tx:.1}" y="{ty:.1}" text-anchor="start" font-size="{font_size}" fill="{external_color}" font-weight="{weight}">{label}</text>"##,
                external_color = crate::render::paint::svg_utils::escape_xml(external_color),
                weight = label_weight(style, label_weight_default),
                label = crate::render::paint::svg_utils::escape_xml(&entity.label),
            )
            .unwrap();
        }
        return;
    }

    let content = render_entity_content(
        entity,
        layout.x,
        layout.y,
        layout.width,
        layout.height,
        style.shape.clone(),
        &text_color,
        &icon_color,
        font_size,
        label_weight(style, label_weight_default),
        &context.icon_resolve,
    );
    writeln!(svg, "{content}").unwrap();
}

fn is_state_initial(diagram_type: &DiagramType, entity: &Entity) -> bool {
    *diagram_type == DiagramType::State
        && entity
            .attributes
            .standard
            .get("type")
            .and_then(|v| v.as_str())
            == Some(crate::types::attr_constants::entity_type::INITIAL)
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
    let icon_color = darken(&style.stroke, ICON_DARKEN_AMOUNT);
    let font_size = entity_label_font_size(entity, diagram_type, context, 12.0);
    let content = render_entity_content(
        entity,
        node_layout.x,
        node_layout.y,
        node_layout.width,
        node_layout.height,
        shape,
        &text_color,
        &icon_color,
        font_size,
        label_weight(style, "500"),
        &context.icon_resolve,
    );
    writeln!(svg, "{content}").unwrap();
}
