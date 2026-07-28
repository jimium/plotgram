//! super graph
//!
//! moved from two_phase.rs (A4, behavior unchanged).

use super::*;

/// Accumulate a single super-edge into the shared aggregation structures.
///
/// If `from_super != to_super`, inserts the directed edge into `super_edges`,
/// increments its weight in `edge_weights`, and increments the normalized
/// undirected pair count in `pair_edge_counts`. No-op when both endpoints
/// resolve to the same super-node.
pub(super) fn accumulate_super_edge(
    from_super: String,
    to_super: String,
    super_edges: &mut HashSet<(String, String)>,
    edge_weights: &mut HashMap<(String, String), usize>,
    pair_edge_counts: &mut HashMap<(String, String), usize>,
) {
    if from_super != to_super {
        super_edges.insert((from_super.clone(), to_super.clone()));
        *edge_weights
            .entry((from_super.clone(), to_super.clone()))
            .or_insert(0) += 1;
        let pair = if from_super <= to_super {
            (from_super, to_super)
        } else {
            (to_super, from_super)
        };
        *pair_edge_counts.entry(pair).or_insert(0) += 1;
    }
}

/// 为容器组构建超级节点图
///
/// 超级节点 = 子组 + 直接实体块
/// 超级边 = 跨越不同超级节点的有效边
pub(super) fn build_super_graph_for_group(
    group_id: &str,
    group_tree: &GroupTree,
    graph: &GraphIndex,
    reversed: &HashSet<(String, String)>,
) -> (
    HashMap<String, Vec<String>>,
    HashSet<(String, String)>,
    HashMap<(String, String), usize>,
    HashMap<(String, String), usize>,
) {
    let children = group_tree.children_of(group_id);
    let direct_entities = group_tree.entities_of(group_id);

    // 超级节点成员：子组→后代实体，直接实体块→直接实体
    let mut super_members: HashMap<String, Vec<String>> = HashMap::new();
    for child_id in children {
        super_members.insert(child_id.clone(), group_tree.descendant_entities(child_id));
    }
    if !direct_entities.is_empty() {
        super_members.insert(format!("@direct:{group_id}"), direct_entities.to_vec());
    }

    // 节点 → 所属超级节点
    let mut node_to_super: HashMap<String, String> = HashMap::new();
    for (super_id, members) in &super_members {
        for m in members {
            node_to_super.insert(m.clone(), super_id.clone());
        }
    }

    // 超级边：跨超级节点的有效边
    let mut super_edges: HashSet<(String, String)> = HashSet::new();
    // Phase 3：per-pair 边数（归一化为无向 pair）
    let mut pair_edge_counts: HashMap<(String, String), usize> = HashMap::new();
    // 有向边权（跨组实际边数），供加权 FAS 裁决双向对
    let mut edge_weights: HashMap<(String, String), usize> = HashMap::new();
    for (super_id, members) in &super_members {
        for node in members {
            if let Some(succs) = graph.out_edges.get(node) {
                for succ in succs {
                    if !is_effective_edge(node, succ, reversed) {
                        continue;
                    }
                    let from_super = super_id.clone();
                    let to_super = match node_to_super.get(succ) {
                        Some(s) => s.clone(),
                        None => continue,
                    };
                    accumulate_super_edge(
                        from_super,
                        to_super,
                        &mut super_edges,
                        &mut edge_weights,
                        &mut pair_edge_counts,
                    );
                }
            }
        }
    }

    (super_members, super_edges, pair_edge_counts, edge_weights)
}

pub(super) fn build_super_graph(
    graph: &GraphIndex,
    group_map: &GroupMap,
    reversed: &HashSet<(String, String)>,
) -> (
    HashMap<String, Vec<String>>,
    HashSet<(String, String)>,
    HashMap<(String, String), usize>,
    HashMap<(String, String), usize>,
) {
    let mut super_members: HashMap<String, Vec<String>> = HashMap::new();

    for gid in &group_map.top_groups {
        super_members.insert(
            gid.clone(),
            group_map
                .top_group_members
                .get(gid)
                .cloned()
                .unwrap_or_default(),
        );
    }
    for node in &group_map.ungrouped {
        super_members.insert(format!("@node:{node}"), vec![node.clone()]);
    }

    let mut super_edges: HashSet<(String, String)> = HashSet::new();
    // Phase 3：per-pair 边数（归一化为无向 pair），用于按 pair 计算通道间距
    let mut pair_edge_counts: HashMap<(String, String), usize> = HashMap::new();
    // 有向边权（跨组实际边数），供加权 FAS 裁决双向对
    let mut edge_weights: HashMap<(String, String), usize> = HashMap::new();
    for node in &graph.node_ids {
        if let Some(succs) = graph.out_edges.get(node) {
            for succ in succs {
                if !is_effective_edge(node, succ, reversed) {
                    continue;
                }
                let from_super = super_node_id(node, group_map);
                let to_super = super_node_id(succ, group_map);
                accumulate_super_edge(
                    from_super,
                    to_super,
                    &mut super_edges,
                    &mut edge_weights,
                    &mut pair_edge_counts,
                );
            }
        }
    }

    (super_members, super_edges, pair_edge_counts, edge_weights)
}

pub(super) fn super_node_id(node: &str, group_map: &GroupMap) -> String {
    group_map
        .node_to_top_group
        .get(node)
        .cloned()
        .unwrap_or_else(|| format!("@node:{node}"))
}

