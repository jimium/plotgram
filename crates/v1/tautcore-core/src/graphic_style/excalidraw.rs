use crate::types::GraphicStyleId;
use crate::render::visual::{EdgeStyle, NodeShape, NodeStyle};
use super::GraphicStylePainter;
use super::common::*;

/// Fill mode for Excalidraw-style rendering.
#[derive(Clone, Copy)]
pub enum FillMode {
    /// Sparse clipped dot fill.
    Hachure,
    /// Slightly denser clipped dot fill.
    CrossHatch,
}

pub struct ExcalidrawGraphicStylePainter {
    pub fill_mode: FillMode,
}

// Sparse dots: keep the hand-drawn feel without visual clutter.
const EX_DOT_GAP_MIN: f64 = 8.0;
const EX_DOT_GAP_MAX: f64 = 14.0;
const EX_DOT_RADIUS: f64 = 1.0;
const EX_DOT_OPACITY: f64 = 0.38;

// Cross-hatch variant: a touch denser, still restrained.
const CH_DOT_GAP_MIN: f64 = 6.0;
const CH_DOT_GAP_MAX: f64 = 10.0;
const CH_DOT_RADIUS: f64 = 1.0;
const CH_DOT_OPACITY: f64 = 0.42;

impl GraphicStylePainter for ExcalidrawGraphicStylePainter {
    fn id(&self) -> GraphicStyleId {
        match self.fill_mode {
            FillMode::Hachure => GraphicStyleId::Excalidraw,
            FillMode::CrossHatch => GraphicStyleId::CrossHatch,
        }
    }

    fn shared_svg_defs(&self) -> Option<String> {
        None
    }

    fn marker_defs(&self, active_stroke: &str, passive_stroke: &str) -> String {
        excalidraw_marker_defs(active_stroke, passive_stroke)
    }

    fn decorate_node_style(&self, style: &mut NodeStyle) {
        style.stroke_width = style.stroke_width.max(1.5);
        style.stroke_linecap = Some("round".to_string());
        style.stroke_linejoin = Some("round".to_string());
        style.hand_drawn = true;
    }

