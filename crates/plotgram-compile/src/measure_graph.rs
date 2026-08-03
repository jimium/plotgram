//! Build [`NodeSizes`] for every leaf before layout.

use plotgram_content::{measure, parse as parse_content, Align, MeasureParams};
use plotgram_model::geometry::Size;
use plotgram_model::graph::Graph;
use plotgram_model::sizes::NodeSizes;

use crate::error::BuildError;
use crate::options::BuildOptions;

/// Default box when a node has only a short label (heuristic stub).
const DEFAULT_W: f64 = 72.0;
const DEFAULT_H: f64 = 36.0;
const PAD_X: f64 = 16.0;
const PAD_Y: f64 = 12.0;

fn default_measure_params() -> MeasureParams {
    MeasureParams {
        font_size: 14.0,
        line_height: 1.35,
        font_family: "sans-serif".into(),
        paragraph_gap: 6.0,
        list_indent: 16.0,
        rule_thickness: 1.0,
        rule_gap: 8.0,
        max_width: None,
        align: Align::Left,
        max_lines: None,
    }
}

/// Measure preferred sizes for all nodes in `graph`.
///
/// Skeleton: label-width heuristic; `content:` MD via `plotgram-content` when present
/// in attrs as `content` string. Full theme → MeasureParams mapping is TODO.
pub fn measure_node_sizes(
    graph: &Graph,
    options: &BuildOptions,
) -> Result<NodeSizes, BuildError> {
    let params = default_measure_params();
    let mut sizes = NodeSizes::new();

    for id in graph.all_node_ids() {
        let node = graph.find_node(&id).ok_or_else(|| {
            BuildError::Measure(format!("node `{id}` listed but not found"))
        })?;

        let size = if !options.labels_only {
            if let Some(raw) = node.attrs.get("content").and_then(|v| v.as_str()) {
                let layout = measure(&parse_content(raw), &params);
                Size::new(layout.width + PAD_X, layout.height + PAD_Y)
            } else {
                size_from_label(node.label.as_deref(), &params)
            }
        } else {
            size_from_label(node.label.as_deref(), &params)
        };

        sizes.insert(id, size);
    }

    sizes
        .require_all(graph)
        .map_err(|e| BuildError::Measure(e.to_string()))?;
    Ok(sizes)
}

fn size_from_label(label: Option<&str>, params: &MeasureParams) -> Size {
    match label {
        None | Some("") => Size::new(DEFAULT_W, DEFAULT_H),
        Some(text) => {
            let layout = measure(&parse_content(text), params);
            Size::new(
                (layout.width + PAD_X).max(40.0),
                (layout.height + PAD_Y).max(24.0),
            )
        }
    }
}
