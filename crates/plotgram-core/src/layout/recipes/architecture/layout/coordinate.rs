//! Phase 4: 坐标分配与邻接对齐。

use crate::layout::constants;
use crate::layout::{NodeLayout};
use std::collections::{HashMap, HashSet};

use super::acyclic::is_effective_edge;
use super::constants::{LAYER_GAP, NEIGHBOR_ALIGN_MAX_PASSES, NODE_GAP, PADDING};
use super::types::{ArchDiagramFacts, GraphIndex, GroupMap};
use crate::layout::engines::layered::coordinate::assign_layer_centers_for_string_graph;

pub(in super::super) fn assign_coordinates(
    facts: &ArchDiagramFacts,
    graph: &GraphIndex,
    group_map: &GroupMap,
    layers: &[Vec<String>],
    sizes: &HashMap<String, (f64, f64)>,
    reversed: &HashSet<(String, String)>,
) -> (HashMap<String, NodeLayout>, Option<crate::layout::kernel::coordinate::model::CoordinateProblem>) {
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

    // S2：邻层边带需求抬高 layer gap（路由前写权）
    // 无组 architecture：可读余量更高，但仍按 demand 封顶——边少时不抬缝
    let parallel_gap =
        crate::layout::routing::segment_pair::parallel_gap_for_diagram(facts.diagram_type.clone());
    let has_groups = facts.has_groups;
    let profile = crate::layout::demand::band::EdgeBandDemandProfile::for_diagram(
        facts.diagram_type.clone(),
        has_groups,
    );
    let per_layer_gaps = crate::layout::demand::band::layer_gaps_from_demand(
        layers,
        &facts.relations,
        LAYER_GAP,
        parallel_gap,
        profile,
    );

    // S4：无分组时侧通道水平 gutter（L/R 外廊）；竖向仍用 PADDING，避免层缝诊断虚高
    let side_gutter = if !has_groups {
        crate::layout::demand::band::side_channel_gutter(
            &facts.relations,
            parallel_gap,
            profile,
        )
    } else {
        0.0
    };
    let h_pad = PADDING + side_gutter;

    // 计算每层的 y 偏移
    let mut layer_y_offsets = vec![PADDING];
    for i in 1..layers.len() {
        let gap = per_layer_gaps.get(i - 1).copied().unwrap_or(LAYER_GAP);
        layer_y_offsets.push(layer_y_offsets[i - 1] + layer_heights[i - 1] + gap);
    }

    // 完整 BK 四趟：effective DAG + 跨层 dummy
    let bk_centers = assign_layer_centers_for_string_graph(
        layers,
        sizes,
        &graph.out_edges,
        reversed,
        NODE_GAP,
        h_pad,
    );

    // Phase A1: 使用 CoordinateSolver 替代 resolve_x_overlaps
    let build_output = super::super::arch_builder::build_arch_coordinate_problem(
        layers,
        sizes,
        &bk_centers,
        graph,
        reversed,
        &facts.relations,
        &facts.diagram_type,
        has_groups,
        group_map,
    );
    let solver_result = crate::layout::kernel::coordinator::CoordinateKernel::solve(
        "architecture",
        &build_output.problem,
    );
    let solved_problem = Some(build_output.problem);

    // 从 solver 结果提取中心坐标
    let solved_centers: HashMap<String, f64> = build_output
        .node_to_var
        .iter()
        .map(|(node_id, &var_id)| (node_id.clone(), solver_result.coordinates[var_id]))
        .collect();

    // 对每层分配坐标（使用 solver 结果）
    for (layer_idx, layer) in layers.iter().enumerate() {
        let y_center = layer_y_offsets[layer_idx] + layer_heights[layer_idx] / 2.0;

        for node in layer {
            let (width, height) = sizes
                .get(node)
                .copied()
                .unwrap_or((constants::DEFAULT_NODE_WIDTH, constants::DEFAULT_NODE_HEIGHT));
            let x_center = solved_centers.get(node).copied().unwrap_or_else(|| {
                bk_centers.get(node).copied().unwrap_or(0.0)
            });

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

    // 基础设施层居中（L4: 已由 arch_builder Phase I objective 替代）
    // 保留作为安全网：若 objective 未完全收敛，此处做最终修正。
    // 待 objective 稳定后可删除。
    for (layer_idx, layer) in layers.iter().enumerate() {
        if is_infrastructure_layer(layer, group_map) {
            if let Some(anchor_x) = infrastructure_anchor_x(layer, graph, &nodes, reversed) {
                let mut centers: Vec<f64> = layer
                    .iter()
                    .map(|node| node_center_x(node, &nodes))
                    .collect();
                center_layer_on_anchor(layer, &mut centers, sizes, anchor_x);
                for (node, cx) in layer.iter().zip(centers.iter()) {
                    if let Some(nl) = nodes.get_mut(node) {
                        nl.x = cx - nl.width / 2.0;
                    }
                }
            }
        }
    }

    (nodes, solved_problem)
}

/// 同层 X 消重叠：无组可读走廊用 `adjacent_rank_gap`，否则 NODE_GAP。
fn resolve_layer_x_gaps(
    layer: &[String],
    positions: &[f64],
    sizes: &HashMap<String, (f64, f64)>,
    relations: &[crate::ast::Relation],
    parallel_gap: f64,
    profile: crate::layout::demand::band::EdgeBandDemandProfile,
) -> Vec<f64> {
    if profile.horizontal_max_extra <= 0.0 {
        return resolve_x_overlaps(layer, positions, sizes);
    }
    let layer_ids: HashSet<&str> = layer.iter().map(|s| s.as_str()).collect();
    resolve_x_overlaps_with_gaps(layer, positions, sizes, |a, b| {
        crate::layout::demand::band::adjacent_rank_gap(
            a,
            b,
            &layer_ids,
            relations,
            NODE_GAP,
            parallel_gap,
            profile,
        )
    })
}

/// 邻接对齐 / 重叠消除后重申水平 demand 缝（否则会被 `resolve_x_overlaps(NODE_GAP)` 压回）。
pub(in super::super) fn enforce_horizontal_demand_gaps(
    facts: &ArchDiagramFacts,
    layers: &[Vec<String>],
    sizes: &HashMap<String, (f64, f64)>,
    nodes: &mut HashMap<String, NodeLayout>,
) {
    let profile = crate::layout::demand::band::EdgeBandDemandProfile::for_diagram(
        facts.diagram_type.clone(),
        facts.has_groups,
    );
    if profile.horizontal_max_extra <= 0.0 {
        return;
    }
    let parallel_gap =
        crate::layout::routing::segment_pair::parallel_gap_for_diagram(facts.diagram_type.clone());
    for layer in layers {
        if layer.len() < 2 {
            continue;
        }
        let centers: Vec<f64> = layer.iter().map(|n| node_center_x(n, nodes)).collect();
        let resolved = resolve_layer_x_gaps(
            layer,
            &centers,
            sizes,
            &facts.relations,
            parallel_gap,
            profile,
        );
        for (node, cx) in layer.iter().zip(resolved.iter()) {
            if let Some(nl) = nodes.get_mut(node) {
                nl.x = cx - nl.width / 2.0;
            }
        }
    }
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
    reversed: &HashSet<(String, String)>,
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
                            is_effective_edge(hub, s, reversed)
                                && lower_set.contains(*s)
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
    reversed: &HashSet<(String, String)>,
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
                            is_effective_edge(node, s, reversed)
                                && lower_set.contains(*s)
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

        let mut hub_keys: Vec<i64> = by_hub.keys().copied().collect();
        hub_keys.sort_unstable();
        for hub_key in hub_keys {
            let Some(mut indices) = by_hub.remove(&hub_key) else {
                continue;
            };
            if indices.is_empty() {
                continue;
            }
            indices.sort_by(|&a, &b| {
                centers[a]
                    .partial_cmp(&centers[b])
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.cmp(&b))
            });

            let hub_cx = hub_targets[indices[0]].unwrap();
            // V3b-A：任意数量客户端绕 hub 对称放置，组质心对齐 hub（含 2 叶，不再左贴右甩）
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
            // 不因 padding 右推整组：否则组质心会偏离 hub（曾出现稳定的 8px 偏差）。
            // 画布左侧留白由后续 normalize / finalize 统一处理。

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
/// 邻居必须是 effective DAG 边（过滤 FAS 反转边）。
pub(in super::super) fn pull_toward_neighbors(
    layer: &[String],
    positions: &mut [f64],
    neighbor_x: &HashMap<String, f64>,
    graph: &GraphIndex,
    reversed: &HashSet<(String, String)>,
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
            .filter(|n| {
                if from_upper {
                    is_effective_edge(n, node, reversed)
                } else {
                    is_effective_edge(node, n, reversed)
                }
            })
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

pub(in super::super) fn resolve_x_overlaps(
    layer: &[String],
    positions: &[f64],
    sizes: &HashMap<String, (f64, f64)>,
) -> Vec<f64> {
    resolve_x_overlaps_with_gaps(layer, positions, sizes, |_, _| NODE_GAP)
}

/// 同层 X 重叠消除，间距由 `gap_between(left_id, right_id)` 提供（空间契约）。
pub(in super::super) fn resolve_x_overlaps_with_gaps<F>(
    layer: &[String],
    positions: &[f64],
    sizes: &HashMap<String, (f64, f64)>,
    gap_between: F,
) -> Vec<f64>
where
    F: Fn(&str, &str) -> f64,
{
    let n = layer.len();
    if n <= 1 {
        return positions.to_vec();
    }

    let mut adjusted = positions.to_vec();

    for i in 1..n {
        let prev_width = sizes.get(&layer[i - 1]).map(|(w, _)| *w).unwrap_or(constants::DEFAULT_NODE_WIDTH);
        let curr_width = sizes.get(&layer[i]).map(|(w, _)| *w).unwrap_or(constants::DEFAULT_NODE_WIDTH);
        let gap = gap_between(&layer[i - 1], &layer[i]);
        let min_center = adjusted[i - 1] + prev_width / 2.0 + gap + curr_width / 2.0;
        if adjusted[i] < min_center {
            adjusted[i] = min_center;
        }
    }

    for i in (0..n.saturating_sub(1)).rev() {
        let next_width = sizes.get(&layer[i + 1]).map(|(w, _)| *w).unwrap_or(constants::DEFAULT_NODE_WIDTH);
        let curr_width = sizes.get(&layer[i]).map(|(w, _)| *w).unwrap_or(constants::DEFAULT_NODE_WIDTH);
        let gap = gap_between(&layer[i], &layer[i + 1]);
        let max_center = adjusted[i + 1] - next_width / 2.0 - gap - curr_width / 2.0;
        if adjusted[i] > max_center {
            adjusted[i] = max_center;
        }
    }

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
    reversed: &HashSet<(String, String)>,
) -> Option<f64> {
    let mut xs = Vec::new();
    for node in layer {
        // 上游：in_edges 中已放置且 effective 的节点
        if let Some(preds) = graph.in_edges.get(node) {
            for pred in preds {
                if !is_effective_edge(pred, node, reversed) {
                    continue;
                }
                if let Some(nl) = placed.get(pred) {
                    xs.push(nl.x + nl.width / 2.0);
                }
            }
        }
        // 下游：out_edges 中已放置且 effective 的节点
        if let Some(succs) = graph.out_edges.get(node) {
            for succ in succs {
                if !is_effective_edge(node, succ, reversed) {
                    continue;
                }
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
    facts: &ArchDiagramFacts,
    graph: &GraphIndex,
    group_map: &GroupMap,
    layers: &[Vec<String>],
    sizes: &HashMap<String, (f64, f64)>,
    nodes: &mut HashMap<String, NodeLayout>,
    reversed: &HashSet<(String, String)>,
) {
    let parallel_gap =
        crate::layout::routing::segment_pair::parallel_gap_for_diagram(facts.diagram_type.clone());
    let profile = crate::layout::demand::band::EdgeBandDemandProfile::for_diagram(
        facts.diagram_type.clone(),
        facts.has_groups,
    );
    for layer in layers {
        if !is_infrastructure_layer(layer, group_map) {
            continue;
        }
        let Some(anchor_x) = infrastructure_anchor_x(layer, graph, nodes, reversed) else {
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
        // 与 assign_coordinates 一致：居中后再按 demand 缝消重叠
        centers = resolve_layer_x_gaps(
            layer,
            &centers,
            sizes,
            &facts.relations,
            parallel_gap,
            profile,
        );

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
    reversed: &HashSet<(String, String)>,
) {
    let mut layer_of: HashMap<&str, usize> = HashMap::new();
    for (li, layer) in layers.iter().enumerate() {
        for node in layer {
            layer_of.insert(node.as_str(), li);
        }
    }

    for _pass in 0..NEIGHBOR_ALIGN_MAX_PASSES {
        let mut any_moved = false;

        for layer in layers {
            // 跳过组内节点（由 group 布局管理）
            if !layer.is_empty() && layer.iter().all(|n| group_map.node_to_top_group.contains_key(n)) {
                continue;
            }

            // 收集本层各节点的邻接目标 center_x（仅 effective + 邻层）
            let mut targets: Vec<Option<f64>> = Vec::with_capacity(layer.len());
            for node in layer {
                let Some(&node_layer) = layer_of.get(node.as_str()) else {
                    targets.push(None);
                    continue;
                };
                let mut xs: Vec<f64> = Vec::new();
                if let Some(preds) = graph.in_edges.get(node) {
                    for pred in preds {
                        if !is_effective_edge(pred, node, reversed) {
                            continue;
                        }
                        let Some(&pred_layer) = layer_of.get(pred.as_str()) else {
                            continue;
                        };
                        if pred_layer + 1 != node_layer {
                            continue;
                        }
                        if let Some(nl) = nodes.get(pred) {
                            xs.push(nl.x + nl.width / 2.0);
                        }
                    }
                }
                if let Some(succs) = graph.out_edges.get(node) {
                    for succ in succs {
                        if !is_effective_edge(node, succ, reversed) {
                            continue;
                        }
                        let Some(&succ_layer) = layer_of.get(succ.as_str()) else {
                            continue;
                        };
                        if succ_layer != node_layer + 1 {
                            continue;
                        }
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

/// 计算 architecture 布局专用的中位数。
///
/// **注意**:此实现与 `crate::layout::engines::common::stats::median_f64` 有意不同:
/// - 此处对输入内部排序(调用方传入无序的 in/out 邻居 center_x 混合切片)
/// - 此处对偶数长度返回 `sorted[len/2]`(上中位元素),而非算术平均
///
/// 保持此差异是为了零行为变更;如需统一,必须先验证偶数长度输入下的渲染结果。
fn median_f64(xs: &[f64]) -> f64 {
    let mut sorted = xs.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    sorted[sorted.len() / 2]
}

