use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::Direction;
use std::collections::{HashMap, HashSet};

use super::graph::{LayerNode, LayerNodeKind};
use super::order;
use super::postprocess;
use super::preset::{self, SugiyamaPreset};

use crate::layout::kernel::coordinate::auditor::audit_p0;
use crate::layout::kernel::coordinate::builder_legacy::build_with_mapping;
use crate::layout::kernel::coordinate::objectives::{build_basic_objectives, ObjectiveWeights};
use crate::layout::kernel::coordinate::structure_objectives::{build_structure_objectives, StructureWeights};
use crate::layout::kernel::coordinate::optimizer::solve;

pub(in crate::layout) fn assign_coordinates_brandes_koepf(
    dag: &DiGraph<String, ()>,
    layered_graph: &DiGraph<LayerNode, ()>,
    layers: &[Vec<NodeIndex>],
    sizes: &HashMap<NodeIndex, (f64, f64)>,
    horizontal: bool,
    preset: &SugiyamaPreset,
    layer_gaps: &[f64],
    _has_same_layer_edges: bool,
    end_ids: &[String],
) -> (HashMap<String, crate::layout::NodeLayout>, Option<crate::layout::kernel::coordinate::model::CoordinateProblem>) {
    assign_coordinates_brandes_koepf_with_main_tops(
        dag,
        layered_graph,
        layers,
        sizes,
        horizontal,
        preset,
        layer_gaps,
        _has_same_layer_edges,
        end_ids,
        None,
        false,
    )
}

/// 与 [`assign_coordinates_brandes_koepf`] 相同，但可用 Main 轴 LP 产出的层顶替换启发式 Y 堆叠。
///
/// `emit_canonical`：true 时跳过画布轴转置，恒输出规范空间（rank=Y）；LTR 由管线末端
/// [`crate::layout::orientation::apply_layout_orientation`] 统一处理（Atlas M5）。
pub(in crate::layout) fn assign_coordinates_brandes_koepf_with_main_tops(
    dag: &DiGraph<String, ()>,
    layered_graph: &DiGraph<LayerNode, ()>,
    layers: &[Vec<NodeIndex>],
    sizes: &HashMap<NodeIndex, (f64, f64)>,
    horizontal: bool,
    preset: &SugiyamaPreset,
    layer_gaps: &[f64],
    _has_same_layer_edges: bool,
    end_ids: &[String],
    layer_tops: Option<&[f64]>,
    emit_canonical: bool,
) -> (HashMap<String, crate::layout::NodeLayout>, Option<crate::layout::kernel::coordinate::model::CoordinateProblem>) {
    let spine = compute_spine_nodes(dag);
    let mut centers =
        assign_layer_centers_brandes_koepf(layered_graph, layers, sizes, preset, &spine);
    // BK 后受限紧凑化（无 spine 加权，spine 概念已由 solver region axis 替代）
    compact_layer_centers(&mut centers, layered_graph, layers, sizes, preset, 3);

    // Phase 3+4: 统一坐标求解器——在 BK + compact + fan_symmetry 之后运行 optimizer
    let solved_problem = {
        let build_output = build_with_mapping(
            layered_graph, layers, sizes, &centers, preset, horizontal,
        );
        let mut problem = build_output.problem;

        // 基础 objectives（P2: 边拉直 + dummy 共线，P3: BK 位置保持）
        let mut objectives = build_basic_objectives(
            layered_graph,
            &build_output.node_to_var,
            &problem.initial.values,
            &ObjectiveWeights::default(),
        );

        // 结构 objectives（P1: end 跟随 + pendant 对齐，P2: singleton 对齐）
        let structure_objs = build_structure_objectives(
            dag,
            layered_graph,
            layers,
            &build_output.node_to_var,
            &problem.initial.values,
            end_ids,
            &StructureWeights::default(),
        );
        objectives.extend(structure_objs);
        problem.objectives = objectives;

        let result = solve(&problem);

        // Phase 8: P0 审计——验证硬约束满足
        let audit = audit_p0(&problem, &result.coordinates);
        if !audit.passed() {
            crate::perf_log!(
                "[solver] P0 audit FAILED: {} violations, max={:.2}px",
                audit.separation_violations,
                audit.max_violation
            );
        }

        // 用优化结果更新 centers（仅 Real 节点）
        for (node, &var_id) in &build_output.node_to_var {
            if matches!(&layered_graph[*node].kind, LayerNodeKind::Real(_)) {
                centers.insert(*node, result.coordinates[var_id]);
            }
        }
        Some(problem)
    };

    let mut nodes = HashMap::new();
    let (default_w, default_h) = preset.default_node_size();
    let layer_heights = postprocess::compute_layer_heights(layers, sizes, preset);
    let mut layer_offsets = vec![preset.padding; layers.len()];
    if let Some(tops) = layer_tops {
        for (i, &top) in tops.iter().enumerate().take(layers.len()) {
            layer_offsets[i] = top;
        }
        // 若 tops 短于层数，余下层用启发式续推
        for layer_index in tops.len().max(1)..layers.len() {
            let gap = layer_gaps
                .get(layer_index - 1)
                .copied()
                .unwrap_or(preset.layer_gap);
            layer_offsets[layer_index] =
                layer_offsets[layer_index - 1] + layer_heights[layer_index - 1] + gap;
        }
    } else {
        for layer_index in 1..layers.len() {
            let gap = layer_gaps
                .get(layer_index - 1)
                .copied()
                .unwrap_or(preset.layer_gap);
            layer_offsets[layer_index] =
                layer_offsets[layer_index - 1] + layer_heights[layer_index - 1] + gap;
        }
    }

    for (layer_index, layer) in layers.iter().enumerate() {
        for node in layer {
            let (width, height) = sizes.get(node).copied().unwrap_or((default_w, default_h));
            let x = centers[node];
            let center_y = layer_offsets[layer_index] + layer_heights[layer_index] / 2.0;
            let LayerNodeKind::Real(original_node) = layered_graph[*node].kind.clone() else {
                continue;
            };
            // M5：emit_canonical 时恒规范空间；recipe 仍可用 horizontal 画布转置
            let layout = if horizontal && !emit_canonical {
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

    // Phase 6: solver 是唯一相对坐标写者，无后处理 pass
    postprocess::normalize_layout_to_padding(&mut nodes, preset.padding);
    (nodes, solved_problem)
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
fn compact_layer_centers(
    centers: &mut HashMap<NodeIndex, f64>,
    layered_graph: &DiGraph<LayerNode, ()>,
    layers: &[Vec<NodeIndex>],
    sizes: &HashMap<NodeIndex, (f64, f64)>,
    preset: &SugiyamaPreset,
    passes: usize,
) {
    const DAMPING: f64 = 0.35;
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
                let weight_sum = neighbors.len() as f64;
                let weighted: f64 = neighbors.iter().map(|n| centers[n]).sum();
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


