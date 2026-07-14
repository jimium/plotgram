use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::Direction;
use std::collections::{HashMap, HashSet};

use super::graph::{LayerNode, LayerNodeKind};
use super::order;
use super::postprocess;
use super::preset::{self, SugiyamaPreset};

use crate::layout::node::common::stats::median_f64;

pub(super) fn assign_coordinates_brandes_koepf(
    dag: &DiGraph<String, ()>,
    layered_graph: &DiGraph<LayerNode, ()>,
    layers: &[Vec<NodeIndex>],
    sizes: &HashMap<NodeIndex, (f64, f64)>,
    horizontal: bool,
    preset: &SugiyamaPreset,
    layer_gaps: &[f64],
) -> HashMap<String, crate::layout::NodeLayout> {
    let spine = compute_spine_nodes(dag);
    let mut centers =
        assign_layer_centers_brandes_koepf(layered_graph, layers, sizes, preset, &spine);
    // Iteration 3：compaction 保持 3 轮；spine 邻居轻微加权（1.5，避免过度拉扯增交叉）
    compact_layer_centers(
        &mut centers,
        layered_graph,
        layers,
        sizes,
        preset,
        &spine,
        3,
    );

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
    align_singleton_layers_to_predecessors(dag, layered_graph, layers, &mut nodes, horizontal);
    // 先 normalize，再对齐悬挂叶：避免 pack 探出左缘后二次 normalize 把锚点（auth）整体平移。
    postprocess::normalize_layout_to_padding(&mut nodes, preset.padding);
    align_pendants_under_anchors(dag, layered_graph, layers, &mut nodes, horizontal, preset);
    nodes
}

/// 单节点层对齐邻层前驱：拉直主轴，避免 fan-out / 回边假前驱把链拉歪。
///
/// 对恰好 1 个 Real 节点的层：只对齐**紧邻上一层**的前驱中心（median）；
/// 无邻层前驱时回退到邻层后继；再没有才用全部前驱/后继。
/// 多节点层不动，以免破坏 fan-out 间距。
///
/// 关键：FAS 反转长回边后，远端节点会变成「假前驱」，若参与 median
/// 会把 `last_ack` 一类主链节点拉成阶梯右偏。
fn align_singleton_layers_to_predecessors(
    dag: &DiGraph<String, ()>,
    layered_graph: &DiGraph<LayerNode, ()>,
    layers: &[Vec<NodeIndex>],
    nodes: &mut HashMap<String, crate::layout::NodeLayout>,
    horizontal: bool,
) {
    // Real 节点 → 层下标（确定性：同 id 不跨层）
    let mut real_layer: HashMap<String, usize> = HashMap::new();
    for (layer_index, layer) in layers.iter().enumerate() {
        for node in layer {
            if let LayerNodeKind::Real(original) = &layered_graph[*node].kind {
                real_layer.insert(dag[*original].clone(), layer_index);
            }
        }
    }

    for (layer_index, layer) in layers.iter().enumerate() {
        let real_ids: Vec<String> = layer
            .iter()
            .filter_map(|node| match &layered_graph[*node].kind {
                LayerNodeKind::Real(original) => Some(dag[*original].clone()),
                LayerNodeKind::Dummy { .. } => None,
            })
            .collect();
        if real_ids.len() != 1 {
            continue;
        }
        let id = &real_ids[0];
        let Some(original) = dag.node_indices().find(|&n| dag[n] == *id) else {
            continue;
        };

        let mut adj_pred_centers: Vec<f64> = Vec::new();
        for pred in dag.neighbors_directed(original, Direction::Incoming) {
            let pred_id = &dag[pred];
            let Some(nl) = nodes.get(pred_id) else {
                continue;
            };
            let c = axis_center(nl, horizontal);
            if real_layer.get(pred_id).copied() == layer_index.checked_sub(1) {
                adj_pred_centers.push(c);
            }
        }
        adj_pred_centers.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let target = if !adj_pred_centers.is_empty() {
            median_f64(&adj_pred_centers)
        } else {
            // FAS 反转后，DAG 后继可能落在上一层（layer-1）而非 layer+1。
            // 凡紧邻层的出边邻居均可作为对齐目标（仍禁止全图假邻居）。
            let mut adj_succ_centers: Vec<f64> = Vec::new();
            for succ in dag.neighbors_directed(original, Direction::Outgoing) {
                let succ_id = &dag[succ];
                let Some(nl) = nodes.get(succ_id) else {
                    continue;
                };
                let c = axis_center(nl, horizontal);
                let Some(&succ_layer) = real_layer.get(succ_id) else {
                    continue;
                };
                let adjacent = succ_layer
                    .checked_add(1)
                    .is_some_and(|s| s == layer_index)
                    || succ_layer == layer_index + 1;
                if adjacent {
                    adj_succ_centers.push(c);
                }
            }
            adj_succ_centers.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            if !adj_succ_centers.is_empty() {
                median_f64(&adj_succ_centers)
            } else {
                // 无邻层邻居则跳过，禁止全图假邻居 fallback
                continue;
            }
        };

        let Some(nl) = nodes.get_mut(id) else {
            continue;
        };
        let size = axis_size(nl, horizontal);
        let old = axis_center(nl, horizontal);
        if (old - target).abs() <= 0.5 {
            continue;
        }
        set_axis_center(nl, horizontal, target, size);
    }
}

