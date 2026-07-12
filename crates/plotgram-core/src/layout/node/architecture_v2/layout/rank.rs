//! Phase 2: 分组感知层分配。

use crate::ast::Diagram;
use std::collections::{HashMap, HashSet, VecDeque};

use super::acyclic::is_effective_edge;
use super::types::{GraphIndex, GroupMap};

fn super_node_id(node: &str, group_map: &GroupMap) -> String {
    group_map
        .node_to_top_group
        .get(node)
        .cloned()
        .unwrap_or_else(|| format!("@node:{node}"))
}

/// 分组感知的层分配算法
///
/// `constraints` 为 DSL `constrain` 边集合 `(from, to)`，语义为 `rank(from) < rank(to)`。
/// 约束边永不被反转：在 macro 级 `resolve_bidirectional_pairs_by_weight` 和 super-node FAS
/// 中均被排除出反转候选。
pub(in super::super) fn assign_ranks_group_aware(
    diagram: &Diagram,
    graph: &GraphIndex,
    group_map: &GroupMap,
    reversed: &HashSet<(String, String)>,
    constraints: &HashSet<(String, String)>,
) -> HashMap<String, usize> {
    let group_decl = crate::layout::decl_order::group_sibling_decl_index(diagram);
    let mut ranks = assign_macro_group_ranks(graph, group_map, reversed, &group_decl, constraints);

    // 归一化
    let min_rank = ranks.values().copied().min().unwrap_or(0);
    if min_rank > 0 {
        for rank in ranks.values_mut() {
            *rank -= min_rank;
        }
    }

    // 确保每个实体都有 rank
    for entity in &diagram.entities {
        let id = entity.id.as_str().to_string();
        if !ranks.contains_key(&id) {
            ranks.insert(id, 0);
        }
    }

    ranks
}

/// 宏观 + 微观两层 rank 合并
fn assign_macro_group_ranks(
    graph: &GraphIndex,
    group_map: &GroupMap,
    reversed: &HashSet<(String, String)>,
    group_decl: &HashMap<String, usize>,
    constraints: &HashSet<(String, String)>,
) -> HashMap<String, usize> {
    // 1. 超级节点成员
    let mut super_members: HashMap<String, Vec<String>> = HashMap::new();
    for node in &graph.node_ids {
        super_members
            .entry(super_node_id(node, group_map))
            .or_default()
            .push(node.clone());
    }
    for members in super_members.values_mut() {
        members.sort();
    }

    // 2. 超级节点间边（去重）+ 有向边权（跨组实际边数）
    let mut super_edges: HashSet<(String, String)> = HashSet::new();
    let mut edge_weights: HashMap<(String, String), usize> = HashMap::new();
    for node in &graph.node_ids {
        if let Some(succs) = graph.out_edges.get(node) {
            for succ in succs {
                if !is_effective_edge(node, succ, reversed) {
                    continue;
                }
                let from_super = super_node_id(node, group_map);
                let to_super = super_node_id(succ, group_map);
                if from_super != to_super {
                    *edge_weights
                        .entry((from_super.clone(), to_super.clone()))
                        .or_insert(0) += 1;
                    super_edges.insert((from_super, to_super));
                }
            }
        }
    }

    // 2b. 约束边映射到超级节点级别，供 macro 级排除反转
    let constraint_super_edges: HashSet<(String, String)> = constraints
        .iter()
        .map(|(from, to)| (super_node_id(from, group_map), super_node_id(to, group_map)))
        .collect();

    // 3. 宏观 rank
    let macro_ranks = assign_super_macro_ranks(
        &super_members,
        &super_edges,
        &edge_weights,
        &graph.node_ids,
        group_decl,
        &constraint_super_edges,
    );

    // 4. 微观 rank + 各超级节点层带宽度
    let mut intra_ranks: HashMap<String, usize> = HashMap::new();
    let mut super_stride: HashMap<String, usize> = HashMap::new();
    for (super_id, members) in &super_members {
        let intra = assign_intra_ranks(members, graph, reversed);
        let max_intra = intra.values().copied().max().unwrap_or(0);
        super_stride.insert(super_id.clone(), max_intra + 1);
        intra_ranks.extend(intra);
    }

    // 5. 按 macro rank 合并为全局 rank
    let max_macro = macro_ranks.values().copied().max().unwrap_or(0);
    let mut ranks = HashMap::new();
    let mut offset = 0usize;

    for macro_rank in 0..=max_macro {
        let mut supers_at_level: Vec<String> = super_members
            .keys()
            .filter(|s| macro_ranks.get(*s).copied().unwrap_or(0) == macro_rank)
            .cloned()
            .collect();
        supers_at_level.sort();

        if supers_at_level.is_empty() {
            continue;
        }

        let base = offset;
        let mut level_stride = 0usize;
        for super_id in &supers_at_level {
            if let Some(members) = super_members.get(super_id) {
                for node in members {
                    let intra = intra_ranks.get(node).copied().unwrap_or(0);
                    ranks.insert(node.clone(), base + intra);
                }
            }
            level_stride = level_stride.max(
                super_stride.get(super_id).copied().unwrap_or(1),
            );
        }
        offset = base + level_stride;
    }

    ranks
}

