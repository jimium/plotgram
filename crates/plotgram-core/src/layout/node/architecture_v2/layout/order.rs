//! Phase 3: 分组感知层内排序。

use std::collections::{HashMap, HashSet};

use super::acyclic::is_effective_edge;
use super::constants::{
    CROSSING_SWEEPS_MAX, CROSSING_SWEEPS_MIN, LONG_EDGE_BARYCENTER_WEIGHT, TRANSPOSE_MAX_ROUNDS,
};
use super::types::{GraphIndex, GroupMap};

pub(in super::super) fn build_layers(ranks: &HashMap<String, usize>) -> Vec<Vec<String>> {
    if ranks.is_empty() {
        return vec![];
    }

    let max_rank = ranks.values().copied().max().unwrap_or(0);
    let mut layers = vec![Vec::new(); max_rank + 1];

    for (node, &rank) in ranks {
        layers[rank].push(node.clone());
    }

    // 每层内按 node_order 排序（确定性初始顺序）
    for layer in &mut layers {
        layer.sort();
    }

    layers
}

/// 分组感知的交叉最小化
///
/// 策略：
/// 1. 加权中位数排序，但同组节点保持相邻
/// 2. 相邻交换优化
/// 3. 多轮迭代（sweep 数根据初始交叉数自适应，稀疏图减少迭代）
pub(in super::super) fn order_layers_group_aware(
    graph: &GraphIndex,
    group_map: &GroupMap,
    layers: &[Vec<String>],
    reversed: &HashSet<(String, String)>,
) -> Vec<Vec<String>> {
    if layers.len() <= 1 {
        return layers.to_vec();
    }

    let mut current_layers = layers.to_vec();

    // P0.1: 自适应 sweep 数——根据初始交叉数动态调整迭代轮数
    // 稀疏图（< 10 交叉）降至 4 轮，密集图保持 12+ 轮
    let initial_crossings = total_crossings(&current_layers, graph, reversed);
    let sweeps = adaptive_sweeps(initial_crossings, current_layers.len());

    for sweep in 0..sweeps {
        let downward = sweep % 2 == 0;

        if downward {
            // 从上到下扫描
            for layer_idx in 1..current_layers.len() {
                let upper_pos = index_map(&current_layers[layer_idx - 1]);
                let layer = &current_layers[layer_idx];
                let ordered = order_layer_by_median(layer, &upper_pos, graph, reversed, downward);
                let grouped = group_aware_reorder(&ordered, group_map);
                let optimized = transpose_adjacent_group_aware(
                    &grouped, layer_idx, &current_layers, graph, reversed, group_map,
                );
                current_layers[layer_idx] = optimized;
            }
        } else {
            // 从下到上扫描
            for layer_idx in (0..current_layers.len().saturating_sub(1)).rev() {
                let lower_pos = index_map(&current_layers[layer_idx + 1]);
                let layer = &current_layers[layer_idx];
                let ordered = order_layer_by_median(layer, &lower_pos, graph, reversed, downward);
                let grouped = group_aware_reorder(&ordered, group_map);
                let optimized = transpose_adjacent_group_aware(
                    &grouped, layer_idx, &current_layers, graph, reversed, group_map,
                );
                current_layers[layer_idx] = optimized;
            }
        }
    }

    current_layers
}

/// 根据初始交叉数与层数决定 sweep 轮数
///
/// - `initial_crossings / 10` 提供粗粒度的密度估计：每 10 个交叉增加 1 轮
/// - `clamp(MIN, MAX)` 保证下限（基本收敛）与上限（避免过度迭代）
/// - `.min(layer_count * 2)` 防止层数很少时过度迭代（层数少则交叉空间小）
fn adaptive_sweeps(initial_crossings: usize, layer_count: usize) -> usize {
    let base = (initial_crossings / 10).clamp(CROSSING_SWEEPS_MIN, CROSSING_SWEEPS_MAX);
    base.min(layer_count.saturating_mul(2).max(CROSSING_SWEEPS_MIN))
}

