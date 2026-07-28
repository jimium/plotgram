use std::f64::consts::TAU;

use crate::render::visual::{NodeShape, NodeStyle};

#[derive(Clone, Copy, Debug)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn lerp(self, other: Self, t: f64) -> Self {
        Self {
            x: self.x + (other.x - self.x) * t,
            y: self.y + (other.y - self.y) * t,
        }
    }

    pub fn add(self, dx: f64, dy: f64) -> Self {
        Self {
            x: self.x + dx,
            y: self.y + dy,
        }
    }
}

// ---------------------------------------------------------------------------
// Geometry helpers
// ---------------------------------------------------------------------------

pub fn normalize(vector: Point) -> Point {
    let len = (vector.x * vector.x + vector.y * vector.y).sqrt();
    if len <= 1e-6 {
        Point::new(0.0, 0.0)
    } else {
        Point::new(vector.x / len, vector.y / len)
    }
}

pub fn distance(a: Point, b: Point) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    (dx * dx + dy * dy).sqrt()
}

pub fn same_point(a: Point, b: Point) -> bool {
    (a.x - b.x).abs() < 0.01 && (a.y - b.y).abs() < 0.01
}

pub fn noise(seed: f64, value: f64) -> f64 {
    let raw = (value * 12.9898 + seed * 78.233).sin() * 43758.5453;
    raw.fract() * 2.0 - 1.0
}

