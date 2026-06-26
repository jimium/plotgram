//! 时序图 SVG 绘制与样式物化（消费 ExportScene）。

use std::fmt::Write as FmtWrite;

use crate::ast::*;
use crate::layout::node::sequence::LIFELINE_MESSAGE_GAP_HALF;
use crate::layout::{EdgeLayout, NodeLayout};
use crate::render::paint::node::paint_rect_header;
use crate::render::paint::style_mapping::{edge_paint_attrs, node_style_from_attributes, edge_style_from_attributes};
use crate::render::paint::svg_utils::{self, marker_filter_attr, marker_head_path};
use crate::render::color_queries::{edge_label_color, edge_stroke_color};
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
    let arrow_type = match relation.arrow {
        ArrowType::Active => ArrowStyle::Normal,
        ArrowType::Passive => ArrowStyle::Hollow,
        ArrowType::Bidirectional => ArrowStyle::Normal,
    };
    style.dashed = matches!(relation.arrow, ArrowType::Passive);
    style.arrow = arrow_type;
    context.graphic_painter().decorate_edge_style(&mut style);
    style
}

pub fn paint_svg_defs(context: &CompiledRenderContext) -> Option<String> {
    let active = edge_stroke_color(&DiagramType::Sequence, context, "#555");
    let passive = edge_label_color(&DiagramType::Sequence, context, "#999");
    let path = marker_head_path(context);
    let filter = marker_filter_attr(context);
    Some(format!(
        r##"
    <marker id="seq-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
        <path d="{path}" fill="{active}" {filter}/>
    </marker>
    <marker id="seq-arrow-back" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
        <path d="{path}" fill="{passive}" {filter}/>
    </marker>
"##
    ))
}

pub fn paint_export_node(node: &ExportNode<'_>, scene: &ExportScene<'_>, svg: &mut String) {
    let gap_ys = scene
        .layout
        .hints
        .sequence
        .as_ref()
        .and_then(|h| h.lifeline_gaps.get(node.entity.id.as_str()))
        .map(|v| v.as_slice())
        .unwrap_or(&[]);
    paint_sequence_node(
        node.entity,
        &node.layout,
        &node.style,
        scene.layout.total_height,
        gap_ys,
        &scene.context,
        svg,
    );
}

pub fn paint_export_edge(edge: &ExportEdge<'_>, scene: &ExportScene<'_>, svg: &mut String) {
    if edge.layout.path_len() < 2 {
        return;
    }
    paint_sequence_edge_path(edge.relation, &edge.layout, &edge.style, &scene.context, svg);
}

/// 渲染边标签（三图层顶层）。
///
/// 遍历 `edge.layout.labels` 渲染所有标签（中段/头部/尾部）。
pub fn paint_export_edge_label(edge: &ExportEdge<'_>, scene: &ExportScene<'_>, svg: &mut String) {
    if edge.layout.path_len() < 2 {
        return;
    }
    svg_utils::render_edge_labels(
        &edge.layout,
        &edge.style.label_style,
        &scene.context,
        &DiagramType::Sequence,
        svg,
    );
}

fn paint_sequence_node(
    entity: &Entity,
    node_layout: &NodeLayout,
    style: &NodeStyle,
    total_height: f64,
    lifeline_gap_ys: &[f64],
    context: &CompiledRenderContext,
    svg: &mut String,
) {
    paint_rect_header(
        entity,
        node_layout,
        style,
        &DiagramType::Sequence,
        context,
        svg,
    );

    let cx = node_layout.x + node_layout.width / 2.0;
    let lifeline_start_y = node_layout.y + node_layout.height;
    let lifeline_end_y = lifeline_start_y + (total_height - lifeline_start_y);
    paint_lifeline_segments(
        cx,
        lifeline_start_y,
        lifeline_end_y,
        lifeline_gap_ys,
        edge_stroke_color(&DiagramType::Sequence, context, "#999"),
        svg,
    );
}