    fn decorate_edge_style(&self, style: &mut EdgeStyle) {
        style.stroke_width = style.stroke_width.max(1.5);
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
        Some(render_excalidraw_node_shape(shape, x, y, width, height, style, self.fill_mode))
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
        Some(render_edge_for_fill_mode(
            &points,
            stroke,
            style,
            marker_end,
            marker_start,
            self.fill_mode,
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
        Some(render_edge_for_fill_mode(
            &points,
            stroke,
            style,
            marker_end,
            marker_start,
            self.fill_mode,
        ))
    }
}

// ---------------------------------------------------------------------------
// Excalidraw marker defs
// ---------------------------------------------------------------------------

fn excalidraw_marker_defs(active_stroke: &str, passive_stroke: &str) -> String {
    const STYLE: ArrowMarkerStyle = ArrowMarkerStyle {
        view_box: "0 0 12 12",
        ref_x: "11",
        ref_y: "6",
        marker_width: "9",
        marker_height: "9",
        active_shape: r#"<path d="M 2 2 L 11 6 L 2 10 L 4 6 z" fill="{stroke}"/>"#,
        passive_shape: r#"<path d="M 2 2 L 11 6 L 2 10 L 4 6 z" fill="{stroke}"/>"#,
    };
    arrow_markers(&STYLE, active_stroke, passive_stroke)
}

// ---------------------------------------------------------------------------
// Dot fill helpers
// ---------------------------------------------------------------------------

/// Clipped sparse dot fill for Excalidraw style.
pub fn excalidraw_dot_fill(
    clip_content: &str,
    bbox: &(f64, f64, f64, f64),
    style: &NodeStyle,
) -> String {
    let gap = adaptive_gap(bbox, EX_DOT_GAP_MIN, EX_DOT_GAP_MAX);
    clipped_dot_fill(
        clip_content,
        "ex",
        bbox,
        &style.fill,
        gap,
        EX_DOT_RADIUS,
        EX_DOT_OPACITY,
    )
}

fn cross_hatch_dot_fill(
    clip_content: &str,
    bbox: &(f64, f64, f64, f64),
    style: &NodeStyle,
) -> String {
    let gap = adaptive_gap(bbox, CH_DOT_GAP_MIN, CH_DOT_GAP_MAX);
    clipped_dot_fill(
        clip_content,
        "ch",
        bbox,
        &style.fill,
        gap,
        CH_DOT_RADIUS,
        CH_DOT_OPACITY,
    )
}

fn dot_fill_for_mode(
    clip_content: &str,
    bbox: &(f64, f64, f64, f64),
    style: &NodeStyle,
    fill_mode: FillMode,
) -> String {
    match fill_mode {
        FillMode::Hachure => excalidraw_dot_fill(clip_content, bbox, style),
        FillMode::CrossHatch => cross_hatch_dot_fill(clip_content, bbox, style),
    }
}

// ---------------------------------------------------------------------------
// Excalidraw node rendering
// ---------------------------------------------------------------------------

fn render_excalidraw_node_shape(
    shape: &NodeShape,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    style: &NodeStyle,
    fill_mode: FillMode,
) -> String {
    dispatch_node_shape(
        shape,
        x,
        y,
        width,
        height,
        style,
        |points, style, shape| render_dot_fill_closed_shape(points, style, shape, fill_mode),
        |x, y, w, h, style| render_dot_fill_cylinder(x, y, w, h, style, fill_mode),
        |x, y, w, h, style| render_dot_fill_person(x, y, w, h, style, fill_mode),
        |x, y, w, h, style| render_dot_fill_subprocess(x, y, w, h, style, fill_mode),
    )
}

/// Render a closed shape: clipped dot fill + double-stroke rough outline.
fn render_dot_fill_closed_shape(
    points: &[Point],
    style: &NodeStyle,
    _shape: &NodeShape,
    fill_mode: FillMode,
) -> String {
    let style_name = match fill_mode {
        FillMode::Hachure => "excalidraw",
        FillMode::CrossHatch => "cross-hatch",
    };
    let clip_d = polyline_path(points, true);
    let clip_content = format!(r##"<path d="{clip_d}"/>"##);
    let (x, y, width, height) = bounding_box(points);
    let group_attrs = node_group_attrs(style, style_name, x, y, width, height);
    let stroke_attrs = node_stroke_attrs(style);

    let bbox = bounding_box(points);
    let dots = dot_fill_for_mode(&clip_content, &bbox, style, fill_mode);

    let stroke_a = excalidraw_rough_polyline(points, true, 0.0, 0.65);
    let stroke_b = excalidraw_rough_polyline(points, true, 2.0, 0.55);

    format!(
        r##"{group_open}{dots}<path d="{stroke_a}" fill="none" stroke="{stroke}" stroke-width="{stroke_width}" {stroke_attrs}/><path d="{stroke_b}" fill="none" stroke="{stroke}" stroke-width="{secondary_width}" opacity="0.55" {stroke_attrs}/></g>"##,
        group_open = group_open(&group_attrs),
        dots = dots,
        stroke_a = stroke_a,
        stroke_b = stroke_b,
        stroke = style.stroke,
        stroke_width = style.stroke_width,
        secondary_width = (style.stroke_width * 0.75).max(1.0),
        stroke_attrs = stroke_attrs,
    )
}

fn render_dot_fill_cylinder(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    style: &NodeStyle,
    fill_mode: FillMode,
) -> String {
    use std::f64::consts::PI;

    let style_name = match fill_mode {
        FillMode::Hachure => "excalidraw",
        FillMode::CrossHatch => "cross-hatch",
    };

    let rx = width / 2.0;
    let ry = (height * 0.14).clamp(6.0, 12.0);
    let cx = x + rx;
    let top_cy = y + ry;
    let bottom_cy = y + height - ry;

    let fill_body = format!(
        "M {x:.1} {top_cy:.1} A {rx:.1} {ry:.1} 0 0 1 {right:.1} {top_cy:.1} L {right:.1} {bottom_cy:.1} A {rx:.1} {ry:.1} 0 0 1 {x:.1} {bottom_cy:.1} Z",
        right = x + width,
    );
    let top = ellipse_points(cx, top_cy, rx, ry, 28);
    let bottom_front = ellipse_arc_points(cx, bottom_cy, rx, ry, 0.0, PI, 16);
    let left_side = [Point::new(x, top_cy), Point::new(x, bottom_cy)];
    let right_side = [Point::new(x + width, top_cy), Point::new(x + width, bottom_cy)];
    let group_attrs = node_group_attrs(style, style_name, x, y, width, height);
    let stroke_attrs = node_stroke_attrs(style);

    let clip_content = format!(
        r##"<path d="{fill_body}"/><ellipse cx="{cx:.1}" cy="{top_cy:.1}" rx="{rx:.1}" ry="{ry:.1}"/>"##,
    );
    let bbox = (x, y, width, height);
    let dots = dot_fill_for_mode(&clip_content, &bbox, style, fill_mode);

    format!(
        r##"{group_open}{dots}<path d="{top_a}" fill="none" stroke="{stroke}" stroke-width="{stroke_width}" {stroke_attrs}/><path d="{top_b}" fill="none" stroke="{stroke}" stroke-width="{secondary_width}" opacity="0.55" {stroke_attrs}/><path d="{left_a}" fill="none" stroke="{stroke}" stroke-width="{stroke_width}" {stroke_attrs}/><path d="{left_b}" fill="none" stroke="{stroke}" stroke-width="{secondary_width}" opacity="0.55" {stroke_attrs}/><path d="{right_a}" fill="none" stroke="{stroke}" stroke-width="{stroke_width}" {stroke_attrs}/><path d="{right_b}" fill="none" stroke="{stroke}" stroke-width="{secondary_width}" opacity="0.55" {stroke_attrs}/><path d="{bottom_a}" fill="none" stroke="{stroke}" stroke-width="{stroke_width}" {stroke_attrs}/><path d="{bottom_b}" fill="none" stroke="{stroke}" stroke-width="{secondary_width}" opacity="0.55" {stroke_attrs}/></g>"##,
        group_open = group_open(&group_attrs),
        dots = dots,
        top_a = excalidraw_rough_polyline(&top, true, 0.0, 0.6),
        top_b = excalidraw_rough_polyline(&top, true, 2.0, 0.5),
        left_a = excalidraw_rough_polyline(&left_side, false, 0.5, 0.5),
        left_b = excalidraw_rough_polyline(&left_side, false, 2.5, 0.45),
        right_a = excalidraw_rough_polyline(&right_side, false, 1.0, 0.5),
        right_b = excalidraw_rough_polyline(&right_side, false, 3.0, 0.45),
        bottom_a = excalidraw_rough_polyline(&bottom_front, false, 0.3, 0.55),
        bottom_b = excalidraw_rough_polyline(&bottom_front, false, 2.3, 0.5),
        stroke = style.stroke,
        stroke_width = style.stroke_width,
        secondary_width = (style.stroke_width * 0.75).max(1.0),
        stroke_attrs = stroke_attrs,
    )
}

fn render_dot_fill_person(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    style: &NodeStyle,
    fill_mode: FillMode,
) -> String {
    let style_name = match fill_mode {
        FillMode::Hachure => "excalidraw",
        FillMode::CrossHatch => "cross-hatch",
    };

    let head_r = width.min(height) * 0.24;
    let head_cx = x + width / 2.0;
    let head_cy = y + head_r + 1.0;
    let head = ellipse_points(head_cx, head_cy, head_r, head_r, 24);
    let body = vec![
        Point::new(head_cx, y + head_r * 2.1),
        Point::new(x + width * 0.9, y + height),
        Point::new(x + width * 0.1, y + height),
    ];
    let group_attrs = node_group_attrs(style, style_name, x, y, width, height);
    let stroke_attrs = node_stroke_attrs(style);

    let head_clip = format!(r##"<path d="{}"/>"##, polyline_path(&head, true));
    let body_clip = format!(r##"<path d="{}"/>"##, polyline_path(&body, true));
    let head_dots = dot_fill_for_mode(&head_clip, &bounding_box(&head), style, fill_mode);
    let body_dots = dot_fill_for_mode(&body_clip, &bounding_box(&body), style, fill_mode);

    format!(
        r##"{group_open}{head_dots}{body_dots}<path d="{head_a}" fill="none" stroke="{stroke}" stroke-width="{stroke_width}" {stroke_attrs}/><path d="{head_b}" fill="none" stroke="{stroke}" stroke-width="{secondary_width}" opacity="0.55" {stroke_attrs}/><path d="{body_a}" fill="none" stroke="{stroke}" stroke-width="{stroke_width}" {stroke_attrs}/><path d="{body_b}" fill="none" stroke="{stroke}" stroke-width="{secondary_width}" opacity="0.55" {stroke_attrs}/></g>"##,
        group_open = group_open(&group_attrs),
        head_dots = head_dots,
        body_dots = body_dots,
        head_a = excalidraw_rough_polyline(&head, true, 0.0, 0.55),
        head_b = excalidraw_rough_polyline(&head, true, 2.0, 0.5),
        body_a = excalidraw_rough_polyline(&body, true, 0.5, 0.6),
        body_b = excalidraw_rough_polyline(&body, true, 2.5, 0.5),
        stroke = style.stroke,
        stroke_width = style.stroke_width,
        secondary_width = (style.stroke_width * 0.75).max(1.0),
        stroke_attrs = stroke_attrs,
    )
}

fn render_dot_fill_subprocess(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    style: &NodeStyle,
    fill_mode: FillMode,
) -> String {
    let outer = render_dot_fill_closed_shape(
        &rect_points(x, y, width, height),
        style,
        &NodeShape::Subprocess,
        fill_mode,
    );
    let (ix, iy, iw, ih) = subprocess_inset(x, y, width, height);
    let inner_pts = rect_points(ix, iy, iw, ih);
    let inner_stroke = excalidraw_rough_polyline(&inner_pts, true, 0.0, 0.65);
    format!(
        r##"{outer}<path d="{inner_stroke}" fill="none" stroke="{stroke}" stroke-width="{stroke_width}"/>"##,
        outer = outer,
        inner_stroke = inner_stroke,
        stroke = style.stroke,
        stroke_width = style.stroke_width,
    )
}

// ---------------------------------------------------------------------------
// Edge rendering (shared between fill modes)
// ---------------------------------------------------------------------------

fn render_edge_for_fill_mode(
    points: &[Point],
    stroke: &str,
    style: &EdgeStyle,
    marker_end: &str,
    marker_start: &str,
    fill_mode: FillMode,
) -> String {
    let style_name = match fill_mode {
        FillMode::Hachure => "excalidraw",
        FillMode::CrossHatch => "cross-hatch",
    };
    let attrs = edge_attrs(style, style_name);
    let primary = excalidraw_rough_polyline(points, false, 0.0, 0.6);
    let secondary = excalidraw_rough_polyline(points, false, 2.0, 0.5);
    format!(
        r##"<path d="{primary}" fill="none" stroke="{stroke}" stroke-width="{stroke_width}" {attrs}/><path d="{secondary}" fill="none" stroke="{stroke}" stroke-width="{secondary_width}" opacity="0.55" {attrs} marker-end="{marker_end}" marker-start="{marker_start}"/>"##,
        primary = primary,
        secondary = secondary,
        stroke = stroke,
        stroke_width = style.stroke_width,
        secondary_width = (style.stroke_width * 0.75).max(1.0),
        attrs = attrs,
        marker_end = marker_end,
        marker_start = marker_start,
    )
}

// ---------------------------------------------------------------------------
// Excalidraw rough polyline algorithm
// ---------------------------------------------------------------------------

/// Excalidraw-style rough polyline: gentler jitter than hand-drawn,
/// using smooth cubic bezier curves with subtle random offsets.
pub fn excalidraw_rough_polyline(points: &[Point], closed: bool, seed: f64, roughness: f64) -> String {
    let params = RoughnessParams {
        closed,
        seed,
        roughness,
        tangent_jitter: 0.3,
        lerp_a: 0.33,
        lerp_b: 0.67,
        base_len_divisor: 60.0,
        base_clamp_lo: 0.2,
        base_clamp_hi: 1.0,
        base_offset: 0.1,
        drift_tangent: 0.25,
    };
    rough_polyline(points, &params)
}
