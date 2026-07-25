//! Phase 4: 坐标分配与邻接对齐。

use crate::layout::constants;
use crate::layout::{NodeLayout};
use std::collections::{HashMap, HashSet};

use super::acyclic::is_effective_edge;
use super::constants::{LAYER_GAP, NODE_GAP, PADDING};
use super::types::{ArchDiagramFacts, GraphIndex, GroupMap};
use crate::layout::kernel::layered::coordinate::assign_layer_centers_for_string_graph;

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

    // 计算每层的 y 偏移（Phase 5：Main 轴二次求解，替代纯启发式堆叠）
    let layer_y_offsets = crate::layout::kernel::coordinate::main_axis::solve_main_axis_layer_tops(
        &layer_heights,
        &per_layer_gaps,
        PADDING,
        LAYER_GAP,
    );

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
        None, // hierarchical 走 two_phase；flat 无组时 diagram 可选
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

fn node_center_x(node: &str, nodes: &HashMap<String, NodeLayout>) -> f64 {
    nodes
        .get(node)
        .map(|nl| nl.x + nl.width / 2.0)
        .unwrap_or(0.0)
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
