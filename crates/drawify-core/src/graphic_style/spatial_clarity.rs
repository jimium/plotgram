use crate::types::GraphicStyleId;
use crate::render::visual::{EdgeStyle, NodeShape, NodeStyle};
use super::GraphicStylePainter;
use super::common::*;

pub struct SpatialClarityGraphicStylePainter;

const SC_STROKE_WIDTH: f64 = 1.0;
const SC_EDGE_WIDTH: f64 = 1.25;
const SC_FILL_OPACITY: f64 = 0.96;
const SC_STROKE_OPACITY: f64 = 0.10;
const SC_EDGE_OPACITY: f64 = 0.55;
const SC_EDGE_PASSIVE_OPACITY: f64 = 0.40;
const SC_CORNER_RATIO: f64 = 0.22;
const SC_CORNER_MIN: f64 = 8.0;
const SC_CORNER_MAX: f64 = 20.0;
const SC_EDGE_CORNER_RADIUS: f64 = 6.0;
const STYLE_NAME: &str = "spatial-clarity";

impl GraphicStylePainter for SpatialClarityGraphicStylePainter {
    fn id(&self) -> GraphicStyleId {
        GraphicStyleId::SpatialClarity
    }

    fn shared_svg_defs(&self) -> Option<String> {
        Some(
            r##"  <filter id="sc-shadow" x="-30%" y="-30%" width="160%" height="160%">
    <feDropShadow dx="0" dy="1.5" stdDeviation="2" flood-color="#000000" flood-opacity="0.12"/>
    <feDropShadow dx="0" dy="6" stdDeviation="8" flood-color="#000000" flood-opacity="0.06"/>
  </filter>"##
                .to_string(),
        )
    }

    fn marker_defs(&self, active_stroke: &str, passive_stroke: &str) -> String {
        spatial_clarity_marker_defs(active_stroke, passive_stroke)
    }

    fn decorate_node_style(&self, style: &mut NodeStyle) {
        style.stroke_width = SC_STROKE_WIDTH;
        style.stroke_linecap = Some("round".to_string());
        style.stroke_linejoin = Some("round".to_string());
        style.hand_drawn = false;
    }

    fn decorate_edge_style(&self, style: &mut EdgeStyle) {
        style.stroke_width = SC_EDGE_WIDTH;
        style.stroke_linecap = Some("round".to_string());
        style.stroke_linejoin = Some("round".to_string());
        style.hand_drawn = false;
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
        Some(render_spatial_clarity_node_shape(
            shape, x, y, width, height, style,
        ))
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
        Some(render_spatial_clarity_edge(
            &points, stroke, style, marker_end, marker_start,
        ))
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
        Some(render_spatial_clarity_edge(
            &points, stroke, style, marker_end, marker_start,
        ))
    }
}

fn sc_corner_radius(width: f64, height: f64) -> f64 {
    (width.min(height) * SC_CORNER_RATIO).clamp(SC_CORNER_MIN, SC_CORNER_MAX)
}

fn spatial_clarity_marker_defs(active_stroke: &str, passive_stroke: &str) -> String {
    format!(
        r##"  <marker id="arrow-active" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
    <path d="M 1.5 2 L 9 5 L 1.5 8 L 3 5 Z" fill="{active_stroke}" opacity="0.55"/>
  </marker>
  <marker id="arrow-passive" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
    <path d="M 1.5 2 L 9 5 L 1.5 8 L 3 5 Z" fill="{passive_stroke}" opacity="0.40"/>
  </marker>
  <marker id="arrow-bidi" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
    <path d="M 1.5 2 L 9 5 L 1.5 8 L 3 5 Z" fill="{active_stroke}" opacity="0.55"/>
  </marker>"##
    )
}

fn render_spatial_clarity_node_shape(
    shape: &NodeShape,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    style: &NodeStyle,
) -> String {
    match shape {
        NodeShape::Rect | NodeShape::RoundedRect => {
            let rx = style
                .radius
                .unwrap_or_else(|| sc_corner_radius(width, height));
            render_sc_round_rect(x, y, width, height, rx, style)
        }
        NodeShape::Circle => render_sc_circle(x, y, width, height, style),
        NodeShape::Diamond => render_sc_closed_path(
            &[
                Point::new(x + width / 2.0, y),
                Point::new(x + width, y + height / 2.0),
                Point::new(x + width / 2.0, y + height),
                Point::new(x, y + height / 2.0),
            ],
            style,
        ),
        NodeShape::Hexagon => render_sc_closed_path(
            &[
                Point::new(x + width * 0.22, y),
                Point::new(x + width * 0.78, y),
                Point::new(x + width, y + height / 2.0),
                Point::new(x + width * 0.78, y + height),
                Point::new(x + width * 0.22, y + height),
                Point::new(x, y + height / 2.0),
            ],
            style,
        ),
        NodeShape::Cylinder => render_sc_cylinder(x, y, width, height, style),
        NodeShape::Person => render_sc_person(x, y, width, height, style),
        NodeShape::Stadium => {
            let rx = style.corner_radius(shape, width, height);
            render_sc_round_rect(x, y, width, height, rx, style)
        }
        NodeShape::Parallelogram => render_sc_closed_path(
            &parallelogram_points(x, y, width, height),
            style,
        ),
        NodeShape::Document => render_sc_closed_path(
            &document_points(x, y, width, height),
            style,
        ),
        NodeShape::Cloud => render_sc_closed_path(
            &cloud_points(x, y, width, height),
            style,
        ),
        NodeShape::Subprocess => render_sc_subprocess(x, y, width, height, style),
    }
}

