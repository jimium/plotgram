use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::Direction;
use std::collections::{HashMap, HashSet};

use super::graph::{LayerNode, LayerNodeKind};
use super::order;
use super::postprocess;
use super::preset::SugiyamaPreset;

pub(super) fn assign_coordinates_brandes_koepf(
    dag: &DiGraph<String, ()>,
    layered_graph: &DiGraph<LayerNode, ()>,
    layers: &[Vec<NodeIndex>],
    sizes: &HashMap<NodeIndex, (f64, f64)>,
    horizontal: bool,
    preset: &SugiyamaPreset,
    layer_gaps: &[f64],
) -> HashMap<String, crate::layout::NodeLayout> {
    let centers = assign_layer_centers_brandes_koepf(layered_graph, layers, sizes, preset);

    let mut nodes = HashMap::new();
    let (default_w, default_h) = preset.default_node_size();
    let layer_heights = postprocess::compute_layer_heights(layers, sizes, preset);
    let mut layer_offsets = vec![preset.padding; layers.len()];
    for layer_index in 1..layers.len() {
        // 逐层密度感知：优先使用 per-layer gap，回退到 preset.layer_gap
        let gap = layer_gaps
            .get(layer_index - 1)
            .copied()
            .unwrap_or(preset.layer_gap);
        layer_offsets[layer_index] =
            layer_offsets[layer_index - 1] + layer_heights[layer_index - 1] + gap;
    }

    for (layer_index, layer) in layers.iter().enumerate() {
        for node in layer {
            let (width, height) = sizes.get(node).copied().unwrap_or((default_w, default_h));
            let x = centers[node];
            let center_y = layer_offsets[layer_index] + layer_heights[layer_index] / 2.0;
            let LayerNodeKind::Real(original_node) = layered_graph[*node].kind.clone() else {
                continue;
            };
            let layout = if horizontal {
                crate::layout::NodeLayout {
                    x: center_y - height / 2.0,
                    y: x - width / 2.0,
                    width: height,
                    height: width,
                    ..Default::default()
                }
            } else {
                crate::layout::NodeLayout {
                    x: x - width / 2.0,
                    y: center_y - height / 2.0,
                    width,
                    height,
                    ..Default::default()
                }
            };

            nodes.insert(dag[original_node].clone(), layout);
        }
    }

    resolve_real_node_overlaps(dag, layered_graph, layers, &mut nodes, horizontal, preset);
    postprocess::normalize_layout_to_padding(&mut nodes, preset.padding);
    nodes
}

fn resolve_real_node_overlaps(
    dag: &DiGraph<String, ()>,
    layered_graph: &DiGraph<LayerNode, ()>,
    layers: &[Vec<NodeIndex>],
    nodes: &mut HashMap<String, crate::layout::NodeLayout>,
    horizontal: bool,
    preset: &SugiyamaPreset,
) {
    for layer in layers {
        let ordered = layer
            .iter()
            .filter_map(|node| match layered_graph[*node].kind {
                LayerNodeKind::Real(original) => {
                    let id = dag[original].clone();
                    nodes.get(&id).map(|layout| (*node, id, axis_center(layout, horizontal), axis_size(layout, horizontal)))
                }
                LayerNodeKind::Dummy { .. } => None,
            })
            .collect::<Vec<_>>();
        if ordered.len() <= 1 {
            continue;
        }

        let preferred = ordered.iter().map(|(_, _, center, _)| *center).collect::<Vec<_>>();
        let sizes = ordered.iter().map(|(_, _, _, size)| *size).collect::<Vec<_>>();
        let mut adjusted = preferred.clone();

        for index in 1..adjusted.len() {
            let min_center = adjusted[index - 1]
                + sizes[index - 1] / 2.0
                + sizes[index] / 2.0
                + preset.node_gap;
            if adjusted[index] < min_center {
                adjusted[index] = min_center;
            }
        }

        for index in (0..adjusted.len() - 1).rev() {
            let max_center = adjusted[index + 1]
                - sizes[index + 1] / 2.0
                - sizes[index] / 2.0
                - preset.node_gap;
            if adjusted[index] > max_center {
                adjusted[index] = max_center;
            }
        }

        let average_preferred = preferred.iter().sum::<f64>() / preferred.len() as f64;
        let average_adjusted = adjusted.iter().sum::<f64>() / adjusted.len() as f64;
        let min_shift = adjusted
            .iter()
            .zip(sizes.iter())
            .map(|(center, size)| preset.padding + size / 2.0 - center)
            .fold(f64::NEG_INFINITY, f64::max);
        let shift = (average_preferred - average_adjusted).max(min_shift);

        for (((_, id, _, _), center), size) in ordered.iter().zip(adjusted.iter_mut()).zip(sizes.iter()) {
            *center += shift;
            if let Some(layout) = nodes.get_mut(id) {
                set_axis_center(layout, horizontal, *center, *size);
            }
        }
    }
}

