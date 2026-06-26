//! SVG 文档结构工具（头部、尾部、分组、边路径渲染）。

use crate::ast::*;
use crate::types::DiagramType;
use crate::layout::edge::edge_bundling::{EdgePathRoles, SegmentRole};
use crate::layout::geometry::Point;
use crate::layout::{EdgeLabelLayout, EdgeLayout, PathGeometry};
use crate::render::CompiledRenderContext;
use crate::render::scene::ExportScene;
use crate::render::visual::{EdgeLabelStyle, EdgeStyle, LabelRotation, NodeStyle};
use std::fmt::Write;

pub const ARROW_SIZE: f64 = 8.0;
pub const FONT_SIZE: f64 = 13.0;
pub const NODE_RX: f64 = 8.0;

pub const ATTRIBUTION_URL: &str = "https://drawify.studio";
const ATTRIBUTION_TEXT: &str = "powered by drawify";
const ATTRIBUTION_PILL_WIDTH: f64 = 108.0;
const ATTRIBUTION_PILL_HEIGHT: f64 = 16.0;
const ATTRIBUTION_FONT_SIZE: f64 = 10.0;
/// 署名锚点距画布右/底边的留白（复用 layout 已预留的 padding，不额外扩展画布）。
const ATTRIBUTION_MARGIN_RIGHT: f64 = 20.0;
const ATTRIBUTION_MARGIN_BOTTOM: f64 = 18.0;
/// 10px sans-serif 在 pill 垂直中心处的基线偏移（相对中心向下为正）。
const ATTRIBUTION_TEXT_BASELINE_OFFSET: f64 = 2.4;

