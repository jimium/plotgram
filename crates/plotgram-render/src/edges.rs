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

    // Arrow marker references (skipped when the theme says arrow_style: none)
    let arrow_style = if style.arrow_style.is_empty() {
        theme.defaults.edge.arrow_style.as_str()
    } else {
        style.arrow_style.as_str()
    };
    if arrow_style != "none" {
        attrs.push_str(r##" marker-end="url(#arrow-head)""##);
        if style.arrow == Arrow::Bidirectional {
            attrs.push_str(r##" marker-start="url(#arrow-head)""##);
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

/// Generate the arrow marker SVG defs.
///
/// Marker appearance follows the theme's `arrow_style`:
/// - `normal`: filled triangle
/// - `hollow`: outlined triangle filled with the canvas color
/// - `none`: no markers at all (returns an empty string)
///
/// A single marker with `orient="auto-start-reverse"` serves both
/// `marker-end` and `marker-start` (bidirectional edges).
pub fn arrow_marker_defs(theme: &CompiledTheme) -> String {
    let fill = &theme.defaults.edge.arrow_fill;
    match theme.defaults.edge.arrow_style.as_str() {
        "none" => String::new(),
        "hollow" => {
            let canvas = &theme.defaults.canvas_background;
            format!(
                r#"<marker id="arrow-head" markerWidth="12" markerHeight="9" refX="10.5" refY="4.5" orient="auto-start-reverse"><polygon points="1 1, 11 4.5, 1 8" fill="{canvas}" stroke="{fill}" stroke-width="1.2" stroke-linejoin="miter"/></marker>"#
            )
        }
        _ => format!(
            r#"<marker id="arrow-head" markerWidth="10" markerHeight="7" refX="9" refY="3.5" orient="auto-start-reverse"><polygon points="0 0, 10 3.5, 0 7" fill="{fill}"/></marker>"#
        ),
    }
}