fn render_sc_round_rect(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    rx: f64,
    style: &NodeStyle,
) -> String {
    let group_attrs = node_group_attrs(style, STYLE_NAME, x, y, width, height);
    format!(
        r##"{group_open}<rect x="{x:.1}" y="{y:.1}" width="{width:.1}" height="{height:.1}" rx="{rx:.1}" ry="{rx:.1}" fill="{fill}" fill-opacity="{fill_op}" filter="url(#sc-shadow)"/><rect x="{x:.1}" y="{y:.1}" width="{width:.1}" height="{height:.1}" rx="{rx:.1}" ry="{rx:.1}" fill="none" stroke="{stroke}" stroke-opacity="{stroke_op}" stroke-width="{sw}" stroke-linejoin="round"/></g>"##,
        group_open = group_open(&group_attrs),
        x = x,
        y = y,
        width = width,
        height = height,
        rx = rx,
        fill = style.fill,
        fill_op = SC_FILL_OPACITY,
        stroke = style.stroke,
        stroke_op = SC_STROKE_OPACITY,
        sw = style.stroke_width,
    )
}

fn render_sc_circle(x: f64, y: f64, width: f64, height: f64, style: &NodeStyle) -> String {
    let r = width.min(height) / 2.0;
    let cx = x + width / 2.0;
    let cy = y + height / 2.0;
    let group_attrs = node_group_attrs(style, STYLE_NAME, x, y, width, height);
    format!(
        r##"{group_open}<circle cx="{cx:.1}" cy="{cy:.1}" r="{r:.1}" fill="{fill}" fill-opacity="{fill_op}" filter="url(#sc-shadow)"/><circle cx="{cx:.1}" cy="{cy:.1}" r="{r:.1}" fill="none" stroke="{stroke}" stroke-opacity="{stroke_op}" stroke-width="{sw}"/></g>"##,
        group_open = group_open(&group_attrs),
        cx = cx,
        cy = cy,
        r = r,
        fill = style.fill,
        fill_op = SC_FILL_OPACITY,
        stroke = style.stroke,
        stroke_op = SC_STROKE_OPACITY,
        sw = style.stroke_width,
    )
}

fn render_sc_subprocess(x: f64, y: f64, width: f64, height: f64, style: &NodeStyle) -> String {
    let (ix, iy, iw, ih) = subprocess_inset(x, y, width, height);
    let group_attrs = node_group_attrs(style, STYLE_NAME, x, y, width, height);
    format!(
        r##"{group_open}<rect x="{x:.1}" y="{y:.1}" width="{width:.1}" height="{height:.1}" fill="{fill}" fill-opacity="{fill_op}" filter="url(#sc-shadow)"/><rect x="{x:.1}" y="{y:.1}" width="{width:.1}" height="{height:.1}" fill="none" stroke="{stroke}" stroke-opacity="{stroke_op}" stroke-width="{sw}"/><rect x="{ix:.1}" y="{iy:.1}" width="{iw:.1}" height="{ih:.1}" fill="none" stroke="{stroke}" stroke-opacity="{stroke_op}" stroke-width="{sw}"/></g>"##,
        group_open = group_open(&group_attrs),
        x = x,
        y = y,
        width = width,
        height = height,
        ix = ix,
        iy = iy,
        iw = iw,
        ih = ih,
        fill = style.fill,
        fill_op = SC_FILL_OPACITY,
        stroke = style.stroke,
        stroke_op = SC_STROKE_OPACITY,
        sw = style.stroke_width,
    )
}

fn render_sc_closed_path(points: &[Point], style: &NodeStyle) -> String {
    let d = polyline_path(points, true);
    let (x, y, width, height) = bounding_box(points);
    let group_attrs = node_group_attrs(style, STYLE_NAME, x, y, width, height);
    format!(
        r##"{group_open}<path d="{d}" fill="{fill}" fill-opacity="{fill_op}" filter="url(#sc-shadow)"/><path d="{d}" fill="none" stroke="{stroke}" stroke-opacity="{stroke_op}" stroke-width="{sw}" stroke-linejoin="round"/></g>"##,
        group_open = group_open(&group_attrs),
        d = d,
        fill = style.fill,
        fill_op = SC_FILL_OPACITY,
        stroke = style.stroke,
        stroke_op = SC_STROKE_OPACITY,
        sw = style.stroke_width,
    )
}

