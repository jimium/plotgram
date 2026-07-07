//! 图标 SVG 输出与内侧排版度量。

use crate::ast::Entity;
use crate::render::visual::NodeShape;
use crate::layout::edge::common::label_avoidance::estimate_label_width;

use super::catalog::{IconDef, IconPlacement};
use super::resolve::{resolve, ResolveOptions};
use std::fmt::Write;

const GLYPH_VIEWBOX: f64 = 24.0;

/// 内侧图标 + 标签的排版结果。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IconLayout {
    pub icon_size: f64,
    pub gap: f64,
    pub group_width: f64,
    pub group_height: f64,
    pub group_x: f64,
    pub group_y: f64,
    pub icon_x: f64,
    pub icon_y: f64,
    pub label_x: f64,
    pub label_y: f64,
}

/// 节点是否满足图标最小尺寸要求。
pub fn can_render(def: &IconDef, node_width: f64, node_height: f64) -> bool {
    def.placement == IconPlacement::Inside
        && node_width >= def.min_node_width
        && node_height >= def.min_node_height
}

/// 计算 icon + label 作为一组在节点内水平居中时的坐标。
pub fn layout_inside(
    node_x: f64,
    node_y: f64,
    node_width: f64,
    node_height: f64,
    label_width: f64,
    font_size: f64,
    def: &IconDef,
) -> Option<IconLayout> {
    if !can_render(def, node_width, node_height) {
        return None;
    }

    let icon_size = font_size * def.scale;
    let gap = def.gap;
    let group_width = icon_size + gap + label_width;
    let group_height = icon_size.max(font_size);
    let group_x = node_x + (node_width - group_width) / 2.0;
    let group_y = node_y + (node_height - group_height) / 2.0;

    let icon_x = group_x;
    let icon_y = group_y + (group_height - icon_size) / 2.0 + def.optical_align_dy;
    let label_x = group_x + icon_size + gap;
    let label_y = node_y + node_height / 2.0;

    Some(IconLayout {
        icon_size,
        gap,
        group_width,
        group_height,
        group_x,
        group_y,
        icon_x,
        icon_y,
        label_x,
        label_y,
    })
}

/// 为布局阶段估算节点额外宽度：`icon_size + gap`（有图标时）。
pub fn extra_node_width(def: &IconDef, font_size: f64) -> f64 {
    if def.placement != IconPlacement::Inside {
        return 0.0;
    }
    font_size * def.scale + def.gap
}

/// 渲染节点内侧内容：有图标时 icon+label 横排居中，否则标签居中。
pub fn render_entity_content(
    entity: &Entity,
    node_x: f64,
    node_y: f64,
    node_width: f64,
    node_height: f64,
    shape: NodeShape,
    text_color: &str,
    font_size: f64,
    font_weight: &str,
    options: &ResolveOptions,
) -> String {
    if let Some(def) = resolve(entity, shape, options) {
        let label_width = estimate_label_width(&entity.label);
        if let Some(layout) = layout_inside(
            node_x,
            node_y,
            node_width,
            node_height,
            label_width,
            font_size,
            def,
        ) {
            return render_inside(
                def,
                &layout,
                &entity.label,
                text_color,
                font_size,
                font_weight,
            );
        }
    }

    render_centered_label(
        node_x,
        node_y,
        node_width,
        node_height,
        font_size,
        text_color,
        font_weight,
        &entity.label,
    )
}

fn render_centered_label(
    node_x: f64,
    node_y: f64,
    node_width: f64,
    node_height: f64,
    font_size: f64,
    text_color: &str,
    font_weight: &str,
    label: &str,
) -> String {
    let tx = node_x + node_width / 2.0;
    let ty = node_y + node_height / 2.0 + font_size / 3.0;
    format!(
        r##"<text x="{tx:.1}" y="{ty:.1}" text-anchor="middle" font-size="{font_size}" fill="{text_color}" font-weight="{font_weight}">{label}</text>"##,
        text_color = escape_xml(text_color),
        label = escape_xml(label),
    )
}

/// 渲染单个图标 glyph（不含标签）。
pub fn render_icon(def: &IconDef, x: f64, y: f64, size: f64, color: &str) -> String {
    let scale = size / GLYPH_VIEWBOX;
    let inner = svg_inner(def.asset);
    format!(
        r#"<g transform="translate({x:.2},{y:.2}) scale({scale:.4})" color="{color}">{inner}</g>"#,
        x = x,
        y = y,
        scale = scale,
        color = escape_xml(color),
        inner = inner,
    )
}

/// 渲染内侧图标 + 标签（整组已由 [`layout_inside`] 定位）。
pub fn render_inside(
    def: &IconDef,
    layout: &IconLayout,
    label: &str,
    color: &str,
    font_size: f64,
    font_weight: &str,
) -> String {
    let mut svg = String::new();
    writeln!(
        &mut svg,
        "{}",
        render_icon(def, layout.icon_x, layout.icon_y, layout.icon_size, color)
    )
    .unwrap();
    writeln!(
        &mut svg,
        r#"<text x="{:.2}" y="{:.2}" text-anchor="start" dominant-baseline="central" font-size="{font_size}" font-weight="{font_weight}" fill="{color}">{label}</text>"#,
        layout.label_x,
        layout.label_y,
        font_size = font_size,
        font_weight = font_weight,
        color = escape_xml(color),
        label = escape_xml(label),
    )
    .unwrap();
    svg
}

/// 从完整 SVG 资产中提取 `<svg>` 内部元素。
fn svg_inner(asset: &str) -> &str {
    let start = asset.find('>').map(|idx| idx + 1).unwrap_or(0);
    let end = asset.rfind("</svg>").unwrap_or(asset.len());
    asset.get(start..end).unwrap_or("").trim()
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