pub fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// 写入 SVG 头部（含背景、标题、分组）
pub fn write_svg_preamble(
    scene: &ExportScene<'_>,
    svg: &mut String,
    extra_defs: Option<&str>,
) -> f64 {
    let diagram = scene.diagram();
    let context = &scene.context;
    let w = scene.canvas.width;
    let h = scene.canvas.height;
    let title_offset = if scene.canvas.title.is_some() { 30.0 } else { 0.0 };
    let total_h = h + title_offset;
    let canvas_background = &scene.canvas.background;
    let canvas_transparent = super::color_queries::is_transparent_canvas(canvas_background);
    let title_color = &scene.canvas.title_color;
    let marker_stroke = super::color_queries::edge_stroke(diagram, context, "#555");
    let muted_marker_stroke = super::color_queries::muted_text_color(&diagram.diagram_type, context, "#999");

    writeln!(
        svg,
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {total_h}" width="{w}" height="{total_h}" font-family="'Noto Sans CJK SC', 'Segoe UI', 'Helvetica Neue', Arial, sans-serif"{debug_attr}>"##,
        debug_attr = super::svg_debug::svg_root_attr(),
    )
    .unwrap();

    writeln!(svg, "<defs>").unwrap();
    if let Some(defs) = context.graphic_painter().shared_svg_defs() {
        writeln!(svg, "{defs}").unwrap();
    }
    writeln!(svg, "{}", standard_markers(context, &marker_stroke, &muted_marker_stroke)).unwrap();
    if let Some(defs) = extra_defs {
        write!(svg, "{defs}").unwrap();
    }
    writeln!(svg, "</defs>").unwrap();

    if !canvas_transparent {
        writeln!(
            svg,
            r##"<rect width="{w}" height="{total_h}" fill="{canvas_background}"/>"##
        )
        .unwrap();
    }

    if let Some(title) = &scene.canvas.title {
        let escaped = escape_xml(title);
        writeln!(
            svg,
            r##"<text x="{center}" y="22" text-anchor="middle" font-size="16" font-weight="bold" fill="{title_color}">{escaped}</text>"##,
            center = w / 2.0,
            title_color = title_color
        )
        .unwrap();
    }

    if title_offset > 0.0 {
        writeln!(svg, r##"<g transform="translate(0, {title_offset})">"##).unwrap();
    }

    render_groups(scene, svg);
    title_offset
}

pub fn write_svg_postamble(
    svg: &mut String,
    scene: &ExportScene<'_>,
    title_offset: f64,
    skipped_nodes: usize,
    skipped_edges: usize,
) {
    if title_offset > 0.0 {
        writeln!(svg, "</g>").unwrap();
    }

    if scene.canvas.attribution {
        write_attribution(svg, scene, title_offset);
    }

    if skipped_nodes > 0 || skipped_edges > 0 {
        writeln!(
            svg,
            "<!-- Drawify 降级渲染提示：跳过了 {} 个节点，{} 条边 -->",
            skipped_nodes, skipped_edges
        )
        .unwrap();
        writeln!(
            svg,
            "<!-- 这通常是因为节点或关系引用了不存在的实体 -->"
        )
        .unwrap();
    }

    writeln!(svg, "</svg>").unwrap();
}

fn write_attribution(
    svg: &mut String,
    scene: &ExportScene<'_>,
    title_offset: f64,
) {
    let diagram = scene.diagram();
    let layout = &scene.layout;
    let context = &scene.context;
    let canvas_background = &scene.canvas.background;
    let muted = super::color_queries::muted_text_color(&diagram.diagram_type, context, "#999");
    let style = super::color_queries::attribution_style(&canvas_background, &muted);

    let canvas_bottom = layout.total_height + title_offset;
    let x = layout.total_width - ATTRIBUTION_MARGIN_RIGHT;
    let pill_half_h = ATTRIBUTION_PILL_HEIGHT / 2.0;
    // 锚点为 pill 垂直中心，距画布底边 ATTRIBUTION_MARGIN_BOTTOM + pill 半高
    let y = canvas_bottom - ATTRIBUTION_MARGIN_BOTTOM - pill_half_h;

    writeln!(
        svg,
        r##"<g class="drawify-attribution" transform="translate({x:.1},{y:.1})">"##
    )
    .unwrap();
    writeln!(
        svg,
        r##"<rect x="-{pw:.0}" y="-{pill_half_h:.1}" width="{pw:.0}" height="{ph:.0}" rx="3" fill="{pill_fill}" fill-opacity="{pill_fill_opacity:.2}" stroke="{pill_stroke}" stroke-opacity="{pill_stroke_opacity:.2}" stroke-width="0.5"/>"##,
        pw = ATTRIBUTION_PILL_WIDTH,
        pill_half_h = pill_half_h,
        ph = ATTRIBUTION_PILL_HEIGHT,
        pill_fill = style.pill_fill,
        pill_fill_opacity = style.pill_fill_opacity,
        pill_stroke = style.pill_stroke,
        pill_stroke_opacity = style.pill_stroke_opacity,
    )
    .unwrap();
    writeln!(
        svg,
        r##"<a href="{ATTRIBUTION_URL}" target="_blank" rel="noopener noreferrer">"##
    )
    .unwrap();
    writeln!(
        svg,
        r##"<text x="-6" y="{text_y:.1}" text-anchor="end" font-size="{font_size:.0}" fill="{text_fill}" fill-opacity="{text_fill_opacity:.2}">{ATTRIBUTION_TEXT}</text>"##,
        text_y = ATTRIBUTION_TEXT_BASELINE_OFFSET,
        font_size = ATTRIBUTION_FONT_SIZE,
        text_fill = style.text_fill,
        text_fill_opacity = style.text_fill_opacity,
    )
    .unwrap();
    writeln!(svg, "</a>").unwrap();
    writeln!(svg, "</g>").unwrap();
}

pub fn render_groups(
    scene: &ExportScene<'_>,
    svg: &mut String,
) {
    // P2.2: 检查是否有需要阴影的分组，若有则添加 SVG filter 定义
    let has_any_shadow = scene.groups.iter().any(|g| g.has_shadow);
    if has_any_shadow {
        writeln!(
            svg,
            r##"<defs><filter id="group-shadow" x="-4%" y="-4%" width="108%" height="108%"><feDropShadow dx="2" dy="2" stdDeviation="4" flood-color="#000" flood-opacity="0.1"/></filter></defs>"##
        )
        .unwrap();
    }

    for export_group in &scene.groups {
        let group = export_group.group;
        let gl = &export_group.layout;
        super::svg_debug::open_group_g(group, svg);
        let stroke_dash = match group.attributes.standard.get("border_style") {
            Some(AttributeValue::String(ref s)) if s == "dashed" => "stroke-dasharray=\"8,4\"",
            Some(AttributeValue::String(ref s)) if s == "dotted" => "stroke-dasharray=\"2,4\"",
            _ => "",
        };
        let label = escape_xml(&group.label);
        let shadow_attr = if export_group.has_shadow {
            r#" filter="url(#group-shadow)""#
        } else {
            ""
        };

        writeln!(
            svg,
            r##"<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{rx}" fill="{fill}" stroke="{stroke}" stroke-width="{stroke_width}" {stroke_dash}{shadow_attr}/>"##,
            x = gl.x, y = gl.y, w = gl.width, h = gl.height,
            rx = export_group.border_radius,
            fill = export_group.fill,
            stroke = export_group.stroke,
            stroke_width = export_group.stroke_width,
        )
        .unwrap();
        writeln!(
            svg,
            r##"<text x="{tx}" y="{ty}" font-size="11" fill="{group_label}" font-weight="600">{label}</text>"##,
            tx = gl.x + 8.0,
            ty = gl.y + 14.0,
            group_label = export_group.label_color
        )
        .unwrap();
        super::svg_debug::close_g(svg);
    }
}

/// 根据 layout 边信息渲染 SVG 路径（直线 / 折线 / 贝塞尔）
pub fn render_edge_path(
    el: &EdgeLayout,
    context: &CompiledRenderContext,
    style: &EdgeStyle,
    stroke: &str,
    dash_pattern: Option<&str>,
    marker_end: &str,
    marker_start: &str,
    svg: &mut String,
) {
    if el.path_len() < 2 {
        return;
    }

    let paint_attrs = super::style_mapping::edge_paint_attrs(style, dash_pattern);

    match &el.geometry {
        PathGeometry::Bezier { start, end, controls } => {
            let sx = start.x;
            let sy = start.y;
            let ex = end.x;
            let ey = end.y;
            let cp1 = controls[0];
            let cp2 = controls[1];
            let d = format!(
                "M {sx:.1} {sy:.1} C {cp1x:.1} {cp1y:.1}, {cp2x:.1} {cp2y:.1}, {ex:.1} {ey:.1}",
                cp1x = cp1.x,
                cp1y = cp1.y,
                cp2x = cp2.x,
                cp2y = cp2.y,
            );
            if let Some(custom_svg) = context
                .graphic_painter()
                .render_edge_path(&d, stroke, style, marker_end, marker_start)
            {
                writeln!(svg, "{custom_svg}").unwrap();
            } else {
                writeln!(
                    svg,
                    r##"<path d="{d}" fill="none" stroke="{stroke}" stroke-width="{stroke_width}" {paint_attrs} marker-end="{marker_end}" marker-start="{marker_start}"/>"##,
                    stroke_width = style.stroke_width,
                    paint_attrs = paint_attrs
                )
                .unwrap();
            }
        }
        PathGeometry::Straight { start, end } => {
            let sx = start.x;
            let sy = start.y;
            let ex = end.x;
            let ey = end.y;
            if let Some(custom_svg) = context
                .graphic_painter()
                .render_edge_line(sx, sy, ex, ey, stroke, style, marker_end, marker_start)
            {
                writeln!(svg, "{custom_svg}").unwrap();
            } else {
                writeln!(
                    svg,
                    r##"<line x1="{sx:.1}" y1="{sy:.1}" x2="{ex:.1}" y2="{ey:.1}" stroke="{stroke}" stroke-width="{stroke_width}" {paint_attrs} marker-end="{marker_end}" marker-start="{marker_start}"/>"##,
                    stroke_width = style.stroke_width,
                    paint_attrs = paint_attrs
                )
                .unwrap();
            }
        }
        PathGeometry::Polyline { points } => {
            let d = rounded_polyline_path(points, CORNER_RADIUS);
            if let Some(custom_svg) = context
                .graphic_painter()
                .render_edge_path(&d, stroke, style, marker_end, marker_start)
            {
                writeln!(svg, "{custom_svg}").unwrap();
            } else {
                writeln!(
                    svg,
                    r##"<path d="{d}" fill="none" stroke="{stroke}" stroke-width="{stroke_width}" {paint_attrs} marker-end="{marker_end}" marker-start="{marker_start}"/>"##,
                    stroke_width = style.stroke_width,
                    paint_attrs = paint_attrs
                )
                .unwrap();
            }
        }
    }
}

/// P6 §6: Bundle 渲染信息——由 `paint_export_edge` 从 `scene.layout.hints.edge_bundling`
/// 提取，传入渲染层用于透明度叠加和线宽累加。
pub struct BundleRenderInfo<'a> {
    /// bundle 内边数（用于线宽累加 √n）
    pub bundle_size: usize,
    /// 路径区段分解（含 FromStub/MergeLeg/Trunk/ForkLeg/ToStub 角色）
    pub roles: &'a EdgePathRoles,
}