fn paint_lifeline_segments(
    cx: f64,
    start_y: f64,
    end_y: f64,
    gap_ys: &[f64],
    lifeline_color: String,
    svg: &mut String,
) {
    if gap_ys.is_empty() {
        writeln!(
            svg,
            r##"<line x1="{cx}" y1="{start_y}" x2="{cx}" y2="{end_y}" stroke="{lifeline_color}" stroke-width="1" stroke-dasharray="4,4"/>"##,
            cx = cx,
            start_y = start_y,
            end_y = end_y,
            lifeline_color = lifeline_color
        )
        .unwrap();
        return;
    }

    let gap_half = LIFELINE_MESSAGE_GAP_HALF;
    let mut ranges: Vec<(f64, f64)> = gap_ys
        .iter()
        .map(|y| (y - gap_half, y + gap_half))
        .collect();
    ranges.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut merged: Vec<(f64, f64)> = Vec::with_capacity(ranges.len());
    for (lo, hi) in ranges {
        if let Some(last) = merged.last_mut() {
            if lo <= last.1 {
                last.1 = last.1.max(hi);
                continue;
            }
        }
        merged.push((lo, hi));
    }

    let mut cursor = start_y;
    for (gap_start, gap_end) in merged {
        let gap_start = gap_start.max(start_y);
        let gap_end = gap_end.min(end_y);
        if gap_start > cursor {
            writeln!(
                svg,
                r##"<line x1="{cx}" y1="{y1}" x2="{cx}" y2="{y2}" stroke="{lifeline_color}" stroke-width="1" stroke-dasharray="4,4"/>"##,
                cx = cx,
                y1 = cursor,
                y2 = gap_start,
                lifeline_color = lifeline_color
            )
            .unwrap();
        }
        cursor = cursor.max(gap_end);
    }
    if cursor < end_y {
        writeln!(
            svg,
            r##"<line x1="{cx}" y1="{y1}" x2="{cx}" y2="{y2}" stroke="{lifeline_color}" stroke-width="1" stroke-dasharray="4,4"/>"##,
            cx = cx,
            y1 = cursor,
            y2 = end_y,
            lifeline_color = lifeline_color
        )
        .unwrap();
    }
}

fn paint_sequence_edge_path(
    relation: &Relation,
    edge_layout: &EdgeLayout,
    style: &EdgeStyle,
    _context: &CompiledRenderContext,
    svg: &mut String,
) {
    let (marker_end, marker_start) = match relation.arrow {
        ArrowType::Active => ("url(#seq-arrow)", ""),
        ArrowType::Passive => ("url(#seq-arrow-back)", ""),
        ArrowType::Bidirectional => ("url(#seq-arrow)", "url(#seq-arrow)"),
    };

    let path = edge_layout.path_points();
    let path_len = path.len();

    if path_len == 2 {
        let x1 = path[0].x;
        let y1 = path[0].y;
        let x2 = path[1].x;
        let y2 = path[1].y;
        let dash = if style.dashed { "6,3" } else { "" };
        writeln!(
            svg,
            r##"<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="{stroke}" stroke-width="{stroke_width}" {paint_attrs} marker-end="{marker_end}" marker-start="{marker_start}"/>"##,
            x1 = x1, y1 = y1, x2 = x2, y2 = y2,
            stroke = style.stroke, stroke_width = style.stroke_width,
            paint_attrs = edge_paint_attrs(style, Some(if dash.is_empty() { "" } else { "6,3" })),
            marker_end = marker_end, marker_start = marker_start,
        )
        .unwrap();
    } else {
        let points: Vec<String> = path
            .iter()
            .map(|p| format!("{:.1},{:.1}", p.x, p.y))
            .collect();
        let dash = if style.dashed { "6,3" } else { "" };
        writeln!(
            svg,
            r##"<polyline points="{points}" fill="none" stroke="{stroke}" stroke-width="{stroke_width}" {paint_attrs} marker-end="{marker_end}"/>"##,
            points = points.join(" "),
            stroke = style.stroke, stroke_width = style.stroke_width,
            paint_attrs = edge_paint_attrs(style, Some(if dash.is_empty() { "" } else { "6,3" })),
            marker_end = marker_end,
        )
        .unwrap();
    }
}
