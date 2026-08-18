//! Layout-derived decorations (ADR-009). Paint only — never invent geometry.

use tautcore_model::result::Decoration;

use crate::theme::CompiledTheme;
use crate::util::escape_xml;
use crate::SvgBuilder;

/// Half-height of a lifeline notch around a recorded crossing y (v1 constant).
const LIFELINE_GAP_HALF: f64 = 5.0;

/// Draw decorations in z-order: lifelines, activation bars, then fragment frames.
///
/// Dispatch is by decoration *kind*, never by `layout.name` / `profile`.
pub fn render_decorations(svg: &mut SvgBuilder, decorations: &[Decoration], theme: &CompiledTheme) {
    for d in decorations {
        if let Decoration::Lifeline { .. } = d {
            render_one(svg, d, theme);
        }
    }
    for d in decorations {
        if let Decoration::Activation { .. } = d {
            render_one(svg, d, theme);
        }
    }
    for d in decorations {
        if let Decoration::FragmentFrame { .. } = d {
            render_one(svg, d, theme);
        }
    }
}

fn render_one(svg: &mut SvgBuilder, decoration: &Decoration, theme: &CompiledTheme) {
    match decoration {
        Decoration::Lifeline {
            id,
            x,
            y0,
            y1,
            gaps,
            ..
        } => {
            let stroke = escape_xml(&theme.defaults.edge.stroke);
            paint_lifeline(svg, id, *x, *y0, *y1, gaps, &stroke);
        }
        Decoration::Activation { id, frame, .. } => {
            let fill = escape_xml(&theme.defaults.group.fill);
            let stroke = escape_xml(&theme.defaults.node.stroke);
            svg.add_element(format!(
                r#"<rect data-decoration="{id}" x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" fill="{fill}" stroke="{stroke}" stroke-width="1"/>"#,
                frame.x, frame.y, frame.width, frame.height
            ));
        }
        Decoration::FragmentFrame {
            id,
            operator,
            label,
            frame,
            operands,
        } => {
            paint_fragment(svg, id, operator, label.as_deref(), *frame, operands, theme);
        }
    }
}

const FRAGMENT_TITLE_H: f64 = 14.0;
const FRAGMENT_CHAR_W: f64 = 6.4;

fn paint_fragment(
    svg: &mut SvgBuilder,
    id: &str,
    operator: &str,
    label: Option<&str>,
    frame: tautcore_model::geometry::Rect,
    operands: &[f64],
    theme: &CompiledTheme,
) {
    let stroke = escape_xml(&theme.defaults.node.stroke);
    let fill = escape_xml(&theme.defaults.group.fill);
    let text_fill = escape_xml(&theme.defaults.node.text_fill);
    let font = escape_xml(&theme.defaults.typography.font_family);
    svg.add_element(format!(
        r#"<rect data-decoration="{id}" x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" fill="none" stroke="{stroke}" stroke-width="1.2"/>"#,
        frame.x, frame.y, frame.width, frame.height
    ));
    let title = match label {
        Some(l) if !l.is_empty() => format!("{operator} [{l}]"),
        _ => operator.to_string(),
    };
    let title_esc = escape_xml(&title);
    let text_w = (title.chars().count() as f64 * FRAGMENT_CHAR_W + 10.0)
        .min(frame.width.max(24.0) - 8.0)
        .max(24.0);
    let th = FRAGMENT_TITLE_H.min(frame.height.max(1.0));
    let x0 = frame.x;
    let y0 = frame.y;
    let x1 = frame.x + text_w;
    let notch = (th * 0.45).min(8.0);
    let d = format!(
        "M {x0:.1} {y0:.1} L {x1:.1} {y0:.1} L {:.1} {:.1} L {x1:.1} {:.1} L {x0:.1} {:.1} Z",
        x1 + notch,
        y0 + th / 2.0,
        y0 + th,
        y0 + th
    );
    svg.add_element(format!(
        r#"<path data-decoration="{id}" data-role="fragment-title" d="{d}" fill="{fill}" stroke="{stroke}" stroke-width="1"/>"#
    ));
    svg.add_element(format!(
        r#"<text data-decoration="{id}" x="{:.1}" y="{:.1}" text-anchor="start" dominant-baseline="central" fill="{text_fill}" font-size="10" font-family="{font}">{title_esc}</text>"#,
        x0 + 6.0,
        y0 + th / 2.0
    ));
    for (i, y) in operands.iter().enumerate() {
        if *y <= frame.y + th || *y >= frame.bottom() {
            continue;
        }
        svg.add_element(format!(
            r#"<line data-decoration="{id}" data-role="fragment-operand" data-seg="{i}" x1="{:.1}" y1="{y:.1}" x2="{:.1}" y2="{y:.1}" stroke="{stroke}" stroke-width="1" stroke-dasharray="6 4" fill="none"/>"#,
            frame.x,
            frame.right()
        ));
    }
}

fn paint_lifeline(
    svg: &mut SvgBuilder,
    id: &str,
    x: f64,
    y0: f64,
    y1: f64,
    gaps: &[f64],
    stroke: &str,
) {
    let (top, bot) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
    let segments = notched_segments(top, bot, gaps);
    for (i, (a, b)) in segments.iter().enumerate() {
        svg.add_element(format!(
            r#"<line data-decoration="{id}" data-seg="{i}" x1="{x:.1}" y1="{a:.1}" x2="{x:.1}" y2="{b:.1}" stroke="{stroke}" stroke-width="1" stroke-dasharray="4 4" fill="none"/>"#
        ));
    }
}

fn notched_segments(start: f64, end: f64, gap_ys: &[f64]) -> Vec<(f64, f64)> {
    if gap_ys.is_empty() {
        return vec![(start, end)];
    }
    let mut ranges: Vec<(f64, f64)> = gap_ys
        .iter()
        .map(|y| (y - LIFELINE_GAP_HALF, y + LIFELINE_GAP_HALF))
        .collect();
    ranges.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for (lo, hi) in ranges {
        if let Some(last) = merged.last_mut() {
            if lo <= last.1 {
                last.1 = last.1.max(hi);
                continue;
            }
        }
        merged.push((lo, hi));
    }
    let mut out = Vec::new();
    let mut cursor = start;
    for (gap_start, gap_end) in merged {
        let gap_start = gap_start.max(start);
        let gap_end = gap_end.min(end);
        if gap_start > cursor {
            out.push((cursor, gap_start));
        }
        cursor = cursor.max(gap_end);
    }
    if cursor < end {
        out.push((cursor, end));
    }
    if out.is_empty() {
        vec![(start, end)]
    } else {
        out
    }
}