/// P6 §6: bundled 边的低 stroke-opacity（透明度叠加）。
///
/// 多条边的主干段几何重合，低 alpha 叠加后高密度束颜色更深。
const BUNDLE_STROKE_ALPHA: f64 = 0.35;

/// P6 §6: 渲染 bundled 边——按区段角色分拆为多个 `<path>`：
/// - **Trunk 段**：`stroke-width = base × √(n_edges)`（线宽累加），`stroke-opacity = 0.35`
/// - **非 Trunk 段**（stub/leg）：`stroke-width = base`，`stroke-opacity = 0.35`
/// - `marker-end` 仅最后一段，`marker-start` 仅第一段
///
/// 非 Polyline 几何或空 spans 时回退到 `render_edge_path`。
pub fn render_bundled_edge_path(
    el: &EdgeLayout,
    context: &CompiledRenderContext,
    style: &EdgeStyle,
    stroke: &str,
    dash_pattern: Option<&str>,
    marker_end: &str,
    marker_start: &str,
    bundle: &BundleRenderInfo<'_>,
    svg: &mut String,
) {
    let points = match &el.geometry {
        PathGeometry::Polyline { points } => points.as_slice(),
        _ => {
            render_edge_path(el, context, style, stroke, dash_pattern, marker_end, marker_start, svg);
            return;
        }
    };

    let spans = &bundle.roles.spans;
    if spans.is_empty() {
        render_edge_path(el, context, style, stroke, dash_pattern, marker_end, marker_start, svg);
        return;
    }

    let paint_attrs = super::style_mapping::edge_paint_attrs(style, dash_pattern);
    let base_width = style.stroke_width;
    let trunk_width = base_width * (bundle.bundle_size as f64).sqrt();
    let alpha = BUNDLE_STROKE_ALPHA;
    let n = spans.len();

    for (i, span) in spans.iter().enumerate() {
        let start_idx = span.point_start;
        let end_idx = span.point_end; // 不含
        if start_idx >= end_idx || end_idx > points.len() {
            continue;
        }
        let seg_points = &points[start_idx..end_idx];
        if seg_points.len() < 2 {
            continue;
        }

        let d = rounded_polyline_path(seg_points, CORNER_RADIUS);
        let width = if span.role == SegmentRole::Trunk {
            trunk_width
        } else {
            base_width
        };
        let m_end = if i == n - 1 { marker_end } else { "" };
        let m_start = if i == 0 { marker_start } else { "" };

        writeln!(
            svg,
            r##"<path d="{d}" fill="none" stroke="{stroke}" stroke-width="{width}" stroke-opacity="{alpha}" {paint_attrs} marker-end="{m_end}" marker-start="{m_start}"/>"##,
        )
        .unwrap();
    }
}