/// 统计所有相邻层之间的交叉总数（用于自适应 sweep 决策）
fn total_crossings(
    layers: &[Vec<String>],
    graph: &GraphIndex,
    reversed: &HashSet<(String, String)>,
) -> usize {
    let mut total = 0;
    for i in 1..layers.len() {
        total += count_layer_crossings(&layers[i - 1], &layers[i], graph, reversed);
    }
    total
}

fn index_map(layer: &[String]) -> HashMap<String, usize> {
    layer.iter().enumerate().map(|(idx, node)| (node.clone(), idx)).collect()
}

fn order_layer_by_median(
    layer: &[String],
    neighbor_pos: &HashMap<String, usize>,
    graph: &GraphIndex,
    reversed: &HashSet<(String, String)>,
    downward: bool,
) -> Vec<String> {
    let mut nodes_with_median: Vec<(String, f64)> = layer
        .iter()
        .map(|node| {
            let positions: Vec<f64> = if downward {
                graph.in_edges.get(node)
                    .map(|preds| {
                        preds.iter()
                            .filter(|p| is_effective_edge(p, node, reversed))
                            .filter_map(|p| neighbor_pos.get(p).map(|&pos| pos as f64))
                            .collect()
                    })
                    .unwrap_or_default()
            } else {
                graph.out_edges.get(node)
                    .map(|succs| {
                        succs.iter()
                            .filter(|s| is_effective_edge(node, s, reversed))
                            .filter_map(|s| neighbor_pos.get(s).map(|&pos| pos as f64))
                            .collect()
                    })
                    .unwrap_or_default()
            };

            let median = if positions.is_empty() {
                layer.len() as f64 / 2.0
            } else {
                let mut sorted = positions;
                sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
                sorted[sorted.len() / 2]
            };

            (node.clone(), median)
        })
        .collect();

    nodes_with_median.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    nodes_with_median.into_iter().map(|(node, _)| node).collect()
}

/// 分组感知的重排序：同组节点保持相邻
///
/// 策略：按中位数排序后，将同组节点"吸附"到组内中位数最低的节点位置
fn group_aware_reorder(ordered: &[String], group_map: &GroupMap) -> Vec<String> {
    if ordered.len() <= 1 {
        return ordered.to_vec();
    }

    // 找出每个组在当前层中的成员
    let mut group_positions: HashMap<String, Vec<usize>> = HashMap::new();
    for (idx, node) in ordered.iter().enumerate() {
        let gid = group_map.node_to_top_group.get(node).cloned().unwrap_or_default();
        group_positions.entry(gid).or_default().push(idx);
    }

    // 只处理有多个成员的组，按组中位数位置排序（从后往前处理避免索引偏移）
    let mut multi_groups: Vec<(String, Vec<usize>)> = group_positions
        .into_iter()
        .filter(|(_, positions)| positions.len() > 1)
        .collect();
    // 先按 gid 排序保证确定性底序，再按中位数位置排序（稳定排序），
    // 避免 median 相同时保持 HashMap 迭代顺序（非确定）→ 处理顺序不同 → result 不同
    multi_groups.sort_by(|a, b| a.0.cmp(&b.0));
    // 按中位数位置从后往前排序，这样从后往前处理时不会影响前面的索引
    multi_groups.sort_by(|a, b| {
        let a_med = a.1[a.1.len() / 2];
        let b_med = b.1[b.1.len() / 2];
        b_med.cmp(&a_med)
    });

    let mut result = ordered.to_vec();
    for (_, mut positions) in multi_groups {
        positions.sort();
        let group_nodes: Vec<String> = positions.iter().map(|&idx| result[idx].clone()).collect();

        // 从 result 中移除这些节点（从后往前移除避免索引偏移）
        for &idx in positions.iter().rev() {
            result.remove(idx);
        }

        // 计算插入位置：找到组内第一个非组节点在 result 中的位置
        // 使用组内中位数节点在原始序列中的相对位置
        let original_median_idx = positions[positions.len() / 2];
        // 计算在移除组内节点后，中位数之前有多少个非组节点
        let non_group_before_median = positions.iter().filter(|&&idx| idx < original_median_idx).count();
        let insert_pos = original_median_idx - non_group_before_median;
        let insert_pos = insert_pos.min(result.len());

        for (i, node) in group_nodes.into_iter().enumerate() {
            result.insert(insert_pos + i, node);
        }
    }

    result
}