pub fn bounding_box(points: &[Point]) -> (f64, f64, f64, f64) {
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
// Shape point generators
// ---------------------------------------------------------------------------

pub fn rect_points(x: f64, y: f64, width: f64, height: f64) -> Vec<Point> {
    vec![
        Point::new(x, y),
        Point::new(x + width, y),
        Point::new(x + width, y + height),
        Point::new(x, y + height),
    ]
}

pub fn rounded_rect_points(x: f64, y: f64, width: f64, height: f64, radius: f64) -> Vec<Point> {
    use std::f64::consts::PI;

    let rx = radius.min(width / 2.0).max(0.0);
    let ry = radius.min(height / 2.0).max(0.0);
    if rx <= 0.0 || ry <= 0.0 {
        return rect_points(x, y, width, height);
    }

    let mut points = Vec::new();
    push_point(&mut points, Point::new(x + rx, y));
    push_point(&mut points, Point::new(x + width - rx, y));
    append_arc(
        &mut points,
        Point::new(x + width - rx, y + ry),
        rx,
        ry,
        -PI / 2.0,
        0.0,
        6,
    );
    push_point(&mut points, Point::new(x + width, y + height - ry));
    append_arc(
        &mut points,
        Point::new(x + width - rx, y + height - ry),
        rx,
        ry,
        0.0,
        PI / 2.0,
        6,
    );
    push_point(&mut points, Point::new(x + rx, y + height));
    append_arc(
        &mut points,
        Point::new(x + rx, y + height - ry),
        rx,
        ry,
        PI / 2.0,
        PI,
        6,
    );
    push_point(&mut points, Point::new(x, y + ry));
    append_arc(
        &mut points,
        Point::new(x + rx, y + ry),
        rx,
        ry,
        PI,
        PI * 1.5,
        6,
    );
    points
}

pub fn ellipse_points(cx: f64, cy: f64, rx: f64, ry: f64, samples: usize) -> Vec<Point> {
    let mut points = Vec::new();
    for i in 0..samples {
        let angle = TAU * i as f64 / samples as f64;
        push_point(
            &mut points,
            Point::new(cx + angle.cos() * rx, cy + angle.sin() * ry),
        );
    }
    points
}

pub fn ellipse_arc_points(
    cx: f64,
    cy: f64,
    rx: f64,
    ry: f64,
    start: f64,
    end: f64,
    samples: usize,
) -> Vec<Point> {
    let mut points = Vec::new();
    append_arc(
        &mut points,
        Point::new(cx, cy),
        rx,
        ry,
        start,
        end,
        samples,
    );
    points
}

pub fn append_arc(
    points: &mut Vec<Point>,
    center: Point,
    rx: f64,
    ry: f64,
    start: f64,
    end: f64,
    samples: usize,
) {
    for i in 0..=samples {
        let t = i as f64 / samples as f64;
        let angle = start + (end - start) * t;
        push_point(
            points,
            Point::new(center.x + angle.cos() * rx, center.y + angle.sin() * ry),
        );
    }
}

pub fn push_point(points: &mut Vec<Point>, point: Point) {
    if points
        .last()
        .map(|last| same_point(*last, point))
        .unwrap_or(false)
    {
        return;
    }
    points.push(point);
}

// ---------------------------------------------------------------------------
// SVG path helpers
// ---------------------------------------------------------------------------

pub fn polyline_path(points: &[Point], closed: bool) -> String {
    if points.is_empty() {
        return String::new();
    }

    let mut d = format!("M {:.1} {:.1}", points[0].x, points[0].y);
    for point in &points[1..] {
        d.push_str(&format!(" L {:.1} {:.1}", point.x, point.y));
    }
    if closed {
        d.push_str(" Z");
    }
    d
}

pub fn svg_path_to_points(path_data: &str) -> Option<(Vec<Point>, bool)> {
    let tokens = tokenize_path_data(path_data);
    if tokens.is_empty() {
        return None;
    }

    let mut idx = 0;
    let mut current = Point::new(0.0, 0.0);
    let mut start = Point::new(0.0, 0.0);
    let mut points = Vec::new();
    let mut closed = false;

    while idx < tokens.len() {
        let token = tokens[idx].as_str();
        idx += 1;
        let command = token.chars().next()?;
        match command {
            'M' | 'm' => {
                let point = parse_point(&tokens, &mut idx)?;
                current = point;
                start = point;
                push_point(&mut points, point);
            }
            'L' | 'l' => {
                let point = parse_point(&tokens, &mut idx)?;
                current = point;
                push_point(&mut points, point);
            }
            'Q' | 'q' => {
                let ctrl = parse_point(&tokens, &mut idx)?;
                let end = parse_point(&tokens, &mut idx)?;
                for step in 1..=6 {
                    let t = step as f64 / 6.0;
                    push_point(&mut points, quad_point(current, ctrl, end, t));
                }
                current = end;
            }
            'C' | 'c' => {
                let ctrl1 = parse_point(&tokens, &mut idx)?;
                let ctrl2 = parse_point(&tokens, &mut idx)?;
                let end = parse_point(&tokens, &mut idx)?;
                for step in 1..=8 {
                    let t = step as f64 / 8.0;
                    push_point(&mut points, cubic_point(current, ctrl1, ctrl2, end, t));
                }
                current = end;
            }
            'Z' | 'z' => {
                closed = true;
                push_point(&mut points, start);
                current = start;
            }
            _ => return None,
        }
    }

    if points.len() < 2 {
        None
    } else {
        Some((points, closed))
    }
}

fn tokenize_path_data(path_data: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();

    for ch in path_data.chars() {
        if ch.is_ascii_alphabetic() {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            tokens.push(ch.to_string());
        } else if ch.is_ascii_digit() || matches!(ch, '-' | '.' | '+' | 'e' | 'E') {
            current.push(ch);
        } else if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

fn parse_point(tokens: &[String], idx: &mut usize) -> Option<Point> {
    let x = tokens.get(*idx)?.parse::<f64>().ok()?;
    *idx += 1;
    let y = tokens.get(*idx)?.parse::<f64>().ok()?;
    *idx += 1;
    Some(Point::new(x, y))
}

fn quad_point(start: Point, ctrl: Point, end: Point, t: f64) -> Point {
    let mt = 1.0 - t;
    Point::new(
        mt * mt * start.x + 2.0 * mt * t * ctrl.x + t * t * end.x,
        mt * mt * start.y + 2.0 * mt * t * ctrl.y + t * t * end.y,
    )
}

fn cubic_point(start: Point, ctrl1: Point, ctrl2: Point, end: Point, t: f64) -> Point {
    let mt = 1.0 - t;
    Point::new(
        mt * mt * mt * start.x
            + 3.0 * mt * mt * t * ctrl1.x
            + 3.0 * mt * t * t * ctrl2.x
            + t * t * t * end.x,
        mt * mt * mt * start.y
            + 3.0 * mt * mt * t * ctrl1.y
            + 3.0 * mt * t * t * ctrl2.y
            + t * t * t * end.y,
    )
}

// ---------------------------------------------------------------------------
// SVG attribute helpers
// ---------------------------------------------------------------------------

/// SVG transform 绕节点中心施加，避免 skew 等变换相对画布原点偏移。
pub fn centered_node_transform(transform: &str, cx: f64, cy: f64) -> String {
    format!(
        r#"transform="translate({cx:.1},{cy:.1}) {transform} translate({ncx:.1},{ncy:.1})""#,
        cx = cx,
        cy = cy,
        ncx = -cx,
        ncy = -cy,
        transform = transform,
    )
}

pub fn node_group_attrs(
    style: &crate::render::visual::NodeStyle,
    graphic_style_name: &str,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> String {
    let mut attrs = Vec::new();
    if let Some(transform) = &style.transform {
        let cx = x + width / 2.0;
        let cy = y + height / 2.0;
        attrs.push(centered_node_transform(transform, cx, cy));
    }
    if !graphic_style_name.is_empty() {
        attrs.push(format!(r#"data-graphic-style="{graphic_style_name}""#));
    }
    attrs.join(" ")
}

pub fn node_stroke_attrs(style: &crate::render::visual::NodeStyle) -> String {
    let mut attrs = Vec::new();
    if let Some(dash) = &style.stroke_dasharray {
        attrs.push(format!(r#"stroke-dasharray="{dash}""#));
    }
    if let Some(linecap) = &style.stroke_linecap {
        attrs.push(format!(r#"stroke-linecap="{linecap}""#));
    }
    if let Some(linejoin) = &style.stroke_linejoin {
        attrs.push(format!(r#"stroke-linejoin="{linejoin}""#));
    }
    attrs.join(" ")
}

pub fn edge_attrs(style: &crate::render::visual::EdgeStyle, graphic_style_name: &str) -> String {
    let mut attrs = Vec::new();
    if let Some(dash) = &style.stroke_dasharray {
        attrs.push(format!(r#"stroke-dasharray="{dash}""#));
    }
    if let Some(linecap) = &style.stroke_linecap {
        attrs.push(format!(r#"stroke-linecap="{linecap}""#));
    }
    if let Some(linejoin) = &style.stroke_linejoin {
        attrs.push(format!(r#"stroke-linejoin="{linejoin}""#));
    }
    if !graphic_style_name.is_empty() {
        attrs.push(format!(r#"data-graphic-style="{graphic_style_name}""#));
    }
    attrs.join(" ")
}

pub fn group_open(attrs: &str) -> String {
    if attrs.is_empty() {
        "<g>".to_string()
    } else {
        format!(r#"<g {attrs}>"#)
    }
}

// ---------------------------------------------------------------------------
// Line clipping (Cohen-Sutherland-like) — used by unit tests and hachure helpers
// ---------------------------------------------------------------------------

#[cfg(test)]
pub fn clip_line_to_rect(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    rx: f64,
    ry: f64,
    rw: f64,
    rh: f64,
) -> Option<(f64, f64, f64, f64)> {
    let dx = x2 - x1;
    let dy = y2 - y1;

    let mut t_min = 0.0_f64;
    let mut t_max = 1.0_f64;

    for (p, q) in [(-dx, x1 - rx), (dx, rx + rw - x1), (-dy, y1 - ry), (dy, ry + rh - y1)] {
        if p.abs() < 1e-10 {
            if q < 0.0 {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t_min = t_min.max(t);
            } else {
                t_max = t_max.min(t);
            }
        }
    }

    if t_min > t_max {
        return None;
    }

    Some((
        x1 + dx * t_min,
        y1 + dy * t_min,
        x1 + dx * t_max,
        y1 + dy * t_max,
    ))
}

// ---------------------------------------------------------------------------
// Hachure fill (shared by Excalidraw and Cross-hatch)
// ---------------------------------------------------------------------------

/// Generic hachure (diagonal line) fill for a bounding box.
/// `angle` in radians, `gap` between lines, `jitter` per-endpoint noise amplitude,
/// `line_width` and `opacity` for the fill strokes.
#[cfg(test)]
pub fn hachure_fill(
    bbox: &(f64, f64, f64, f64),
    stroke: &str,
    gap: f64,
    angle: f64,
    line_width: f64,
    opacity: f64,
    jitter: f64,
) -> String {
    let (bx, by, bw, bh) = *bbox;

    let cos_a = angle.cos();
    let sin_a = angle.sin();

    let diagonal = (bw * bw + bh * bh).sqrt();
    let cx = bx + bw / 2.0;
    let cy = by + bh / 2.0;
    let half = diagonal / 2.0;

    let mut lines = Vec::new();
    let mut offset = -half;
    let mut idx = 0;
    while offset <= half {
        let lx1 = cx + cos_a * half - sin_a * offset;
        let ly1 = cy + sin_a * half + cos_a * offset;
        let lx2 = cx - cos_a * half - sin_a * offset;
        let ly2 = cy - sin_a * half + cos_a * offset;

        if let Some(clipped) = clip_line_to_rect(lx1, ly1, lx2, ly2, bx, by, bw, bh) {
            let n1 = noise(idx as f64 * 0.73, lx1 * 0.11);
            let n2 = noise(idx as f64 * 0.73 + 0.5, ly1 * 0.11);
            let n3 = noise(idx as f64 * 0.73 + 1.0, lx2 * 0.11);
            let n4 = noise(idx as f64 * 0.73 + 1.5, ly2 * 0.11);
            lines.push(format!(
                "M {:.1} {:.1} L {:.1} {:.1}",
                clipped.0 + n1 * jitter,
                clipped.1 + n2 * jitter,
                clipped.2 + n3 * jitter,
                clipped.3 + n4 * jitter,
            ));
        }
        offset += gap;
        idx += 1;
    }

    if lines.is_empty() {
        return String::new();
    }

    let d = lines.join(" ");
    format!(
        r##"<path d="{d}" fill="none" stroke="{stroke}" stroke-width="{line_width:.1}" opacity="{opacity:.2}"/>"##,
    )
}

/// Compute an adaptive gap based on the bounding box area.
/// Larger shapes get wider gaps; smaller shapes get tighter gaps.
pub fn adaptive_gap(bbox: &(f64, f64, f64, f64), min_gap: f64, max_gap: f64) -> f64 {
    let (_, _, bw, bh) = *bbox;
    let area = bw * bh;
    let min_area = 1500.0; // ~50x30
    let max_area = 80000.0; // ~400x200
    let t = ((area - min_area) / (max_area - min_area)).clamp(0.0, 1.0);
    min_gap + (max_gap - min_gap) * t
}

// ---------------------------------------------------------------------------
// Dot fill (shared by Dotted style)
// ---------------------------------------------------------------------------

/// Sparse dot fill: place small circles in a grid pattern within the bounding box.
pub fn dot_fill(
    bbox: &(f64, f64, f64, f64),
    fill: &str,
    gap: f64,
    dot_r: f64,
    opacity: f64,
) -> String {
    let (bx, by, bw, bh) = *bbox;
    let mut circles = Vec::new();
    let mut y = by + gap / 2.0;
    let mut row = 0;
    while y < by + bh {
        let x_offset = if row % 2 == 1 { gap / 2.0 } else { 0.0 };
        let mut x = bx + gap / 2.0 + x_offset;
        while x < bx + bw {
            circles.push(format!(
                r##"<circle cx="{:.1}" cy="{:.1}" r="{:.1}"/>"##,
                x, y, dot_r
            ));
            x += gap;
        }
        y += gap;
        row += 1;
    }

    if circles.is_empty() {
        return String::new();
    }

    format!(
        r##"<g fill="{fill}" opacity="{opacity:.2}">{}</g>"##,
        circles.join("")
    )
}

/// Deterministic short id from clip content for SVG `clipPath` references.
pub fn clip_path_id(prefix: &str, clip_content: &str) -> String {
    let hash = clip_content
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    format!("{prefix}-clip-{hash:08x}")
}

/// Dot fill clipped to arbitrary SVG clip-path content (`<path>`, `<ellipse>`, etc.).
pub fn clipped_dot_fill(
    clip_content: &str,
    clip_id_prefix: &str,
    bbox: &(f64, f64, f64, f64),
    fill: &str,
    gap: f64,
    dot_r: f64,
    opacity: f64,
) -> String {
    let dots = dot_fill(bbox, fill, gap, dot_r, opacity);
    if dots.is_empty() || clip_content.is_empty() {
        return dots;
    }
    let clip_id = clip_path_id(clip_id_prefix, clip_content);
    format!(
        r##"<defs><clipPath id="{clip_id}">{clip_content}</clipPath></defs><g clip-path="url(#{clip_id})">{dots}</g>"##,
    )
}

// ---------------------------------------------------------------------------
// Shape point generators (Diamond, Hexagon)
// ---------------------------------------------------------------------------

/// 菱形顶点（4 点）
pub fn diamond_points(x: f64, y: f64, width: f64, height: f64) -> Vec<Point> {
    vec![
        Point::new(x + width / 2.0, y),
        Point::new(x + width, y + height / 2.0),
        Point::new(x + width / 2.0, y + height),
        Point::new(x, y + height / 2.0),
    ]
}

/// 六边形顶点（6 点）
pub fn hexagon_points(x: f64, y: f64, width: f64, height: f64) -> Vec<Point> {
    vec![
        Point::new(x + width * 0.22, y),
        Point::new(x + width * 0.78, y),
        Point::new(x + width, y + height / 2.0),
        Point::new(x + width * 0.78, y + height),
        Point::new(x + width * 0.22, y + height),
        Point::new(x, y + height / 2.0),
    ]
}

/// 平行四边形水平倾斜量（约为宽度的 18%）。
pub fn parallelogram_skew(width: f64) -> f64 {
    (width * 0.18).clamp(6.0, width * 0.28)
}

/// 平行四边形顶点（4 点，顶边右倾）。
pub fn parallelogram_points(x: f64, y: f64, width: f64, height: f64) -> Vec<Point> {
    let skew = parallelogram_skew(width);
    vec![
        Point::new(x + skew, y),
        Point::new(x + width + skew, y),
        Point::new(x + width - skew, y + height),
        Point::new(x - skew, y + height),
    ]
}

/// 文档形主体占整体高度的比例。
pub const DOCUMENT_BODY_RATIO: f64 = 0.78;

/// 文档形波浪底边的 y 坐标。
pub fn document_wave_y(y: f64, height: f64) -> f64 {
    y + height * DOCUMENT_BODY_RATIO
}

/// 文档形顶点（矩形主体 + 采样波浪底边）。
pub fn document_points(x: f64, y: f64, width: f64, height: f64) -> Vec<Point> {
    let wave_y = document_wave_y(y, height);
    let wave_h = height * (1.0 - DOCUMENT_BODY_RATIO);
    let mut points = vec![
        Point::new(x, y),
        Point::new(x + width, y),
        Point::new(x + width, wave_y),
    ];
    let steps = 8;
    for i in 0..=steps {
        let t = i as f64 / steps as f64;
        let px = x + width * (1.0 - t);
        let bump = wave_h * (std::f64::consts::PI * t).sin().max(0.0);
        points.push(Point::new(px, wave_y + bump));
    }
    points.push(Point::new(x, wave_y));
    points
}

/// 文档形 SVG path（平滑波浪底边）。
pub fn document_path(x: f64, y: f64, width: f64, height: f64) -> String {
    let wave_y = document_wave_y(y, height);
    let wave_h = height * (1.0 - DOCUMENT_BODY_RATIO);
    let x1 = x + width;
    format!(
        "M {x:.1} {y:.1} H {x1:.1} V {wy:.1} \
         C {c1x:.1} {c1y:.1} {c2x:.1} {c2y:.1} {mx:.1} {wy:.1} \
         C {c3x:.1} {c3y:.1} {c4x:.1} {c4y:.1} {x:.1} {wy:.1} Z",
        wy = wave_y,
        c1x = x + width * 0.85,
        c1y = wave_y + wave_h * 1.15,
        c2x = x + width * 0.65,
        c2y = wave_y - wave_h * 0.15,
        mx = x + width * 0.5,
        c3x = x + width * 0.35,
        c3y = wave_y + wave_h * 1.15,
        c4x = x + width * 0.15,
        c4y = wave_y - wave_h * 0.15,
    )
}

/// 云形轮廓采样点（用于手绘风格多边形近似）。
pub fn cloud_points(x: f64, y: f64, width: f64, height: f64) -> Vec<Point> {
    let samples = 28;
    let cx = x + width / 2.0;
    let cy = y + height * 0.55;
    let rx = width / 2.0;
    let ry = height * 0.42;
    (0..samples)
        .map(|i| {
            let t = i as f64 / samples as f64 * TAU;
            let r_mod = 1.0 + 0.2 * (3.0 * t).sin() + 0.14 * (5.0 * t + 0.8).cos();
            Point::new(cx + rx * t.cos() * r_mod, cy + ry * t.sin() * r_mod)
        })
        .collect()
}

/// 子流程内框缩进。
pub fn subprocess_padding(width: f64, height: f64) -> f64 {
    (width.min(height) * 0.08).clamp(4.0, 10.0)
}

/// 子流程内框坐标 (ix, iy, iw, ih)。
pub fn subprocess_inset(x: f64, y: f64, width: f64, height: f64) -> (f64, f64, f64, f64) {
    let pad = subprocess_padding(width, height);
    (x + pad, y + pad, width - 2.0 * pad, height - 2.0 * pad)
}

// ---------------------------------------------------------------------------
// Parameterized marker definitions
// ---------------------------------------------------------------------------

/// 箭头 marker 样式参数
pub struct ArrowMarkerStyle {
    pub view_box: &'static str,
    pub ref_x: &'static str,
    pub ref_y: &'static str,
    pub marker_width: &'static str,
    pub marker_height: &'static str,
    /// active/bidi marker 的 SVG 内部元素（如 `<path .../>` 或 `<circle .../>`）
    pub active_shape: &'static str,
    /// passive marker 的 SVG 内部元素
    pub passive_shape: &'static str,
}

/// 生成三个标准箭头 marker（active, passive, bidi）
pub fn arrow_markers(style: &ArrowMarkerStyle, active_stroke: &str, passive_stroke: &str) -> String {
    let active_filled = style.active_shape.replace("{stroke}", active_stroke);
    let passive_filled = style.passive_shape.replace("{stroke}", passive_stroke);
    let bidi_filled = style.active_shape.replace("{stroke}", active_stroke);

    format!(
        r##"  <marker id="arrow-active" viewBox="{vb}" refX="{rx}" refY="{ry}" markerWidth="{mw}" markerHeight="{mh}" orient="auto-start-reverse">
    {active}
  </marker>
  <marker id="arrow-passive" viewBox="{vb}" refX="{rx}" refY="{ry}" markerWidth="{mw}" markerHeight="{mh}" orient="auto-start-reverse">
    {passive}
  </marker>
  <marker id="arrow-bidi" viewBox="{vb}" refX="{rx}" refY="{ry}" markerWidth="{mw}" markerHeight="{mh}" orient="auto-start-reverse">
    {bidi}
  </marker>"##,
        vb = style.view_box,
        rx = style.ref_x,
        ry = style.ref_y,
        mw = style.marker_width,
        mh = style.marker_height,
        active = active_filled,
        passive = passive_filled,
        bidi = bidi_filled,
    )
}

// ---------------------------------------------------------------------------
// Centralized shape dispatch
// ---------------------------------------------------------------------------

/// Centralized shape dispatch: matches on NodeShape and calls the appropriate
/// rendering callback, eliminating the duplicated 8-branch match across styles.
///
/// - `closed_shape(points, style, shape)`: render a closed polygon shape.
///   The `shape` parameter allows per-shape customization (e.g., blueprint's
///   dashed flag for Diamond/Hexagon, hand_drawn's roughness_bias per shape).
/// - `cylinder(x, y, width, height, style)`: render a cylinder shape.
/// - `person(x, y, width, height, style)`: render a person/actor shape.
/// - `subprocess(x, y, width, height, style)`: render a double-border subprocess shape.
pub fn dispatch_node_shape<C, Y, P, S>(
    shape: &NodeShape,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    style: &NodeStyle,
    closed_shape: C,
    cylinder: Y,
    person: P,
    subprocess: S,
) -> String
where
    C: Fn(&[Point], &NodeStyle, &NodeShape) -> String,
    Y: Fn(f64, f64, f64, f64, &NodeStyle) -> String,
    P: Fn(f64, f64, f64, f64, &NodeStyle) -> String,
    S: Fn(f64, f64, f64, f64, &NodeStyle) -> String,
{
    match shape {
        NodeShape::Rect => closed_shape(
            &rect_points(x, y, width, height),
            style,
            shape,
        ),
        NodeShape::RoundedRect => closed_shape(
            &rounded_rect_points(x, y, width, height, style.corner_radius(shape, width, height)),
            style,
            shape,
        ),
        NodeShape::Circle => closed_shape(
            &ellipse_points(x + width / 2.0, y + height / 2.0, width / 2.0, height / 2.0, 32),
            style,
            shape,
        ),
        NodeShape::Diamond => closed_shape(
            &diamond_points(x, y, width, height),
            style,
            shape,
        ),
        NodeShape::Cylinder => cylinder(x, y, width, height, style),
        NodeShape::Hexagon => closed_shape(
            &hexagon_points(x, y, width, height),
            style,
            shape,
        ),
        NodeShape::Person => person(x, y, width, height, style),
        NodeShape::Stadium => closed_shape(
            &rounded_rect_points(x, y, width, height, style.corner_radius(shape, width, height)),
            style,
            shape,
        ),
        NodeShape::Parallelogram => closed_shape(
            &parallelogram_points(x, y, width, height),
            style,
            shape,
        ),
        NodeShape::Document => closed_shape(
            &document_points(x, y, width, height),
            style,
            shape,
        ),
        NodeShape::Cloud => closed_shape(
            &cloud_points(x, y, width, height),
            style,
            shape,
        ),
        NodeShape::Subprocess => subprocess(x, y, width, height, style),
    }
}

// ---------------------------------------------------------------------------
// Parameterized rough polyline
// ---------------------------------------------------------------------------

/// 粗糙线参数
pub struct RoughnessParams {
    /// 闭合点去重后长度不足 2 时返回空
    pub closed: bool,
    /// 随机种子
    pub seed: f64,
    /// 粗糙度系数
    pub roughness: f64,
    /// 抖动法线位移系数（hand_drawn=0.45, excalidraw=0.3）
    pub tangent_jitter: f64,
    /// 弯曲控制点 lerp 位置（hand_drawn=0.34/0.72, excalidraw=0.33/0.67）
    pub lerp_a: f64,
    pub lerp_b: f64,
    /// 弯曲基础缩放（hand_drawn=len/48, excalidraw=len/60）
    pub base_len_divisor: f64,
    pub base_clamp_lo: f64,
    pub base_clamp_hi: f64,
    pub base_offset: f64,
    /// 弯曲切线漂移系数（hand_drawn=0.35, excalidraw=0.25）
    pub drift_tangent: f64,
}

/// 参数化粗糙折线路径
pub fn rough_polyline(points: &[Point], params: &RoughnessParams) -> String {
    if points.len() < 2 {
        return String::new();
    }
    let mut base = points.to_vec();
    if params.closed && same_point(base[0], *base.last().unwrap()) {
        base.pop();
    }
    if base.len() < 2 {
        return String::new();
    }
    let jittered = jitter_points_rough(&base, params);
    let mut d = format!("M {:.1} {:.1}", jittered[0].x, jittered[0].y);
    let segment_count = if params.closed { jittered.len() } else { jittered.len() - 1 };
    for i in 0..segment_count {
        let a = jittered[i];
        let b = jittered[(i + 1) % jittered.len()];
        if distance(a, b) < 0.01 {
            continue;
        }
        let (c1, c2) = bowed_controls_rough(a, b, params.seed + i as f64 * 0.41, params);
        d.push_str(&format!(
            " C {:.1} {:.1}, {:.1} {:.1}, {:.1} {:.1}",
            c1.x, c1.y, c2.x, c2.y, b.x, b.y
        ));
    }
    if params.closed {
        d.push_str(" Z");
    }
    d
}

fn jitter_points_rough(points: &[Point], params: &RoughnessParams) -> Vec<Point> {
    let len = points.len();
    points.iter().enumerate().map(|(idx, point)| {
        if !params.closed && (idx == 0 || idx + 1 == len) {
            return *point;
        }
        let prev = points[(idx + len - 1) % len];
        let next = points[(idx + 1) % len];
        let tangent = normalize(Point::new(next.x - prev.x, next.y - prev.y));
        let normal = Point::new(-tangent.y, tangent.x);
        let seg_len = distance(prev, next).min(120.0);
        let scale = params.roughness * (seg_len / 80.0).clamp(0.3, 1.0);
        let n = noise(params.seed, point.x * 0.13 + point.y * 0.09 + idx as f64);
        let t = noise(params.seed + 1.7, point.x * 0.07 - point.y * 0.11 + idx as f64 * 0.6);
        point
            .add(normal.x * scale * n, normal.y * scale * n)
            .add(tangent.x * scale * params.tangent_jitter * t, tangent.y * scale * params.tangent_jitter * t)
    }).collect()
}

fn bowed_controls_rough(a: Point, b: Point, seed: f64, params: &RoughnessParams) -> (Point, Point) {
    let delta = Point::new(b.x - a.x, b.y - a.y);
    let len = distance(a, b);
    let tangent = normalize(delta);
    let normal = Point::new(-tangent.y, tangent.x);
    let base = ((len / params.base_len_divisor).clamp(params.base_clamp_lo, params.base_clamp_hi) + params.base_offset) * params.roughness;
    let curve_a = noise(seed, a.x * 0.11 + a.y * 0.06 + len);
    let curve_b = noise(seed + 0.8, b.x * 0.15 - b.y * 0.04 + len);
    let drift_a = noise(seed + 2.3, len * 0.27);
    let drift_b = noise(seed + 3.6, len * 0.19);
    let c1 = a.lerp(b, params.lerp_a)
        .add(normal.x * base * curve_a, normal.y * base * curve_a)
        .add(tangent.x * base * params.drift_tangent * drift_a, tangent.y * base * params.drift_tangent * drift_a);
    let c2 = a.lerp(b, params.lerp_b)
        .add(normal.x * base * curve_b, normal.y * base * curve_b)
        .add(tangent.x * base * params.drift_tangent * drift_b, tangent.y * base * params.drift_tangent * drift_b);
    (c1, c2)
}

// ---------------------------------------------------------------------------
// Smooth polyline (rounded corner) path
// ---------------------------------------------------------------------------

/// Generate an SVG path string from a list of Point with rounded corners.
/// `radius` controls the corner radius.
pub fn smooth_polyline_path_from_points(points: &[Point], radius: f64) -> String {
    let tuple_points: Vec<(f64, f64)> = points.iter().map(|p| (p.x, p.y)).collect();
    smooth_polyline_path(&tuple_points, radius)
}

/// Generate an SVG path string from a list of (x,y) tuples with rounded corners.
/// `radius` controls the corner radius.
pub fn smooth_polyline_path(points: &[(f64, f64)], radius: f64) -> String {
    if points.len() < 2 {
        return String::new();
    }
    if points.len() == 2 {
        return format!(
            "M {:.1} {:.1} L {:.1} {:.1}",
            points[0].0, points[0].1, points[1].0, points[1].1
        );
    }

    let mut d = format!("M {:.1} {:.1}", points[0].0, points[0].1);

    for i in 1..points.len() - 1 {
        let prev = points[i - 1];
        let curr = points[i];
        let next = points[i + 1];

        let (in_dx, in_dy, in_len) = edge_unit(prev, curr);
        let (out_dx, out_dy, out_len) = edge_unit(curr, next);
        let r = radius.min(in_len / 2.0).min(out_len / 2.0);

        let p_before = (curr.0 - in_dx * r, curr.1 - in_dy * r);
        let p_after = (curr.0 + out_dx * r, curr.1 + out_dy * r);

        d.push_str(&format!(
            " L {:.1} {:.1} Q {:.1} {:.1} {:.1} {:.1}",
            p_before.0, p_before.1, curr.0, curr.1, p_after.0, p_after.1
        ));
    }

    let last = points[points.len() - 1];
    d.push_str(&format!(" L {:.1} {:.1}", last.0, last.1));
    d
}

fn edge_unit(a: (f64, f64), b: (f64, f64)) -> (f64, f64, f64) {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let len = (dx * dx + dy * dy).sqrt();
    if len <= 1e-6 {
        (0.0, 0.0, 0.0)
    } else {
        (dx / len, dy / len, len)
    }
}