/// 超级节点宏观 rank（最长路径）
///
/// **超级图去环**：节点级 FAS 只保证节点图无环，但超级图（group 级商图）可能仍有环
/// （例如 `pay_core -> service_mesh` 产生 `payment_ns -> platform_ns`，
/// `argo -> pay_gateway` 产生 `platform_ns -> payment_ns`，形成 group 级环）。
/// 因此在 rank 分配前，对超级图额外运行 `greedy_fas` 去环，确保拓扑排序和最长路径
/// rank 分配在 DAG 上进行，避免 sink 节点因环中前驱未排序而被错误地分配到 rank 0。
///
/// **加权双向对裁决**：超级边是去重的（每对组只记一条），但组间实际边数差异
/// 是决定主流向的关键信号。例如 `private_subnet → data_subnet` 有 4 条实际边、
/// `data_subnet → private_subnet` 只有 1 条回流边（mq→worker），主流向应保留
/// 4 边方向。因此在 FAS 之前先按 `edge_weights`（有向跨组实际边数）裁决所有
/// 双向对：多数方向保留、少数方向直接标记反转；权重平局时，组声明序更早的
/// 组视为上游（优先 group sibling decl，回退到成员全局声明序）。
pub(in super::super) fn assign_super_macro_ranks(
    super_members: &HashMap<String, Vec<String>>,
    super_edges: &HashSet<(String, String)>,
    edge_weights: &HashMap<(String, String), usize>,
    node_decl_order: &[String],
    group_decl: &HashMap<String, usize>,
    constraint_super_edges: &HashSet<(String, String)>,
) -> HashMap<String, usize> {
    let all_supers: HashSet<String> = super_members.keys().cloned().collect();

    // ── 加权裁决双向对（2-环），约束边永不被反转 ──
    let pre_reversed = resolve_bidirectional_pairs_by_weight(
        super_members,
        super_edges,
        edge_weights,
        node_decl_order,
        group_decl,
        constraint_super_edges,
    );

    // 构建去除"已裁决反转边"后的邻接表，供 FAS 处理剩余的长环
    let mut super_in: HashMap<String, Vec<String>> = HashMap::new();
    let mut super_out: HashMap<String, Vec<String>> = HashMap::new();

    for (from, to) in super_edges {
        if pre_reversed.contains(&(from.clone(), to.clone())) {
            continue;
        }
        super_out.entry(from.clone()).or_default().push(to.clone());
        super_in.entry(to.clone()).or_default().push(from.clone());
    }
    // 排序每个邻接表 Vec，保证 BFS 拓扑排序中邻居处理顺序确定
    // （super_edges 是 HashSet，迭代顺序随机，否则会导致 macro_ranks 非确定）
    for v in super_out.values_mut() {
        v.sort();
    }
    for v in super_in.values_mut() {
        v.sort();
    }

    // 对超级图运行 FAS 去环：识别后向边并在 rank 分配时忽略它们。
    // 这确保 sink 节点（如 data_ns）不会被错误地分配到 rank 0。
    let mut sorted_supers: Vec<String> = all_supers.iter().cloned().collect();
    sorted_supers.sort();
    let mut super_reversed = crate::layout::node::common::acyclic::greedy_fas(&sorted_supers, &super_out, &super_in);
    // 约束边永不被反转：从 FAS 结果中剔除约束 super-edge
    super_reversed.retain(|e| !constraint_super_edges.contains(e));
    super_reversed.extend(pre_reversed);

    // 构建去环后的邻接表（移除后向边）
    let mut acyclic_in: HashMap<String, Vec<String>> = HashMap::new();
    let mut acyclic_out: HashMap<String, Vec<String>> = HashMap::new();
    for (from, to) in super_edges {
        if super_reversed.contains(&(from.clone(), to.clone())) {
            continue;
        }
        acyclic_out.entry(from.clone()).or_default().push(to.clone());
        acyclic_in.entry(to.clone()).or_default().push(from.clone());
    }
    for v in acyclic_out.values_mut() {
        v.sort();
    }
    for v in acyclic_in.values_mut() {
        v.sort();
    }

    let sorted = topological_sort_super_nodes(&all_supers, &acyclic_out, &acyclic_in);
    let mut macro_ranks: HashMap<String, usize> = HashMap::new();

    for super_id in sorted {
        let mut max_pred = 0usize;
        if let Some(preds) = acyclic_in.get(&super_id) {
            for pred in preds {
                if let Some(&pr) = macro_ranks.get(pred) {
                    max_pred = max_pred.max(pr + 1);
                }
            }
        }
        macro_ranks.insert(super_id, max_pred);
    }

    for super_id in &all_supers {
        macro_ranks.entry(super_id.clone()).or_insert(0);
    }

    macro_ranks
}

