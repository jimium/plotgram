use crate::types::GraphicStyleId;
use crate::render::visual::{EdgeStyle, NodeShape, NodeStyle};
use super::GraphicStylePainter;
use super::common::*;

pub struct NeonGlowGraphicStylePainter;

const NG_STROKE_WIDTH: f64 = 2.0;
const NG_EDGE_WIDTH: f64 = 2.0;
const NG_GLOW_WIDTH: f64 = 6.0;
const NG_FILL_OPACITY: f64 = 0.08;
const NG_EDGE_CORNER_RADIUS: f64 = 6.0;
const STYLE_NAME: &str = "neon-glow";

impl GraphicStylePainter for NeonGlowGraphicStylePainter {
    fn id(&self) -> GraphicStyleId {
        GraphicStyleId::NeonGlow
    }

    fn shared_svg_defs(&self) -> Option<String> {
        Some(
            r##"  <filter id="ng-glow" x="-50%" y="-50%" width="200%" height="200%">
    <feGaussianBlur in="SourceGraphic" stdDeviation="3" result="blur1"/>
    <feGaussianBlur in="SourceGraphic" stdDeviation="8" result="blur2"/>
    <feMerge>
      <feMergeNode in="blur2"/>
      <feMergeNode in="blur1"/>
      <feMergeNode in="SourceGraphic"/>
    </feMerge>
  </filter>
  <filter id="ng-glow-soft" x="-50%" y="-50%" width="200%" height="200%">
    <feGaussianBlur in="SourceGraphic" stdDeviation="2" result="blur1"/>
    <feGaussianBlur in="SourceGraphic" stdDeviation="6" result="blur2"/>
    <feMerge>
      <feMergeNode in="blur2"/>
      <feMergeNode in="blur1"/>
      <feMergeNode in="SourceGraphic"/>
    </feMerge>
  </filter>"##
                .to_string(),
        )
    }

    fn marker_defs(&self, active_stroke: &str, passive_stroke: &str) -> String {
        neon_glow_marker_defs(active_stroke, passive_stroke)
    }

    fn decorate_node_style(&self, style: &mut NodeStyle) {
        style.stroke_width = NG_STROKE_WIDTH;
        style.stroke_linecap = Some("round".to_string());
        style.stroke_linejoin = Some("round".to_string());
        style.hand_drawn = false;
    }