/// 折线拐弯处的圆角半径
const CORNER_RADIUS: f64 = 8.0;

/// 端口 clearance stub 索引（与 `grid_snap::protected_path_indices` 语义一致）
fn is_port_stub_index(index: usize, len: usize) -> bool {
    if len < 3 || index == 0 || index + 1 >= len {
        return false;
    }
    if index == 1 {
        return true;
    }
    len >= 5 && index == len - 2
}

/// 将折线点序列生成带圆角拐弯的 SVG path
fn rounded_polyline_path(points: &[Point], radius: f64) -> String {
    if points.len() < 2 {
        return String::new();
    }
    if points.len() == 2 {
        return format!(
            "M {:.1} {:.1} L {:.1} {:.1}",
            points[0].x, points[0].y, points[1].x, points[1].y
        );
    }

    let len = points.len();
    let mut d = format!("M {:.1} {:.1}", points[0].x, points[0].y);

    for i in 1..len - 1 {
        let prev = points[i - 1];
        let curr = points[i];
        let next = points[i + 1];

        // stub 是 clearance 直线段，不做 Q 圆角（避免 snap 后 stub 处弧半径/切线漂移）
        if is_port_stub_index(i, len) {
            d.push_str(&format!(" L {:.1} {:.1}", curr.x, curr.y));
            continue;
        }

        let (in_dx, in_dy, in_len) = unit(prev, curr);
        let (out_dx, out_dy, out_len) = unit(curr, next);

        let r = radius.min(in_len / 2.0).min(out_len / 2.0);

        let p_before = Point::new(curr.x - in_dx * r, curr.y - in_dy * r);
        let p_after = Point::new(curr.x + out_dx * r, curr.y + out_dy * r);

        d.push_str(&format!(
            " L {:.1} {:.1} Q {:.1} {:.1} {:.1} {:.1}",
            p_before.x, p_before.y, curr.x, curr.y, p_after.x, p_after.y
        ));
    }

    let last = points[len - 1];
    d.push_str(&format!(" L {:.1} {:.1}", last.x, last.y));
    d
}

