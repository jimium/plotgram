use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::Direction;
use std::cmp::Ordering;
use std::collections::HashMap;

use super::graph::{LayerNode, LayerNodeKind};
use crate::layout::node::common::crossings::count_crossings_from_edges;

/// Group 偏置触发阈值：当两节点 median 差小于此值时，视为"位置接近"，
/// 启用 group 偏置 tiebreaker（优先同 group 节点相邻）。
///
/// 取 1.0：median 为整数位置索引，差 < 1.0 即同位置；放宽到 ≤ 1.0 可覆盖
/// 相邻位置，让 group 偏置在"位置接近"时生效，而非仅完全相等时。
const GROUP_BIAS_EPSILON: f64 = 1.0;

pub(super) fn order_layers_weighted_median(
    dag: &DiGraph<LayerNode, ()>,
    mut layers: Vec<Vec<NodeIndex>>,
    ordering_sweeps: usize,
    long_edge_barycenter_weight: f64,
    node_group: &HashMap<NodeIndex, Option<String>>,
) -> Vec<Vec<NodeIndex>> {
    for _ in 0..ordering_sweeps {
        for layer_index in 1..layers.len() {
            let upper_pos = index_map(&layers[layer_index - 1]);
            let layer_snapshot = layers[layer_index].clone();
            layers[layer_index].sort_by(|left, right| {
                compare_nodes_for_layer(
                    dag,
                    layer_snapshot.as_slice(),
                    *left,
                    *right,
                    &upper_pos,
                    Direction::Incoming,
                    long_edge_barycenter_weight,
                    node_group,
                )
            });
            transpose_adjacent(layer_index, &mut layers, dag, long_edge_barycenter_weight);
        }

        for layer_index in (0..layers.len().saturating_sub(1)).rev() {
            let lower_pos = index_map(&layers[layer_index + 1]);
            let layer_snapshot = layers[layer_index].clone();
            layers[layer_index].sort_by(|left, right| {
                compare_nodes_for_layer(
                    dag,
                    layer_snapshot.as_slice(),
                    *left,
                    *right,
                    &lower_pos,
                    Direction::Outgoing,
                    long_edge_barycenter_weight,
                    node_group,
                )
            });
            transpose_adjacent(layer_index, &mut layers, dag, long_edge_barycenter_weight);
        }
    }

    layers
}

pub(super) fn index_map(layer: &[NodeIndex]) -> HashMap<NodeIndex, usize> {
    layer.iter().enumerate().map(|(idx, node)| (*node, idx)).collect()
}

struct OrderingStats {
    median: f64,
    barycenter: f64,
    spread: f64,
    degree: usize,
    is_dummy: bool,
}

fn compare_nodes_for_layer(
    dag: &DiGraph<LayerNode, ()>,
    layer: &[NodeIndex],
    left: NodeIndex,
    right: NodeIndex,
    neighbor_pos: &HashMap<NodeIndex, usize>,
    direction: Direction,
    long_edge_barycenter_weight: f64,
    node_group: &HashMap<NodeIndex, Option<String>>,
) -> Ordering {
    let left_stats = weighted_median_stats(dag, left, neighbor_pos, direction, long_edge_barycenter_weight);
    let right_stats = weighted_median_stats(dag, right, neighbor_pos, direction, long_edge_barycenter_weight);

    // Group 偏置：当 median 接近时（差 < epsilon），优先把同 group 节点排在一起。
    // 仅在 median 接近时生效，避免破坏基于 median 的交叉最小化。
    // 两节点都有 group 且不同时按 group id 排序（聚拢同 group 节点）；
    // 任一节点无 group 时返回 Equal（不影响后续 tiebreaker）。
    let median_diff = (left_stats.median - right_stats.median).abs();
    let median_cmp = left_stats
        .median
        .partial_cmp(&right_stats.median)
        .unwrap_or(Ordering::Equal);
    let group_bias = if median_diff < GROUP_BIAS_EPSILON {
        match (
            node_group.get(&left).and_then(|g| g.as_deref()),
            node_group.get(&right).and_then(|g| g.as_deref()),
        ) {
            (Some(lg), Some(rg)) => lg.cmp(rg),
            _ => Ordering::Equal,
        }
    } else {
        Ordering::Equal
    };

    median_cmp
        .then_with(|| group_bias)
        .then_with(|| {
            left_stats
                .barycenter
                .partial_cmp(&right_stats.barycenter)
                .unwrap_or(Ordering::Equal)
        })
        .then_with(|| right_stats.degree.cmp(&left_stats.degree))
        // Sugiyama 标准：median/barycenter 相同时，dummy 节点优先于真节点，
        // 使长边 dummy 链更易竖直对齐，减少折弯。
        // 旧版为真节点优先（left.cmp(right)），此处反转为 dummy 优先。
        .then_with(|| right_stats.is_dummy.cmp(&left_stats.is_dummy))
        .then_with(|| {
            left_stats
                .spread
                .partial_cmp(&right_stats.spread)
                .unwrap_or(Ordering::Equal)
        })
        .then_with(|| layer_node_sort_key(dag, left).cmp(&layer_node_sort_key(dag, right)))
        .then_with(|| {
            layer.iter()
                .position(|node| *node == left)
                .cmp(&layer.iter().position(|node| *node == right))
        })
}

