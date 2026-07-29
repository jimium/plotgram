//! Label rendering: text placement in SVG.

use plotgram_model::result::{LabelOwner, LabelSlot};

use crate::icons;
use crate::resolve::ResolvedGraph;
use crate::theme::CompiledTheme;
use crate::util::escape_xml;
use crate::SvgBuilder;

/// Render the diagram title near the top of the canvas.
///
/// Currently NOT wired into `render_svg`: layout reserves no space for the
/// title, so painting it would overlap top-most nodes. Re-enable once the
/// canvas grows a dedicated title band.
pub fn render_title(svg: &mut SvgBuilder, title: &str, theme: &CompiledTheme) {
    let fill = &theme.defaults.title_fill;
    let font_size = theme.defaults.title_font_size;
    let font_family = &theme.defaults.typography.font_family;
    // Slight inset from top-left of the canvas
    let x = 16.0;
    let y = font_size + 8.0;
    svg.add_element(format!(
        r#"<text x="{x:.1}" y="{y:.1}" text-anchor="start" dominant-baseline="auto" fill="{fill}" font-size="{font_size}" font-family="{font_family}" font-weight="600">{text}</text>"#,
        text = escape_xml(title)
    ));
}

/// Render a label slot to SVG.
pub fn render_label(
    svg: &mut SvgBuilder,
    slot: &LabelSlot,
    resolved: &ResolvedGraph,
    theme: &CompiledTheme,
) {
    let frame = &slot.frame;
    let cx = frame.x + frame.width / 2.0;
    let cy = frame.y + frame.height / 2.0;

    // Determine text style based on owner type
    let (fill, font_size, font_weight) = match &slot.owner {
        LabelOwner::Node(id) => {
            if let Some(ns) = resolved.nodes.get(id) {
                // Node with icon: render icon + left-anchored label as a group
                if let Some(icon) = ns.icon {
                    render_icon_label(svg, slot, icon, ns, theme);
                    return;
                }
                (
                    ns.text_fill.clone(),
                    ns.font_size,
                    ns.font_weight.clone(),
                )
            } else {
                (
                    theme.defaults.node.text_fill.clone(),
                    theme.defaults.node.font_size,
                    None,
                )
            }
        }
        LabelOwner::Edge(_) => {
            maybe_edge_label_bg(svg, slot, theme);
            // Backlog (not dsl-spec promised): per-edge inline `style.text_fill` /
            // `style.font_size` overrides and `slot.role` (mid/head/tail) based
            // styling are not supported — all edge labels use theme edge defaults.
            (
                theme.defaults.edge.text_fill.clone(),
                theme.defaults.edge.font_size,
                None,
            )
        }
        LabelOwner::Group(_) => (
            theme.defaults.group.text_fill.clone(),
            theme.defaults.typography.small_size,
            Some("500".to_string()),
        ),
    };

    let weight_attr = font_weight
        .map(|w| format!(r#" font-weight="{w}""#))
        .unwrap_or_default();

    let font_family = &theme.defaults.typography.font_family;

    svg.add_element(format!(
        r#"<text x="{cx:.1}" y="{cy:.1}" text-anchor="middle" dominant-baseline="central" fill="{fill}" font-size="{font_size}" font-family="{font_family}"{weight_attr}>{text}</text>"#,
        text = escape_xml(&slot.text)
    ));
}

/// Optional background rect behind an edge label (`label_bg` theme field).
fn maybe_edge_label_bg(svg: &mut SvgBuilder, slot: &LabelSlot, theme: &CompiledTheme) {
    let Some(raw) = theme.defaults.edge.label_bg.as_deref() else {
        return;
    };
    if raw.is_empty() || raw == "none" {
        return;
    }
    let fill = if raw == "canvas" {
        theme.defaults.canvas_background.as_str()
    } else {
        raw
    };
    let opacity = theme.defaults.edge.label_bg_opacity;
    let frame = &slot.frame;
    let pad = 2.0;
    svg.add_element(format!(
        r#"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" rx="2" fill="{fill}" fill-opacity="{opacity:.2}"/>"#,
        x = frame.x - pad,
        y = frame.y - pad,
        w = frame.width + pad * 2.0,
        h = frame.height + pad * 2.0,
    ));
}

/// Render a node label with its decoration icon: icon glyph + left-anchored text,
/// the pair centered as a group inside the label frame.
fn render_icon_label(
    svg: &mut SvgBuilder,
    slot: &LabelSlot,
    icon: &'static icons::IconDef,
    ns: &crate::resolve::ResolvedNodeStyle,
    theme: &CompiledTheme,
) {
    let frame = &slot.frame;
    let label_width = estimate_text_width(&slot.text, ns.font_size);
    let layout = icons::render::layout_inside(
        frame.x,
        frame.y,
        frame.width,
        frame.height,
        ns.font_size,
        label_width,
    );

    svg.add_element(icons::render::render_icon(
        icon,
        layout.icon_x,
        layout.icon_y,
        layout.icon_size,
        &ns.text_fill,
    ));

    if slot.text.is_empty() {
        return;
    }
    let weight_attr = ns
        .font_weight
        .as_deref()
        .map(|w| format!(r#" font-weight="{w}""#))
        .unwrap_or_default();
    let font_family = &theme.defaults.typography.font_family;
    svg.add_element(format!(
        r#"<text x="{:.1}" y="{:.1}" text-anchor="start" dominant-baseline="central" fill="{fill}" font-size="{font_size}" font-family="{font_family}"{weight_attr}>{text}</text>"#,
        layout.label_x,
        layout.label_y,
        fill = ns.text_fill,
        font_size = ns.font_size,
        text = escape_xml(&slot.text)
    ));
}

/// Rough text width estimate: wide (2-column) glyphs ≈ 1.0 em, others ≈ 0.6 em.
/// Wide-char detection via `unicode-width`, same yardstick as the ASCII backend.
fn estimate_text_width(text: &str, font_size: f64) -> f64 {
    use unicode_width::UnicodeWidthChar;
    text.chars()
        .map(|c| {
            if UnicodeWidthChar::width(c).unwrap_or(1) >= 2 {
                font_size
            } else {
                font_size * 0.6
            }
        })
        .sum()
}
