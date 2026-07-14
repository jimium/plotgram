//! Phase 1: 去环与不可逆约束边注入。

use crate::layout::node::common::acyclic;
use std::collections::HashSet;

use super::types::{GraphIndex, GroupMap};

/// 贪心反馈边集（Feedback Arc Set）：委托给 `common::acyclic::greedy_fas`。
///
/// 传入完整图拓扑（含约束边）使 FAS 能识别约束引入的环，并可能反转 relation 边来打破环。
/// `non_reversible` 中的边（约束边）从反转结果中剔除——约束边永不被反转。
/// 若剔除约束边后仍有残余环，由下游 rank 分配兜底处理。
pub(in super::super) fn find_edges_to_reverse(
    graph: &GraphIndex,
    non_reversible: &HashSet<(String, String)>,
) -> HashSet<(String, String)> {
    let reversed = acyclic::greedy_fas(&graph.node_ids, &graph.out_edges, &graph.in_edges);
    // 剔除约束边：它们永不被反转
    reversed
        .into_iter()
        .filter(|e| !non_reversible.contains(e))
        .collect()
}

/// 判断边 (from -> to) 是否为有效边（未被反转）
pub(in super::super) fn is_effective_edge(from: &str, to: &str, reversed: &HashSet<(String, String)>) -> bool {
    !reversed.contains(&(from.to_string(), to.to_string()))
}

/// 将不可逆约束边注入图索引（FAS 之前调用，约束边永不被反转）。
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
