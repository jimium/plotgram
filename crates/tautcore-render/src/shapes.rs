//! Node shape rendering: NodeShape → SVG element.
//!
//! Closed set from `tautcore_model::NodeShape` (dsl-spec §14.6).

use tautcore_model::geometry::Rect;
use tautcore_model::result::NodePlacement;
use tautcore_model::NodeShape;

use crate::outline::{self, closed_path_d};
use crate::resolve::{ResolvedGraph, ResolvedNodeStyle};
use crate::strategy::{FillMode, Strategy};
use crate::util;
use crate::SvgBuilder;

/// Render a single node to SVG and append to builder.
pub fn render_node(
    svg: &mut SvgBuilder,
    placement: &NodePlacement,
    resolved: &ResolvedGraph,
    strategy: &Strategy,
) {
    let style = match resolved.nodes.get(&placement.id) {
        Some(s) => s,
        None => return,
    };

    let frame = &placement.frame;
    let seed = util::hash_id(&placement.id, 0);

    // Hatch fill (sketch): swap solid fill for a per-color diagonal pattern
    let hatched;
    let style = if strategy.fill_mode() == FillMode::Hatch && is_paintable(&style.fill) {
        let pattern_id = hatch_pattern_id(&style.fill);
        svg.add_def_once(hatch_pattern_def(&pattern_id, &style.fill));
        hatched = ResolvedNodeStyle {
            fill: format!("url(#{pattern_id})"),
            ..style.clone()
        };
        &hatched
    } else {
        style
    };

    let elem = shape_svg(style.shape, frame, style, strategy, seed);
    svg.add_element(elem);
}

