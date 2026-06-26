//! ER 图 SVG 绘制与样式物化（消费 ExportScene）。

use std::fmt::Write;

use crate::ast::{Entity, Relation};
use crate::kinds::er::semantics::{
    entity_columns, point_along_path, relation_cardinality, relation_semantic_label, ErColumnKind,
    ER_HEADER_HEIGHT, ER_ROW_HEIGHT,
};
use crate::layout::geometry::Point;
use crate::layout::{EdgeLayout, NodeLayout};
use crate::render::paint::style_mapping::{edge_style_from_attributes, node_style_from_attributes};
use crate::render::paint::svg_utils::{self, render_edge_path};
use crate::render::color_queries::{
    edge_label_color, edge_stroke_color, entity_label_font_size, entity_text_fill, group_fill_color,
    group_stroke_color,
};
use crate::render::visual::{ArrowStyle, EdgeStyle, NodeStyle};
use crate::render::{ExportEdge, ExportNode, ExportScene, CompiledRenderContext};
use crate::types::DiagramType;

pub fn materialize_node_style(entity: &Entity, context: &CompiledRenderContext) -> NodeStyle {
    let mut style = node_style_from_attributes(entity);
    context.graphic_painter().decorate_node_style(&mut style);
    style
}

pub fn materialize_edge_style(relation: &Relation, context: &CompiledRenderContext) -> EdgeStyle {
    let mut style = edge_style_from_attributes(relation);
    style.arrow = ArrowStyle::None;
    context.graphic_painter().decorate_edge_style(&mut style);
    style
}

pub fn paint_svg_defs(_context: &CompiledRenderContext) -> Option<String> {
    None
}

pub fn paint_export_node(node: &ExportNode<'_>, scene: &ExportScene<'_>, svg: &mut String) {
    paint_er_node(node.entity, &node.layout, &node.style, &scene.context, svg);
}

pub fn paint_export_edge(edge: &ExportEdge<'_>, scene: &ExportScene<'_>, svg: &mut String) {
    if edge.layout.path_len() < 2 {
        return;
    }
    paint_er_edge(edge.relation, &edge.layout, &edge.style, &scene.context, svg);
}

fn paint_er_node(
    entity: &Entity,
    node_layout: &NodeLayout,
    style: &NodeStyle,
    context: &CompiledRenderContext,
    svg: &mut String,
) {
    let label = svg_utils::escape_xml(&entity.label);
    let text_color = entity_text_fill(entity, &DiagramType::Er, context, "#333");
    let header_stroke = style.stroke.clone();

    let shape_svg = style.shape.render_with_context(
        node_layout.x,
        node_layout.y,
        node_layout.width,
        node_layout.height,
        style,
        context,
    );
    writeln!(svg, "{shape_svg}").unwrap();

    let header_y = node_layout.y + ER_HEADER_HEIGHT;
    writeln!(
        svg,
        r##"<line x1="{x}" y1="{y}" x2="{x2}" y2="{y}" stroke="{stroke}" stroke-width="1.5"/>"##,
        x = node_layout.x,
        x2 = node_layout.x + node_layout.width,
        y = header_y,
        stroke = header_stroke
    )
    .unwrap();

    let font_size = entity_label_font_size(entity, &DiagramType::Er, context, 12.0);
    writeln!(
        svg,
        r##"<text x="{cx}" y="{cy}" text-anchor="middle" font-size="{font_size}" font-weight="bold" fill="{text_color}">{label}</text>"##,
        cx = node_layout.x + node_layout.width / 2.0,
        cy = node_layout.y + ER_HEADER_HEIGHT / 2.0 + 4.0,
        label = label,
        text_color = text_color
    )
    .unwrap();

    let columns = entity_columns(entity);
    let pk_color = edge_stroke_color(&DiagramType::Er, context, "#1565C0");
    let fk_color = edge_stroke_color(&DiagramType::Er, context, "#C62828");
    let field_color = edge_label_color(&DiagramType::Er, context, "#455A64");

    let attr_x = node_layout.x + 10.0;
    let mut attr_y = node_layout.y + ER_HEADER_HEIGHT + 12.0;

    for column in &columns {
        let (prefix, color) = match column.kind {
            ErColumnKind::PrimaryKey => ("PK ", pk_color.as_str()),
            ErColumnKind::ForeignKey => ("FK ", fk_color.as_str()),
            ErColumnKind::Field => ("", field_color.as_str()),
        };
        let text = svg_utils::escape_xml(&column.name);
        writeln!(
            svg,
            r##"<text x="{x}" y="{y}" font-size="10" fill="{color}">{prefix}{text}</text>"##,
            x = attr_x,
            y = attr_y,
            color = color,
            prefix = prefix,
            text = text
        )
        .unwrap();
        attr_y += ER_ROW_HEIGHT;
    }
}