/// 按有向边权裁决超级图中的双向对（2-环）。
///
/// 对每个双向对 (A↔B)：
/// 1. 若其中一侧是约束边（`constraint_super_edges`），**永不被反转**——
///    反转另一侧（非约束方向）。若两侧都是约束边，不反转（矛盾约束由下游兜底）。
/// 2. 否则比较 `weight(A→B)` 与 `weight(B→A)`（跨组实际边数），反转权重小的方向；
/// 3. 权重平局时，组声明序更早的组视为上游（优先 `group_decl`，否则成员全局声明序），
///    反转"下游→上游"方向。
///
/// 返回需要标记反转的边集合（这些边不参与后续 FAS / 拓扑排序）。
fn resolve_bidirectional_pairs_by_weight(
    super_members: &HashMap<String, Vec<String>>,
    super_edges: &HashSet<(String, String)>,
    edge_weights: &HashMap<(String, String), usize>,
    node_decl_order: &[String],
    group_decl: &HashMap<String, usize>,
    constraint_super_edges: &HashSet<(String, String)>,
) -> HashSet<(String, String)> {
    // 找出所有双向对，归一化为 (min, max) 去重
    let mut bidir_pairs: Vec<(String, String)> = Vec::new();
    let mut seen: HashSet<(String, String)> = HashSet::new();
    for (from, to) in super_edges {
        if from == to {
            continue;
        }
        if super_edges.contains(&(to.clone(), from.clone())) {
            let normalized = if from < to {
                (from.clone(), to.clone())
            } else {
                (to.clone(), from.clone())
            };
            if seen.insert(normalized.clone()) {
                bidir_pairs.push(normalized);
            }
        }
    }
    let mut reversed = HashSet::new();
    if bidir_pairs.is_empty() {
        return reversed;
    }
    bidir_pairs.sort();

    // 节点声明序索引（用于权重平局时的上游判定回退）
    let decl_index: HashMap<&str, usize> = node_decl_order
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    // 优先组 sibling 声明序；无组映射时回退到最早成员全局声明序
    let super_decl_rank = |super_id: &str| -> usize {
        if let Some(&idx) = group_decl.get(super_id) {
            return idx;
        }
        super_members
            .get(super_id)
            .map(|members| {
                members
                    .iter()
                    .filter_map(|m| decl_index.get(m.as_str()).copied())
                    .min()
                    .unwrap_or(usize::MAX)
            })
            .unwrap_or(usize::MAX)
    };

    for (a, b) in &bidir_pairs {
        let ab_is_constraint = constraint_super_edges.contains(&(a.clone(), b.clone()));
        let ba_is_constraint = constraint_super_edges.contains(&(b.clone(), a.clone()));

        // 约束边永不被反转：反转非约束方向
        if ab_is_constraint && !ba_is_constraint {
            reversed.insert((b.clone(), a.clone()));
            continue;
        }
        if ba_is_constraint && !ab_is_constraint {
            reversed.insert((a.clone(), b.clone()));
            continue;
        }
        if ab_is_constraint && ba_is_constraint {
            // 两侧都是约束边（矛盾约束），不反转，由下游拓扑排序兜底
            continue;
        }

        let w_ab = edge_weights.get(&(a.clone(), b.clone())).copied().unwrap_or(1);
        let w_ba = edge_weights.get(&(b.clone(), a.clone())).copied().unwrap_or(1);

        let reverse_a_to_b = if w_ab != w_ba {
            // 反转权重小的方向
            w_ab < w_ba
        } else {
            // 平局：声明序更早的组视为上游 → 反转"晚→早"方向
            super_decl_rank(a) > super_decl_rank(b)
        };

        if reverse_a_to_b {
            reversed.insert((a.clone(), b.clone()));
        } else {
            reversed.insert((b.clone(), a.clone()));
        }
    }

    reversed
}