fn unit(a: Point, b: Point) -> (f64, f64, f64) {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-6 {
        (0.0, 0.0, 0.0)
    } else {
        (dx / len, dy / len, len)
    }
}

/// 根据 ArrowType 获取箭头样式
pub fn arrow_style<'a>(
    relation: &Relation,
    style: &'a EdgeStyle,
    passive_stroke: &'a str,
) -> (&'a str, &'static str, &'static str, &'static str) {
    let dash = if matches!(relation.arrow, ArrowType::Passive) {
        "6,3"
    } else {
        ""
    };

    match relation.arrow {
        ArrowType::Active => (&style.stroke, dash, "url(#arrow-active)", ""),
        ArrowType::Passive => (passive_stroke, dash, "url(#arrow-passive)", ""),
        ArrowType::Bidirectional => (&style.stroke, dash, "url(#arrow-bidi)", "url(#arrow-bidi)"),
    }
}

/// 渲染单个边标签：背景矩形 + 文字（带 padding 与圆角）+ 可选引线。
///
/// 标签位置 `label.center` 为标签包围框的几何中心（中心语义）。
/// 标签尺寸 `label.size` 已含 padding（由路由器通过 `label_metrics` 统一计算）。
///
/// 旋转：根据 `label_style.rotation` 决定旋转角度：
/// - `None` → 不旋转
/// - `Fixed(θ)` → 旋转 θ 度
/// - `AlongEdge` → 使用 `label.rotation`（路由阶段预计算的路径切线角度）
/// 旋转通过 SVG `<g transform="rotate(...)">` 包裹背景矩形与文字实现；
/// 引线不旋转（它连接标签实际位置与边路径）。
///
/// 渲染层级：调用方应保证标签在边路径与节点之上（三图层 edges → nodes → labels）。
pub fn render_edge_label(
    label: &EdgeLabelLayout,
    label_style: &EdgeLabelStyle,
    context: &CompiledRenderContext,
    diagram_type: &DiagramType,
    svg: &mut String,
) {
    let escaped = escape_xml(&label.text);
    let lx = label.center.x;
    let ly = label.center.y;
    let (w, h) = label.size;

    // 文字颜色：优先 compiled theme 中的 text_fill/label_color，回退到 label_style.text_color
    let edge = context
        .compiled
        .edge_block(diagram_type.style_key(), None);
    let color = edge
        .get("text_fill")
        .and_then(|v| v.as_str())
        .or_else(|| edge.get("label_color").and_then(|v| v.as_str()))
        .unwrap_or(&label_style.text_color)
        .to_string();

    // 引线：标签被推离边路径时，从标签包围框边缘连接到边路径上的最近点
    // 引线不参与旋转，连接标签实际位置与边路径
    if let Some(leader_to) = label.leader_to {
        let (exit_x, exit_y) = bbox_exit_point((lx, ly), (w, h), (leader_to.x, leader_to.y));
        writeln!(
            svg,
            r##"<line x1="{ex:.1}" y1="{ey:.1}" x2="{tx:.1}" y2="{ty:.1}" stroke="#666" stroke-width="0.8" stroke-opacity="0.6" />"##,
            ex = exit_x,
            ey = exit_y,
            tx = leader_to.x,
            ty = leader_to.y,
        )
        .unwrap();
    }

    // 计算有效旋转角度
    let angle = match &label_style.rotation {
        LabelRotation::None => 0.0,
        LabelRotation::Fixed(a) => *a,
        LabelRotation::AlongEdge => label.rotation,
    };
    let need_rotate = angle.abs() > 1e-6;
    if need_rotate {
        writeln!(
            svg,
            r##"<g transform="rotate({a:.2} {cx:.1} {cy:.1})">"##,
            a = angle,
            cx = lx,
            cy = ly,
        )
        .unwrap();
    }

    // 背景矩形（半透明白底，避免边路径穿过文字）
    if let Some(ref bg) = label_style.bg_color {
        let rx = lx - w / 2.0;
        let ry = ly - h / 2.0;
        let border_attr = label_style
            .border_color
            .as_ref()
            .map(|c| {
                format!(
                    " stroke=\"{}\" stroke-width=\"{:.1}\"",
                    escape_xml(c),
                    label_style.border_width
                )
            })
            .unwrap_or_default();
        writeln!(
            svg,
            r##"<rect x="{rx:.1}" y="{ry:.1}" width="{w:.1}" height="{h:.1}" rx="{br:.1}" fill="{bg}" fill-opacity="{op:.2}"{border}/>"##,
            rx = rx,
            ry = ry,
            w = w,
            h = h,
            br = label_style.border_radius,
            bg = escape_xml(bg),
            op = label_style.bg_opacity,
            border = border_attr,
        )
        .unwrap();
    }

    // 文字（中心对齐）
    writeln!(
        svg,
        r##"<text x="{lx:.1}" y="{ly:.1}" text-anchor="middle" dominant-baseline="central" font-size="{fs:.1}" fill="{color}">{escaped}</text>"##,
        lx = lx,
        ly = ly,
        fs = label_style.font_size,
        color = escape_xml(&color),
        escaped = escaped,
    )
    .unwrap();

    if need_rotate {
        writeln!(svg, "</g>").unwrap();
    }
}

