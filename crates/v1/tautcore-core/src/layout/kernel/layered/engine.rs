//! Sugiyama v2 共享布局引擎。

use crate::ast::Diagram;
use crate::layout::algorithm_config::SugiyamaLayoutConfig;
use crate::layout::kernel::group::bounds::{self as group_bounds, GroupPadding};
use crate::layout::{LayoutResult, EdgeRoutingStyle};
use petgraph::graph::NodeIndex;
use std::collections::{HashMap, HashSet};

use super::preset::SugiyamaPreset;
use super::coordinate;

/// 同层边：rank 相同，order 阶段偏置（hub 排主前驱左侧），路由阶段消费。
pub(super) struct SameLayerEdge {
    /// 主前驱（DAG 节点索引）
    pub from: NodeIndex,
    /// feedback hub（DAG 节点索引）
    pub to: NodeIndex,
}

/// 密度感知间距：每条跨层边为 layer_gap 额外增加的像素
const DENSITY_LAYER_GAP_SCALE: f64 = 2.0;
/// 密度感知间距：layer_gap 额外增加的上限（Phase C：40 → 20）
const DENSITY_MAX_EXTRA_LAYER_GAP: f64 = 20.0;
/// 密度感知间距：层内平均度数每超 1.0 为 node_gap 额外增加的像素
const DENSITY_NODE_GAP_SCALE: f64 = 8.0;
/// 密度感知间距：node_gap 额外增加的上限（Phase C：32 → 20）
const DENSITY_MAX_EXTRA_NODE_GAP: f64 = 20.0;
/// 触发密度感知的层内平均度数阈值
const DENSITY_DEGREE_THRESHOLD: f64 = 2.0;

pub fn compute_with_preset(
    diagram: &Diagram,
    preset: &SugiyamaPreset,
    layout_config: SugiyamaLayoutConfig,
) -> LayoutResult {
    compute_with_preset_inner(diagram, preset, layout_config, false)
}

/// Atlas M5：子布局恒输出规范空间（rank=Y）；LTR 由管线末端 orientation 统一处理。
pub fn compute_with_preset_canonical(
    diagram: &Diagram,
    preset: &SugiyamaPreset,
    layout_config: SugiyamaLayoutConfig,
) -> LayoutResult {
    compute_with_preset_inner(diagram, preset, layout_config, true)
}

fn compute_with_preset_inner(
    diagram: &Diagram,
    preset: &SugiyamaPreset,
    layout_config: SugiyamaLayoutConfig,
    emit_canonical: bool,
) -> LayoutResult {
    if diagram.entities.is_empty() {
        return LayoutResult {
            nodes: HashMap::new(),
            groups: crate::layout::GroupTable::new(),
            edges: vec![],
            total_width: preset.padding * 2.0,
            total_height: preset.padding * 2.0,
            hints: Default::default(),
        };
    }

    // compile: LayeredKernel (Step 1-7)
    let draft = super::layered_kernel::LayeredKernel::compute(diagram, preset);

    // solve: CoordinateKernel (Step 8)
    // Atlas canonical：强制 TB + emit_canonical，忽略 draft.horizontal
    let (nodes, solved_problem) = if emit_canonical {
        coordinate::assign_coordinates_brandes_koepf_with_main_tops(
            &draft.dag,
            &draft.proper_graph,
            &draft.layers,
            &draft.sizes,
            false,
            &draft.preset,
            &draft.per_layer_gaps,
            draft.has_order_bias,
            &draft.end_ids,
            None,
            true,
        )
    } else {
        coordinate::assign_coordinates_brandes_koepf(
            &draft.dag,
            &draft.proper_graph,
            &draft.layers,
            &draft.sizes,
            draft.horizontal,
            &draft.preset,
            &draft.per_layer_gaps,
            draft.has_order_bias,
            &draft.end_ids,
        )
    };

    // product: 组装 LayoutResult (Step 9-10)
    let groups = group_bounds::compute_group_bounds(
        diagram,
        &nodes,
        GroupPadding::uniform(layout_config.group_padding, 16.0),
    );
    let group_warnings = group_bounds::detect_group_layout_warnings(diagram, &nodes, &groups);
    let (total_width, total_height) = crate::layout::kernel::common::canvas_bounds::canvas_size(
        &nodes,
        &groups,
        draft.padding,
    );

    let mut result = LayoutResult {
        nodes,
        groups: groups.into(),
        edges: vec![],
        total_width,
        total_height,
        hints: crate::layout::LayoutHints {
            edge_routing_style: EdgeRoutingStyle::Orthogonal,
            sugiyama_ranks: Some(draft.sugiyama_ranks),
            group_layout_warnings: group_warnings,
            same_layer_edges: draft.same_layer_edges,
            feedback_hubs: draft.feedback_hubs,
            coordinate_problem: solved_problem.map(Box::new),
            ..Default::default()
        },
    };

    if let Some(finish) = draft.preset.finish_layout {
        finish(&mut result, &draft.preset);
    }

    result
}