/// V3b：邻层悬挂叶（P-Pendant）对齐到枢纽锚点。
///
/// - 单 pendant：主轴中心与锚点重合（无同层冲突时）。
/// - 同锚同层多 pendant：整层皆为该组时 S-PackUnderAnchor（组质心 = 锚点）；否则逐个 SkipIfConflict。
/// - 锚点永不移动；一轮内每个节点至多写入一次；pack 失败整组回滚。
///
/// 须在 `normalize_layout_to_padding` **之后**调用，以免 pack 后再次 normalize 拖动锚点。
fn align_pendants_under_anchors(
    dag: &DiGraph<String, ()>,
    layered_graph: &DiGraph<LayerNode, ()>,
    layers: &[Vec<NodeIndex>],
    nodes: &mut HashMap<String, crate::layout::NodeLayout>,
    horizontal: bool,
    preset: &SugiyamaPreset,
) {
    const EPS: f64 = 1.0;

    let mut real_layer: HashMap<String, usize> = HashMap::new();
    let mut layer_reals: Vec<Vec<String>> = vec![Vec::new(); layers.len()];
    for (layer_index, layer) in layers.iter().enumerate() {
        for node in layer {
            if let LayerNodeKind::Real(original) = &layered_graph[*node].kind {
                let id = dag[*original].clone();
                real_layer.insert(id.clone(), layer_index);
                layer_reals[layer_index].push(id);
            }
        }
        layer_reals[layer_index].sort();
    }

    // id → 无向邻层 Real 邻居（稳定：按 id 排序）
    let mut adj_layer_nbrs: HashMap<String, Vec<String>> = HashMap::new();
    for id in real_layer.keys() {
        let Some(original) = dag.node_indices().find(|&n| dag[n] == *id) else {
            continue;
        };
        let Some(&my_layer) = real_layer.get(id) else {
            continue;
        };
        let mut nbrs: Vec<String> = Vec::new();
        for nbr in dag
            .neighbors_directed(original, Direction::Incoming)
            .chain(dag.neighbors_directed(original, Direction::Outgoing))
        {
            let nbr_id = dag[nbr].clone();
            let Some(&nbr_layer) = real_layer.get(&nbr_id) else {
                continue;
            };
            let adjacent = my_layer.abs_diff(nbr_layer) == 1;
            if adjacent && !nbrs.contains(&nbr_id) {
                nbrs.push(nbr_id);
            }
        }
        nbrs.sort();
        adj_layer_nbrs.insert(id.clone(), nbrs);
    }

    // 候选 (movable, anchor)：movable 邻层邻居唯一且为锚点；锚点邻层邻居数 > 1（枢纽）
    let mut candidates: Vec<(String, String)> = Vec::new();
    let mut movable_ids: Vec<String> = real_layer.keys().cloned().collect();
    movable_ids.sort();
    for movable in &movable_ids {
        let Some(nbrs) = adj_layer_nbrs.get(movable) else {
            continue;
        };
        if nbrs.len() != 1 {
            continue;
        }
        let anchor = &nbrs[0];
        let Some(anchor_nbrs) = adj_layer_nbrs.get(anchor) else {
            continue;
        };
        if anchor_nbrs.len() <= 1 {
            // 两端皆叶 → P-Mutual，本切片不做
            continue;
        }
        candidates.push((movable.clone(), anchor.clone()));
    }

    // 按 (movable_layer, anchor) 分组
    let mut groups: HashMap<(usize, String), Vec<String>> = HashMap::new();
    for (movable, anchor) in candidates {
        let Some(&layer) = real_layer.get(&movable) else {
            continue;
        };
        groups
            .entry((layer, anchor))
            .or_default()
            .push(movable);
    }
    let mut group_keys: Vec<(usize, String)> = groups.keys().cloned().collect();
    group_keys.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

    let mut moved: HashSet<String> = HashSet::new();

    for key in group_keys {
        let Some(mut pendants) = groups.remove(&key) else {
            continue;
        };
        pendants.retain(|id| !moved.contains(id));
        if pendants.is_empty() {
            continue;
        }
        let (layer, anchor) = key;
        let Some(anchor_layout) = nodes.get(&anchor).cloned() else {
            continue;
        };
        let anchor_cx = axis_center(&anchor_layout, horizontal);

        // 同层非组成员
        let layer_ids = &layer_reals[layer];
        let non_group: Vec<&String> = layer_ids
            .iter()
            .filter(|id| !pendants.contains(id))
            .collect();

        if pendants.len() == 1 {
            let movable = &pendants[0];
            let Some(layout) = nodes.get(movable).cloned() else {
                continue;
            };
            let size = axis_size(&layout, horizontal);
            let old = axis_center(&layout, horizontal);
            if (old - anchor_cx).abs() <= EPS {
                moved.insert(movable.clone());
                continue;
            }
            if same_layer_conflicts(
                nodes,
                horizontal,
                movable,
                anchor_cx,
                size,
                layer_ids,
                preset.node_gap,
            ) {
                continue;
            }
            if let Some(nl) = nodes.get_mut(movable) {
                set_axis_center(nl, horizontal, anchor_cx, size);
            }
            moved.insert(movable.clone());
            continue;
        }

        // 多 pendant：E1/E2 — 整层 Real 全是本组时才 pack，否则 SkipIfConflict 逐个试
        let entire_layer_is_group = non_group.is_empty() && layer_ids.len() == pendants.len();
        if entire_layer_is_group {
            if try_pack_under_anchor(
                nodes,
                horizontal,
                &pendants,
                anchor_cx,
                preset.node_gap,
                EPS,
            ) {
                for id in &pendants {
                    moved.insert(id.clone());
                }
            }
        } else {
            // 按 (center, id) 稳定序逐个尝试对齐
            pendants.sort_by(|a, b| {
                let ca = nodes
                    .get(a)
                    .map(|n| axis_center(n, horizontal))
                    .unwrap_or(0.0);
                let cb = nodes
                    .get(b)
                    .map(|n| axis_center(n, horizontal))
                    .unwrap_or(0.0);
                ca.partial_cmp(&cb)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.cmp(b))
            });
            for movable in &pendants {
                if moved.contains(movable) {
                    continue;
                }
                let Some(layout) = nodes.get(movable).cloned() else {
                    continue;
                };
                let size = axis_size(&layout, horizontal);
                let old = axis_center(&layout, horizontal);
                if (old - anchor_cx).abs() <= EPS {
                    moved.insert(movable.clone());
                    continue;
                }
                if same_layer_conflicts(
                    nodes,
                    horizontal,
                    movable,
                    anchor_cx,
                    size,
                    layer_ids,
                    preset.node_gap,
                ) {
                    continue;
                }
                if let Some(nl) = nodes.get_mut(movable) {
                    set_axis_center(nl, horizontal, anchor_cx, size);
                }
                moved.insert(movable.clone());
            }
        }
    }
}