/// 分组感知的相邻交换优化
///
/// 注意：必须确保 before 和 after 都从当前层状态计算，
/// 否则会出现不一致导致无限循环。
///
/// P1.1: 引入对齐惩罚作为次级目标——当交换前后交叉数相同时，
/// 选择对齐惩罚更低的排布。度数为 1 的节点（长边段等价）惩罚减半，
/// 鼓励其与唯一邻居对齐，使跨层长边视觉上更接近竖直。
fn transpose_adjacent_group_aware(
    layer: &[String],
    layer_idx: usize,
    layers: &[Vec<String>],
    graph: &GraphIndex,
    reversed: &HashSet<(String, String)>,
    group_map: &GroupMap,
) -> Vec<String> {
    let mut current = layer.to_vec();

    for _ in 0..TRANSPOSE_MAX_ROUNDS {
        let mut improved = false;
        for i in 0..current.len().saturating_sub(1) {
            let a = &current[i];
            let b = &current[i + 1];

            // 同组节点不允许交换（保持相邻）
            let a_group = group_map.node_to_top_group.get(a).cloned();
            let b_group = group_map.node_to_top_group.get(b).cloned();
            if a_group.is_some() && a_group == b_group {
                continue;
            }

            // 交换前：交叉数 + 对齐惩罚
            let before_cross = crossing_score_for_layer_with_current(layer_idx, &current, layers, graph, reversed);
            let before_penalty = alignment_penalty_around(layer_idx, &current, layers, graph, reversed);

            current.swap(i, i + 1);

            // 交换后：交叉数 + 对齐惩罚
            let after_cross = crossing_score_for_layer_with_current(layer_idx, &current, layers, graph, reversed);
            let after_penalty = alignment_penalty_around(layer_idx, &current, layers, graph, reversed);

            // 次级目标：交叉数相同时，选择对齐惩罚更低的排布
            if after_cross < before_cross
                || (after_cross == before_cross && after_penalty < before_penalty)
            {
                improved = true;
            } else {
                current.swap(i, i + 1);
            }
        }
        if !improved {
            break;
        }
    }

    current
}

/// 计算当前层与相邻层之间的对齐惩罚总和
///
/// 惩罚 = Σ |current_pos - barycenter| × 100
/// 度数为 1 的节点（单邻居，等价于 sugiyama_v2 的 dummy 长边段）惩罚除以
/// `LONG_EDGE_BARYCENTER_WEIGHT`，鼓励其与唯一邻居对齐。
fn alignment_penalty_around(
    layer_idx: usize,
    current_layer: &[String],
    layers: &[Vec<String>],
    graph: &GraphIndex,
    reversed: &HashSet<(String, String)>,
) -> usize {
    let current_pos: HashMap<&str, usize> = current_layer
        .iter()
        .enumerate()
        .map(|(i, n)| (n.as_str(), i))
        .collect();

    let mut total = 0usize;

    if layer_idx > 0 {
        let upper_pos: HashMap<&str, usize> = layers[layer_idx - 1]
            .iter()
            .enumerate()
            .map(|(i, n)| (n.as_str(), i))
            .collect();
        total += layer_alignment_penalty(
            &current_pos,
            &upper_pos,
            graph,
            reversed,
            true, // upward = in_edges
        );
    }
    if layer_idx + 1 < layers.len() {
        let lower_pos: HashMap<&str, usize> = layers[layer_idx + 1]
            .iter()
            .enumerate()
            .map(|(i, n)| (n.as_str(), i))
            .collect();
        total += layer_alignment_penalty(
            &current_pos,
            &lower_pos,
            graph,
            reversed,
            false, // downward = out_edges
        );
    }

    total
}

