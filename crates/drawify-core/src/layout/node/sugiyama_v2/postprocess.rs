use crate::types::DiagramType;
use crate::ast::{Diagram, Entity};
use crate::layout::{GroupLayout, LayoutResult, NodeLayout};
use crate::kinds::er::semantics::entity_node_size;
use petgraph::graph::NodeIndex;
use std::collections::HashMap;

use super::preset::SugiyamaPreset;
use crate::layout::node::common::node_sizing::NodeSizing;

pub(super) fn compute_layer_heights(
    layers: &[Vec<NodeIndex>],
    sizes: &HashMap<NodeIndex, (f64, f64)>,
    preset: &SugiyamaPreset,
) -> Vec<f64> {
    let (default_w, default_h) = preset.default_node_size();
    layers
        .iter()
        .map(|layer| {
            layer
                .iter()
                .map(|node| sizes.get(node).copied().unwrap_or((default_w, default_h)).1)
                .fold(default_h, f64::max)
        })
        .collect()
}

pub(super) fn normalize_layout_to_padding(
    nodes: &mut HashMap<String, NodeLayout>,
    padding: f64,
) {
    let min_x = nodes.values().map(|node| node.x).fold(f64::INFINITY, f64::min);
    let min_y = nodes.values().map(|node| node.y).fold(f64::INFINITY, f64::min);
    if !min_x.is_finite() || !min_y.is_finite() {
        return;
    }
    let dx = if min_x < padding { padding - min_x } else { 0.0 };
    let dy = if min_y < padding { padding - min_y } else { 0.0 };
    for node in nodes.values_mut() {
        node.x += dx;
        node.y += dy;
    }
}

pub(super) fn normalize_layout_result_to_padding(result: &mut LayoutResult, padding: f64) {
    normalize_layout_to_padding(&mut result.nodes, padding);
    let min_x = result.nodes.values().map(|node| node.x).fold(f64::INFINITY, f64::min);
    let min_y = result.nodes.values().map(|node| node.y).fold(f64::INFINITY, f64::min);
    if !min_x.is_finite() || !min_y.is_finite() {
        return;
    }
    let dx = if min_x < padding {
        padding - min_x
    } else {
        0.0
    };
    let dy = if min_y < padding {
        padding - min_y
    } else {
        0.0
    };
    if dx <= 0.0 && dy <= 0.0 {
        return;
    }
    for group in result.groups.values_mut() {
        group.x += dx;
        group.y += dy;
    }
    result.total_width += dx;
    result.total_height += dy;
}

pub(super) fn bounds_from_layout(
    nodes: &HashMap<String, NodeLayout>,
    groups: &HashMap<String, GroupLayout>,
    padding: f64,
) -> (f64, f64) {
    let node_max_x = nodes.values().map(|node| node.x + node.width).fold(0.0_f64, f64::max);
    let node_max_y = nodes.values().map(|node| node.y + node.height).fold(0.0_f64, f64::max);
    let group_max_x = groups.values().map(|group| group.x + group.width).fold(0.0_f64, f64::max);
    let group_max_y = groups.values().map(|group| group.y + group.height).fold(0.0_f64, f64::max);
    (
        node_max_x.max(group_max_x) + padding,
        node_max_y.max(group_max_y) + padding,
    )
}

fn state_entity_type(_diagram: &Diagram, entity: &Entity) -> String {
    entity
        .attributes
        .standard
        .get("type")
        .and_then(|value| value.as_str())
        .unwrap_or("state")
        .to_string()
}

fn state_fallback_node_size(diagram: &Diagram, entity: &Entity) -> (f64, f64) {
    match state_entity_type(diagram, entity).as_str() {
        "initial" => (28.0, 28.0),
        "final" => (36.0, 36.0),
        "choice" => {
            let chars = entity.label.chars().count() as f64;
            let side = (chars * 13.0 + 48.0).clamp(72.0, 120.0);
            (side, side * 0.72)
        }
        _ => {
            let chars = entity.label.chars().count() as f64;
            ((chars * 14.0 + 36.0).clamp(80.0, 200.0), 44.0)
        }
    }
}

pub(super) fn sized_node_for(
    diagram: &Diagram,
    entity: &Entity,
    preset: &SugiyamaPreset,
) -> (f64, f64) {
    let (default_w, default_h) = match preset.node_sizing {
        NodeSizing::Er => entity_node_size(entity),
        NodeSizing::State => state_fallback_node_size(diagram, entity),
        NodeSizing::InferFromDiagram => {
            if diagram.diagram_type == DiagramType::Er {
                entity_node_size(entity)
            } else if diagram.diagram_type == DiagramType::State {
                state_fallback_node_size(diagram, entity)
            } else {
                preset.default_node_size()
            }
        }
        NodeSizing::Standard => preset.default_node_size(),
    };

    let (width, height) = crate::layout::styled_node_size(entity, default_w, default_h);
    (width, height)
}
