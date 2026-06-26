use crate::types::GraphicStyleId;
use crate::render::visual::{EdgeStyle, NodeShape, NodeStyle};
use super::GraphicStylePainter;
use super::common::*;

pub struct BlueprintGraphicStylePainter;

impl GraphicStylePainter for BlueprintGraphicStylePainter {
    fn id(&self) -> GraphicStyleId {
        GraphicStyleId::Blueprint
    }

    fn marker_defs(&self, active_stroke: &str, passive_stroke: &str) -> String {
        blueprint_marker_defs(active_stroke, passive_stroke)
    }

    fn decorate_node_style(&self, style: &mut NodeStyle) {
        style.stroke_width = style.stroke_width.min(1.0).max(0.6);
        style.stroke_linecap = Some("butt".to_string());
        style.stroke_linejoin = Some("miter".to_string());
        style.hand_drawn = true;
    }

    fn decorate_edge_style(&self, style: &mut EdgeStyle) {
        style.stroke_width = style.stroke_width.min(1.0).max(0.6);
        style.stroke_linecap = Some("butt".to_string());
        style.stroke_linejoin = Some("miter".to_string());
        style.hand_drawn = true;
    }

    fn render_node_shape(
        &self,
        shape: &NodeShape,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        style: &NodeStyle,
    ) -> Option<String> {
        Some(render_blueprint_node_shape(shape, x, y, width, height, style))
    }

    fn render_edge_line(
        &self,
        sx: f64,
        sy: f64,
        ex: f64,
        ey: f64,
        stroke: &str,
        style: &EdgeStyle,
        marker_end: &str,
        marker_start: &str,
    ) -> Option<String> {
        let points = [Point::new(sx, sy), Point::new(ex, ey)];
        Some(render_blueprint_edge(&points, stroke, style, marker_end, marker_start))
    }

    fn render_edge_path(
        &self,
        path_data: &str,
        stroke: &str,
        style: &EdgeStyle,
        marker_end: &str,
        marker_start: &str,
    ) -> Option<String> {
        let (points, _) = svg_path_to_points(path_data)?;
        Some(render_blueprint_edge(&points, stroke, style, marker_end, marker_start))
    }
}

// ---------------------------------------------------------------------------
// Marker defs
// ---------------------------------------------------------------------------

fn blueprint_marker_defs(active_stroke: &str, passive_stroke: &str) -> String {
    const STYLE: ArrowMarkerStyle = ArrowMarkerStyle {
        view_box: "0 0 10 10",
        ref_x: "10",
        ref_y: "5",
        marker_width: "7",
        marker_height: "7",
        active_shape: r#"<path d="M 0 1 L 10 5 L 0 9" fill="none" stroke="{stroke}" stroke-width="1"/>"#,
        passive_shape: r#"<path d="M 0 1 L 10 5 L 0 9" fill="none" stroke="{stroke}" stroke-width="1"/>"#,
    };
    arrow_markers(&STYLE, active_stroke, passive_stroke)
}

// ---------------------------------------------------------------------------
// Blueprint node rendering
// ---------------------------------------------------------------------------

fn render_blueprint_node_shape(
    shape: &NodeShape,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    style: &NodeStyle,
) -> String {
    dispatch_node_shape(shape, x, y, width, height, style,
        render_blueprint_closed_shape,
        render_blueprint_cylinder,
        render_blueprint_person,
        render_blueprint_subprocess,
    )
}