/// 渲染一条边的所有标签（中段/头部/尾部）。
///
/// 在三图层架构的顶层调用，遍历 `el.labels` 逐个渲染。
pub fn render_edge_labels(
    el: &EdgeLayout,
    label_style: &EdgeLabelStyle,
    context: &CompiledRenderContext,
    diagram_type: &DiagramType,
    svg: &mut String,
) {
    for label in &el.labels {
        render_edge_label(label, label_style, context, diagram_type, svg);
    }
}

pub fn standard_markers(
    context: &CompiledRenderContext,
    active_stroke: &str,
    passive_stroke: &str,
) -> String {
    context
        .graphic_painter()
        .marker_defs(active_stroke, passive_stroke)
}

pub fn marker_head_path(context: &CompiledRenderContext) -> &'static str {
    match context.graphic_style.as_str() {
        "excalidraw" | "cross-hatch" => "M 2 2 L 11 6 L 2 10 L 4 6 z",
        "blueprint" => "M 0 1 L 10 5 L 0 9",
        "spatial-clarity" => "M 1.5 2 L 9 5 L 1.5 8 L 3 5 Z",
        _ => "M 0 0 L 10 5 L 0 10 z",
    }
}

pub fn marker_filter_attr(_context: &CompiledRenderContext) -> &'static str {
    ""
}

pub fn label_weight<'a>(style: &'a NodeStyle, fallback: &'a str) -> &'a str {
    style.label_weight.as_deref().unwrap_or(fallback)
}