fn paint_er_edge(
    relation: &Relation,
    edge_layout: &EdgeLayout,
    style: &EdgeStyle,
    context: &CompiledRenderContext,
    svg: &mut String,
) {
    render_edge_path(edge_layout, context, style, &style.stroke, None, "", "", svg);

    let lp = edge_layout.label_pos();
    let mx = lp.x;
    let my = lp.y;
    let diamond_half = 12.0;
    let diamond_fill = group_fill_color(&DiagramType::Er, context, "#FFF9C4");
    let diamond_stroke = group_stroke_color(&DiagramType::Er, context, "#F57F17");

    writeln!(
        svg,
        r##"<polygon points="{mx},{my_top} {mx_left},{my} {mx},{my_bottom} {mx_right},{my}" fill="{diamond_fill}" stroke="{diamond_stroke}" stroke-width="1.5"/>"##,
        mx = mx,
        my = my,
        mx_left = mx - diamond_half,
        my_top = my - diamond_half,
        mx_right = mx + diamond_half,
        my_bottom = my + diamond_half,
        diamond_fill = diamond_fill,
        diamond_stroke = diamond_stroke
    )
    .unwrap();

    if let Some(semantic) = relation_semantic_label(relation) {
        let label_text = svg_utils::escape_xml(&semantic);
        let label_color = edge_label_color(&DiagramType::Er, context, "#333");
        writeln!(
            svg,
            r##"<text x="{mx}" y="{my}" text-anchor="middle" dominant-baseline="central" font-size="9" font-weight="bold" fill="{label_color}">{label_text}</text>"##,
            mx = mx,
            my = my,
            label_text = label_text,
            label_color = label_color
        )
        .unwrap();
    }

    if let Some((from_card, to_card)) = relation_cardinality(relation) {
        let card_color = edge_stroke_color(&DiagramType::Er, context, "#C62828");
        let sampled = edge_layout.sampled_path(12);
        paint_cardinality_marker(&sampled, 0.14, &from_card, &card_color, svg);
        paint_cardinality_marker(&sampled, 0.86, &to_card, &card_color, svg);
    }
}

fn paint_cardinality_marker(
    path: &[Point],
    t: f64,
    text: &str,
    color: &str,
    svg: &mut String,
) {
    let Some((pos, angle)) = point_along_path(path, t) else {
        return;
    };
    let x = pos.x;
    let y = pos.y;

    let perp_x = -angle.sin();
    let perp_y = angle.cos();
    let offset = 10.0;
    let tx = x + perp_x * offset;
    let ty = y + perp_y * offset;

    let escaped = svg_utils::escape_xml(text);
    let box_w = (escaped.chars().count() as f64 * 6.5 + 10.0).clamp(18.0, 36.0);
    writeln!(
        svg,
        r##"<rect x="{rx:.1}" y="{ry:.1}" width="{w:.1}" height="14" rx="3" fill="#FFFFFF" stroke="{color}" stroke-width="0.8"/>
<text x="{tx:.1}" y="{ty:.1}" text-anchor="middle" dominant-baseline="central" font-size="9" font-weight="bold" fill="{color}">{escaped}</text>"##,
        rx = tx - box_w / 2.0,
        ry = ty - 7.0,
        w = box_w,
        tx = tx,
        ty = ty,
        color = color,
        escaped = escaped
    )
    .unwrap();
}
