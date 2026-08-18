//! PartitionGrid swimlane / matrix chrome.
//!
//! Paint is a projection of Metric band intervals already on
//! [`HierarchicalObs`] (partition-grid.md PG-2/PG-4). Render never invents
//! cells from `Node.partition_cell`; no bands → no paint.

use tautcore_model::diagnostics::{HierarchicalObs, PartitionBandObs, PartitionRowBandObs};
use tautcore_model::graph::Graph;
use tautcore_model::partition::PartitionGrid;
use tautcore_model::result::LayoutResult;

use crate::theme::schema::StyleValue;
use crate::theme::CompiledTheme;
use crate::util::escape_xml;
use crate::SvgBuilder;

/// Palette roles cycled by declaration index. Pale fills from the theme
/// token board — not a diagram-type branch (ADR-001).
const LANE_ROLES: [&str; 6] = ["blue", "orange", "green", "purple", "cyan", "gray"];

const HEADER_Y: f64 = 12.0;
const HEADER_X: f64 = 12.0;

/// Paint column / row bands behind groups and nodes.
pub fn render_partition_bands(
    svg: &mut SvgBuilder,
    graph: &Graph,
    layout: &LayoutResult,
    theme: &CompiledTheme,
) {
    let Some(obs) = layout.diagnostics.hierarchical.as_ref() else {
        return;
    };
    if obs.partition_bands.is_empty() && obs.partition_row_bands.is_empty() {
        return;
    }

    let grid = graph.partition.as_ref();
    let w = layout.canvas_width;
    let h = layout.canvas_height;
    let stroke = &theme.defaults.group.stroke;
    let text_fill = &theme.defaults.group.text_fill;
    let font_family = &theme.defaults.typography.font_family;
    let font_size = theme.defaults.typography.small_size.min(12.0);

    svg.add_element(r#"<g class="pg-partition">"#.to_string());

    if !obs.partition_bands.is_empty() && !obs.partition_row_bands.is_empty() {
        paint_cells(
            svg,
            &obs.partition_bands,
            &obs.partition_row_bands,
            theme,
            stroke,
        );
    } else if !obs.partition_bands.is_empty() {
        for (i, band) in obs.partition_bands.iter().enumerate() {
            let width = (band.end - band.start).max(0.0);
            if width <= 0.0 {
                continue;
            }
            paint_rect(
                svg,
                band.start,
                0.0,
                width,
                h,
                &lane_fill(theme, i),
                stroke,
                "pg-partition-col",
                &band.column,
            );
        }
    } else {
        for (i, band) in obs.partition_row_bands.iter().enumerate() {
            let height = (band.end - band.start).max(0.0);
            if height <= 0.0 {
                continue;
            }
            paint_rect(
                svg,
                0.0,
                band.start,
                w,
                height,
                &lane_fill(theme, i),
                stroke,
                "pg-partition-row",
                &band.row,
            );
        }
    }

    paint_column_titles(svg, obs, grid, text_fill, font_family, font_size);
    paint_row_titles(svg, obs, grid, text_fill, font_family, font_size);

    svg.add_element("</g>".to_string());
}

fn paint_cells(
    svg: &mut SvgBuilder,
    cols: &[PartitionBandObs],
    rows: &[PartitionRowBandObs],
    theme: &CompiledTheme,
    stroke: &str,
) {
    for (ci, col) in cols.iter().enumerate() {
        let width = (col.end - col.start).max(0.0);
        if width <= 0.0 {
            continue;
        }
        let fill = lane_fill(theme, ci);
        for row in rows {
            let height = (row.end - row.start).max(0.0);
            if height <= 0.0 {
                continue;
            }
            let id = format!("{}:{}", col.column, row.row);
            paint_rect(
                svg,
                col.start,
                row.start,
                width,
                height,
                &fill,
                stroke,
                "pg-partition-cell",
                &id,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_rect(
    svg: &mut SvgBuilder,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    fill: &str,
    stroke: &str,
    class: &str,
    axis: &str,
) {
    svg.add_element(format!(
        r#"<rect class="{class}" data-axis="{axis}" x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{h:.2}" fill="{fill}" fill-opacity="0.72" stroke="{stroke}" stroke-width="0.75" stroke-opacity="0.35"/>"#,
        axis = escape_xml(axis),
    ));
}

fn paint_column_titles(
    svg: &mut SvgBuilder,
    obs: &HierarchicalObs,
    grid: Option<&PartitionGrid>,
    fill: &str,
    font_family: &str,
    font_size: f64,
) {
    for band in &obs.partition_bands {
        let text = axis_label(grid, &band.column);
        if text.is_empty() {
            continue;
        }
        let cx = (band.start + band.end) / 2.0;
        svg.add_element(format!(
            r#"<text class="pg-partition-title" x="{cx:.1}" y="{HEADER_Y:.1}" text-anchor="middle" dominant-baseline="central" fill="{fill}" font-size="{font_size}" font-family="{font_family}" font-weight="500">{text}</text>"#,
            text = escape_xml(&text),
        ));
    }
}

fn paint_row_titles(
    svg: &mut SvgBuilder,
    obs: &HierarchicalObs,
    grid: Option<&PartitionGrid>,
    fill: &str,
    font_family: &str,
    font_size: f64,
) {
    for band in &obs.partition_row_bands {
        let text = axis_label(grid, &band.row);
        if text.is_empty() {
            continue;
        }
        let cy = (band.start + band.end) / 2.0;
        svg.add_element(format!(
            r#"<text class="pg-partition-title" transform="rotate(-90 {HEADER_X:.1} {cy:.1})" x="{HEADER_X:.1}" y="{cy:.1}" text-anchor="middle" dominant-baseline="central" fill="{fill}" font-size="{font_size}" font-family="{font_family}" font-weight="500">{text}</text>"#,
            text = escape_xml(&text),
        ));
    }
}

fn axis_label(grid: Option<&PartitionGrid>, id: &str) -> String {
    let Some(grid) = grid else {
        return id.to_string();
    };
    grid.columns
        .iter()
        .chain(grid.rows.iter())
        .find(|a| a.id == id)
        .and_then(|a| a.label.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| id.to_string())
}

fn lane_fill(theme: &CompiledTheme, index: usize) -> String {
    let role = LANE_ROLES[index % LANE_ROLES.len()];
    palette_fill(theme, role).unwrap_or_else(|| theme.defaults.group.fill.clone())
}

fn palette_fill(theme: &CompiledTheme, role: &str) -> Option<String> {
    let value = theme.tokens.palette.get(role)?.get("fill")?;
    match value {
        StyleValue::String(s) if s.starts_with('{') => None,
        StyleValue::String(s) => Some(s.clone()),
        other => Some(other.to_svg_string()),
    }
}