/// 密度感知间距：根据图密度动态调整 preset 的 node_gap
///
/// - **node_gap**：按**最密集层**的平均度数放大（取代旧版全局平均度数），
///   度数高说明同层节点连接复杂，需要更大同层间距避免端口拥挤。
///
/// `layer_gap` 的密度感知已迁移到 [`compute_per_layer_gaps`]，按每个层间边界
/// 单独评估，稀疏层不被无谓拉大。
///
/// 调整在 preset 副本上进行，不修改原 const preset。
pub(super) fn apply_density_aware_spacing(
    dag: &petgraph::graph::DiGraph<String, ()>,
    ranks: &HashMap<petgraph::graph::NodeIndex, usize>,
    mut preset: SugiyamaPreset,
) -> SugiyamaPreset {
    use petgraph::visit::EdgeRef;

    // 统计每层节点数
    let max_rank = ranks.values().copied().max().unwrap_or(0);
    let mut layer_sizes: Vec<usize> = vec![0; max_rank + 1];
    for &r in ranks.values() {
        if r <= max_rank {
            layer_sizes[r] += 1;
        }
    }

    // 逐层度数：统计每层节点的总度数（入+出），取最密集层的平均度数
    // 取代旧版全局平均度数，使稀疏图的整体 node_gap 不被个别密集层拉大，
    // 同时保证最密集层有足够间距。
    let mut layer_degree: Vec<usize> = vec![0; max_rank + 1];
    for edge in dag.edge_references() {
        let (from, to) = (edge.source(), edge.target());
        let from_rank = ranks.get(&from).copied().unwrap_or(0);
        let to_rank = ranks.get(&to).copied().unwrap_or(0);
        if from_rank <= max_rank {
            layer_degree[from_rank] += 1;
        }
        if to_rank <= max_rank {
            layer_degree[to_rank] += 1;
        }
    }
    let max_avg_degree = (0..=max_rank)
        .map(|r| {
            let n = layer_sizes[r].max(1) as f64;
            layer_degree[r] as f64 / n
        })
        .fold(0.0_f64, f64::max);

    // 放大 node_gap（仅当最密集层平均度数超过阈值时）
    if max_avg_degree > DENSITY_DEGREE_THRESHOLD {
        let extra_node_gap =
            ((max_avg_degree - DENSITY_DEGREE_THRESHOLD) * DENSITY_NODE_GAP_SCALE)
                .min(DENSITY_MAX_EXTRA_NODE_GAP);
        preset.node_gap += extra_node_gap;
    }

    preset
}

/// 逐层密度感知：为每个层间边界（gap between rank i and i+1）单独计算 layer_gap。
///
/// 对每个 gap，统计跨越该 gap 的边数（含长边拆分后的每段 dummy），
/// 跨越边越多 → 该 gap 越大，为正交路由预留通道。稀疏 gap 保持 base 不变。
///
/// 返回长度为 `max_rank` 的向量（gap[i] 用于 rank i 与 i+1 之间）。
pub(super) fn compute_per_layer_gaps(
    dag: &petgraph::graph::DiGraph<String, ()>,
    ranks: &HashMap<petgraph::graph::NodeIndex, usize>,
    base_layer_gap: f64,
) -> Vec<f64> {
    use petgraph::visit::EdgeRef;

    let max_rank = ranks.values().copied().max().unwrap_or(0);
    if max_rank == 0 {
        return Vec::new();
    }

    // 统计每个 gap 被多少条边跨越：边 (u,v) with rank(u)=a, rank(v)=b (a<b)
    // 跨越 gap a, a+1, ..., b-1，每段对应一个 dummy segment。
    let mut gap_load: Vec<usize> = vec![0; max_rank];
    for edge in dag.edge_references() {
        let (from, to) = (edge.source(), edge.target());
        let from_rank = ranks.get(&from).copied().unwrap_or(0) as i32;
        let to_rank = ranks.get(&to).copied().unwrap_or(0) as i32;
        if to_rank <= from_rank {
            continue;
        }
        // 跨越 gap from_rank..to_rank-1
        for g in from_rank..to_rank {
            if (g as usize) < max_rank {
                gap_load[g as usize] += 1;
            }
        }
    }

    gap_load
        .iter()
        .map(|&load| {
            let extra = (load as f64 * DENSITY_LAYER_GAP_SCALE).min(DENSITY_MAX_EXTRA_LAYER_GAP);
            base_layer_gap + extra
        })
        .collect()
}