/// Generate SVG element(s) for a shape.
pub fn shape_svg(
    shape: NodeShape,
    frame: &Rect,
    style: &ResolvedNodeStyle,
    strategy: &Strategy,
    seed: u64,
) -> String {
    if strategy.sample_outlines() {
        return sketch_shape_svg(shape, frame, style, strategy, seed);
    }

    let x = frame.x;
    let y = frame.y;
    let w = frame.width;
    let h = frame.height;
    let extra = extra_attrs(style);

    match shape {
        NodeShape::Rect => {
            let rx = style.radius.unwrap_or(0.0);
            format!(
                r#"<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{rx}" fill="{fill}" stroke="{stroke}" stroke-width="{sw}" {extra}/>"#,
                fill = style.fill,
                stroke = style.stroke,
                sw = style.stroke_width
            )
        }
        NodeShape::RoundedRect => {
            let rx = style.radius.unwrap_or(4.0);
            format!(
                r#"<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{rx}" fill="{fill}" stroke="{stroke}" stroke-width="{sw}" {extra}/>"#,
                fill = style.fill,
                stroke = style.stroke,
                sw = style.stroke_width
            )
        }
        NodeShape::Circle => {
            let r = w.min(h) / 2.0;
            let cx = x + w / 2.0;
            let cy = y + h / 2.0;
            format!(
                r#"<circle cx="{cx}" cy="{cy}" r="{r}" fill="{fill}" stroke="{stroke}" stroke-width="{sw}" {extra}/>"#,
                fill = style.fill,
                stroke = style.stroke,
                sw = style.stroke_width
            )
        }
        NodeShape::Diamond => {
            let cx = x + w / 2.0;
            let cy = y + h / 2.0;
            let points = format!("{cx},{y} {},{cy} {cx},{} {x},{cy}", x + w, y + h);
            format!(
                r#"<polygon points="{points}" fill="{fill}" stroke="{stroke}" stroke-width="{sw}" {extra}/>"#,
                fill = style.fill,
                stroke = style.stroke,
                sw = style.stroke_width
            )
        }
        NodeShape::Cylinder => {
            let ry = 8.0_f64.min(h / 4.0);
            let cx = x + w / 2.0;
            let rx = w / 2.0;
            format!(
                r#"<path d="M {x} {y_ry} A {rx} {ry} 0 0 1 {x_w} {y_ry} L {x_w} {y_h_ry} A {rx} {ry} 0 0 1 {x} {y_h_ry} Z" fill="{fill}" stroke="{stroke}" stroke-width="{sw}" {extra}/><ellipse cx="{cx}" cy="{y_ry}" rx="{rx}" ry="{ry}" fill="{fill}" stroke="{stroke}" stroke-width="{sw}" {extra}/>"#,
                y_ry = y + ry,
                x_w = x + w,
                y_h_ry = y + h - ry,
                fill = style.fill,
                stroke = style.stroke,
                sw = style.stroke_width
            )
        }
        NodeShape::Hexagon => {
            let hm = h / 2.0;
            let w4 = w / 4.0;
            let points = format!(
                "{} {y} {} {y} {} {} {} {} {} {} {x} {}",
                x + w4,
                x + w - w4,
                x + w,
                y + hm,
                x + w - w4,
                y + h,
                x + w4,
                y + h,
                y + hm
            );
            format!(
                r#"<polygon points="{points}" fill="{fill}" stroke="{stroke}" stroke-width="{sw}" {extra}/>"#,
                fill = style.fill,
                stroke = style.stroke,
                sw = style.stroke_width
            )
        }
        NodeShape::Person => {
            // Bust silhouette: head circle + rounded-shoulder torso (cf. C4 / draw.io person)
            let head_r = (w * 0.30).min(h * 0.24);
            let cx = x + w / 2.0;
            let ty = y + head_r * 2.2; // torso top, small gap under the head
            let tw = w.min(head_r * 4.4);
            let sr = (tw * 0.35).min((y + h - ty) * 0.9);
            format!(
                r#"<circle cx="{cx}" cy="{head_cy}" r="{head_r}" fill="{fill}" stroke="{stroke}" stroke-width="{sw}" {extra}/><path d="M {lx} {by} L {lx} {tys} A {sr} {sr} 0 0 1 {lxs} {ty} L {rxs} {ty} A {sr} {sr} 0 0 1 {rx} {tys} L {rx} {by} Z" fill="{fill}" stroke="{stroke}" stroke-width="{sw}" {extra}/>"#,
                head_cy = y + head_r,
                lx = cx - tw / 2.0,
                rx = cx + tw / 2.0,
                lxs = cx - tw / 2.0 + sr,
                rxs = cx + tw / 2.0 - sr,
                tys = ty + sr,
                by = y + h,
                fill = style.fill,
                stroke = style.stroke,
                sw = style.stroke_width
            )
        }
        NodeShape::Stadium => {
            let rx = h.min(w) / 2.0;
            format!(
                r#"<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{rx}" fill="{fill}" stroke="{stroke}" stroke-width="{sw}" {extra}/>"#,
                fill = style.fill,
                stroke = style.stroke,
                sw = style.stroke_width
            )
        }
        NodeShape::Parallelogram => {
            let skew = w * 0.15;
            let points = format!(
                "{},{} {},{} {},{} {},{}",
                x + skew,
                y,
                x + w,
                y,
                x + w - skew,
                y + h,
                x,
                y + h
            );
            format!(
                r#"<polygon points="{points}" fill="{fill}" stroke="{stroke}" stroke-width="{sw}" {extra}/>"#,
                fill = style.fill,
                stroke = style.stroke,
                sw = style.stroke_width
            )
        }
        NodeShape::Document => {
            let wave_y = y + h * 0.85;
            let wave_h = h * 0.15;
            let x1 = x + w;
            let d = format!(
                "M {x:.1} {y:.1} H {x1:.1} V {wy:.1} C {c1x:.1} {c1y:.1} {c2x:.1} {c2y:.1} {mx:.1} {wy:.1} C {c3x:.1} {c3y:.1} {c4x:.1} {c4y:.1} {x:.1} {wy:.1} Z",
                wy = wave_y,
                c1x = x + w * 0.85, c1y = wave_y + wave_h * 1.15,
                c2x = x + w * 0.65, c2y = wave_y - wave_h * 0.15,
                mx = x + w * 0.5,
                c3x = x + w * 0.35, c3y = wave_y + wave_h * 1.15,
                c4x = x + w * 0.15, c4y = wave_y - wave_h * 0.15,
            );
            format!(
                r#"<path d="{d}" fill="{fill}" stroke="{stroke}" stroke-width="{sw}" {extra}/>"#,
                fill = style.fill,
                stroke = style.stroke,
                sw = style.stroke_width
            )
        }
        NodeShape::Cloud => {
            let cx = x + w / 2.0;
            let cy = y + h * 0.55;
            let rx = w / 2.0;
            let ry = h * 0.42;
            let samples = 28;
            let tau = std::f64::consts::TAU;
            let mut points = String::new();
            for i in 0..samples {
                let t = i as f64 / samples as f64 * tau;
                let r_mod = 1.0 + 0.2 * (3.0 * t).sin() + 0.14 * (5.0 * t + 0.8).cos();
                let px = cx + rx * t.cos() * r_mod;
                let py = cy + ry * t.sin() * r_mod;
                if i > 0 {
                    points.push(' ');
                }
                points.push_str(&format!("{px:.1},{py:.1}"));
            }
            format!(
                r#"<polygon points="{points}" fill="{fill}" stroke="{stroke}" stroke-width="{sw}" {extra}/>"#,
                fill = style.fill,
                stroke = style.stroke,
                sw = style.stroke_width
            )
        }
        NodeShape::Subprocess => {
            let pad = (w.min(h) * 0.08).clamp(4.0, 10.0);
            let ix = x + pad;
            let iy = y + pad;
            let iw = w - pad * 2.0;
            let ih = h - pad * 2.0;
            format!(
                r#"<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{fill}" stroke="{stroke}" stroke-width="{sw}" {extra}/><rect x="{ix}" y="{iy}" width="{iw}" height="{ih}" fill="none" stroke="{stroke}" stroke-width="{sw}" {extra}/>"#,
                fill = style.fill,
                stroke = style.stroke,
                sw = style.stroke_width
            )
        }
    }
}