/// 计算单层内所有节点的 barycenter 对齐惩罚
///
/// 对于每个节点，收集其在邻层的有效邻居位置，计算中位数（barycenter），
/// 惩罚 = |当前位置 - barycenter| × 100。
/// 度数为 1 时（长边段等价），惩罚除以 `LONG_EDGE_BARYCENTER_WEIGHT`。
fn layer_alignment_penalty(
    current_pos: &HashMap<&str, usize>,
    neighbor_pos: &HashMap<&str, usize>,
    graph: &GraphIndex,
    reversed: &HashSet<(String, String)>,
    upward: bool,
) -> usize {
    // 按 current_pos 的索引顺序遍历，保证确定性
    let mut nodes: Vec<(&str, usize)> = current_pos.iter().map(|(n, &i)| (*n, i)).collect();
    nodes.sort_by_key(|(_, i)| *i);

    nodes
        .into_iter()
        .map(|(node, current_idx)| {
            let positions: Vec<f64> = if upward {
                graph.in_edges.get(node)
                    .map(|preds| {
                        preds.iter()
                            .filter(|p| is_effective_edge(p, node, reversed))
                            .filter_map(|p| neighbor_pos.get(p.as_str()).map(|&pos| pos as f64))
                            .collect()
                    })
                    .unwrap_or_default()
            } else {
                graph.out_edges.get(node)
                    .map(|succs| {
                        succs.iter()
                            .filter(|s| is_effective_edge(node, s, reversed))
                            .filter_map(|s| neighbor_pos.get(s.as_str()).map(|&pos| pos as f64))
                            .collect()
                    })
                    .unwrap_or_default()
            };

            if positions.is_empty() {
                return 0;
            }

            let degree = positions.len();
            let mut sorted = positions;
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let barycenter = sorted[sorted.len() / 2];

            let mut penalty = ((current_idx as f64 - barycenter).abs() * 100.0) as usize;

            // 度数为 1 的节点等价于 sugiyama_v2 中的 dummy 长边段：
            // 惩罚减半，鼓励其与唯一邻居对齐
            if degree == 1 {
                penalty = (penalty as f64 / LONG_EDGE_BARYCENTER_WEIGHT) as usize;
            }

            penalty
        })
        .sum()
}

/// 使用当前层状态计算交叉分数
fn crossing_score_for_layer_with_current(
    layer_idx: usize,
    current_layer: &[String],
    layers: &[Vec<String>],
    graph: &GraphIndex,
    reversed: &HashSet<(String, String)>,
) -> usize {
    let mut total = 0;
    if layer_idx > 0 {
        total += count_layer_crossings(&layers[layer_idx - 1], current_layer, graph, reversed);
    }
    if layer_idx + 1 < layers.len() {
        total += count_layer_crossings(current_layer, &layers[layer_idx + 1], graph, reversed);
    }
    total
}

fn count_layer_crossings(
    upper: &[String],
    lower: &[String],
    graph: &GraphIndex,
    reversed: &HashSet<(String, String)>,
) -> usize {
    // 构建 lower 节点 → 位置索引映射
    let lower_pos: HashMap<&str, usize> = lower
        .iter()
        .enumerate()
        .map(|(idx, n)| (n.as_str(), idx))
        .collect();
    let mut edges: Vec<(usize, usize)> = Vec::new();

    for (u_idx, upper_node) in upper.iter().enumerate() {
        // 后继边：upper -> lower
        if let Some(successors) = graph.out_edges.get(upper_node) {
            for succ in successors {
                if !is_effective_edge(upper_node, succ, reversed) {
                    continue;
                }
                if let Some(&l_idx) = lower_pos.get(succ.as_str()) {
                    edges.push((u_idx, l_idx));
                }
            }
        }
        // 前驱边（反转后方向相反）：lower -> upper，等价于 upper 在 lower 侧
        if let Some(predecessors) = graph.in_edges.get(upper_node) {
            for pred in predecessors {
                if !is_effective_edge(pred, upper_node, reversed) {
                    continue;
                }
                if let Some(&l_idx) = lower_pos.get(pred.as_str()) {
                    edges.push((u_idx, l_idx));
                }
            }
        }
    }

    // 使用共享的 Fenwick Tree 扫描线算法，O(E log V) 替代原 O(E²) 双重循环
    crate::layout::node::common::crossings::count_crossings_from_edges(&edges, lower.len())
}