/// group 感知的 rank 重分配：为每个 group 分配不重叠的 rank 窗口。
///
/// 在正常分层后调用，按 group 的 min_rank 排序 group，为每个 group 分配
/// 不重叠的 rank 窗口。group 内节点保持原始 rank 的相对顺序，整体平移到新窗口。
/// 无 group 节点保持原 rank（不参与重分配）。
///
/// 这确保不同 group 的节点不会交错排列，从而消除 group 包围框在
/// 分层方向（y 或 x）上的重叠。
///
/// **确定性**：group 按 (min_rank, group_id) 排序，group 内节点按
/// (orig_rank, node_index) 排序，避免 HashMap 迭代顺序影响结果。
pub(super) fn apply_group_rank_constraints(
    dag: &petgraph::graph::DiGraph<String, ()>,
    ranks: &mut HashMap<petgraph::graph::NodeIndex, usize>,
    diagram: &Diagram,
) {
    let node_to_group =
        crate::layout::kernel::common::group_map::build_node_to_top_group(diagram);

    // 收集每个 group 的节点
    let mut group_nodes: HashMap<String, Vec<petgraph::graph::NodeIndex>> = HashMap::new();
    for node in dag.node_indices() {
        let entity_id = &dag[node];
        if let Some(gid) = node_to_group.get(entity_id) {
            group_nodes.entry(gid.clone()).or_default().push(node);
        }
    }

    if group_nodes.is_empty() {
        return;
    }

    // 计算每个 group 的 rank 范围 [min, max]
    let group_ranges: HashMap<String, (usize, usize)> = group_nodes
        .iter()
        .map(|(gid, nodes)| {
            let min = nodes.iter().map(|n| ranks[n]).min().unwrap();
            let max = nodes.iter().map(|n| ranks[n]).max().unwrap();
            (gid.clone(), (min, max))
        })
        .collect();

    // 按 (min_rank, group_id) 排序 group，保证确定性
    let mut sorted_groups: Vec<String> = group_nodes.keys().cloned().collect();
    sorted_groups.sort_by(|a, b| {
        group_ranges[a]
            .0
            .cmp(&group_ranges[b].0)
            .then_with(|| a.cmp(b))
    });

    // 为每个 group 分配不重叠的 rank 窗口
    let mut current_rank = 0;
    for gid in &sorted_groups {
        let nodes = &group_nodes[gid];
        let (orig_min, orig_max) = group_ranges[gid];
        let orig_span = orig_max - orig_min + 1;
        let window_size = orig_span.max(nodes.len());

        // group 内节点按 (orig_rank, node_index) 排序，保证确定性
        let mut sorted_nodes = nodes.clone();
        sorted_nodes.sort_by_key(|n| (ranks[n], n.index()));

        // 重新分配 rank：保持相对顺序，整体平移到新窗口
        for (i, node) in sorted_nodes.iter().enumerate() {
            ranks.insert(*node, current_rank + i);
        }

        // +1 为 group 间留空（dummy 链填充）
        current_rank += window_size + 1;
    }

    // L6：ungrouped 节点 rank 夹取到与邻居一致的合法区间，避免跨 group 窗口倒挂。
    let grouped: std::collections::HashSet<petgraph::graph::NodeIndex> = group_nodes
        .values()
        .flat_map(|v| v.iter().copied())
        .collect();
    let mut ungrouped: Vec<_> = dag
        .node_indices()
        .filter(|n| !grouped.contains(n))
        .collect();
    ungrouped.sort_by_key(|n| n.index());
    for node in ungrouped {
        use petgraph::Direction;
        let mut lo = 0usize;
        let mut hi = usize::MAX;
        let mut preds: Vec<_> = dag.neighbors_directed(node, Direction::Incoming).collect();
        preds.sort_by_key(|n| n.index());
        for p in preds {
            lo = lo.max(ranks[&p].saturating_add(1));
        }
        let mut succs: Vec<_> = dag.neighbors_directed(node, Direction::Outgoing).collect();
        succs.sort_by_key(|n| n.index());
        for s in succs {
            hi = hi.min(ranks[&s].saturating_sub(1));
        }
        let cur = ranks[&node];
        if lo <= hi {
            ranks.insert(node, cur.clamp(lo, hi));
        } else {
            // 上下界冲突：优先满足前驱（lower），下游靠后续 layering/dummy 处理
            ranks.insert(node, lo);
        }
    }
}