    fn decorate_edge_style(&self, style: &mut EdgeStyle) {
        style.stroke_width = NG_EDGE_WIDTH;
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
        Some(render_neon_glow_node_shape(
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
        Some(render_neon_glow_edge(
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
        Some(render_neon_glow_edge(
            &points, stroke, style, marker_end, marker_start,
        ))
    }
}

// ---------------------------------------------------------------------------
// Marker defs
// ---------------------------------------------------------------------------

fn neon_glow_marker_defs(active_stroke: &str, passive_stroke: &str) -> String {
    format!(
        r##"  <marker id="arrow-active" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
    <path d="M 1 2 L 9 5 L 1 8 Z" fill="{active_stroke}" filter="url(#ng-glow-soft)"/>
    <path d="M 1.5 2.5 L 8 5 L 1.5 7.5 Z" fill="#ffffff" opacity="0.9"/>
  </marker>
  <marker id="arrow-passive" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
    <path d="M 1 2 L 9 5 L 1 8 Z" fill="{passive_stroke}" filter="url(#ng-glow-soft)"/>
    <path d="M 1.5 2.5 L 8 5 L 1.5 7.5 Z" fill="#ffffff" opacity="0.6"/>
  </marker>
  <marker id="arrow-bidi" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
    <path d="M 1 2 L 9 5 L 1 8 Z" fill="{active_stroke}" filter="url(#ng-glow-soft)"/>
    <path d="M 1.5 2.5 L 8 5 L 1.5 7.5 Z" fill="#ffffff" opacity="0.9"/>
  </marker>"##
    )
}

// ---------------------------------------------------------------------------
// Node rendering
// ---------------------------------------------------------------------------

fn render_neon_glow_node_shape(
    shape: &NodeShape,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    style: &NodeStyle,
) -> String {
    dispatch_node_shape(shape, x, y, width, height, style,
        render_ng_closed_shape,
        render_ng_cylinder,
        render_ng_person,
        render_ng_subprocess,
    )
}

fn render_ng_closed_shape(points: &[Point], style: &NodeStyle, _shape: &NodeShape) -> String {
    let (x, y, width, height) = bounding_box(points);
    let group_attrs = node_group_attrs(style, STYLE_NAME, x, y, width, height);
    let d = polyline_path(points, true);

    format!(
        r##"{group_open}<path d="{d}" fill="{fill}" fill-opacity="{fill_op}"/><path d="{d}" fill="none" stroke="{stroke}" stroke-width="{glow_width}" stroke-linecap="round" stroke-linejoin="round" opacity="0.45" filter="url(#ng-glow)"/><path d="{d}" fill="none" stroke="{stroke}" stroke-width="{stroke_width}" stroke-linecap="round" stroke-linejoin="round"/></g>"##,
        group_open = group_open(&group_attrs),
        d = d,
        fill = style.fill,
        fill_op = NG_FILL_OPACITY,
        stroke = style.stroke,
        glow_width = NG_GLOW_WIDTH,
        stroke_width = style.stroke_width,
    )
}

fn render_ng_cylinder(x: f64, y: f64, width: f64, height: f64, style: &NodeStyle) -> String {
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
        r##"{group_open}<path d="{body}" fill="{fill}" fill-opacity="{fill_op}"/><ellipse cx="{cx:.1}" cy="{top_cy:.1}" rx="{rx:.1}" ry="{ry:.1}" fill="{fill}" fill-opacity="{fill_op}"/><path d="{body}" fill="none" stroke="{stroke}" stroke-width="{glow_width}" stroke-linecap="round" stroke-linejoin="round" opacity="0.45" filter="url(#ng-glow)"/><path d="{body}" fill="none" stroke="{stroke}" stroke-width="{stroke_width}" stroke-linecap="round" stroke-linejoin="round"/><path d="{top}" fill="none" stroke="{stroke}" stroke-width="{glow_width}" stroke-linecap="round" opacity="0.45" filter="url(#ng-glow)"/><path d="{top}" fill="none" stroke="{stroke}" stroke-width="{stroke_width}" stroke-linecap="round"/></g>"##,
        group_open = group_open(&group_attrs),
        body = body,
        top = top,
        fill = style.fill,
        fill_op = NG_FILL_OPACITY,
        cx = cx,
        top_cy = top_cy,
        rx = rx,
        ry = ry,
        stroke = style.stroke,
        glow_width = NG_GLOW_WIDTH,
        stroke_width = style.stroke_width,
    )
}

fn render_ng_person(x: f64, y: f64, width: f64, height: f64, style: &NodeStyle) -> String {
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
        r##"{group_open}<circle cx="{head_cx:.1}" cy="{head_cy:.1}" r="{head_r:.1}" fill="{fill}" fill-opacity="{fill_op}"/><path d="{body_d}" fill="{fill}" fill-opacity="{fill_op}"/><circle cx="{head_cx:.1}" cy="{head_cy:.1}" r="{head_r:.1}" fill="none" stroke="{stroke}" stroke-width="{glow_width}" opacity="0.45" filter="url(#ng-glow)"/><circle cx="{head_cx:.1}" cy="{head_cy:.1}" r="{head_r:.1}" fill="none" stroke="{stroke}" stroke-width="{stroke_width}"/><path d="{body_d}" fill="none" stroke="{stroke}" stroke-width="{glow_width}" stroke-linejoin="round" opacity="0.45" filter="url(#ng-glow)"/><path d="{body_d}" fill="none" stroke="{stroke}" stroke-width="{stroke_width}" stroke-linejoin="round"/></g>"##,
        group_open = group_open(&group_attrs),
        head_cx = head_cx,
        head_cy = head_cy,
        head_r = head_r,
        body_d = body_d,
        fill = style.fill,
        fill_op = NG_FILL_OPACITY,
        stroke = style.stroke,
        glow_width = NG_GLOW_WIDTH,
        stroke_width = style.stroke_width,
    )
}

fn render_ng_subprocess(x: f64, y: f64, width: f64, height: f64, style: &NodeStyle) -> String {
    let outer = render_ng_closed_shape(&rect_points(x, y, width, height), style, &NodeShape::Subprocess);
    let (ix, iy, iw, ih) = subprocess_inset(x, y, width, height);
    let inner_d = polyline_path(&rect_points(ix, iy, iw, ih), true);
    format!(
        r##"{outer}<path d="{inner_d}" fill="none" stroke="{stroke}" stroke-width="{glow_width}" opacity="0.45" filter="url(#ng-glow)"/><path d="{inner_d}" fill="none" stroke="{stroke}" stroke-width="{stroke_width}"/>"##,
        outer = outer,
        inner_d = inner_d,
        stroke = style.stroke,
        glow_width = NG_GLOW_WIDTH,
        stroke_width = style.stroke_width,
    )
}

// ---------------------------------------------------------------------------
// Edge rendering
// ---------------------------------------------------------------------------

fn render_neon_glow_edge(
    points: &[Point],
    stroke: &str,
    style: &EdgeStyle,
    marker_end: &str,
    marker_start: &str,
) -> String {
    let d = smooth_polyline_path_from_points(points, NG_EDGE_CORNER_RADIUS);
    let attrs = edge_attrs(style, STYLE_NAME);
    let stroke_opacity = if style
        .stroke_dasharray
        .as_deref()
        .is_some_and(|d| !d.is_empty())
        || style.dashed
    {
        0.35
    } else {
        0.55
    };

    // Glow layer (thick, blurred) + core layer (thin, sharp)
    format!(
        r##"<path d="{d}" fill="none" stroke="{stroke}" stroke-opacity="{stroke_opacity}" stroke-width="{glow_width}" stroke-linecap="round" stroke-linejoin="round" filter="url(#ng-glow)"/><path d="{d}" fill="none" stroke="{stroke}" stroke-opacity="{stroke_opacity}" stroke-width="{stroke_width}" stroke-linecap="round" stroke-linejoin="round" {attrs} marker-end="{marker_end}" marker-start="{marker_start}"/>"##,
        d = d,
        stroke = stroke,
        stroke_opacity = stroke_opacity,
        glow_width = NG_GLOW_WIDTH,
        stroke_width = style.stroke_width,
        attrs = attrs,
        marker_end = marker_end,
        marker_start = marker_start,
    )
}