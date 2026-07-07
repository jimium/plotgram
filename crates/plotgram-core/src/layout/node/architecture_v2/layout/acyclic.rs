//! Phase 1: 去环与意图边注入。

use crate::layout::intent::topology::ValidTopologyIntent;
use crate::layout::node::common::acyclic;
use std::collections::HashSet;

use super::types::{GraphIndex, GroupMap};

/// 贪心反馈边集（Feedback Arc Set）：委托给 `common::acyclic::greedy_fas`。
pub(in super::super) fn find_edges_to_reverse(graph: &GraphIndex) -> HashSet<(String, String)> {
    acyclic::greedy_fas(&graph.node_ids, &graph.out_edges, &graph.in_edges)
}

/// 判断边 (from -> to) 是否为有效边（未被反转）
pub(in super::super) fn is_effective_edge(from: &str, to: &str, reversed: &HashSet<(String, String)>) -> bool {
    !reversed.contains(&(from.to_string(), to.to_string()))
}

/// 将拓扑意图边注入图索引（FAS 之后调用，意图边永不被反转）。
pub(in super::super) fn inject_intent_edges(
    graph: &mut GraphIndex,
    valid_topology: &[ValidTopologyIntent],
    group_map: &GroupMap,
) -> Vec<usize> {
    let has_groups = !group_map.top_groups.is_empty();
    let mut skipped = Vec::new();

    for (i, v) in valid_topology.iter().enumerate() {
        let (edge_from, edge_to) = v.edge();

        if has_groups {
            let from_group = group_map.node_to_top_group.get(edge_from);
            let to_group = group_map.node_to_top_group.get(edge_to);
            if from_group != to_group {
                skipped.push(i);
                continue;
            }
        }
        if !graph.out_edges.contains_key(edge_from) || !graph.out_edges.contains_key(edge_to) {
            continue;
        }
        graph.out_edges.entry(edge_from.to_string()).or_default().push(edge_to.to_string());
        graph.in_edges.entry(edge_to.to_string()).or_default().push(edge_from.to_string());
    }

    skipped
}
