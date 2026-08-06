//! Icon SVG embedding and layout.
//!
//! Renders icon glyphs inside nodes alongside labels.
//! Migrated from V1 `icons/render.rs`, simplified for V2.

use super::catalog::IconDef;

/// All glyph assets use a 24×24 viewBox.
const GLYPH_VIEWBOX: f64 = 24.0;

/// Icon size relative to label font size.
const ICON_TO_FONT_RATIO: f64 = 1.45;

/// Extra gap beyond base to avoid crowding.
const ICON_GAP_EXTRA: f64 = 1.0;

/// Base gap between icon and label text.
const ICON_GAP_BASE: f64 = 4.0;

/// Layout result for icon + label inside a node.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IconLayout {
    /// Rendered icon size (px).
    pub icon_size: f64,
    /// Gap between icon and label.
    pub gap: f64,
    /// Total group width (icon + gap + label_width).
    pub group_width: f64,
    /// Total group height (max of icon_size, font_size).
    pub group_height: f64,
    /// Icon top-left x (absolute).
    pub icon_x: f64,
    /// Icon top-left y (absolute).
    pub icon_y: f64,
    /// Label baseline x (absolute).
    pub label_x: f64,
    /// Label baseline y (absolute, vertical center).
    pub label_y: f64,
}

/// Compute icon size from font size.
pub fn icon_size(font_size: f64) -> f64 {
    font_size * ICON_TO_FONT_RATIO
}

/// Compute gap between icon and label.
pub fn icon_gap() -> f64 {
    ICON_GAP_BASE + ICON_GAP_EXTRA
}

/// Compute the layout for icon + label centered inside a node.
///
/// `label_width` is the estimated text width of the label.
pub fn layout_inside(
    node_x: f64,
    node_y: f64,
    node_width: f64,
    node_height: f64,
    font_size: f64,
    label_width: f64,
) -> IconLayout {
    let size = icon_size(font_size);
    let gap = icon_gap();
    let group_width = size + gap + label_width;
    let group_height = size.max(font_size);

    // Center the group inside the node
    let group_x = node_x + (node_width - group_width) / 2.0;
    let group_y = node_y + (node_height - group_height) / 2.0;

    let icon_x = group_x;
    let icon_y = group_y + (group_height - size) / 2.0;

    let label_x = group_x + size + gap;
    let label_y = group_y + group_height / 2.0;

    IconLayout {
        icon_size: size,
        gap,
        group_width,
        group_height,
        icon_x,
        icon_y,
        label_x,
        label_y,
    }
}

/// Render a single icon glyph at (x, y) with given size and color.
///
/// The glyph SVG asset is stripped of its outer `<svg>` wrapper and
/// embedded in a `<g>` with translate + scale transform.
///
/// `color` is interpolated as-is: resolved style values are already
/// XML-escaped at the resolve choke point (`resolve::attr_string`).
pub fn render_icon(def: &IconDef, x: f64, y: f64, size: f64, color: &str) -> String {
    let scale = size / GLYPH_VIEWBOX;
    let inner = svg_inner(def.svg_content);
    format!(
        r#"<g transform="translate({x:.2},{y:.2}) scale({scale:.4})" color="{color}" fill="none">{inner}</g>"#
    )
}

/// Check whether an icon is compatible with the given shape.
pub fn is_compatible(def: &IconDef, shape: plotgram_model::NodeShape) -> bool {
    !def.incompatible_shapes.contains(&shape.as_str())
}

/// Extra width needed for icon inside a node (icon_size + gap).
pub fn extra_node_width(font_size: f64) -> f64 {
    icon_size(font_size) + icon_gap()
}

/// Extract inner content from a full SVG asset (strip `<svg ...>` and `</svg>`).
fn svg_inner(asset: &str) -> &str {
    let start = asset.find('>').map(|idx| idx + 1).unwrap_or(0);
    let end = asset.rfind("</svg>").unwrap_or(asset.len());
    asset.get(start..end).unwrap_or("").trim()
}