/// 超级节点图拓扑排序
fn topological_sort_super_nodes(
    all_supers: &HashSet<String>,
    super_out: &HashMap<String, Vec<String>>,
    super_in: &HashMap<String, Vec<String>>,
) -> Vec<String> {
    let mut in_degree: HashMap<String, usize> = all_supers
        .iter()
        .map(|s| (s.clone(), super_in.get(s).map_or(0, |v| v.len())))
        .collect();
    let adj = super_out.clone();

    let mut queue: VecDeque<String> = VecDeque::new();
    let mut zero: Vec<String> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(n, _)| n.clone())
        .collect();
    zero.sort();
    for node in zero {
        queue.push_back(node);
    }

    let mut sorted = Vec::new();
    while let Some(node) = queue.pop_front() {
        sorted.push(node.clone());
        if let Some(neighbors) = adj.get(&node) {
            for neighbor in neighbors {
                if let Some(deg) = in_degree.get_mut(neighbor) {
                    *deg = deg.saturating_sub(1);
                    if *deg == 0 {
                        queue.push_back(neighbor.clone());
                    }
                }
            }
        }
    }

    // 收集未在 BFS 中处理的节点（环中节点），排序后追加保证确定性
    // （all_supers 是 HashSet，迭代顺序随机）
    let sorted_set: HashSet<&String> = sorted.iter().collect();
    let mut remaining: Vec<&String> = all_supers.iter().filter(|s| !sorted_set.contains(s)).collect();
    remaining.sort();
    for super_id in remaining {
        sorted.push(super_id.clone());
    }

    sorted
}

/// 超级节点内部微观 rank（仅组内边）
pub(in super::super) fn assign_intra_ranks(
    members: &[String],
    graph: &GraphIndex,
    reversed: &HashSet<(String, String)>,
) -> HashMap<String, usize> {
    let sorted = topological_sort_subset(members, graph, reversed);
    let member_set: HashSet<&str> = members.iter().map(|m| m.as_str()).collect();
    let mut ranks = HashMap::new();

    for node in sorted {
        let mut max_pred = 0usize;
        if let Some(preds) = graph.in_edges.get(&node) {
            for pred in preds {
                if !member_set.contains(pred.as_str()) {
                    continue;
                }
                if !is_effective_edge(pred, &node, reversed) {
                    continue;
                }
                max_pred = max_pred.max(ranks.get(pred).copied().unwrap_or(0) + 1);
            }
        }
        ranks.insert(node, max_pred);
    }

    ranks
}

fn topological_sort_subset(
    members: &[String],
    graph: &GraphIndex,
    reversed: &HashSet<(String, String)>,
) -> Vec<String> {
    let member_set: HashSet<String> = members.iter().cloned().collect();
    let mut in_degree: HashMap<String, usize> = members.iter().map(|m| (m.clone(), 0)).collect();
    let mut adj: HashMap<String, Vec<String>> = members.iter().map(|m| (m.clone(), Vec::new())).collect();

    for node in members {
        if let Some(successors) = graph.out_edges.get(node) {
            for succ in successors {
                if !member_set.contains(succ) {
                    continue;
                }
                if !is_effective_edge(node, succ, reversed) {
                    continue;
                }
                *in_degree.entry(succ.clone()).or_insert(0) += 1;
                adj.entry(node.clone()).or_default().push(succ.clone());
            }
        }
    }

    let mut queue: VecDeque<String> = VecDeque::new();
    let mut zero_nodes: Vec<&String> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(n, _)| n)
        .collect();
    zero_nodes.sort();
    for node in zero_nodes {
        queue.push_back(node.clone());
    }

    let mut sorted = Vec::new();
    while let Some(node) = queue.pop_front() {
        sorted.push(node.clone());
        if let Some(neighbors) = adj.get(&node) {
            for neighbor in neighbors {
                if let Some(deg) = in_degree.get_mut(neighbor) {
                    *deg = deg.saturating_sub(1);
                    if *deg == 0 {
                        queue.push_back(neighbor.clone());
                    }
                }
            }
        }
    }

    // 有环的追加到末尾
    for node in members {
        if !sorted.contains(node) {
            sorted.push(node.clone());
        }
    }

    sorted
}

