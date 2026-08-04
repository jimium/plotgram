//! Edge rendering: polyline / cubic → SVG path + arrow markers + labels.

use plotgram_model::geometry::Point;
use plotgram_model::graph::Arrow;
use plotgram_model::result::{EdgePath, EdgePlacement};

use crate::resolve::ResolvedGraph;
use crate::strategy::Strategy;
use crate::theme::CompiledTheme;
use crate::SvgBuilder;

/// Render a single edge to SVG.
pub fn render_edge(
    svg: &mut SvgBuilder,
    placement: &EdgePlacement,
    resolved: &ResolvedGraph,
    theme: &CompiledTheme,
    strategy: &Strategy,
) {
    let style = match resolved.edges.get(&placement.id) {
        Some(s) => s,
        None => return,
    };

    let seed = crate::util::hash_id(&placement.id, 7);
    let d = match &placement.path {
        EdgePath::Polyline { points } => {
            if points.len() < 2 {
                return;
            }
            let transformed = strategy.transform_path(points, seed);
            build_polyline_d(&transformed)
        }
        EdgePath::Cubic {
            start,
            end,
            controls,
        } => {
            // Sketch strategy: jitter the four Bezier points as a short polyline.
            let raw = [*start, controls[0], controls[1], *end];
            let t = strategy.transform_path(&raw, seed);
            if t.len() < 4 {
                return;
            }
            build_cubic_d(t[0], t[1], t[2], t[3])
        }
    };

    let mut attrs = format!(
        r#"fill="none" stroke="{}" stroke-width="{}""#,
        style.stroke, style.stroke_width
    );

    if let Some(lc) = &style.stroke_linecap {
        attrs.push_str(&format!(r#" stroke-linecap="{lc}""#));
    }
    if let Some(lj) = &style.stroke_linejoin {
        attrs.push_str(&format!(r#" stroke-linejoin="{lj}""#));
    }
    if let Some(op) = style.stroke_opacity {
        attrs.push_str(&format!(r#" stroke-opacity="{op:.2}""#));
    }
    if let Some(dash) = &style.stroke_dasharray {
        attrs.push_str(&format!(r#" stroke-dasharray="{dash}""#));
    }

    // Arrow markers: emitted per (style, color) so heads follow inline
    // stroke overrides; `arrow_style: none` (theme or per-edge) skips them.
    if style.arrow_style != "none" {
        let marker_id = arrow_marker_id(&style.arrow_style, &style.arrow_fill);
        svg.add_def_once(arrow_marker_def(
            &marker_id,
            &style.arrow_style,
            &style.arrow_fill,
            &theme.defaults.canvas_background,
        ));
        attrs.push_str(&format!(r##" marker-end="url(#{marker_id})""##));
        if style.arrow == Arrow::Bidirectional {
            attrs.push_str(&format!(r##" marker-start="url(#{marker_id})""##));
        }
    }

    svg.add_element(format!(r#"<path d="{d}" {attrs}/>"#));
}

/// Build SVG path `d` from polyline points (M ... L ...).
fn build_polyline_d(points: &[Point]) -> String {
    let mut d = String::new();
    for (i, p) in points.iter().enumerate() {
        if i == 0 {
            d.push_str(&format!("M {:.1} {:.1}", p.x, p.y));
        } else {
            d.push_str(&format!(" L {:.1} {:.1}", p.x, p.y));
        }
    }
    d
}

fn build_cubic_d(p0: Point, p1: Point, p2: Point, p3: Point) -> String {
    format!(
        "M {:.1} {:.1} C {:.1} {:.1}, {:.1} {:.1}, {:.1} {:.1}",
        p0.x, p0.y, p1.x, p1.y, p2.x, p2.y, p3.x, p3.y
    )
}

/// Stable marker id derived from arrow style + fill color (cf. hatch ids).
pub fn arrow_marker_id(arrow_style: &str, fill: &str) -> String {
    let key: String = fill
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(12)
        .collect();
    format!(
        "arrow-head-{arrow_style}-{key}-{:x}",
        crate::util::hash_id(fill, 23)
    )
}

/// Generate one arrow marker SVG def.
///
/// Marker appearance follows `arrow_style`:
/// - `normal` (default): filled triangle
/// - `hollow`: outlined triangle filled with the canvas color
///
/// Uses a square `viewBox` and equal `markerWidth`/`markerHeight` so the
/// head is not stretched along the edge direction.
///
/// A single marker with `orient="auto-start-reverse"` serves both
/// `marker-end` and `marker-start` (bidirectional edges).
pub fn arrow_marker_def(id: &str, arrow_style: &str, fill: &str, canvas: &str) -> String {
    // Footprint is 2/3 of v1 standard (8×8 normal, 7×7 hollow).
    match arrow_style {
        "hollow" => format!(
            r#"<marker id="{id}" viewBox="0 0 10 10" refX="10" refY="5" markerWidth="4.67" markerHeight="4.67" orient="auto-start-reverse"><polygon points="1 1, 10 5, 1 9" fill="{canvas}" stroke="{fill}" stroke-width="0.8" stroke-linejoin="miter"/></marker>"#
        ),
        _ => format!(
            r#"<marker id="{id}" viewBox="0 0 10 10" refX="10" refY="5" markerWidth="5.33" markerHeight="5.33" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z" fill="{fill}"/></marker>"#
        ),
    }
}