fn layer_node_sort_key(
    dag: &DiGraph<LayerNode, ()>,
    node: NodeIndex,
) -> (u8, usize, usize, usize, usize) {
    match dag[node].kind {
        LayerNodeKind::Real(original) => (0, original.index(), 0, 0, node.index()),
        LayerNodeKind::Dummy {
            source,
            target,
            segment,
        } => (1, source.index(), target.index(), segment, node.index()),
    }
}

fn weighted_median_stats(
    dag: &DiGraph<LayerNode, ()>,
    node: NodeIndex,
    neighbor_pos: &HashMap<NodeIndex, usize>,
    direction: Direction,
    long_edge_barycenter_weight: f64,
) -> OrderingStats {
    // 收集 (位置, 是否 dummy) 对，用于加权 barycenter 计算
    let mut positions_with_dummy: Vec<(f64, bool)> = match direction {
        Direction::Incoming => dag
            .neighbors_directed(node, Direction::Incoming)
            .filter_map(|pred| {
                neighbor_pos.get(&pred).copied().map(|value| {
                    let is_dummy = matches!(dag[pred].kind, LayerNodeKind::Dummy { .. });
                    (value as f64, is_dummy)
                })
            })
            .collect::<Vec<_>>(),
        Direction::Outgoing => dag
            .neighbors_directed(node, Direction::Outgoing)
            .filter_map(|succ| {
                neighbor_pos.get(&succ).copied().map(|value| {
                    let is_dummy = matches!(dag[succ].kind, LayerNodeKind::Dummy { .. });
                    (value as f64, is_dummy)
                })
            })
            .collect::<Vec<_>>(),
    };

    if positions_with_dummy.is_empty() {
        return OrderingStats {
            median: neighbor_pos.len() as f64 / 2.0,
            barycenter: neighbor_pos.len() as f64 / 2.0,
            spread: f64::INFINITY,
            degree: 0,
            is_dummy: matches!(dag[node].kind, LayerNodeKind::Dummy { .. }),
        };
    }

    positions_with_dummy.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));
    let degree = positions_with_dummy.len();
    let positions: Vec<f64> = positions_with_dummy.iter().map(|(p, _)| *p).collect();
    // Eades-Sugiyama 标准偶数偏移：偶数个邻居时，向上扫描（Incoming）取左中位数，
    // 向下扫描（Outgoing）取右中位数，避免偶数邻居中位数不确定导致的抖动。
    let median = if degree % 2 == 1 {
        positions[degree / 2]
    } else if direction == Direction::Incoming {
        positions[degree / 2 - 1]
    } else {
        positions[degree / 2]
    };

    // Phase 3：长边跨层惩罚 — 加权 barycenter
    // dummy 邻居（长边段）权重 = long_edge_barycenter_weight，
    // 鼓励节点向长边 dummy 链对齐，减少水平偏移从而缩短边总长。
    let barycenter = if long_edge_barycenter_weight == 1.0 {
        // 快速路径：无加权（与原实现一致）
        positions.iter().sum::<f64>() / degree as f64
    } else {
        let weighted_sum: f64 = positions_with_dummy
            .iter()
            .map(|(pos, is_dummy)| {
                pos * if *is_dummy { long_edge_barycenter_weight } else { 1.0 }
            })
            .sum();
        let total_weight: f64 = positions_with_dummy
            .iter()
            .map(|(_, is_dummy)| {
                if *is_dummy { long_edge_barycenter_weight } else { 1.0 }
            })
            .sum();
        weighted_sum / total_weight
    };

    let spread = positions.last().copied().unwrap_or(median) - positions.first().copied().unwrap_or(median);

    OrderingStats {
        median,
        barycenter,
        spread,
        degree,
        is_dummy: matches!(dag[node].kind, LayerNodeKind::Dummy { .. }),
    }
}

