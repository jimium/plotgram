use crate::types::GraphicStyleId;
use crate::render::visual::{EdgeStyle, NodeShape, NodeStyle};
use super::GraphicStylePainter;
use super::common::*;
use super::standard::standard_marker_defs;

pub struct StippleGraphicStylePainter;

const ST_STROKE_WIDTH: f64 = 2.0;
const ST_EDGE_WIDTH: f64 = 2.0;
const ST_DOT_GAP: f64 = 6.0;
const ST_DOT_RADIUS: f64 = 1.2;
const ST_DOT_OPACITY: f64 = 0.55;
const ST_EDGE_CORNER_RADIUS: f64 = 5.0;
const STYLE_NAME: &str = "stipple";

/// Light roughness: keep the organic feel without losing legibility.
const ST_ROUGHNESS: RoughnessParams = RoughnessParams {
    closed: true,
    seed: 0.0,
    roughness: 0.35,
    tangent_jitter: 0.2,
    lerp_a: 0.25,
    lerp_b: 0.75,
    base_len_divisor: 80.0,
    base_clamp_lo: 0.1,
    base_clamp_hi: 0.35,
    base_offset: 0.05,
    drift_tangent: 0.4,
};

impl GraphicStylePainter for StippleGraphicStylePainter {
    fn id(&self) -> GraphicStyleId {
        GraphicStyleId::Stipple
    }

    fn shared_svg_defs(&self) -> Option<String> {
        None
    }

    fn marker_defs(&self, active_stroke: &str, passive_stroke: &str) -> String {
        standard_marker_defs(active_stroke, passive_stroke)
    }

    fn decorate_node_style(&self, style: &mut NodeStyle) {
        style.stroke_width = ST_STROKE_WIDTH;
        style.stroke_linecap = Some("round".to_string());
        style.stroke_linejoin = Some("round".to_string());
        style.hand_drawn = true;
    }

