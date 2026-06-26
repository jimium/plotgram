//! Phase 4: 坐标分配与邻接对齐。

use crate::ast::Diagram;
use crate::layout::constants;
use crate::layout::{NodeLayout};
use std::collections::{HashMap, HashSet};

use super::constants::{
    COORDINATE_REFINE_EPSILON, COORDINATE_REFINE_ITERATIONS, GROUP_CENTER_PULL_FACTOR, LAYER_GAP,
    NEIGHBOR_ALIGN_MAX_PASSES, NEIGHBOR_PULL_FACTOR, NODE_GAP, PADDING,
};
use super::types::{GraphIndex, GroupMap};

pub(in super::super) fn assign_coordinates(
    _diagram: &Diagram,
    graph: &GraphIndex,
    group_map: &GroupMap,
    layers: &[Vec<String>],
    sizes: &HashMap<String, (f64, f64)>,
) -> HashMap<String, NodeLayout> {
    let mut nodes = HashMap::new();

    // 计算每层的高度
    let layer_heights: Vec<f64> = layers
        .iter()
        .map(|layer| {
            layer
                .iter()
                .map(|node| sizes.get(node).map(|(_, h)| *h).unwrap_or(constants::DEFAULT_NODE_HEIGHT))
                .fold(0.0_f64, f64::max)
        })
        .collect();

    // 计算每层的 y 偏移
    let mut layer_y_offsets = vec![PADDING];
    for i in 1..layers.len() {
        layer_y_offsets.push(layer_y_offsets[i - 1] + layer_heights[i - 1] + LAYER_GAP);
    }

    // 对每层分配 x 坐标（Brandes-Köpf 风格简化版）
    for (layer_idx, layer) in layers.iter().enumerate() {
        let y_center = layer_y_offsets[layer_idx] + layer_heights[layer_idx] / 2.0;

        // 计算每个节点的理想 x 位置
        let ideal_positions = compute_ideal_x_positions(
            layer, layers, layer_idx, graph, sizes, group_map, &nodes,
        );

        // 解决重叠：确保节点不重叠
        let mut adjusted_positions = resolve_x_overlaps(layer, &ideal_positions, sizes);

        // 无组基础设施层：以连入该层的上游节点为锚点水平居中
        if is_infrastructure_layer(layer, group_map) {
            if let Some(anchor_x) = infrastructure_anchor_x(layer, graph, &nodes) {
                center_layer_on_anchor(layer, &mut adjusted_positions, sizes, anchor_x);
                adjusted_positions = resolve_x_overlaps(layer, &adjusted_positions, sizes);
            }
        }

        for (i, node) in layer.iter().enumerate() {
            let (width, height) = sizes
                .get(node)
                .copied()
                .unwrap_or((constants::DEFAULT_NODE_WIDTH, constants::DEFAULT_NODE_HEIGHT));
            let x_center = adjusted_positions[i];

            let layout = NodeLayout {
                x: x_center - width / 2.0,
                y: y_center - height / 2.0,
                width,
                height,
                ..Default::default()
            };

            nodes.insert(node.clone(), layout);
        }
    }

    nodes
}

/// 计算层内节点的理想 x 位置（Brandes-Köpf 简化版）
///
/// 四遍扫描：左对齐 → 右对齐 → 取平均
fn compute_ideal_x_positions(
    layer: &[String],
    layers: &[Vec<String>],
    layer_idx: usize,
    graph: &GraphIndex,
    sizes: &HashMap<String, (f64, f64)>,
    group_map: &GroupMap,
    placed: &HashMap<String, NodeLayout>,
) -> Vec<f64> {
    let n = layer.len();
    if n == 0 {
        return vec![];
    }

    // 初始位置：均匀分布（保证非负且不重叠）
    let mut positions = uniform_initial_positions(layer, sizes);

    // 已放置的邻层使用真实中心；未放置的邻层退化为均匀分布估计
    let upper_x: Option<HashMap<String, f64>> = if layer_idx > 0 {
        Some(layer_centers_from_placed(
            &layers[layer_idx - 1],
            placed,
            sizes,
        ))
    } else {
        None
    };
    let lower_x: Option<HashMap<String, f64>> = if layer_idx + 1 < layers.len() {
        Some(layer_centers_from_placed(
            &layers[layer_idx + 1],
            placed,
            sizes,
        ))
    } else {
        None
    };

    // 多轮迭代优化位置（P0.2: 检测收敛提前退出）
    for _ in 0..COORDINATE_REFINE_ITERATIONS {
        let prev_positions = positions.clone();

        if let Some(ref upper) = upper_x {
            pull_toward_neighbors(layer, &mut positions, upper, graph, None, true, NEIGHBOR_PULL_FACTOR);
        }
        if let Some(ref lower) = lower_x {
            pull_toward_neighbors(layer, &mut positions, lower, graph, None, false, NEIGHBOR_PULL_FACTOR);
        }

        // 分组引力：同组节点向质心靠拢
        pull_toward_group_center(layer, &mut positions, group_map, sizes);

        // 收敛检测：所有节点位置变化均小于 ε 时提前退出
        let converged = positions
            .iter()
            .zip(prev_positions.iter())
            .all(|(new, old)| (*new - *old).abs() < COORDINATE_REFINE_EPSILON);
        if converged {
            break;
        }
    }

    positions
}

