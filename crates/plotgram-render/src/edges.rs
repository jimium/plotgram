//! Edge rendering: polyline → SVG path + arrow markers + labels.

use plotgram_model::graph::Arrow;
use plotgram_model::result::EdgePlacement;

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
    let points = &placement.path.points;
    if points.len() < 2 {
        return;
    }

    let style = match resolved.edges.get(&placement.id) {
        Some(s) => s,
        None => return,
    };

    let seed = crate::util::hash_id(&placement.id, 7);
    let transformed = strategy.transform_path(points, seed);
    let d = build_path_d(&transformed);

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

/// Build SVG path `d` from points (M ... L ...).
fn build_path_d(points: &[plotgram_model::geometry::Point]) -> String {
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
/// A single marker with `orient="auto-start-reverse"` serves both
/// `marker-end` and `marker-start` (bidirectional edges).
pub fn arrow_marker_def(id: &str, arrow_style: &str, fill: &str, canvas: &str) -> String {
    match arrow_style {
        "hollow" => format!(
            r#"<marker id="{id}" markerWidth="12" markerHeight="9" refX="10.5" refY="4.5" orient="auto-start-reverse"><polygon points="1 1, 11 4.5, 1 8" fill="{canvas}" stroke="{fill}" stroke-width="1.2" stroke-linejoin="miter"/></marker>"#
        ),
        _ => format!(
            r#"<marker id="{id}" markerWidth="10" markerHeight="7" refX="9" refY="3.5" orient="auto-start-reverse"><polygon points="0 0, 10 3.5, 0 7" fill="{fill}"/></marker>"#
        ),
    }
}
