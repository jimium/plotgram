//! 基础 Objectives 构建器。
//!
//! 从分层图结构生成 Phase 3 的基础目标项：
//! - P3: PreferBKPosition — 保持 BK 初值
//! - P2: PreferShortHorizontalEdge — 相连节点横向对齐
//! - P2: DummyChainAlignment — dummy 链共线

use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
use std::collections::HashMap;

use super::model::*;
use crate::layout::node::sugiyama_v2::graph::{LayerNode, LayerNodeKind};

/// 目标权重配置。
pub(in crate::layout) struct ObjectiveWeights {
    /// P3: BK 位置保持权重。
    pub prefer_bk: f64,
    /// P2: 边拉直权重（真实边）。
    pub edge_straight: f64,
    /// P2: dummy 链共线权重。
    pub dummy_chain: f64,
}

impl Default for ObjectiveWeights {
    fn default() -> Self {
        Self {
            prefer_bk: 1.0,
            edge_straight: 2.0,
            dummy_chain: 1.5,
        }
    }
}

/// 从分层图构建基础 objectives。
///
/// - `layered_graph`: 分层图（含 Real 和 Dummy 节点）。
/// - `node_to_var`: NodeIndex → VarId 映射。
/// - `initial`: BK 初值数组。
/// - `weights`: 目标权重配置。
pub(in crate::layout) fn build_basic_objectives(
    layered_graph: &DiGraph<LayerNode, ()>,
    node_to_var: &HashMap<NodeIndex, VarId>,
    initial: &[f64],
    weights: &ObjectiveWeights,
) -> Vec<ObjectiveTerm> {
    let mut objectives = Vec::new();

    // P3: PreferBKPosition — 仅 Real 节点
    if weights.prefer_bk > 0.0 {
        for node in layered_graph.node_indices() {
            if matches!(&layered_graph[node].kind, LayerNodeKind::Real(_)) {
                if let Some(&var) = node_to_var.get(&node) {
                    objectives.push(ObjectiveTerm {
                        priority: ObjectivePriority::P3,
                        coefficients: vec![(var, 1.0)],
                        constant: -initial[var],
                        weight: weights.prefer_bk,
                        source: ConstraintSource {
                            kind: ConstraintSourceKind::LayerOrder,
                            nodes: vec![format!("n{}", node.index())],
                            note: "prefer_bk_position",
                        },
                    });
                }
            }
        }
    }

    // P2: PreferShortHorizontalEdge — 每条边的两端对齐
    if weights.edge_straight > 0.0 {
        // 按边索引排序保证确定性
        let mut edge_pairs: Vec<(VarId, VarId)> = Vec::new();
        for edge in layered_graph.edge_indices() {
            let (from, to) = layered_graph.edge_endpoints(edge).unwrap();
            if let (Some(&from_var), Some(&to_var)) =
                (node_to_var.get(&from), node_to_var.get(&to))
            {
                edge_pairs.push((from_var, to_var));
            }
        }
        // 确定性排序
        edge_pairs.sort();

        for (from_var, to_var) in edge_pairs {
            // (x[from] - x[to])² = (1*x[from] + (-1)*x[to] + 0)²
            objectives.push(ObjectiveTerm {
                priority: ObjectivePriority::P2,
                coefficients: vec![(from_var, 1.0), (to_var, -1.0)],
                constant: 0.0,
                weight: weights.edge_straight,
                source: ConstraintSource {
                    kind: ConstraintSourceKind::NodeSeparation,
                    nodes: vec![],
                    note: "edge_straight",
                },
            });
        }
    }

    // P2: DummyChainAlignment — 同一长边的连续 dummy 共线
    if weights.dummy_chain > 0.0 {
        // 收集 dummy 链：按 (source, target) 分组，按 segment 排序
        let mut chains: HashMap<(usize, usize), Vec<(usize, NodeIndex)>> = HashMap::new();
        for node in layered_graph.node_indices() {
            if let LayerNodeKind::Dummy { source, target, segment } = &layered_graph[node].kind {
                chains
                    .entry((source.index(), target.index()))
                    .or_default()
                    .push((*segment, node));
            }
        }

        // 确定性：按 key 排序
        let mut chain_keys: Vec<_> = chains.keys().copied().collect();
        chain_keys.sort();

        for key in chain_keys {
            let mut chain = chains.remove(&key).unwrap();
            chain.sort_by_key(|(seg, _)| *seg);

            // 连续 dummy 对齐：(x[d_i] - x[d_{i+1}])²
            for window in chain.windows(2) {
                let (_, node_a) = window[0];
                let (_, node_b) = window[1];
                if let (Some(&var_a), Some(&var_b)) =
                    (node_to_var.get(&node_a), node_to_var.get(&node_b))
                {
                    objectives.push(ObjectiveTerm {
                        priority: ObjectivePriority::P2,
                        coefficients: vec![(var_a, 1.0), (var_b, -1.0)],
                        constant: 0.0,
                        weight: weights.dummy_chain,
                        source: ConstraintSource {
                            kind: ConstraintSourceKind::NodeSeparation,
                            nodes: vec![],
                            note: "dummy_chain_align",
                        },
                    });
                }
            }

            // 首尾 Real 节点与 dummy 链对齐（如果链非空）
            if let Some(&(_, first_dummy)) = chain.first() {
                let (source_idx, _) = key;
                // 找 source real node 的 var
                for node in layered_graph.node_indices() {
                    if let LayerNodeKind::Real(real_idx) = &layered_graph[node].kind {
                        if real_idx.index() == source_idx {
                            if let (Some(&real_var), Some(&dummy_var)) =
                                (node_to_var.get(&node), node_to_var.get(&first_dummy))
                            {
                                objectives.push(ObjectiveTerm {
                                    priority: ObjectivePriority::P2,
                                    coefficients: vec![(real_var, 1.0), (dummy_var, -1.0)],
                                    constant: 0.0,
                                    weight: weights.dummy_chain * 0.5,
                                    source: ConstraintSource {
                                        kind: ConstraintSourceKind::NodeSeparation,
                                        nodes: vec![],
                                        note: "dummy_source_align",
                                    },
                                });
                            }
                            break;
                        }
                    }
                }
            }
        }
    }

    objectives
}