fn render_sc_cylinder(x: f64, y: f64, width: f64, height: f64, style: &NodeStyle) -> String {
    let rx = width / 2.0;
    let ry = (height * 0.14).clamp(6.0, 12.0);
    let cx = x + rx;
    let top_cy = y + ry;
    let bottom_cy = y + height - ry;

    let body = format!(
        "M {x:.1} {top_cy:.1} A {rx:.1} {ry:.1} 0 0 1 {right:.1} {top_cy:.1} L {right:.1} {bottom_cy:.1} A {rx:.1} {ry:.1} 0 0 1 {x:.1} {bottom_cy:.1} Z",
        right = x + width,
    );
    let top = format!(
        "M {x:.1} {top_cy:.1} A {rx:.1} {ry:.1} 0 0 1 {right:.1} {top_cy:.1} A {rx:.1} {ry:.1} 0 0 1 {x:.1} {top_cy:.1} Z",
        right = x + width,
    );
    let group_attrs = node_group_attrs(style, STYLE_NAME, x, y, width, height);

    format!(
        r##"{group_open}<path d="{body}" fill="{fill}" fill-opacity="{fill_op}" filter="url(#sc-shadow)"/><ellipse cx="{cx:.1}" cy="{top_cy:.1}" rx="{rx:.1}" ry="{ry:.1}" fill="{fill}" fill-opacity="{fill_op}" filter="url(#sc-shadow)"/><path d="{body}" fill="none" stroke="{stroke}" stroke-opacity="{stroke_op}" stroke-width="{sw}" stroke-linejoin="round"/><path d="{top}" fill="none" stroke="{stroke}" stroke-opacity="{stroke_op}" stroke-width="{sw}"/></g>"##,
        group_open = group_open(&group_attrs),
        body = body,
        top = top,
        fill = style.fill,
        fill_op = SC_FILL_OPACITY,
        cx = cx,
        top_cy = top_cy,
        rx = rx,
        ry = ry,
        stroke = style.stroke,
        stroke_op = SC_STROKE_OPACITY,
        sw = style.stroke_width,
    )
}

fn render_sc_person(x: f64, y: f64, width: f64, height: f64, style: &NodeStyle) -> String {
    let head_r = width.min(height) * 0.24;
    let head_cx = x + width / 2.0;
    let head_cy = y + head_r + 1.0;
    let body = vec![
        Point::new(head_cx, y + head_r * 2.1),
        Point::new(x + width * 0.9, y + height),
        Point::new(x + width * 0.1, y + height),
    ];
    let body_d = polyline_path(&body, true);
    let group_attrs = node_group_attrs(style, STYLE_NAME, x, y, width, height);

    format!(
        r##"{group_open}<circle cx="{head_cx:.1}" cy="{head_cy:.1}" r="{head_r:.1}" fill="{fill}" fill-opacity="{fill_op}" filter="url(#sc-shadow)"/><path d="{body_d}" fill="{fill}" fill-opacity="{fill_op}" filter="url(#sc-shadow)"/><circle cx="{head_cx:.1}" cy="{head_cy:.1}" r="{head_r:.1}" fill="none" stroke="{stroke}" stroke-opacity="{stroke_op}" stroke-width="{sw}"/><path d="{body_d}" fill="none" stroke="{stroke}" stroke-opacity="{stroke_op}" stroke-width="{sw}" stroke-linejoin="round"/></g>"##,
        group_open = group_open(&group_attrs),
        head_cx = head_cx,
        head_cy = head_cy,
        head_r = head_r,
        body_d = body_d,
        fill = style.fill,
        fill_op = SC_FILL_OPACITY,
        stroke = style.stroke,
        stroke_op = SC_STROKE_OPACITY,
        sw = style.stroke_width,
    )
}

fn render_spatial_clarity_edge(
    points: &[Point],
    stroke: &str,
    style: &EdgeStyle,
    marker_end: &str,
    marker_start: &str,
) -> String {
    let d = smooth_polyline_path_from_points(points, SC_EDGE_CORNER_RADIUS);
    let attrs = edge_attrs(style, STYLE_NAME);
    let stroke_opacity = if style
        .stroke_dasharray
        .as_deref()
        .is_some_and(|d| !d.is_empty())
        || style.dashed
    {
        SC_EDGE_PASSIVE_OPACITY
    } else {
        SC_EDGE_OPACITY
    };

    format!(
        r##"<path d="{d}" fill="none" stroke="{stroke}" stroke-opacity="{stroke_opacity}" stroke-width="{stroke_width}" {attrs} marker-end="{marker_end}" marker-start="{marker_start}"/>"##,
        d = d,
        stroke = stroke,
        stroke_opacity = stroke_opacity,
        stroke_width = style.stroke_width,
        attrs = attrs,
        marker_end = marker_end,
        marker_start = marker_start,
    )
}
