//! Group rendering: bounding box with label area.

use plotgram_model::result::GroupPlacement;

use crate::outline::{closed_path_d, sample_rounded_rect};
use crate::resolve::ResolvedGraph;
use crate::shapes::{hatch_pattern_def, hatch_pattern_id, is_paintable};
use crate::strategy::{FillMode, Strategy};
use crate::util;
use crate::SvgBuilder;

/// Render a group bounding box.
pub fn render_group(
    svg: &mut SvgBuilder,
    placement: &GroupPlacement,
    resolved: &ResolvedGraph,
    strategy: &Strategy,
) {
    let style = match resolved.groups.get(&placement.id) {
        Some(s) => s,
        None => return,
    };

    let frame = &placement.frame;
    let mut fill = style.fill.clone();
    if strategy.fill_mode() == FillMode::Hatch && is_paintable(&fill) {
        let pattern_id = hatch_pattern_id(&fill);
        svg.add_def_once(hatch_pattern_def(&pattern_id, &fill));
        fill = format!("url(#{pattern_id})");
    }

    let mut extra = String::new();
    if let Some(dash) = &style.stroke_dasharray {
        extra.push_str(&format!(r#" stroke-dasharray="{dash}""#));
    }
    if let Some(op) = style.fill_opacity {
        extra.push_str(&format!(r#" fill-opacity="{op:.2}""#));
    }

    if strategy.sample_outlines() {
        let seed = util::hash_id(&placement.id, 13);
        let pts = sample_rounded_rect(frame.x, frame.y, frame.width, frame.height, style.radius);
        let jittered = strategy.transform_path(&pts, seed);
        let d = closed_path_d(&jittered);
        svg.add_element(format!(
            r#"<path d="{d}" fill="{fill}" stroke="{stroke}" stroke-width="{sw}"{extra}/>"#,
            stroke = style.stroke,
            sw = style.stroke_width,
        ));
        return;
    }

    svg.add_element(format!(
        r#"<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{rx}" fill="{fill}" stroke="{stroke}" stroke-width="{sw}"{extra}/>"#,
        x = frame.x,
        y = frame.y,
        w = frame.width,
        h = frame.height,
        rx = style.radius,
        stroke = style.stroke,
        sw = style.stroke_width,
    ));
}