fn axis_center(layout: &crate::layout::NodeLayout, horizontal: bool) -> f64 {
    if horizontal {
        layout.y + layout.height / 2.0
    } else {
        layout.x + layout.width / 2.0
    }
}

fn axis_size(layout: &crate::layout::NodeLayout, horizontal: bool) -> f64 {
    if horizontal {
        layout.height
    } else {
        layout.width
    }
}

fn set_axis_center(layout: &mut crate::layout::NodeLayout, horizontal: bool, center: f64, size: f64) {
    if horizontal {
        layout.y = center - size / 2.0;
    } else {
        layout.x = center - size / 2.0;
    }
}

pub(super) fn assign_layer_centers_brandes_koepf(
    dag: &DiGraph<LayerNode, ()>,
    layers: &[Vec<NodeIndex>],
    sizes: &HashMap<NodeIndex, (f64, f64)>,
    preset: &SugiyamaPreset,
) -> HashMap<NodeIndex, f64> {
    let down_left = run_coordinate_pass_bk(dag, layers, sizes, true, true, preset);
    let down_right = run_coordinate_pass_bk(dag, layers, sizes, true, false, preset);
    let up_left = run_coordinate_pass_bk(dag, layers, sizes, false, true, preset);
    let up_right = run_coordinate_pass_bk(dag, layers, sizes, false, false, preset);

    // 标准 Brandes-Kopf：4 趟使用相同层序，Sugiyama 交叉数相同；
    // 按"布局宽度最小（最紧凑）"选取最优趟，而非取平均。
    // 取平均会模糊各趟在对齐方向上的优势，导致坐标不够紧凑。
    // 同宽时按 down_left > down_right > up_left > up_right 的固定优先级选取，
    // 保证确定性（不依赖 HashMap 迭代顺序）。
    let candidates = [
        (&down_left, 0usize),
        (&down_right, 1),
        (&up_left, 2),
        (&up_right, 3),
    ];
    let best = candidates
        .iter()
        .min_by(|(a, idx_a), (b, idx_b)| {
            pass_width(a)
                .partial_cmp(&pass_width(b))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(idx_a.cmp(idx_b))
        })
        .map(|(coords, _)| *coords)
        .expect("at least one BK pass");

    best.clone()
}

/// 计算一趟坐标分配的布局宽度（最左中心到最右中心）。
///
/// 用于 4 趟 BK 比较紧凑度：宽度越小越紧凑。
fn pass_width(coords: &HashMap<NodeIndex, f64>) -> f64 {
    let min = coords.values().copied().fold(f64::INFINITY, f64::min);
    let max = coords.values().copied().fold(f64::NEG_INFINITY, f64::max);
    (max - min).max(0.0)
}