/// 状态图语义 rank 约束：initial → rank 0，final → max rank。
///
/// L5：硬覆盖后传播修复违反 `rank(u) < rank(v)` 的边。
pub(super) fn apply_state_semantic_rank_constraints(
    dag: &petgraph::graph::DiGraph<String, ()>,
    ranks: &mut HashMap<petgraph::graph::NodeIndex, usize>,
    diagram: &Diagram,
) {
    use crate::types::attr_constants::entity_type;

    let entity_type_of = |entity_id: &str| -> &str {
        diagram
            .entities
            .iter()
            .find(|e| e.id.as_str() == entity_id)
            .and_then(|e| e.attributes.standard.get("type").and_then(|v| v.as_str()))
            .unwrap_or(entity_type::STATE)
    };

    for node in dag.node_indices() {
        if entity_type_of(&dag[node]) == entity_type::INITIAL {
            ranks.insert(node, 0);
        }
    }

    let max_rank = ranks.values().copied().max().unwrap_or(0);
    for node in dag.node_indices() {
        if entity_type_of(&dag[node]) == entity_type::FINAL {
            ranks.insert(node, max_rank);
        }
    }

    // L5：硬覆盖后传播调整，修复 rank(u) >= rank(v) 的前向边。
    repair_rank_monotonicity(dag, ranks);
}

/// 传播修复：对每条边若 rank(u) >= rank(v)，将 v（及可达下游）整体后移。
fn repair_rank_monotonicity(
    dag: &petgraph::graph::DiGraph<String, ()>,
    ranks: &mut HashMap<petgraph::graph::NodeIndex, usize>,
) {
    use petgraph::Direction;
    // 按拓扑近似：多轮松弛直到无违反或达到上限。
    for _ in 0..dag.node_count().saturating_add(1) {
        let mut changed = false;
        let mut edges: Vec<(petgraph::graph::NodeIndex, petgraph::graph::NodeIndex)> = dag
            .edge_indices()
            .filter_map(|e| dag.edge_endpoints(e))
            .collect();
        // 确定性：按 (u.index, v.index) 排序
        edges.sort_by_key(|(u, v)| (u.index(), v.index()));
        for (u, v) in edges {
            // 自环不参与单调修复：`rank(u) >= rank(u)` 恒真会导致不收敛。
            // 正常路径下 build_graph 已过滤自环，此处为防御性双保险。
            if u == v {
                continue;
            }
            let ru = ranks[&u];
            let rv = ranks[&v];
            if ru >= rv {
                ranks.insert(v, ru + 1);
                changed = true;
            }
        }
        if !changed {
            break;
        }
        // 顺带把下游也推开：再扫一遍出边
        let mut nodes: Vec<_> = dag.node_indices().collect();
        nodes.sort_by_key(|n| n.index());
        for u in nodes {
            let ru = ranks[&u];
            let mut outs: Vec<_> = dag.neighbors_directed(u, Direction::Outgoing).collect();
            outs.sort_by_key(|n| n.index());
            for v in outs {
                if v == u {
                    continue;
                }
                if ranks[&v] <= ru {
                    ranks.insert(v, ru + 1);
                }
            }
        }
    }
}

/// 流程图 sink / end 的 rank 钳制。
///
/// - `type=end` 且恰有一条 DAG 入边：档 A/B，落在 `rank(pred)+1`（分支局部 / 主干延续）。
/// - `type=end` 多入边或无入边：档 C，仍落 `max_rank`。
/// - 其它 `out_degree==0` 真 sink（非 exempt hub）：`max_rank`。
pub(super) fn apply_sink_rank_constraints(
    dag: &petgraph::graph::DiGraph<String, ()>,
    ranks: &mut HashMap<petgraph::graph::NodeIndex, usize>,
    diagram: &Diagram,
    exempt_nodes: &HashSet<petgraph::graph::NodeIndex>,
) {
    use crate::types::attr_constants::entity_type;
    use petgraph::Direction;

    let entity_type_of = |entity_id: &str| -> &str {
        diagram
            .entities
            .iter()
            .find(|e| e.id.as_str() == entity_id)
            .and_then(|e| e.attributes.standard.get("type").and_then(|v| v.as_str()))
            .unwrap_or("")
    };

    let max_rank = ranks.values().copied().max().unwrap_or(0);
    let mut nodes: Vec<_> = dag.node_indices().collect();
    nodes.sort_by_key(|n| n.index());
    for node in nodes {
        let is_end = entity_type_of(&dag[node]) == entity_type::END;
        let out_degree = dag.neighbors_directed(node, Direction::Outgoing).count();
        if is_end {
            ranks.insert(node, end_sink_rank(node, dag, ranks, max_rank));
            continue;
        }
        // 出度 0：真正的 sink（自环边在 DAG 中仍占出度，不会误伤自环节点）
        // exempt 节点（feedback hub）不因出度 0 沉底
        if out_degree == 0 && !exempt_nodes.contains(&node) {
            ranks.insert(node, max_rank);
        }
    }
}