/// 从已放置节点读取层内中心；缺失节点用均匀分布补齐
pub(in super::super) fn layer_centers_from_placed(
    layer: &[String],
    placed: &HashMap<String, NodeLayout>,
    sizes: &HashMap<String, (f64, f64)>,
) -> HashMap<String, f64> {
    let fallback = uniform_initial_positions(layer, sizes);
    layer
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let cx = placed
                .get(id)
                .map(|nl| nl.x + nl.width / 2.0)
                .unwrap_or(fallback[i]);
            (id.clone(), cx)
        })
        .collect()
}

fn node_center_x(node: &str, nodes: &HashMap<String, NodeLayout>) -> f64 {
    nodes
        .get(node)
        .map(|nl| nl.x + nl.width / 2.0)
        .unwrap_or(0.0)
}

/// 是否参与跨层水平对齐（同组或跨组客户端↔hub；跳过基础设施）
fn should_align_neighbors(
    node: &str,
    neighbor: &str,
    node_layer: usize,
    neighbor_layer: usize,
    group_map: &GroupMap,
) -> bool {
    if node_layer.abs_diff(neighbor_layer) != 1 {
        return false;
    }
    let gn = group_map.node_to_top_group.get(node);
    let nb = group_map.node_to_top_group.get(neighbor);
    match (gn, nb) {
        (None, _) | (_, None) => false,
        (Some(a), Some(b)) if a == b => true,
        (Some(_), Some(_)) => {
            node_layer + 1 == neighbor_layer || node_layer == neighbor_layer + 1
        }
    }
}

/// 将组内 fan-out hub（如 gateway）水平居中到同组直接子节点跨度中心
pub(in super::super) fn center_group_hub_nodes(
    graph: &GraphIndex,
    group_map: &GroupMap,
    layers: &[Vec<String>],
    sizes: &HashMap<String, (f64, f64)>,
    nodes: &mut HashMap<String, NodeLayout>,
) {
    for (layer_idx, layer) in layers.iter().enumerate() {
        if layer_idx + 1 >= layers.len() || is_infrastructure_layer(layer, group_map) {
            continue;
        }
        let lower_set: HashSet<String> = layers[layer_idx + 1].iter().cloned().collect();
        let mut centers: Vec<f64> = layer
            .iter()
            .map(|node| node_center_x(node, nodes))
            .collect();

        for (i, hub) in layer.iter().enumerate() {
            let Some(gid) = group_map.node_to_top_group.get(hub) else {
                continue;
            };
            let children: Vec<f64> = graph
                .out_edges
                .get(hub)
                .map(|succs| {
                    succs
                        .iter()
                        .filter(|s| {
                            lower_set.contains(*s)
                                && group_map.node_to_top_group.get(*s) == Some(gid)
                        })
                        .map(|s| node_center_x(s, nodes))
                        .collect()
                })
                .unwrap_or_default();
            if children.len() >= 2 {
                let min_x = children.iter().copied().fold(f64::INFINITY, f64::min);
                let max_x = children.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                centers[i] = (min_x + max_x) / 2.0;
            }
        }

        let resolved = resolve_x_overlaps(layer, &centers, sizes);
        for (node, cx) in layer.iter().zip(resolved.iter()) {
            if let Some(nl) = nodes.get_mut(node) {
                nl.x = cx - nl.width / 2.0;
            }
        }
    }
}

