use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::Direction;
use std::cmp::Ordering;
use std::collections::HashMap;

use super::graph::{LayerNode, LayerNodeKind};
use crate::layout::kernel::common::crossings::count_crossings_from_edges;

/// Group 偏置触发阈值：当两节点 median 差小于此值时，视为"位置接近"，
/// 启用 group 偏置 tiebreaker（优先同 group 节点相邻）。
///
/// 取 1.0：median 为整数位置索引，差 < 1.0 即同位置；放宽到 ≤ 1.0 可覆盖
/// 相邻位置，让 group 偏置在"位置接近"时生效，而非仅完全相等时。
const GROUP_BIAS_EPSILON: f64 = 1.0;

const ORDERING_SWEEP_MAX: usize = 16;
const ORDERING_NO_IMPROVE_STOP: usize = 2;

pub(super) fn order_layers_weighted_median(
    dag: &DiGraph<LayerNode, ()>,
    mut layers: Vec<Vec<NodeIndex>>,
    ordering_sweeps: usize,
    long_edge_barycenter_weight: f64,
    node_group: &HashMap<NodeIndex, Option<String>>,
    group_decl: &HashMap<String, usize>,
    same_layer_bias: &HashMap<NodeIndex, NodeIndex>,
) -> Vec<Vec<NodeIndex>> {
    let max_sweeps = ordering_sweeps.clamp(1, ORDERING_SWEEP_MAX);
    let mut no_improve = 0usize;
    let mut prev_crossings = count_layer_crossings(dag, &layers);

    for _ in 0..max_sweeps {
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
                    group_decl,
                    same_layer_bias,
                )
            });
            transpose_adjacent(layer_index, &mut layers, dag, long_edge_barycenter_weight, same_layer_bias);
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
                    group_decl,
                    same_layer_bias,
                )
            });
            transpose_adjacent(layer_index, &mut layers, dag, long_edge_barycenter_weight, same_layer_bias);
        }

        let crossings = count_layer_crossings(dag, &layers);
        if crossings < prev_crossings {
            no_improve = 0;
        } else {
            no_improve += 1;
            if no_improve >= ORDERING_NO_IMPROVE_STOP {
                break;
            }
        }
        prev_crossings = crossings;
    }

    layers
}