    fn decorate_edge_style(&self, style: &mut EdgeStyle) {
        style.stroke_width = ST_EDGE_WIDTH;
        style.stroke_linecap = Some("round".to_string());
        style.stroke_linejoin = Some("round".to_string());
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
        Some(render_stipple_node_shape(
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
        Some(render_stipple_edge(
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
        Some(render_stipple_edge(
            &points, stroke, style, marker_end, marker_start,
        ))
    }
}

// ---------------------------------------------------------------------------
// Node rendering
// ---------------------------------------------------------------------------

fn render_stipple_node_shape(
    shape: &NodeShape,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    style: &NodeStyle,
) -> String {
    dispatch_node_shape(
        shape,
        x,
        y,
        width,
        height,
        style,
        render_stipple_closed_shape,
        render_stipple_cylinder,
        render_stipple_person,
        render_stipple_subprocess,
    )
}

fn render_stipple_closed_shape(points: &[Point], style: &NodeStyle, _shape: &NodeShape) -> String {
    let (x, y, width, height) = bounding_box(points);
    let group_attrs = node_group_attrs(style, STYLE_NAME, x, y, width, height);
    let stroke_attrs = node_stroke_attrs(style);
    let d = rough_polyline(points, &ST_ROUGHNESS);
    let bbox = points_bbox(points);
    let gap = adaptive_gap(&bbox, ST_DOT_GAP, ST_DOT_GAP * 1.8);
    let dot_fill_svg = dot_fill(&bbox, &style.fill, gap, ST_DOT_RADIUS, ST_DOT_OPACITY);

    format!(
        r##"{group_open}<g fill="{fill}" opacity="{dot_opacity}">{dot_fill_svg}</g><path d="{d}" fill="none" stroke="{stroke}" stroke-width="{stroke_width}" {stroke_attrs}/></g>"##,
        group_open = group_open(&group_attrs),
        fill = style.fill,
        dot_opacity = ST_DOT_OPACITY,
        dot_fill_svg = dot_fill_svg,
        d = d,
        stroke = style.stroke,
        stroke_width = style.stroke_width,
        stroke_attrs = stroke_attrs,
    )
}

fn render_stipple_cylinder(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    style: &NodeStyle,
) -> String {
    let rx = width / 2.0;
    let ry = (height * 0.14).clamp(6.0, 12.0);
    let cx = x + rx;
    let top_cy = y + ry;
    let bottom_cy = y + height - ry;

    let body_pts = vec![
        Point::new(x, top_cy),
        Point::new(x + width, top_cy),
        Point::new(x + width, bottom_cy),
        Point::new(x, bottom_cy),
    ];
    let body_d = rough_polyline(&body_pts, &ST_ROUGHNESS);

    let top_pts = ellipse_points(cx, top_cy, rx, ry, 24);
    let top_d = rough_polyline(&top_pts, &ST_ROUGHNESS);

    let group_attrs = node_group_attrs(style, STYLE_NAME, x, y, width, height);
    let stroke_attrs = node_stroke_attrs(style);
    let bbox = (x, y, width, height);
    let gap = adaptive_gap(&bbox, ST_DOT_GAP, ST_DOT_GAP * 1.8);
    let dot_fill_svg = dot_fill(&bbox, &style.fill, gap, ST_DOT_RADIUS, ST_DOT_OPACITY);

    format!(
        r##"{group_open}<g fill="{fill}" opacity="{dot_opacity}">{dot_fill_svg}</g><path d="{body_d}" fill="none" stroke="{stroke}" stroke-width="{stroke_width}" {stroke_attrs}/><path d="{top_d}" fill="none" stroke="{stroke}" stroke-width="{stroke_width}" {stroke_attrs}/></g>"##,
        group_open = group_open(&group_attrs),
        fill = style.fill,
        dot_opacity = ST_DOT_OPACITY,
        dot_fill_svg = dot_fill_svg,
        body_d = body_d,
        top_d = top_d,
        stroke = style.stroke,
        stroke_width = style.stroke_width,
        stroke_attrs = stroke_attrs,
    )
}

fn render_stipple_person(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    style: &NodeStyle,
) -> String {
    let head_r = width.min(height) * 0.24;
    let head_cx = x + width / 2.0;
    let head_cy = y + head_r + 1.0;
    let body = vec![
        Point::new(head_cx, y + head_r * 2.1),
        Point::new(x + width * 0.9, y + height),
        Point::new(x + width * 0.1, y + height),
    ];

    let head_pts = ellipse_points(head_cx, head_cy, head_r, head_r, 24);
    let head_d = rough_polyline(&head_pts, &ST_ROUGHNESS);
    let body_d = rough_polyline(&body, &ST_ROUGHNESS);

    let group_attrs = node_group_attrs(style, STYLE_NAME, x, y, width, height);
    let stroke_attrs = node_stroke_attrs(style);
    let bbox = (x, y, width, height);
    let gap = adaptive_gap(&bbox, ST_DOT_GAP, ST_DOT_GAP * 1.8);
    let dot_fill_svg = dot_fill(&bbox, &style.fill, gap, ST_DOT_RADIUS, ST_DOT_OPACITY);

    format!(
        r##"{group_open}<g fill="{fill}" opacity="{dot_opacity}">{dot_fill_svg}</g><path d="{head_d}" fill="none" stroke="{stroke}" stroke-width="{stroke_width}" {stroke_attrs}/><path d="{body_d}" fill="none" stroke="{stroke}" stroke-width="{stroke_width}" {stroke_attrs}/></g>"##,
        group_open = group_open(&group_attrs),
        fill = style.fill,
        dot_opacity = ST_DOT_OPACITY,
        dot_fill_svg = dot_fill_svg,
        head_d = head_d,
        body_d = body_d,
        stroke = style.stroke,
        stroke_width = style.stroke_width,
        stroke_attrs = stroke_attrs,
    )
}

fn render_stipple_subprocess(x: f64, y: f64, width: f64, height: f64, style: &NodeStyle) -> String {
    let outer = render_stipple_closed_shape(&rect_points(x, y, width, height), style, &NodeShape::Subprocess);
    let (ix, iy, iw, ih) = subprocess_inset(x, y, width, height);
    let inner_d = rough_polyline(&rect_points(ix, iy, iw, ih), &ST_ROUGHNESS);
    format!(
        r##"{outer}<path d="{inner_d}" fill="none" stroke="{stroke}" stroke-width="{stroke_width}"/>"##,
        outer = outer,
        inner_d = inner_d,
        stroke = style.stroke,
        stroke_width = style.stroke_width,
    )
}

// ---------------------------------------------------------------------------
// Utility
// ---------------------------------------------------------------------------

fn points_bbox(points: &[Point]) -> (f64, f64, f64, f64) {
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;
    for p in points {
        min_x = min_x.min(p.x);
        min_y = min_y.min(p.y);
        max_x = max_x.max(p.x);
        max_y = max_y.max(p.y);
    }
    (min_x, min_y, max_x - min_x, max_y - min_y)
}

// ---------------------------------------------------------------------------
// Edge rendering
// ---------------------------------------------------------------------------

fn render_stipple_edge(
    points: &[Point],
    stroke: &str,
    style: &EdgeStyle,
    marker_end: &str,
    marker_start: &str,
) -> String {
    let d = smooth_polyline_path_from_points(points, ST_EDGE_CORNER_RADIUS);
    let attrs = edge_attrs(style, STYLE_NAME);

    format!(
        r##"<path d="{d}" fill="none" stroke="{stroke}" stroke-width="{stroke_width}" {attrs} marker-end="{marker_end}" marker-start="{marker_start}"/>"##,
        d = d,
        stroke = stroke,
        stroke_width = style.stroke_width,
        attrs = attrs,
        marker_end = marker_end,
        marker_start = marker_start,
    )
}