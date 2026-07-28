use crate::ast::{Diagram, Entity};
use crate::layout::{GroupLayout, LayoutResult, NodeLayout};
use crate::kinds::er::semantics::entity_node_size;
use petgraph::graph::NodeIndex;
use std::collections::HashMap;

use super::preset::SugiyamaPreset;
use crate::layout::kernel::common::node_sizing::NodeSizing;

pub(in crate::layout) fn compute_layer_heights(
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

pub(in crate::layout) fn normalize_layout_to_padding(
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
    // 从原始节点 min 一次性算 dx/dy，统一应用到 nodes + groups + total_size。
    // 修复 P15：旧实现先调 normalize_layout_to_padding 移动节点，再用移动后的 min 重算
    // dx/dy → 恒 ≤ 0 → early return → groups 和 total_size 永不调整。
    let min_x = result.nodes.values().map(|n| n.x).fold(f64::INFINITY, f64::min);
    let min_y = result.nodes.values().map(|n| n.y).fold(f64::INFINITY, f64::min);
    if !min_x.is_finite() || !min_y.is_finite() {
        return;
    }
    let dx = if min_x < padding { padding - min_x } else { 0.0 };
    let dy = if min_y < padding { padding - min_y } else { 0.0 };
    if dx == 0.0 && dy == 0.0 {
        return;
    }
    for node in result.nodes.values_mut() {
        node.x += dx;
        node.y += dy;
    }
    if !result.groups.is_empty() {
        crate::layout::group::write_counter::record_group_write_at(
            "layered_normalize_translate_groups",
        );
        for group in result.groups.values_mut() {
            group.x += dx;
            group.y += dy;
        }
    }
    result.total_width += dx;
    result.total_height += dy;
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
    use crate::layout::kernel::common::node_sizing::{
        estimate_standard_node_width, DEFAULT_NODE_HEIGHT,
    };

    let (default_w, default_h) = match preset.node_sizing {
        NodeSizing::Er => entity_node_size(entity),
        NodeSizing::State => state_fallback_node_size(diagram, entity),
        // recipes 须显式设 Er/State/Standard；Infer 回退标准估宽（禁 DiagramType）。
        NodeSizing::InferFromDiagram => (
            estimate_standard_node_width(entity.label.as_str()),
            DEFAULT_NODE_HEIGHT,
        ),
        // Phase B：flowchart 按标签估宽，不再固定 160。
        NodeSizing::Standard => (
            estimate_standard_node_width(entity.label.as_str()),
            DEFAULT_NODE_HEIGHT,
        ),
    };

    let (width, height) = crate::layout::styled_node_size(entity, default_w, default_h);
    (width, height)
}
