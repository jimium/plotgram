//! Phase 1: 去环与不可逆约束边注入。

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

/// 将不可逆约束边注入图索引（FAS 之后调用，约束边永不被反转）。
///
/// `edges` 为 `(from, to)`，语义为 `rank(from) < rank(to)`。
/// 跨顶层 group 的约束由调用方决定是否注入或报错；本函数不做静默跳过。
pub(in super::super) fn inject_irreversible_edges(
    graph: &mut GraphIndex,
    edges: &[(&str, &str)],
) {
    for &(edge_from, edge_to) in edges {
        if !graph.out_edges.contains_key(edge_from) || !graph.out_edges.contains_key(edge_to) {
            continue;
        }
        graph
            .out_edges
            .entry(edge_from.to_string())
            .or_default()
            .push(edge_to.to_string());
        graph
            .in_edges
            .entry(edge_to.to_string())
            .or_default()
            .push(edge_from.to_string());
    }
}

/// 供调用方判断两端是否同属一个顶层 group（无 group 时视为同组）。
pub(in super::super) fn same_top_group(
    group_map: &GroupMap,
    from: &str,
    to: &str,
) -> bool {
    if group_map.top_groups.is_empty() {
        return true;
    }
    group_map.node_to_top_group.get(from) == group_map.node_to_top_group.get(to)
}