/// 单前驱 end 的目标 rank：`rank(pred)+1`（档 A/B）；否则 `max_rank`（档 C）。
fn end_sink_rank(
    end: NodeIndex,
    dag: &petgraph::graph::DiGraph<String, ()>,
    ranks: &HashMap<NodeIndex, usize>,
    max_rank: usize,
) -> usize {
    use petgraph::Direction;

    let mut preds: Vec<NodeIndex> = dag.neighbors_directed(end, Direction::Incoming).collect();
    preds.sort_by_key(|n| n.index());
    if preds.len() == 1 {
        let pred_rank = ranks.get(&preds[0]).copied().unwrap_or(0);
        pred_rank + 1
    } else {
        max_rank
    }
}

/// 识别同层边：Feedback Hub（P0）。
///
/// 通过节点 id 字符串判断反转边，避免 NodeIndex 跨图映射问题。
/// `reversed_edge_ids`：原图中的 (from_id, to_id)，即被 FAS 反转的边。
///
/// 注：单前驱 `type=end` 由 `end_sink_rank` 落在 `rank(pred)+1`（档 A/B），
/// 坐标阶段 `align_local_end_nodes` 对齐到前驱列；不再一律沉 `max_rank`。
pub(super) fn identify_same_layer_edges(
    dag: &petgraph::graph::DiGraph<String, ()>,
    reversed_edge_ids: &HashSet<(String, String)>,
) -> Vec<SameLayerEdge> {
    use petgraph::Direction;

    // DAG 边 (u, v) 是否为 FAS 反转边：原图中 (v_id, u_id) 在反转集合中。
    let is_reversed = |u: NodeIndex, v: NodeIndex| -> bool {
        reversed_edge_ids.contains(&(dag[v].clone(), dag[u].clone()))
    };

    let mut result = Vec::new();
    // 确定性：按 node.index() 排序遍历
    let mut nodes: Vec<NodeIndex> = dag.node_indices().collect();
    nodes.sort_by_key(|n| n.index());

    for h in nodes {
        let out_degree = dag.neighbors_directed(h, Direction::Outgoing).count();

        // 收集入边，区分反转边和前向边
        let mut reversed_preds: Vec<NodeIndex> = Vec::new();
        let mut forward_preds: Vec<NodeIndex> = Vec::new();
        let mut preds: Vec<NodeIndex> = dag.neighbors_directed(h, Direction::Incoming).collect();
        preds.sort_by_key(|n| n.index());
        for p in preds {
            if is_reversed(p, h) {
                reversed_preds.push(p);
            } else {
                forward_preds.push(p);
            }
        }

        // P0: Feedback Hub 识别
        // 条件1: DAG 中出度=0
        // 条件2: 有至少一条反转入边（原图 h→u 被 FAS 反转）
        // 条件3: 有至少一条前向入边
        if out_degree == 0 && !reversed_preds.is_empty() && !forward_preds.is_empty() {
            // 主前驱：前向入边中按 node.index() 取最小（确定性，等价于声明序）
            let primary = forward_preds[0];
            result.push(SameLayerEdge {
                from: primary,
                to: h,
            });
        }
    }

    result
}

/// 同层边 rank 覆盖：强制 `rank(to) = rank(from)`。
///
/// 在 sink clamp 之后执行，确保 hub/end 不被沉底后再覆盖回主前驱层。
pub(super) fn apply_same_layer_rank_overrides(
    ranks: &mut HashMap<NodeIndex, usize>,
    same_layer_edges: &[SameLayerEdge],
) {
    for e in same_layer_edges {
        if let Some(&from_rank) = ranks.get(&e.from) {
            ranks.insert(e.to, from_rank);
        }
    }
}

/// 提取 type=end 的实体 ID 列表（供坐标求解器结构 objectives 使用）。
pub(super) fn extract_end_ids(diagram: &Diagram) -> Vec<String> {
    use crate::types::attr_constants::entity_type;
    diagram
        .entities
        .iter()
        .filter(|e| {
            e.attributes.standard.get("type").and_then(|v| v.as_str())
                == Some(entity_type::END)
        })
        .map(|e| e.id.as_str().to_string())
        .collect()
}