/// Blueprint style: precise thin lines with dash patterns for secondary shapes,
/// semi-transparent fill, miter joins.
/// `dashed` controls whether the outline uses a dash pattern (for secondary shapes).
fn render_blueprint_closed_shape(points: &[Point], style: &NodeStyle, shape: &NodeShape) -> String {
    let dashed = matches!(shape, NodeShape::Diamond | NodeShape::Hexagon);
    let (x, y, width, height) = bounding_box(points);
    let group_attrs = node_group_attrs(style, "blueprint", x, y, width, height);
    let fill_path = polyline_path(points, true);
    let dash_attr = if dashed {
        r#"stroke-dasharray="6 3""#
    } else {
        ""
    };

    let center_line = if matches!(shape, NodeShape::Circle) {
        blueprint_center_cross(points, style.stroke_width)
    } else {
        String::new()
    };

    format!(
        r##"{group_open}<path d="{fill_path}" fill="{fill}" fill-opacity="0.15" stroke="{stroke}" stroke-width="{stroke_width}" stroke-linejoin="miter" stroke-linecap="butt" {dash_attr}/>{center_line}</g>"##,
        group_open = group_open(&group_attrs),
        fill_path = fill_path,
        fill = style.fill,
        stroke = style.stroke,
        stroke_width = style.stroke_width,
        dash_attr = dash_attr,
        center_line = center_line,
    )
}

fn render_blueprint_cylinder(x: f64, y: f64, width: f64, height: f64, style: &NodeStyle) -> String {
    let rx = width / 2.0;
    let ry = (height * 0.14).clamp(6.0, 12.0);
    let cx = x + rx;
    let top_cy = y + ry;
    let bottom_cy = y + height - ry;

    let fill_body = format!(
        "M {x:.1} {top_cy:.1} A {rx:.1} {ry:.1} 0 0 1 {right:.1} {top_cy:.1} L {right:.1} {bottom_cy:.1} A {rx:.1} {ry:.1} 0 0 1 {x:.1} {bottom_cy:.1} Z",
        right = x + width,
    );
    let top_path = format!(
        "M {x:.1} {top_cy:.1} A {rx:.1} {ry:.1} 0 0 1 {right:.1} {top_cy:.1} A {rx:.1} {ry:.1} 0 0 1 {x:.1} {top_cy:.1}",
        right = x + width,
    );
    let bottom_front = ellipse_arc_points(cx, bottom_cy, rx, ry, 0.0, std::f64::consts::PI, 16);
    let group_attrs = node_group_attrs(style, "blueprint", x, y, width, height);

    // Center axis line for cylinder
    let center_line = format!(
        r#"<line x1="{cx:.1}" y1="{y:.1}" x2="{cx:.1}" y2="{bottom:.1}" stroke="{stroke}" stroke-width="{sw:.1}" stroke-dasharray="12 4 2 4" opacity="0.3"/>"#,
        cx = cx, y = y, bottom = y + height,
        stroke = style.stroke, sw = style.stroke_width * 0.7,
    );

    format!(
        r##"{group_open}<path d="{fill_body}" fill="{fill}" fill-opacity="0.15" stroke="{stroke}" stroke-width="{stroke_width}" stroke-linejoin="miter" stroke-linecap="butt"/><ellipse cx="{cx:.1}" cy="{top_cy:.1}" rx="{rx:.1}" ry="{ry:.1}" fill="{fill}" fill-opacity="0.15" stroke="{stroke}" stroke-width="{stroke_width}" stroke-linecap="butt"/><path d="{top_path}" fill="none" stroke="{stroke}" stroke-width="{stroke_width}" stroke-dasharray="4 2" stroke-linecap="butt"/><path d="{bottom_d}" fill="none" stroke="{stroke}" stroke-width="{stroke_width}" stroke-dasharray="4 2" stroke-linecap="butt"/>{center_line}</g>"##,
        group_open = group_open(&group_attrs),
        fill_body = fill_body,
        fill = style.fill,
        stroke = style.stroke,
        stroke_width = style.stroke_width,
        cx = cx,
        top_cy = top_cy,
        rx = rx,
        ry = ry,
        top_path = top_path,
        bottom_d = polyline_path(&bottom_front, false),
        center_line = center_line,
    )
}