fn count_layer_crossings(dag: &DiGraph<LayerNode, ()>, layers: &[Vec<NodeIndex>]) -> usize {
    let mut total = 0usize;
    for layer_index in 0..layers.len().saturating_sub(1) {
        total += count_crossings(&layers[layer_index], &layers[layer_index + 1], dag);
    }
    total
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
    group_decl: &HashMap<String, usize>,
    same_layer_bias: &HashMap<NodeIndex, NodeIndex>,
) -> Ordering {
    let left_stats = weighted_median_stats(dag, left, neighbor_pos, direction, long_edge_barycenter_weight);
    let right_stats = weighted_median_stats(dag, right, neighbor_pos, direction, long_edge_barycenter_weight);

    // Group 偏置：当 median 接近时（差 < epsilon），优先把同 group 节点排在一起。
    // 仅在 median 接近时生效，避免破坏基于 median 的交叉最小化。
    // 两节点都有 group 且不同时按 group sibling 声明序（再 id）排序；
    // 任一节点无 group 时返回 Equal（不影响后续 tiebreaker）。
    let median_diff = (left_stats.median - right_stats.median).abs();
    let median_cmp = left_stats
        .median
        .partial_cmp(&right_stats.median)
        .unwrap_or(Ordering::Equal);

    // 同层侧向偏置：当两节点构成同层对时，无条件覆盖 median。
    // hub 排主前驱左侧。
    let same_layer_cmp = match (same_layer_bias.get(&left), same_layer_bias.get(&right)) {
        (Some(pred), _) if *pred == right => Ordering::Less,
        (_, Some(pred)) if *pred == left => Ordering::Greater,
        _ => Ordering::Equal,
    };
    if same_layer_cmp != Ordering::Equal {
        return same_layer_cmp;
    }

    // Group 偏置：当 median 接近时（差 < epsilon），优先把同 group 节点排在一起。
    let group_bias = if median_diff < GROUP_BIAS_EPSILON {
        match (
            node_group.get(&left).and_then(|g| g.as_deref()),
            node_group.get(&right).and_then(|g| g.as_deref()),
        ) {
            (Some(lg), Some(rg)) => {
                crate::layout::decl_order::cmp_by_decl_then_id(group_decl, lg, rg)
            }
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
    same_layer_bias: &HashMap<NodeIndex, NodeIndex>,
) {
    loop {
        let mut improved = false;
        for index in 0..layers[layer_index].len().saturating_sub(1) {
            let a = layers[layer_index][index];
            let b = layers[layer_index][index + 1];
            // 同层约束保护：不允许 transpose 破坏 hub/end 与主前驱的相对顺序。
            if breaks_same_layer_order(a, b, same_layer_bias) {
                continue;
            }
            let before_cross = crossing_score_around(layer_index, layers, dag);
            let before_penalty = alignment_penalty_around(layer_index, layers, dag, long_edge_barycenter_weight);
            let before_long = long_edge_crossing_score(layers, dag);
            layers[layer_index].swap(index, index + 1);
            let after_cross = crossing_score_around(layer_index, layers, dag);
            let after_penalty = alignment_penalty_around(layer_index, layers, dag, long_edge_barycenter_weight);
            let after_long = long_edge_crossing_score(layers, dag);
            if after_cross < before_cross
                || (after_cross == before_cross && after_penalty < before_penalty)
                || (after_cross == before_cross
                    && after_penalty == before_penalty
                    && after_long < before_long)
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

/// 检查交换 (a, b) 是否会破坏同层约束。
/// 当前顺序为 [..a, b..]，交换后变为 [..b, a..]。
/// 如果 a 是 hub（FeedbackSide）且 b 是其主前驱，则当前顺序正确，交换会破坏。
fn breaks_same_layer_order(
    a: NodeIndex,
    b: NodeIndex,
    same_layer_bias: &HashMap<NodeIndex, NodeIndex>,
) -> bool {
    match (same_layer_bias.get(&a), same_layer_bias.get(&b)) {
        // a 是 hub，b 是其主前驱 → 当前 [a, b] 正确，交换破坏
        (Some(pred), _) if *pred == b => true,
        _ => false,
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

/// 跨层长边（rank 差 ≥ 2）在层坐标系下的几何交叉估计。
fn long_edge_crossing_score(
    layers: &[Vec<NodeIndex>],
    dag: &DiGraph<LayerNode, ()>,
) -> usize {
    let mut layer_pos: HashMap<NodeIndex, (usize, usize)> = HashMap::new();
    for (layer_idx, layer) in layers.iter().enumerate() {
        for (pos, node) in layer.iter().enumerate() {
            layer_pos.insert(*node, (layer_idx, pos));
        }
    }

    let mut long_edges: Vec<((usize, usize), (usize, usize))> = Vec::new();
    for node in dag.node_indices() {
        let Some(&(from_layer, from_pos)) = layer_pos.get(&node) else {
            continue;
        };
        for succ in dag.neighbors_directed(node, Direction::Outgoing) {
            let Some(&(to_layer, to_pos)) = layer_pos.get(&succ) else {
                continue;
            };
            if from_layer.abs_diff(to_layer) < 2 {
                continue;
            }
            long_edges.push(((from_layer, from_pos), (to_layer, to_pos)));
        }
    }

    long_edges.sort();
    let mut crossings = 0usize;
    for left in 0..long_edges.len() {
        for right in (left + 1)..long_edges.len() {
            let (a0, a1) = long_edges[left];
            let (b0, b1) = long_edges[right];
            if long_segments_cross(a0, a1, b0, b1) {
                crossings += 1;
            }
        }
    }
    crossings
}

fn long_segments_cross(
    a0: (usize, usize),
    a1: (usize, usize),
    b0: (usize, usize),
    b1: (usize, usize),
) -> bool {
    let (a_layer0, a_pos0) = (a0.0 as f64, a0.1 as f64);
    let (a_layer1, a_pos1) = (a1.0 as f64, a1.1 as f64);
    let (b_layer0, b_pos0) = (b0.0 as f64, b0.1 as f64);
    let (b_layer1, b_pos1) = (b1.0 as f64, b1.1 as f64);

    fn orient(ax: f64, ay: f64, bx: f64, by: f64, cx: f64, cy: f64) -> f64 {
        (bx - ax) * (cy - ay) - (by - ay) * (cx - ax)
    }

    let o1 = orient(a_layer0, a_pos0, a_layer1, a_pos1, b_layer0, b_pos0);
    let o2 = orient(a_layer0, a_pos0, a_layer1, a_pos1, b_layer1, b_pos1);
    let o3 = orient(b_layer0, b_pos0, b_layer1, b_pos1, a_layer0, a_pos0);
    let o4 = orient(b_layer0, b_pos0, b_layer1, b_pos1, a_layer1, a_pos1);

    if o1 == 0.0 && o2 == 0.0 && o3 == 0.0 && o4 == 0.0 {
        let a_min_layer = a_layer0.min(a_layer1);
        let a_max_layer = a_layer0.max(a_layer1);
        let b_min_layer = b_layer0.min(b_layer1);
        let b_max_layer = b_layer0.max(b_layer1);
        let a_min_pos = a_pos0.min(a_pos1);
        let a_max_pos = a_pos0.max(a_pos1);
        let b_min_pos = b_pos0.min(b_pos1);
        let b_max_pos = b_pos0.max(b_pos1);
        return a_min_layer <= b_max_layer
            && b_min_layer <= a_max_layer
            && a_min_pos <= b_max_pos
            && b_min_pos <= a_max_pos;
    }

    (o1 > 0.0) != (o2 > 0.0) && (o3 > 0.0) != (o4 > 0.0)
}