/// 计算从矩形包围框中心射向外部点的射线与包围框边界的交点。
///
/// `(center, size, target)` → 交点坐标。用于引线起点（避免引线穿透标签背景）。
fn bbox_exit_point(
    center: (f64, f64),
    size: (f64, f64),
    target: (f64, f64),
) -> (f64, f64) {
    let dx = target.0 - center.0;
    let dy = target.1 - center.1;
    let hw = size.0 / 2.0;
    let hh = size.1 / 2.0;

    // 退化：标签中心即目标点
    if dx.abs() < 1e-9 && dy.abs() < 1e-9 {
        return center;
    }

    // 计算射线与矩形四条边的交点参数 t（取最小正 t）
    // 右/左边: t = hw/|dx|（dx>0 命中右边，dx<0 命中左边）
    // 下/上边: t = hh/|dy|（dy>0 命中下边，dy<0 命中上边）
    let mut t_min = f64::INFINITY;
    if dx.abs() > 1e-9 {
        t_min = t_min.min(hw / dx.abs());
    }
    if dy.abs() > 1e-9 {
        t_min = t_min.min(hh / dy.abs());
    }

    if t_min.is_infinite() {
        return center;
    }

    (center.0 + dx * t_min, center.1 + dy * t_min)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::geometry::Point;

    #[test]
    fn rounded_polyline_skips_q_arc_at_port_stub() {
        let points = [
            Point::new(100.0, 100.0),
            Point::new(100.0, 116.0),
            Point::new(200.0, 116.0),
            Point::new(200.0, 200.0),
        ];
        let d = rounded_polyline_path(&points, CORNER_RADIUS);
        assert!(d.contains(" L 100.0 116.0"));
        assert!(!d.contains("Q 100.0 116.0"));
    }

    #[test]
    fn rounded_polyline_keeps_q_arc_at_channel_corner() {
        let points = [
            Point::new(100.0, 100.0),
            Point::new(100.0, 116.0),
            Point::new(200.0, 116.0),
            Point::new(200.0, 200.0),
        ];
        let d = rounded_polyline_path(&points, CORNER_RADIUS);
        assert!(d.contains("Q 200.0 116.0"));
    }

    // ─── bbox_exit_point ──────────────────────────────────────────

    #[test]
    fn bbox_exit_point_right_edge() {
        // center=(0,0), size=(20,10) → hw=10, hh=5
        // target=(20,0) → dx=20, dy=0 → exit at right edge (10,0)
        let (ex, ey) = bbox_exit_point((0.0, 0.0), (20.0, 10.0), (20.0, 0.0));
        assert!((ex - 10.0).abs() < 1e-9);
        assert!((ey - 0.0).abs() < 1e-9);
    }

    #[test]
    fn bbox_exit_point_top_edge() {
        // center=(0,0), size=(20,10), target=(0,-20) → exit at top edge (0,-5)
        let (ex, ey) = bbox_exit_point((0.0, 0.0), (20.0, 10.0), (0.0, -20.0));
        assert!((ex - 0.0).abs() < 1e-9);
        assert!((ey - (-5.0)).abs() < 1e-9);
    }

    #[test]
    fn bbox_exit_point_diagonal() {
        // center=(0,0), size=(20,20) → hw=10, hh=10
        // target=(10,10) → dx=10, dy=10
        // t_right = 10/10 = 1.0, t_bottom = 10/10 = 1.0 → exit at (10,10)（角点）
        let (ex, ey) = bbox_exit_point((0.0, 0.0), (20.0, 20.0), (10.0, 10.0));
        assert!((ex - 10.0).abs() < 1e-9);
        assert!((ey - 10.0).abs() < 1e-9);
    }

    #[test]
    fn bbox_exit_point_diagonal_hits_side_first() {
        // center=(0,0), size=(20,10) → hw=10, hh=5
        // target=(10,5) → dx=10, dy=5
        // t_right = 10/10 = 1.0, t_bottom = 5/5 = 1.0 → 角点 (10,5)
        let (ex, ey) = bbox_exit_point((0.0, 0.0), (20.0, 10.0), (10.0, 5.0));
        assert!((ex - 10.0).abs() < 1e-9);
        assert!((ey - 5.0).abs() < 1e-9);
    }

    #[test]
    fn bbox_exit_point_target_equals_center() {
        // target == center → 返回 center
        let (ex, ey) = bbox_exit_point((5.0, 5.0), (20.0, 10.0), (5.0, 5.0));
        assert_eq!((ex, ey), (5.0, 5.0));
    }

    #[test]
    fn bbox_exit_point_offset_center() {
        // center=(100,200), size=(40,20) → hw=20, hh=10
        // target=(100, 300) → dx=0, dy=100 → exit at bottom edge (100, 210)
        let (ex, ey) = bbox_exit_point((100.0, 200.0), (40.0, 20.0), (100.0, 300.0));
        assert!((ex - 100.0).abs() < 1e-9);
        assert!((ey - 210.0).abs() < 1e-9);
    }
}