fn run_coordinate_pass_bk(
    dag: &DiGraph<LayerNode, ()>,
    layers: &[Vec<NodeIndex>],
    sizes: &HashMap<NodeIndex, (f64, f64)>,
    downward: bool,
    left_to_right: bool,
    preset: &SugiyamaPreset,
) -> HashMap<NodeIndex, f64> {
    let oriented_layers = orient_layers(layers, left_to_right);
    let conflicts = detect_alignment_conflicts(dag, &oriented_layers);
    let blocks = vertical_alignment_blocks(dag, &oriented_layers, &conflicts, downward);
    let mut coords = horizontal_compaction(&oriented_layers, sizes, &blocks, preset);
    if !left_to_right {
        coords = mirror_coordinates(&coords);
    }
    normalize_center_coordinates(&mut coords, sizes, preset);
    coords
}

fn orient_layers(layers: &[Vec<NodeIndex>], left_to_right: bool) -> Vec<Vec<NodeIndex>> {
    if left_to_right {
        return layers.to_vec();
    }

    layers
        .iter()
        .map(|layer| {
            let mut reversed = layer.clone();
            reversed.reverse();
            reversed
        })
        .collect()
}

fn detect_alignment_conflicts(
    dag: &DiGraph<LayerNode, ()>,
    layers: &[Vec<NodeIndex>],
) -> HashSet<(NodeIndex, NodeIndex)> {
    let mut conflicts = HashSet::new();

    for layer_index in 0..layers.len().saturating_sub(1) {
        let upper = &layers[layer_index];
        let lower = &layers[layer_index + 1];
        let lower_pos = order::index_map(lower);
        let mut edges = Vec::new();

        for (upper_idx, upper_node) in upper.iter().enumerate() {
            for succ in dag.neighbors_directed(*upper_node, Direction::Outgoing) {
                if let Some(&lower_idx) = lower_pos.get(&succ) {
                    edges.push((
                        *upper_node,
                        succ,
                        upper_idx,
                        lower_idx,
                        is_inner_segment(dag, *upper_node, succ),
                    ));
                }
            }
        }

        edges.sort_by_key(|(_, _, upper_idx, lower_idx, _)| (*upper_idx, *lower_idx));
        for left in 0..edges.len() {
            for right in (left + 1)..edges.len() {
                let (from_a, to_a, upper_a, lower_a, inner_a) = edges[left];
                let (from_b, to_b, upper_b, lower_b, inner_b) = edges[right];
                let crossing = (upper_a < upper_b && lower_a > lower_b)
                    || (upper_a > upper_b && lower_a < lower_b);
                if !crossing || inner_a == inner_b {
                    continue;
                }
                let edge = if inner_a { (from_b, to_b) } else { (from_a, to_a) };
                conflicts.insert(edge);
            }
        }
    }

    conflicts
}

fn is_inner_segment(dag: &DiGraph<LayerNode, ()>, from: NodeIndex, to: NodeIndex) -> bool {
    matches!(dag[from].kind, LayerNodeKind::Dummy { .. })
        && matches!(dag[to].kind, LayerNodeKind::Dummy { .. })
}