fn same_layer_conflicts(
    nodes: &HashMap<String, crate::layout::NodeLayout>,
    horizontal: bool,
    movable: &str,
    trial_center: f64,
    movable_size: f64,
    layer_ids: &[String],
    node_gap: f64,
) -> bool {
    for other in layer_ids {
        if other == movable {
            continue;
        }
        let Some(ol) = nodes.get(other) else {
            continue;
        };
        let oc = axis_center(ol, horizontal);
        let os = axis_size(ol, horizontal);
        let min_sep = movable_size / 2.0 + os / 2.0 + node_gap;
        if (trial_center - oc).abs() + 1e-6 < min_sep {
            return true;
        }
    }
    false
}

/// S-PackUnderAnchor：组内质心对齐锚点；失败回滚。返回是否成功写入。
fn try_pack_under_anchor(
    nodes: &mut HashMap<String, crate::layout::NodeLayout>,
    horizontal: bool,
    pendants: &[String],
    anchor_cx: f64,
    node_gap: f64,
    eps: f64,
) -> bool {
    let mut items: Vec<(String, f64, f64)> = Vec::new(); // id, old_center, size
    for id in pendants {
        let Some(layout) = nodes.get(id) else {
            return false;
        };
        items.push((
            id.clone(),
            axis_center(layout, horizontal),
            axis_size(layout, horizontal),
        ));
    }
    // 稳定序：(center, id)
    items.sort_by(|a, b| {
        a.1.partial_cmp(&b.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });

    let mut trial: Vec<f64> = Vec::with_capacity(items.len());
    trial.push(0.0); // 相对坐标，稍后整体平移
    for i in 1..items.len() {
        let prev = trial[i - 1];
        let sep = items[i - 1].2 / 2.0 + items[i].2 / 2.0 + node_gap;
        trial.push(prev + sep);
    }
    let mean = trial.iter().sum::<f64>() / trial.len() as f64;
    let shift = anchor_cx - mean;
    for c in &mut trial {
        *c += shift;
    }

    // 组内间距已由构造保证；检查相对「应不变」——本组无非成员时跳过外部校验
    // 保存快照以便回滚
    let snapshot: Vec<(String, crate::layout::NodeLayout)> = pendants
        .iter()
        .filter_map(|id| nodes.get(id).map(|n| (id.clone(), n.clone())))
        .collect();

    for (i, (id, _, size)) in items.iter().enumerate() {
        let Some(nl) = nodes.get_mut(id) else {
            // 回滚
            for (sid, layout) in &snapshot {
                if let Some(n) = nodes.get_mut(sid) {
                    *n = layout.clone();
                }
            }
            return false;
        };
        set_axis_center(nl, horizontal, trial[i], *size);
    }

    // 组质心校验
    let centroid: f64 = items
        .iter()
        .filter_map(|(id, _, _)| nodes.get(id).map(|n| axis_center(n, horizontal)))
        .sum::<f64>()
        / items.len() as f64;
    if (centroid - anchor_cx).abs() > eps + 1e-6 {
        for (sid, layout) in &snapshot {
            if let Some(n) = nodes.get_mut(sid) {
                *n = layout.clone();
            }
        }
        return false;
    }

    // 组内两两 node_gap
    for i in 0..items.len() {
        for j in (i + 1)..items.len() {
            let ci = axis_center(&nodes[&items[i].0], horizontal);
            let cj = axis_center(&nodes[&items[j].0], horizontal);
            let min_sep = items[i].2 / 2.0 + items[j].2 / 2.0 + node_gap;
            if (ci - cj).abs() + 1e-6 < min_sep {
                for (sid, layout) in &snapshot {
                    if let Some(n) = nodes.get_mut(sid) {
                        *n = layout.clone();
                    }
                }
                return false;
            }
        }
    }

    true
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
    spine: &HashSet<NodeIndex>,
) -> HashMap<NodeIndex, f64> {
    let down_left = run_coordinate_pass_bk(dag, layers, sizes, true, true, preset, spine);
    let down_right = run_coordinate_pass_bk(dag, layers, sizes, true, false, preset, spine);
    let up_left = run_coordinate_pass_bk(dag, layers, sizes, false, true, preset, spine);
    let up_right = run_coordinate_pass_bk(dag, layers, sizes, false, false, preset, spine);

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
    spine: &HashSet<NodeIndex>,
) -> HashMap<NodeIndex, f64> {
    let oriented_layers = orient_layers(layers, left_to_right);
    let conflicts = detect_alignment_conflicts(dag, &oriented_layers);
    let blocks = vertical_alignment_blocks(dag, &oriented_layers, &conflicts, downward, spine);
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
    spine: &HashSet<NodeIndex>,
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
            let candidates = median_candidates_with_spine(&neighbors, dag, spine);
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

fn median_candidates_with_spine(
    neighbors: &[NodeIndex],
    layered_graph: &DiGraph<LayerNode, ()>,
    spine: &HashSet<NodeIndex>,
) -> Vec<NodeIndex> {
    let mut candidates = median_candidates(neighbors);
    candidates.sort_by_key(|node| {
        let on_spine = match &layered_graph[*node].kind {
            LayerNodeKind::Real(original) => spine.contains(original),
            LayerNodeKind::Dummy { .. } => false,
        };
        (!on_spine, node.index())
    });
    candidates
}

/// 从 source 沿最大出度贪心路径识别主干（spine）节点。
fn compute_spine_nodes(dag: &DiGraph<String, ()>) -> HashSet<NodeIndex> {
    let mut starts: Vec<NodeIndex> = dag
        .node_indices()
        .filter(|n| dag.neighbors_directed(*n, Direction::Incoming).count() == 0)
        .collect();
    starts.sort_by_key(|n| dag[*n].as_str());

    let Some(mut current) = starts.first().copied() else {
        return HashSet::new();
    };
    let mut spine = HashSet::from([current]);
    loop {
        let mut succs: Vec<NodeIndex> = dag.neighbors_directed(current, Direction::Outgoing).collect();
        succs.sort_by_key(|n| {
            let out = dag.neighbors_directed(*n, Direction::Outgoing).count();
            (std::cmp::Reverse(out), dag[*n].as_str())
        });
        let Some(&next) = succs.first() else {
            break;
        };
        if !spine.insert(next) {
            break;
        }
        current = next;
    }
    spine
}

/// BK 四趟后的受限紧凑化：向邻居重心靠拢，保持层内最小间距。
///
/// Iteration 3：spine 邻居在重心中权重 ×2，使主干更直。
fn compact_layer_centers(
    centers: &mut HashMap<NodeIndex, f64>,
    layered_graph: &DiGraph<LayerNode, ()>,
    layers: &[Vec<NodeIndex>],
    sizes: &HashMap<NodeIndex, (f64, f64)>,
    preset: &SugiyamaPreset,
    spine: &HashSet<NodeIndex>,
    passes: usize,
) {
    const DAMPING: f64 = 0.35;
    const SPINE_NEIGHBOR_WEIGHT: f64 = 1.5;
    let (default_w, _) = preset.default_node_size();

    for _ in 0..passes {
        for layer in layers {
            let mut nodes: Vec<NodeIndex> = layer
                .iter()
                .filter(|node| matches!(layered_graph[**node].kind, LayerNodeKind::Real(_)))
                .copied()
                .collect();
            nodes.sort_by(|a, b| {
                centers[a]
                    .partial_cmp(&centers[b])
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.index().cmp(&b.index()))
            });

            for node in &nodes {
                let neighbors: Vec<NodeIndex> = layered_graph
                    .neighbors_directed(*node, Direction::Incoming)
                    .chain(layered_graph.neighbors_directed(*node, Direction::Outgoing))
                    .filter(|n| matches!(layered_graph[*n].kind, LayerNodeKind::Real(_)))
                    .collect();
                if neighbors.is_empty() {
                    continue;
                }
                let mut weight_sum = 0.0;
                let mut weighted = 0.0;
                for n in &neighbors {
                    let on_spine = match &layered_graph[*n].kind {
                        LayerNodeKind::Real(original) => spine.contains(original),
                        LayerNodeKind::Dummy { .. } => false,
                    };
                    let w = if on_spine {
                        SPINE_NEIGHBOR_WEIGHT
                    } else {
                        1.0
                    };
                    weighted += centers[n] * w;
                    weight_sum += w;
                }
                let target = weighted / weight_sum;
                let current = centers[node];
                centers.insert(*node, current + (target - current) * DAMPING);
            }

            nodes.sort_by(|a, b| {
                centers[a]
                    .partial_cmp(&centers[b])
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.index().cmp(&b.index()))
            });
            for index in 1..nodes.len() {
                let left = nodes[index - 1];
                let right = nodes[index];
                let min_center = centers[&left]
                    + sizes.get(&left).map(|(w, _)| w / 2.0).unwrap_or(default_w / 2.0)
                    + sizes.get(&right).map(|(w, _)| w / 2.0).unwrap_or(default_w / 2.0)
                    + preset.node_gap;
                if centers[&right] < min_center {
                    centers.insert(right, min_center);
                }
            }
        }
    }
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
        // C5：lower > upper 时不要静默违反下界；保持 lower（层内分离优先）。
        if lower_bound.is_finite() && upper_bound.is_finite() && lower_bound > upper_bound {
            block_pos.insert(*root, lower_bound);
        } else if candidate.is_finite() && upper_bound.is_finite() {
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

/// 为 architecture 无 group 路径复用完整 BK 四趟坐标分配（字符串图层）。
///
/// `reversed`：FAS 反转边集；建边走 effective DAG，跨层边插 dummy 链（对齐 proper layer graph）。
pub(crate) fn assign_layer_centers_for_string_graph(
    layers: &[Vec<String>],
    sizes: &HashMap<String, (f64, f64)>,
    out_edges: &HashMap<String, Vec<String>>,
    reversed: &HashSet<(String, String)>,
    node_gap: f64,
    padding: f64,
) -> HashMap<String, f64> {
    let mut layered = DiGraph::<LayerNode, ()>::new();
    let mut node_idx: HashMap<String, NodeIndex> = HashMap::new();
    let mut layer_nodes: Vec<Vec<NodeIndex>> = Vec::with_capacity(layers.len());

    for (rank, layer) in layers.iter().enumerate() {
        let mut indices = Vec::with_capacity(layer.len());
        for id in layer {
            let idx = layered.add_node(LayerNode {
                kind: LayerNodeKind::Real(NodeIndex::new(0)),
                rank,
            });
            node_idx.insert(id.clone(), idx);
            indices.push(idx);
        }
        layer_nodes.push(indices);
    }

    for (idx, node) in layered.node_indices().enumerate() {
        layered[node].kind = LayerNodeKind::Real(NodeIndex::new(idx));
    }

    let id_to_layer: HashMap<String, usize> = layers
        .iter()
        .enumerate()
        .flat_map(|(li, layer)| layer.iter().map(move |id| (id.clone(), li)))
        .collect();

    // Effective DAG 边：反转边取反向；按 (src,dst) 去重
    let mut edge_set: HashSet<(String, String)> = HashSet::new();
    let mut from_ids: Vec<String> = out_edges.keys().cloned().collect();
    from_ids.sort();
    for from_id in &from_ids {
        let Some(succs) = out_edges.get(from_id) else {
            continue;
        };
        let mut succ_sorted = succs.clone();
        succ_sorted.sort();
        for to_id in succ_sorted {
            let (src, dst) = if reversed.contains(&(from_id.clone(), to_id.clone())) {
                (to_id, from_id.clone())
            } else {
                (from_id.clone(), to_id)
            };
            edge_set.insert((src, dst));
        }
    }
    let mut edge_keys: Vec<(String, String)> = edge_set.into_iter().collect();
    edge_keys.sort();

    let (dummy_w, dummy_h) = preset::FLOWCHART_PRESET.dummy_node_size();
    let mut sizes_idx: HashMap<NodeIndex, (f64, f64)> = HashMap::new();
    for (id, idx) in &node_idx {
        sizes_idx.insert(
            *idx,
            sizes
                .get(id)
                .copied()
                .unwrap_or((
                    preset::FLOWCHART_PRESET.default_node_width,
                    preset::FLOWCHART_PRESET.default_node_height,
                )),
        );
    }

    for (from_id, to_id) in edge_keys {
        let Some(&from_layer) = id_to_layer.get(&from_id) else {
            continue;
        };
        let Some(&to_layer) = id_to_layer.get(&to_id) else {
            continue;
        };
        if to_layer <= from_layer {
            continue;
        }
        let Some(&from_idx) = node_idx.get(&from_id) else {
            continue;
        };
        let Some(&to_idx) = node_idx.get(&to_id) else {
            continue;
        };

        if to_layer == from_layer + 1 {
            layered.add_edge(from_idx, to_idx, ());
            continue;
        }

        // 跨层：插 dummy 链
        let mut prev = from_idx;
        for rank in (from_layer + 1)..to_layer {
            let dummy = layered.add_node(LayerNode {
                kind: LayerNodeKind::Dummy {
                    source: NodeIndex::new(from_idx.index()),
                    target: NodeIndex::new(to_idx.index()),
                    segment: rank - from_layer,
                },
                rank,
            });
            if rank < layer_nodes.len() {
                layer_nodes[rank].push(dummy);
            }
            sizes_idx.insert(dummy, (dummy_w, dummy_h));
            layered.add_edge(prev, dummy, ());
            prev = dummy;
        }
        layered.add_edge(prev, to_idx, ());
    }

    let preset = SugiyamaPreset {
        node_gap,
        padding,
        ..preset::FLOWCHART_PRESET
    };
    let spine = HashSet::new();
    let centers = assign_layer_centers_brandes_koepf(&layered, &layer_nodes, &sizes_idx, &preset, &spine);

    node_idx
        .into_iter()
        .filter_map(|(id, idx)| centers.get(&idx).map(|cx| (id, *cx)))
        .collect()
}