pub(super) fn transpose_adjacent(
    layer_index: usize,
    layers: &mut [Vec<NodeIndex>],
    dag: &DiGraph<LayerNode, ()>,
    long_edge_barycenter_weight: f64,
) {
    loop {
        let mut improved = false;
        for index in 0..layers[layer_index].len().saturating_sub(1) {
            let before_cross = crossing_score_around(layer_index, layers, dag);
            let before_penalty = alignment_penalty_around(layer_index, layers, dag, long_edge_barycenter_weight);
            layers[layer_index].swap(index, index + 1);
            let after_cross = crossing_score_around(layer_index, layers, dag);
            let after_penalty = alignment_penalty_around(layer_index, layers, dag, long_edge_barycenter_weight);
            if after_cross < before_cross
                || (after_cross == before_cross && after_penalty < before_penalty)
            {
                improved = true;
            } else {
                layers[layer_index].swap(index, index + 1);
            }
        }
        if !improved {
            break;
        }
    }
}

fn crossing_score_around<N>(layer_index: usize, layers: &[Vec<NodeIndex>], dag: &DiGraph<N, ()>) -> usize {
    let mut total = 0;
    if layer_index > 0 {
        total += count_crossings(&layers[layer_index - 1], &layers[layer_index], dag);
    }
    if layer_index + 1 < layers.len() {
        total += count_crossings(&layers[layer_index], &layers[layer_index + 1], dag);
    }
    total
}

pub(super) fn count_crossings<N>(upper: &[NodeIndex], lower: &[NodeIndex], dag: &DiGraph<N, ()>) -> usize {
    let lower_pos = lower
        .iter()
        .enumerate()
        .map(|(idx, node)| (*node, idx))
        .collect::<HashMap<_, _>>();
    let mut edges: Vec<(usize, usize)> = Vec::new();

    for (u_idx, upper_node) in upper.iter().enumerate() {
        for succ in dag.neighbors_directed(*upper_node, Direction::Outgoing) {
            if let Some(&l_idx) = lower_pos.get(&succ) {
                edges.push((u_idx, l_idx));
            }
        }
    }

    count_crossings_from_edges(&edges, lower.len())
}

fn alignment_penalty_around(
    layer_index: usize,
    layers: &[Vec<NodeIndex>],
    dag: &DiGraph<LayerNode, ()>,
    long_edge_barycenter_weight: f64,
) -> usize {
    let mut total = 0usize;
    let current_pos = index_map(&layers[layer_index]);

    if layer_index > 0 {
        let upper_pos = index_map(&layers[layer_index - 1]);
        total += layer_alignment_penalty(dag, &current_pos, &upper_pos, layer_index, Direction::Incoming, long_edge_barycenter_weight);
    }
    if layer_index + 1 < layers.len() {
        let lower_pos = index_map(&layers[layer_index + 1]);
        total += layer_alignment_penalty(dag, &current_pos, &lower_pos, layer_index, Direction::Outgoing, long_edge_barycenter_weight);
    }

    total
}

fn layer_alignment_penalty(
    dag: &DiGraph<LayerNode, ()>,
    current_pos: &HashMap<NodeIndex, usize>,
    neighbor_pos: &HashMap<NodeIndex, usize>,
    layer_index: usize,
    direction: Direction,
    long_edge_barycenter_weight: f64,
) -> usize {
    layers_iter_from_pos(current_pos)
        .into_iter()
        .map(|node| {
            let stats = weighted_median_stats(dag, node, neighbor_pos, direction, long_edge_barycenter_weight);
            let current = current_pos[&node] as f64;
            let mut penalty = ((current - stats.barycenter).abs() * 100.0) as usize;
            if matches!(dag[node].kind, LayerNodeKind::Dummy { .. }) && stats.degree == 1 {
                penalty /= 2;
            }
            if dag[node].rank != layer_index {
                penalty += 10_000;
            }
            penalty
        })
        .sum()
}

fn layers_iter_from_pos(pos: &HashMap<NodeIndex, usize>) -> Vec<NodeIndex> {
    let mut nodes = pos.iter().map(|(node, idx)| (*idx, *node)).collect::<Vec<_>>();
    nodes.sort_by_key(|(idx, _)| *idx);
    nodes.into_iter().map(|(_, node)| node).collect()
}