pub(super) fn vertical_alignment_blocks(
    dag: &DiGraph<LayerNode, ()>,
    layers: &[Vec<NodeIndex>],
    conflicts: &HashSet<(NodeIndex, NodeIndex)>,
    downward: bool,
) -> HashMap<NodeIndex, NodeIndex> {
    let mut parent = dag
        .node_indices()
        .map(|node| (node, node))
        .collect::<HashMap<_, _>>();
    let scan_layers = if downward {
        (1..layers.len()).collect::<Vec<_>>()
    } else {
        (0..layers.len().saturating_sub(1)).rev().collect::<Vec<_>>()
    };

    for layer_index in scan_layers {
        let neighbor_layer = if downward { layer_index - 1 } else { layer_index + 1 };
        let neighbor_pos = order::index_map(&layers[neighbor_layer]);
        let mut last_aligned_pos = None;

        for node in &layers[layer_index] {
            let mut neighbors = if downward {
                dag.neighbors_directed(*node, Direction::Incoming)
                    .filter(|pred| neighbor_pos.contains_key(pred))
                    .collect::<Vec<_>>()
            } else {
                dag.neighbors_directed(*node, Direction::Outgoing)
                    .filter(|succ| neighbor_pos.contains_key(succ))
                    .collect::<Vec<_>>()
            };
            if neighbors.is_empty() {
                continue;
            }

            neighbors.sort_by_key(|neighbor| neighbor_pos[neighbor]);
            let candidates = median_candidates(&neighbors);
            for neighbor in candidates {
                let edge = if downward {
                    (neighbor, *node)
                } else {
                    (*node, neighbor)
                };
                let neighbor_order = neighbor_pos[&neighbor];
                if conflicts.contains(&edge) {
                    continue;
                }
                if last_aligned_pos.is_some_and(|last| neighbor_order < last) {
                    continue;
                }
                if find_block_root(&parent, *node) == find_block_root(&parent, neighbor) {
                    continue;
                }

                union_blocks(&mut parent, *node, neighbor);
                last_aligned_pos = Some(neighbor_order);
                break;
            }
        }
    }

    dag.node_indices()
        .map(|node| (node, find_block_root(&parent, node)))
        .collect()
}

fn median_candidates(neighbors: &[NodeIndex]) -> Vec<NodeIndex> {
    if neighbors.is_empty() {
        return Vec::new();
    }
    if neighbors.len() % 2 == 1 {
        return vec![neighbors[neighbors.len() / 2]];
    }

    let left = neighbors[neighbors.len() / 2 - 1];
    let right = neighbors[neighbors.len() / 2];
    vec![left, right]
}

fn find_block_root(parent: &HashMap<NodeIndex, NodeIndex>, node: NodeIndex) -> NodeIndex {
    let mut current = node;
    while parent[&current] != current {
        current = parent[&current];
    }
    current
}

fn union_blocks(parent: &mut HashMap<NodeIndex, NodeIndex>, left: NodeIndex, right: NodeIndex) {
    let left_root = find_block_root(parent, left);
    let right_root = find_block_root(parent, right);
    if left_root == right_root {
        return;
    }

    let (root, child) = if left_root.index() <= right_root.index() {
        (left_root, right_root)
    } else {
        (right_root, left_root)
    };
    parent.insert(child, root);
}

pub(super) fn horizontal_compaction(
    layers: &[Vec<NodeIndex>],
    sizes: &HashMap<NodeIndex, (f64, f64)>,
    blocks: &HashMap<NodeIndex, NodeIndex>,
    preset: &SugiyamaPreset,
) -> HashMap<NodeIndex, f64> {
    let (default_w, default_h) = preset.default_node_size();
    let initial = initial_x_positions(layers, sizes, true, preset);
    let block_order = ordered_block_roots(layers, blocks);
    let mut block_pos = block_order
        .iter()
        .map(|root| (*root, initial[root]))
        .collect::<HashMap<_, _>>();
    let mut constraints = HashMap::<NodeIndex, Vec<(NodeIndex, f64)>>::new();
    let mut reverse_constraints = HashMap::<NodeIndex, Vec<(NodeIndex, f64)>>::new();

    for layer in layers {
        for window in layer.windows(2) {
            let left = window[0];
            let right = window[1];
            let left_root = blocks[&left];
            let right_root = blocks[&right];
            if left_root == right_root {
                continue;
            }

            let separation = sizes.get(&left).copied().unwrap_or((default_w, default_h)).0
                / 2.0
                + sizes.get(&right).copied().unwrap_or((default_w, default_h)).0 / 2.0
                + preset.node_gap;
            constraints
                .entry(left_root)
                .or_default()
                .push((right_root, separation));
            reverse_constraints
                .entry(right_root)
                .or_default()
                .push((left_root, separation));
        }
    }

    for root in &block_order {
        if let Some(edges) = constraints.get(root) {
            for (target, separation) in edges {
                let candidate = block_pos[root] + separation;
                let entry = block_pos.entry(*target).or_insert(candidate);
                if *entry < candidate {
                    *entry = candidate;
                }
            }
        }
    }

    for root in block_order.iter().rev() {
        let lower_bound = reverse_constraints
            .get(root)
            .into_iter()
            .flatten()
            .map(|(prev, separation)| block_pos[prev] + separation)
            .fold(f64::NEG_INFINITY, f64::max);
        let upper_bound = constraints
            .get(root)
            .into_iter()
            .flatten()
            .map(|(next, separation)| block_pos[next] - separation)
            .fold(f64::INFINITY, f64::min);
        let anchor = initial[root];
        let candidate = anchor.max(lower_bound);
        if candidate.is_finite() && upper_bound.is_finite() {
            block_pos.insert(*root, candidate.min(upper_bound));
        } else if candidate.is_finite() {
            block_pos.insert(*root, candidate);
        }
    }

    let mut centers = HashMap::new();
    for (node, block) in blocks {
        centers.insert(*node, block_pos[block]);
    }
    centers
}