fn render_blueprint_person(x: f64, y: f64, width: f64, height: f64, style: &NodeStyle) -> String {
    let head_r = width.min(height) * 0.24;
    let head_cx = x + width / 2.0;
    let head_cy = y + head_r + 1.0;
    let head = ellipse_points(head_cx, head_cy, head_r, head_r, 24);
    let body = vec![
        Point::new(head_cx, y + head_r * 2.1),
        Point::new(x + width * 0.9, y + height),
        Point::new(x + width * 0.1, y + height),
    ];
    let head_fill = polyline_path(&head, true);
    let body_fill = polyline_path(&body, true);
    let group_attrs = node_group_attrs(style, "blueprint", x, y, width, height);

    format!(
        r##"{group_open}<path d="{head_fill}" fill="{fill}" fill-opacity="0.15" stroke="{stroke}" stroke-width="{stroke_width}" stroke-linejoin="miter" stroke-linecap="butt"/><path d="{body_fill}" fill="{fill}" fill-opacity="0.15" stroke="{stroke}" stroke-width="{stroke_width}" stroke-linejoin="miter" stroke-linecap="butt" stroke-dasharray="6 3"/></g>"##,
        group_open = group_open(&group_attrs),
        head_fill = head_fill,
        body_fill = body_fill,
        fill = style.fill,
        stroke = style.stroke,
        stroke_width = style.stroke_width,
    )
}

fn render_blueprint_subprocess(x: f64, y: f64, width: f64, height: f64, style: &NodeStyle) -> String {
    let (ix, iy, iw, ih) = subprocess_inset(x, y, width, height);
    let outer = rect_points(x, y, width, height);
    let inner = rect_points(ix, iy, iw, ih);
    let group_attrs = node_group_attrs(style, "blueprint", x, y, width, height);
    format!(
        r##"{group_open}<path d="{outer_d}" fill="{fill}" fill-opacity="0.15" stroke="{stroke}" stroke-width="{stroke_width}" stroke-linejoin="miter" stroke-linecap="butt"/><path d="{inner_d}" fill="none" stroke="{stroke}" stroke-width="{stroke_width}" stroke-linejoin="miter" stroke-linecap="butt"/></g>"##,
        group_open = group_open(&group_attrs),
        outer_d = polyline_path(&outer, true),
        inner_d = polyline_path(&inner, true),
        fill = style.fill,
        stroke = style.stroke,
        stroke_width = style.stroke_width,
    )
}

// ---------------------------------------------------------------------------
// Blueprint center-line helpers
// ---------------------------------------------------------------------------

/// Generate cross-hair center lines (engineering drawing style) for a closed shape.
fn blueprint_center_cross(points: &[Point], stroke_width: f64) -> String {
    let bbox = bounding_box(points);
    let cx = bbox.0 + bbox.2 / 2.0;
    let cy = bbox.1 + bbox.3 / 2.0;
    let margin = 4.0;
    let hx1 = bbox.0 - margin;
    let hx2 = bbox.0 + bbox.2 + margin;
    let vy1 = bbox.1 - margin;
    let vy2 = bbox.1 + bbox.3 + margin;
    let sw = stroke_width * 0.5;
    format!(
        r##"<line x1="{hx1:.1}" y1="{cy:.1}" x2="{hx2:.1}" y2="{cy:.1}" stroke="#666" stroke-width="{sw:.1}" stroke-dasharray="8 4 2 4" opacity="0.25"/><line x1="{cx:.1}" y1="{vy1:.1}" x2="{cx:.1}" y2="{vy2:.1}" stroke="#666" stroke-width="{sw:.1}" stroke-dasharray="8 4 2 4" opacity="0.25"/>"##,
    )
}

// ---------------------------------------------------------------------------
// Blueprint edge rendering
// ---------------------------------------------------------------------------

fn render_blueprint_edge(
    points: &[Point],
    stroke: &str,
    style: &EdgeStyle,
    marker_end: &str,
    marker_start: &str,
) -> String {
    let attrs = edge_attrs(style, "blueprint");
    let d = polyline_path(points, false);
    format!(
        r##"<path d="{d}" fill="none" stroke="{stroke}" stroke-width="{stroke_width}" stroke-linecap="butt" stroke-linejoin="miter" {attrs} marker-end="{marker_end}" marker-start="{marker_start}"/>"##,
        d = d,
        stroke = stroke,
        stroke_width = style.stroke_width,
        attrs = attrs,
        marker_end = marker_end,
        marker_start = marker_start,
    )
}