/// 将上层客户端节点对齐到下层 hub（多客户端时绕 hub 对称分布）
pub(in super::super) fn align_client_nodes_to_hubs(
    graph: &GraphIndex,
    group_map: &GroupMap,
    layers: &[Vec<String>],
    sizes: &HashMap<String, (f64, f64)>,
    nodes: &mut HashMap<String, NodeLayout>,
) {
    for (layer_idx, layer) in layers.iter().enumerate() {
        if layer_idx + 1 >= layers.len() || is_infrastructure_layer(layer, group_map) {
            continue;
        }
        let lower_set: HashSet<String> = layers[layer_idx + 1].iter().cloned().collect();

        let mut hub_targets: Vec<Option<f64>> = Vec::with_capacity(layer.len());
        for node in layer {
            let hubs: Vec<f64> = graph
                .out_edges
                .get(node)
                .map(|succs| {
                    succs
                        .iter()
                        .filter(|s| {
                            lower_set.contains(*s)
                                && should_align_neighbors(
                                    node,
                                    s,
                                    layer_idx,
                                    layer_idx + 1,
                                    group_map,
                                )
                        })
                        .map(|s| node_center_x(s, nodes))
                        .collect()
                })
                .unwrap_or_default();

            hub_targets.push(if hubs.len() == 1 { Some(hubs[0]) } else { None });
        }

        let mut centers: Vec<f64> = layer
            .iter()
            .map(|node| node_center_x(node, nodes))
            .collect();

        let mut by_hub: HashMap<i64, Vec<usize>> = HashMap::new();
        for (i, target) in hub_targets.iter().enumerate() {
            if let Some(hub_cx) = target {
                let key = (hub_cx * 10.0).round() as i64;
                by_hub.entry(key).or_default().push(i);
            }
        }

        for (_hub_key, mut indices) in by_hub {
            if indices.is_empty() {
                continue;
            }
            indices.sort_by(|&a, &b| {
                centers[a]
                    .partial_cmp(&centers[b])
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            let hub_cx = hub_targets[indices[0]].unwrap();
            if indices.len() == 2 {
                let left_idx = indices[0];
                let right_idx = indices[1];
                let w_left = sizes
                    .get(&layer[left_idx])
                    .map(|(w, _)| *w)
                    .unwrap_or(constants::DEFAULT_NODE_WIDTH);
                let w_right = sizes
                    .get(&layer[right_idx])
                    .map(|(w, _)| *w)
                    .unwrap_or(constants::DEFAULT_NODE_WIDTH);
                // 左侧客户端与 hub 同列（垂直连线），右侧客户端外移
                centers[left_idx] = hub_cx;
                centers[right_idx] = hub_cx + w_left / 2.0 + NODE_GAP + w_right / 2.0;
                continue;
            }

            let mut total_width = 0.0;
            for (pos, &idx) in indices.iter().enumerate() {
                let width = sizes
                    .get(&layer[idx])
                    .map(|(w, _)| *w)
                    .unwrap_or(constants::DEFAULT_NODE_WIDTH);
                total_width += width;
                if pos + 1 < indices.len() {
                    total_width += NODE_GAP;
                }
            }

            let mut cursor = hub_cx - total_width / 2.0;
            let min_cursor = PADDING;
            if cursor < min_cursor {
                cursor = min_cursor;
            }

            for &idx in &indices {
                let width = sizes
                    .get(&layer[idx])
                    .map(|(w, _)| *w)
                    .unwrap_or(constants::DEFAULT_NODE_WIDTH);
                centers[idx] = cursor + width / 2.0;
                cursor += width + NODE_GAP;
            }
        }

        let resolved = resolve_x_overlaps(layer, &centers, sizes);
        for (node, cx) in layer.iter().zip(resolved.iter()) {
            if let Some(nl) = nodes.get_mut(node) {
                nl.x = cx - nl.width / 2.0;
            }
        }
    }
}

/// 为一层的节点生成均匀非负的初始 x 中心
pub(in super::super) fn uniform_initial_positions(
    layer: &[String],
    sizes: &HashMap<String, (f64, f64)>,
) -> Vec<f64> {
    let mut positions = Vec::with_capacity(layer.len());
    let mut cursor = PADDING;
    for node in layer {
        let width = sizes.get(node).map(|(w, _)| *w).unwrap_or(constants::DEFAULT_NODE_WIDTH);
        positions.push(cursor + width / 2.0);
        cursor += width + NODE_GAP;
    }
    positions
}

/// 朝邻层中位数方向拉动节点（坐标分配阶段）
///
/// `filter` 为 `Some` 时仅考虑 filter 内的邻居（组内布局场景）；
/// 为 `None` 时考虑所有邻居（全局布局场景）。
///
/// `pull_factor` 控制单次拉力强度（0.0=不动，1.0=直接跳到中位数）。
pub(in super::super) fn pull_toward_neighbors(
    layer: &[String],
    positions: &mut [f64],
    neighbor_x: &HashMap<String, f64>,
    graph: &GraphIndex,
    filter: Option<&HashSet<String>>,
    from_upper: bool,
    pull_factor: f64,
) {
    for (i, node) in layer.iter().enumerate() {
        let neighbors = if from_upper {
            graph.in_edges.get(node).cloned().unwrap_or_default()
        } else {
            graph.out_edges.get(node).cloned().unwrap_or_default()
        };

        let positions_set: Vec<f64> = neighbors
            .iter()
            .filter(|n| filter.map_or(true, |f| f.contains(*n)))
            .filter_map(|n| neighbor_x.get(n).copied())
            .collect();

        if positions_set.is_empty() {
            continue;
        }

        let median = {
            let mut sorted = positions_set;
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
            sorted[sorted.len() / 2]
        };

        // 朝中位数方向移动（部分移动，避免跳跃）
        let current = positions[i];
        let pull = (median - current) * pull_factor;
        positions[i] = current + pull;
    }
}

fn pull_toward_group_center(
    layer: &[String],
    positions: &mut [f64],
    group_map: &GroupMap,
    _sizes: &HashMap<String, (f64, f64)>,
) {
    // 找出层内同组节点
    let mut group_indices: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, node) in layer.iter().enumerate() {
        let gid = group_map.node_to_top_group.get(node).cloned().unwrap_or_default();
        group_indices.entry(gid).or_default().push(i);
    }

    for (_, indices) in &group_indices {
        if indices.len() <= 1 {
            continue;
        }

        // 计算组内质心
        let centroid: f64 = indices.iter().map(|&i| positions[i]).sum::<f64>() / indices.len() as f64;

        // 朝质心方向微调（增强分组引力）
        for &i in indices {
            let current = positions[i];
            let pull = (centroid - current) * GROUP_CENTER_PULL_FACTOR;
            positions[i] = current + pull;
        }
    }
}

pub(in super::super) fn resolve_x_overlaps(
    layer: &[String],
    positions: &[f64],
    sizes: &HashMap<String, (f64, f64)>,
) -> Vec<f64> {
    let n = layer.len();
    if n <= 1 {
        return positions.to_vec();
    }

    let mut adjusted = positions.to_vec();

    // 前向扫描：确保不重叠
    for i in 1..n {
        let prev_width = sizes.get(&layer[i - 1]).map(|(w, _)| *w).unwrap_or(constants::DEFAULT_NODE_WIDTH);
        let curr_width = sizes.get(&layer[i]).map(|(w, _)| *w).unwrap_or(constants::DEFAULT_NODE_WIDTH);
        let min_center = adjusted[i - 1] + prev_width / 2.0 + NODE_GAP + curr_width / 2.0;
        if adjusted[i] < min_center {
            adjusted[i] = min_center;
        }
    }

    // 后向扫描：尽量保持原始位置
    for i in (0..n.saturating_sub(1)).rev() {
        let next_width = sizes.get(&layer[i + 1]).map(|(w, _)| *w).unwrap_or(constants::DEFAULT_NODE_WIDTH);
        let curr_width = sizes.get(&layer[i]).map(|(w, _)| *w).unwrap_or(constants::DEFAULT_NODE_WIDTH);
        let max_center = adjusted[i + 1] - next_width / 2.0 - NODE_GAP - curr_width / 2.0;
        if adjusted[i] > max_center {
            adjusted[i] = max_center;
        }
    }

    // 确保不超出左边界
    for i in 0..n {
        let width = sizes.get(&layer[i]).map(|(w, _)| *w).unwrap_or(constants::DEFAULT_NODE_WIDTH);
        let min_x = PADDING + width / 2.0;
        if adjusted[i] < min_x {
            adjusted[i] = min_x;
        }
    }

    adjusted
}

/// 层内节点是否全部无顶层 group（基础设施行）
fn is_infrastructure_layer(layer: &[String], group_map: &GroupMap) -> bool {
    !layer.is_empty()
        && layer
            .iter()
            .all(|node| !group_map.node_to_top_group.contains_key(node))
}

/// 以连入/连出该层节点的上下游已放置节点 x 中心联合跨度为锚点（P1.2: 双向锚点）
///
/// 原实现仅考虑上游（in_edges）已放置节点，对"基础设施行下游还有已放置节点"
/// 的场景（如基础设施行位于图中部）会偏移。扩展为同时收集上游和下游已放置
/// 节点的 x 中心，取联合跨度的中心作为锚点，使基础设施行在上下游之间居中。
pub(in super::super) fn infrastructure_anchor_x(
    layer: &[String],
    graph: &GraphIndex,
    placed: &HashMap<String, NodeLayout>,
) -> Option<f64> {
    let mut xs = Vec::new();
    for node in layer {
        // 上游：in_edges 中已放置的节点
        if let Some(preds) = graph.in_edges.get(node) {
            for pred in preds {
                if let Some(nl) = placed.get(pred) {
                    xs.push(nl.x + nl.width / 2.0);
                }
            }
        }
        // 下游：out_edges 中已放置的节点（P1.2: 双向锚点）
        if let Some(succs) = graph.out_edges.get(node) {
            for succ in succs {
                if let Some(nl) = placed.get(succ) {
                    xs.push(nl.x + nl.width / 2.0);
                }
            }
        }
    }
    if xs.is_empty() {
        return None;
    }
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    // 取上下游联合跨度的中心（min + max）/ 2
    Some((xs[0] + xs[xs.len() - 1]) / 2.0)
}

/// 将一层节点作为整体绕 anchor_x 居中排布
pub(in super::super) fn center_layer_on_anchor(
    layer: &[String],
    positions: &mut [f64],
    sizes: &HashMap<String, (f64, f64)>,
    anchor_x: f64,
) {
    if layer.is_empty() {
        return;
    }

    let mut total_width = 0.0;
    for (i, node) in layer.iter().enumerate() {
        let width = sizes
            .get(node)
            .map(|(w, _)| *w)
            .unwrap_or(constants::DEFAULT_NODE_WIDTH);
        total_width += width;
        if i + 1 < layer.len() {
            total_width += NODE_GAP;
        }
    }

    let mut cursor = anchor_x - total_width / 2.0;
    let min_cursor = PADDING;
    if cursor < min_cursor {
        cursor = min_cursor;
    }

    for (i, node) in layer.iter().enumerate() {
        let width = sizes
            .get(node)
            .map(|(w, _)| *w)
            .unwrap_or(constants::DEFAULT_NODE_WIDTH);
        positions[i] = cursor + width / 2.0;
        cursor += width + NODE_GAP;
    }
}

/// 重叠消除后，将无组基础设施行重新绕上游锚点居中（仅调整 x）
pub(in super::super) fn rebalance_infrastructure_layers(
    graph: &GraphIndex,
    group_map: &GroupMap,
    layers: &[Vec<String>],
    sizes: &HashMap<String, (f64, f64)>,
    nodes: &mut HashMap<String, NodeLayout>,
) {
    for layer in layers {
        if !is_infrastructure_layer(layer, group_map) {
            continue;
        }
        let Some(anchor_x) = infrastructure_anchor_x(layer, graph, nodes) else {
            continue;
        };

        let mut centers: Vec<f64> = Vec::with_capacity(layer.len());
        for node in layer {
            centers.push(
                nodes
                    .get(node)
                    .map(|nl| nl.x + nl.width / 2.0)
                    .unwrap_or(anchor_x),
            );
        }
        center_layer_on_anchor(layer, &mut centers, sizes, anchor_x);

        for (node, cx) in layer.iter().zip(centers.iter()) {
            if let Some(nl) = nodes.get_mut(node) {
                nl.x = cx - nl.width / 2.0;
            }
        }
    }
}

// ─── Phase 5.6: 邻接中心对齐 ────────────────────────────

/// 后处理：将每个节点向其上下游邻居的中心对齐，减少不必要的边拐弯。
///
/// 与 `pull_toward_neighbors`（坐标分配阶段）的区别：
/// - 在所有节点已放置完毕后运行，使用真实位置而非估算值
/// - 完整收敛（直接移到目标），而非 40% 部分移动
/// - 对单节点层无边界钳制（`resolve_x_overlaps` 的 `PADDING + width/2` 约束会
///   阻止宽节点左移对齐），仅保留 `PADDING` 最小边界
/// - 对多节点层做重叠保护：移动后检查是否与同层相邻节点重叠
///
/// 跳过基础设施层（无 group 的节点层）和组内节点（由 group 布局管理）。
pub(in super::super) fn align_nodes_to_neighbors(
    graph: &GraphIndex,
    group_map: &GroupMap,
    layers: &[Vec<String>],
    sizes: &HashMap<String, (f64, f64)>,
    nodes: &mut HashMap<String, NodeLayout>,
) {
    for _pass in 0..NEIGHBOR_ALIGN_MAX_PASSES {
        let mut any_moved = false;

        for layer in layers {
            // 跳过组内节点（由 group 布局管理）
            if !layer.is_empty() && layer.iter().all(|n| group_map.node_to_top_group.contains_key(n)) {
                continue;
            }

            // 收集本层各节点的邻接目标 center_x
            let mut targets: Vec<Option<f64>> = Vec::with_capacity(layer.len());
            for node in layer {
                let mut xs: Vec<f64> = Vec::new();
                if let Some(preds) = graph.in_edges.get(node) {
                    for pred in preds {
                        if let Some(nl) = nodes.get(pred) {
                            xs.push(nl.x + nl.width / 2.0);
                        }
                    }
                }
                if let Some(succs) = graph.out_edges.get(node) {
                    for succ in succs {
                        if let Some(nl) = nodes.get(succ) {
                            xs.push(nl.x + nl.width / 2.0);
                        }
                    }
                }
                targets.push(if xs.is_empty() {
                    None
                } else {
                    Some(median_f64(&xs))
                });
            }

            if layer.len() == 1 {
                // 单节点层：无重叠约束，直接移到目标
                let node = &layer[0];
                if let Some(target) = targets[0] {
                    if let Some(nl) = nodes.get_mut(node) {
                        let old_cx = nl.x + nl.width / 2.0;
                        if (old_cx - target).abs() > 0.5 {
                            nl.x = target - nl.width / 2.0;
                            any_moved = true;
                        }
                    }
                }
            } else {
                // 多节点层：逐个尝试移动，重叠保护
                for (i, node) in layer.iter().enumerate() {
                    let Some(target) = targets[i] else { continue };
                    let Some(nl) = nodes.get(node) else { continue };

                    let old_cx = nl.x + nl.width / 2.0;
                    if (old_cx - target).abs() < 0.5 {
                        continue;
                    }

                    // 构建候选中心数组（本节点替换为 target）
                    let mut centers: Vec<f64> = layer
                        .iter()
                        .map(|n| {
                            nodes
                                .get(n)
                                .map(|nl| nl.x + nl.width / 2.0)
                                .unwrap_or(0.0)
                        })
                        .collect();
                    centers[i] = target;

                    // 重叠保护：用 resolve_x_overlaps 检查是否可接受
                    let resolved = resolve_x_overlaps(layer, &centers, sizes);

                    // 仅在目标节点位置被接受时应用
                    if (resolved[i] - target).abs() < 1.0 {
                        for (j, n) in layer.iter().enumerate() {
                            if let Some(nl) = nodes.get_mut(n) {
                                nl.x = resolved[j] - nl.width / 2.0;
                            }
                        }
                        any_moved = true;
                    }
                }
            }
        }

        if !any_moved {
            break;
        }
    }
}

fn median_f64(xs: &[f64]) -> f64 {
    let mut sorted = xs.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    sorted[sorted.len() / 2]
}