// ─── Sketch outline sampling ────────────────────────────────────

/// Sketch mode: sample the shape outline into polylines, jitter them via the
/// strategy, and render as closed `<path>` subpaths.
fn sketch_shape_svg(
    shape: NodeShape,
    frame: &Rect,
    style: &ResolvedNodeStyle,
    strategy: &Strategy,
    seed: u64,
) -> String {
    let subpaths = outline::shape_outlines(shape, frame, style);
    let extra = extra_attrs(style);
    let mut out = String::new();
    for (i, (pts, fill_override)) in subpaths.iter().enumerate() {
        let jittered = strategy.transform_path(pts, seed.wrapping_add(i as u64 * 101));
        let d = closed_path_d(&jittered);
        let fill = fill_override.as_deref().unwrap_or(&style.fill);
        out.push_str(&format!(
            r#"<path d="{d}" fill="{fill}" stroke="{stroke}" stroke-width="{sw}" {extra}/>"#,
            stroke = style.stroke,
            sw = style.stroke_width
        ));
    }
    out
}

// ─── Hatch fill (sketch) ────────────────────────────────────────

/// Whether a fill value is a real paint (worth hatching).
pub(crate) fn is_paintable(fill: &str) -> bool {
    !(fill.is_empty() || fill == "none" || fill == "transparent" || fill.starts_with("url("))
}

/// Stable pattern id derived from the fill color.
///
/// The alphanumeric prefix keeps ids readable, but it is lossy
/// (`rgb(10,20,30)` and `rgb(102,0,30)` clean to the same key), so a hash
/// of the full fill string is appended to make the id collision-free.
pub(crate) fn hatch_pattern_id(fill: &str) -> String {
    let key: String = fill
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(12)
        .collect();
    format!("hatch-{key}-{:x}", util::hash_id(fill, 17))
}

/// Diagonal hatch pattern def for a fill color.
pub(crate) fn hatch_pattern_def(id: &str, fill: &str) -> String {
    format!(
        r#"<pattern id="{id}" patternUnits="userSpaceOnUse" width="6" height="6" patternTransform="rotate(-45)"><line x1="0" y1="0" x2="0" y2="6" stroke="{fill}" stroke-width="2.4"/></pattern>"#
    )
}

/// Build extra SVG attributes from resolved style.
fn extra_attrs(style: &ResolvedNodeStyle) -> String {
    let mut attrs = Vec::new();
    if let Some(dash) = &style.stroke_dasharray {
        attrs.push(format!(r#"stroke-dasharray="{dash}""#));
    }
    if let Some(lc) = &style.stroke_linecap {
        attrs.push(format!(r#"stroke-linecap="{lc}""#));
    }
    if let Some(lj) = &style.stroke_linejoin {
        attrs.push(format!(r#"stroke-linejoin="{lj}""#));
    }
    if let Some(op) = style.fill_opacity {
        attrs.push(format!(r#"fill-opacity="{op:.2}""#));
    }
    if let Some(op) = style.stroke_opacity {
        attrs.push(format!(r#"stroke-opacity="{op:.2}""#));
    }
    attrs.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hatch_pattern_id_is_stable_and_collision_free() {
        // (fill_a, fill_b, must_differ): pairs whose alphanumeric cleanup
        // collides must still get distinct ids; identical fills must not.
        let cases = [
            ("rgb(10,20,30)", "rgb(102,0,30)", true),
            ("#ABC", "#AB C", true),
            ("rgb(10,20,30)", "rgb(10,20,30)", false),
            ("#E3F2FD", "#E3F2FD", false),
        ];
        for (a, b, must_differ) in cases {
            let (ia, ib) = (hatch_pattern_id(a), hatch_pattern_id(b));
            if must_differ {
                assert_ne!(ia, ib, "{a:?} vs {b:?} must get distinct pattern ids");
            } else {
                assert_eq!(ia, ib, "{a:?} must yield a stable pattern id");
            }
        }
    }
}