fn ordered_block_roots(
    layers: &[Vec<NodeIndex>],
    blocks: &HashMap<NodeIndex, NodeIndex>,
) -> Vec<NodeIndex> {
    let mut order = Vec::new();
    let mut seen = HashSet::new();
    for layer in layers {
        for node in layer {
            let root = blocks[node];
            if seen.insert(root) {
                order.push(root);
            }
        }
    }
    order
}

fn mirror_coordinates(coords: &HashMap<NodeIndex, f64>) -> HashMap<NodeIndex, f64> {
    let min = coords.values().copied().fold(f64::INFINITY, f64::min);
    let max = coords.values().copied().fold(f64::NEG_INFINITY, f64::max);
    coords
        .iter()
        .map(|(node, value)| (*node, min + max - *value))
        .collect()
}

fn normalize_center_coordinates(
    coords: &mut HashMap<NodeIndex, f64>,
    sizes: &HashMap<NodeIndex, (f64, f64)>,
    preset: &SugiyamaPreset,
) {
    let (default_w, _) = preset.default_node_size();
    let min_left = coords
        .iter()
        .map(|(node, center)| {
            center - sizes.get(node).copied().unwrap_or((default_w, 0.0)).0 / 2.0
        })
        .fold(f64::INFINITY, f64::min);
    if !min_left.is_finite() {
        return;
    }
    let shift = if min_left < preset.padding {
        preset.padding - min_left
    } else {
        0.0
    };
    for center in coords.values_mut() {
        *center += shift;
    }
}

fn initial_x_positions(
    layers: &[Vec<NodeIndex>],
    sizes: &HashMap<NodeIndex, (f64, f64)>,
    left_to_right: bool,
    preset: &SugiyamaPreset,
) -> HashMap<NodeIndex, f64> {
    let (default_w, _) = preset.default_node_size();
    let max_span = layers
        .iter()
        .map(|layer| {
            layer
                .iter()
                .map(|node| sizes.get(node).copied().unwrap_or((default_w, 0.0)).0)
                .sum::<f64>()
                + layer.len().saturating_sub(1) as f64 * preset.node_gap
        })
        .fold(0.0_f64, f64::max);

    let mut coords = HashMap::new();
    for layer in layers {
        let widths = layer
            .iter()
            .map(|node| sizes.get(node).copied().unwrap_or((default_w, 0.0)).0)
            .collect::<Vec<_>>();
        let span = widths.iter().sum::<f64>() + layer.len().saturating_sub(1) as f64 * preset.node_gap;
        let mut cursor = preset.padding + (max_span - span) / 2.0;
        let iter = if left_to_right {
            layer.iter().copied().zip(widths.iter().copied()).collect::<Vec<_>>()
        } else {
            layer.iter().copied().zip(widths.iter().copied()).rev().collect::<Vec<_>>()
        };
        for (node, width) in iter {
            coords.insert(node, cursor + width / 2.0);
            cursor += width + preset.node_gap;
        }
    }
    coords
}
