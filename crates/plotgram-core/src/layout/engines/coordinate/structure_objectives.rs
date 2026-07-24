//! 结构目标构建器（Phase 4+5）。
//!
//! 从 DAG 拓扑识别结构模式，生成对应的 objectives：
//! - P1: 线性链共轴（LinearChain）
//! - P2: Singleton 层对齐前驱/后继重心
//! - P1: 单前驱 End 节点跟随所属 chain
//! - P1: Pendant 叶节点对齐锚点（单/多质心）

use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::Direction;
use std::collections::HashMap;

use super::model::*;
use crate::layout::engines::layered::graph::{LayerNode, LayerNodeKind};

/// 结构目标权重。
pub(in crate::layout) struct StructureWeights {
    /// P1: 线性链共轴权重。
    pub chain_align: f64,
    /// P1: fan 对称权重（hub 对齐 children 质心）。
    pub fan_symmetry: f64,
    /// P2: singleton 对齐权重。
    pub singleton_align: f64,
    /// P1: end 跟随权重。
    pub end_follow: f64,
    /// P1: pendant 对齐权重。
    pub pendant_align: f64,
}

impl Default for StructureWeights {
    fn default() -> Self {
        Self {
            chain_align: 8.0,
            fan_symmetry: 3.0, // Phase 6: 启用 fan 对称目标（替代已删除的 enforce_fan_symmetry）
            singleton_align: 3.0,
            end_follow: 5.0,
            pendant_align: 5.0,
        }
    }
}

/// 从 DAG 拓扑构建结构 objectives。
///
/// - `dag`: 原始有向图（节点权重为 entity id）。
/// - `layered_graph`: 分层图。
/// - `layers`: 层序列。
/// - `node_to_var`: NodeIndex(layered) → VarId 映射。
/// - `initial`: BK 初值。
/// - `end_ids`: end 类型节点的 entity id 集合。
/// - `weights`: 结构目标权重。
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn build_structure_objectives(
    dag: &DiGraph<String, ()>,
    layered_graph: &DiGraph<LayerNode, ()>,
    layers: &[Vec<NodeIndex>],
    node_to_var: &HashMap<NodeIndex, VarId>,
    initial: &[f64],
    end_ids: &[String],
    weights: &StructureWeights,
) -> Vec<ObjectiveTerm> {
    let mut objectives = Vec::new();

    // 构建辅助映射：entity_id → layered NodeIndex
    let mut id_to_layered: HashMap<String, NodeIndex> = HashMap::new();
    let mut layered_to_id: HashMap<NodeIndex, String> = HashMap::new();
    for node in layered_graph.node_indices() {
        if let LayerNodeKind::Real(original) = &layered_graph[node].kind {
            let id = dag[*original].clone();
            id_to_layered.insert(id.clone(), node);
            layered_to_id.insert(node, id);
        }
    }

    // entity_id → layer index
    let mut id_to_layer: HashMap<String, usize> = HashMap::new();
    for (layer_idx, layer) in layers.iter().enumerate() {
        for node in layer {
            if let Some(id) = layered_to_id.get(node) {
                id_to_layer.insert(id.clone(), layer_idx);
            }
        }
    }

    // dag NodeIndex → entity_id
    let mut dag_to_id: HashMap<NodeIndex, String> = HashMap::new();
    for node in dag.node_indices() {
        dag_to_id.insert(node, dag[node].clone());
    }

    // ─── P1: 线性链共轴 ───────────────────────────────────────────────────
    // 线性链：每个节点（除首）恰好 1 个前驱，每个节点（除尾）恰好 1 个后继。
    // 连续链节点应共轴（垂直对齐）。
    let mut sorted_nodes: Vec<NodeIndex> = dag.node_indices().collect();
    sorted_nodes.sort_by_key(|n| n.index());

    if weights.chain_align > 0.0 {
        // 找出所有链头：入度=0 或 入度>1 的节点的后继
        let mut chain_starts: Vec<NodeIndex> = Vec::new();

        for node in &sorted_nodes {
            let in_deg = dag.neighbors_directed(*node, Direction::Incoming).count();
            if in_deg == 0 {
                chain_starts.push(*node);
            } else if in_deg > 1 {
                // 多入度节点自身是链头（汇聚后继续）
                chain_starts.push(*node);
            } else {
                // 入度=1：检查前驱是否有多后继
                let pred = dag.neighbors_directed(*node, Direction::Incoming).next().unwrap();
                let pred_out = dag.neighbors_directed(pred, Direction::Outgoing).count();
                if pred_out > 1 {
                    chain_starts.push(*node);
                }
            }
        }
        chain_starts.sort_by_key(|n| n.index());

        // 从每个链头沿唯一后继延伸
        let mut visited: std::collections::HashSet<NodeIndex> = std::collections::HashSet::new();
        for start in &chain_starts {
            if visited.contains(start) {
                continue;
            }
            let mut chain: Vec<NodeIndex> = vec![*start];
            visited.insert(*start);
            let mut current = *start;
            loop {
                let succs: Vec<NodeIndex> = dag.neighbors_directed(current, Direction::Outgoing).collect();
                if succs.len() != 1 {
                    break;
                }
                let next = succs[0];
                let next_in = dag.neighbors_directed(next, Direction::Incoming).count();
                if next_in != 1 {
                    break;
                }
                if visited.contains(&next) {
                    break;
                }
                chain.push(next);
                visited.insert(next);
                current = next;
            }

            // 只对长度 >= 2 的链生成目标
            if chain.len() < 2 {
                continue;
            }

            // 连续对共轴：(x[a] - x[b])²
            for pair in chain.windows(2) {
                let a_id = &dag[pair[0]];
                let b_id = &dag[pair[1]];
                let Some(&a_layered) = id_to_layered.get(a_id) else { continue };
                let Some(&b_layered) = id_to_layered.get(b_id) else { continue };
                let Some(&a_var) = node_to_var.get(&a_layered) else { continue };
                let Some(&b_var) = node_to_var.get(&b_layered) else { continue };

                objectives.push(ObjectiveTerm {
                    priority: ObjectivePriority::P1,
                    coefficients: vec![(a_var, 1.0), (b_var, -1.0)],
                    constant: 0.0,
                    weight: weights.chain_align,
                    source: ConstraintSource {
                        kind: ConstraintSourceKind::NodeSeparation,
                        nodes: vec![a_id.clone(), b_id.clone()],
                        note: "linear_chain_coaxis",
                    },
                });
            }
        }
    }

    // ─── P1: Fan 对称 ─────────────────────────────────────────────────────
    // Fan-out: hub 对齐 children 质心；Fan-in: join 对齐 parents 质心。
    if weights.fan_symmetry > 0.0 {
        for node in &sorted_nodes {
            let node_id = &dag[*node];
            let Some(&node_layered) = id_to_layered.get(node_id) else { continue };
            let Some(&node_var) = node_to_var.get(&node_layered) else { continue };

            // Fan-out: 多出度节点对齐后继质心
            let succs: Vec<NodeIndex> = dag.neighbors_directed(*node, Direction::Outgoing).collect();
            if succs.len() >= 2 {
                let mut coefficients: Vec<(VarId, f64)> = Vec::new();
                let n = succs.len() as f64;
                let mut valid = true;
                for succ in &succs {
                    let succ_id = &dag[*succ];
                    let Some(&succ_layered) = id_to_layered.get(succ_id) else { valid = false; break };
                    let Some(&succ_var) = node_to_var.get(&succ_layered) else { valid = false; break };
                    coefficients.push((succ_var, 1.0 / n));
                }
                if valid && !coefficients.is_empty() {
                    coefficients.push((node_var, -1.0));
                    objectives.push(ObjectiveTerm {
                        priority: ObjectivePriority::P1,
                        coefficients,
                        constant: 0.0,
                        weight: weights.fan_symmetry,
                        source: ConstraintSource {
                            kind: ConstraintSourceKind::NodeSeparation,
                            nodes: vec![node_id.clone()],
                            note: "fan_out_centroid",
                        },
                    });
                }
            }

            // Fan-in: 多入度节点对齐前驱质心
            let preds: Vec<NodeIndex> = dag.neighbors_directed(*node, Direction::Incoming).collect();
            if preds.len() >= 2 {
                let mut coefficients: Vec<(VarId, f64)> = Vec::new();
                let n = preds.len() as f64;
                let mut valid = true;
                for pred in &preds {
                    let pred_id = &dag[*pred];
                    let Some(&pred_layered) = id_to_layered.get(pred_id) else { valid = false; break };
                    let Some(&pred_var) = node_to_var.get(&pred_layered) else { valid = false; break };
                    coefficients.push((pred_var, 1.0 / n));
                }
                if valid && !coefficients.is_empty() {
                    coefficients.push((node_var, -1.0));
                    objectives.push(ObjectiveTerm {
                        priority: ObjectivePriority::P1,
                        coefficients,
                        constant: 0.0,
                        weight: weights.fan_symmetry,
                        source: ConstraintSource {
                            kind: ConstraintSourceKind::NodeSeparation,
                            nodes: vec![node_id.clone()],
                            note: "fan_in_centroid",
                        },
                    });
                }
            }
        }
    }

    // ─── P2: Singleton 层对齐 ───────────────────────────────────────────────
    if weights.singleton_align > 0.0 {
        for (layer_idx, layer) in layers.iter().enumerate() {
            let real_nodes: Vec<NodeIndex> = layer
                .iter()
                .filter(|n| matches!(&layered_graph[**n].kind, LayerNodeKind::Real(_)))
                .copied()
                .collect();
            if real_nodes.len() != 1 {
                continue;
            }
            let singleton_node = real_nodes[0];
            let Some(&singleton_var) = node_to_var.get(&singleton_node) else {
                continue;
            };
            let Some(singleton_id) = layered_to_id.get(&singleton_node) else {
                continue;
            };

            // 找 dag 中的对应节点
            let Some(dag_node) = dag.node_indices().find(|&n| dag[n] == *singleton_id) else {
                continue;
            };

            // 收集邻层前驱的初值中心
            let mut target_positions: Vec<f64> = Vec::new();
            for pred in dag.neighbors_directed(dag_node, Direction::Incoming) {
                let pred_id = &dag[pred];
                if let Some(&pred_layer) = id_to_layer.get(pred_id) {
                    if pred_layer == layer_idx.saturating_sub(1) {
                        if let Some(&pred_layered) = id_to_layered.get(pred_id) {
                            if let Some(&pred_var) = node_to_var.get(&pred_layered) {
                                target_positions.push(initial[pred_var]);
                            }
                        }
                    }
                }
            }
            // 无前驱时看后继
            if target_positions.is_empty() {
                for succ in dag.neighbors_directed(dag_node, Direction::Outgoing) {
                    let succ_id = &dag[succ];
                    if let Some(&succ_layer) = id_to_layer.get(succ_id) {
                        if succ_layer == layer_idx + 1 || succ_layer + 1 == layer_idx {
                            if let Some(&succ_layered) = id_to_layered.get(succ_id) {
                                if let Some(&succ_var) = node_to_var.get(&succ_layered) {
                                    target_positions.push(initial[succ_var]);
                                }
                            }
                        }
                    }
                }
            }

            if target_positions.is_empty() {
                continue;
            }

            // 目标 = 中位数
            target_positions.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let target = target_positions[target_positions.len() / 2];

            // (x[singleton] - target)²
            objectives.push(ObjectiveTerm {
                priority: ObjectivePriority::P2,
                coefficients: vec![(singleton_var, 1.0)],
                constant: -target,
                weight: weights.singleton_align,
                source: ConstraintSource {
                    kind: ConstraintSourceKind::NodeSeparation,
                    nodes: vec![singleton_id.clone()],
                    note: "singleton_align",
                },
            });
        }
    }

    // ─── P1: End 节点跟随单前驱 ─────────────────────────────────────────────
    if weights.end_follow > 0.0 {
        let mut sorted_end_ids = end_ids.to_vec();
        sorted_end_ids.sort();

        for end_id in &sorted_end_ids {
            let Some(dag_node) = dag.node_indices().find(|&n| dag[n] == *end_id) else {
                continue;
            };
            let mut preds: Vec<NodeIndex> = dag.neighbors_directed(dag_node, Direction::Incoming).collect();
            preds.sort_by_key(|n| n.index());
            if preds.len() != 1 {
                continue;
            }

            let pred_id = &dag[preds[0]];
            let Some(&pred_layered) = id_to_layered.get(pred_id) else {
                continue;
            };
            let Some(&pred_var) = node_to_var.get(&pred_layered) else {
                continue;
            };
            let Some(&end_layered) = id_to_layered.get(end_id) else {
                continue;
            };
            let Some(&end_var) = node_to_var.get(&end_layered) else {
                continue;
            };

            // (x[end] - x[pred])² → 共轴
            objectives.push(ObjectiveTerm {
                priority: ObjectivePriority::P1,
                coefficients: vec![(end_var, 1.0), (pred_var, -1.0)],
                constant: 0.0,
                weight: weights.end_follow,
                source: ConstraintSource {
                    kind: ConstraintSourceKind::NodeSeparation,
                    nodes: vec![end_id.clone(), pred_id.clone()],
                    note: "end_follow_pred",
                },
            });
        }
    }

    // ─── P1: Pendant 对齐锚点 ───────────────────────────────────────────────
    if weights.pendant_align > 0.0 {
        // 识别 pendant：邻层 Real 邻居恰好 1 个，且该邻居邻层邻居 > 1（枢纽）
        let mut all_ids: Vec<String> = id_to_layer.keys().cloned().collect();
        all_ids.sort();

        // 收集候选 (movable_id, anchor_id, movable_layer)
        let mut candidates: Vec<(String, String, usize)> = Vec::new();

        for movable_id in &all_ids {
            let Some(&movable_layer) = id_to_layer.get(movable_id) else {
                continue;
            };
            let Some(dag_node) = dag.node_indices().find(|&n| dag[n] == *movable_id) else {
                continue;
            };

            // 收集邻层 Real 邻居
            let mut adj_nbrs: Vec<String> = Vec::new();
            for nbr in dag
                .neighbors_directed(dag_node, Direction::Incoming)
                .chain(dag.neighbors_directed(dag_node, Direction::Outgoing))
            {
                let nbr_id = dag[nbr].clone();
                if let Some(&nbr_layer) = id_to_layer.get(&nbr_id) {
                    if nbr_layer.abs_diff(movable_layer) == 1 && !adj_nbrs.contains(&nbr_id) {
                        adj_nbrs.push(nbr_id);
                    }
                }
            }

            if adj_nbrs.len() != 1 {
                continue;
            }
            let anchor_id = &adj_nbrs[0];

            // 验证锚点是枢纽（邻层邻居 > 1）
            let Some(anchor_dag) = dag.node_indices().find(|&n| dag[n] == *anchor_id) else {
                continue;
            };
            let Some(&anchor_layer) = id_to_layer.get(anchor_id) else {
                continue;
            };
            let mut anchor_nbrs: Vec<String> = Vec::new();
            for nbr in dag
                .neighbors_directed(anchor_dag, Direction::Incoming)
                .chain(dag.neighbors_directed(anchor_dag, Direction::Outgoing))
            {
                let nbr_id = dag[nbr].clone();
                if let Some(&nbr_layer) = id_to_layer.get(&nbr_id) {
                    if nbr_layer.abs_diff(anchor_layer) == 1 && !anchor_nbrs.contains(&nbr_id) {
                        anchor_nbrs.push(nbr_id);
                    }
                }
            }
            if anchor_nbrs.len() <= 1 {
                continue;
            }

            candidates.push((movable_id.clone(), anchor_id.clone(), movable_layer));
        }

        // 按 (layer, anchor) 分组
        let mut groups: HashMap<(usize, String), Vec<String>> = HashMap::new();
        for (movable, anchor, layer) in &candidates {
            groups.entry((*layer, anchor.clone())).or_default().push(movable.clone());
        }
        let mut group_keys: Vec<(usize, String)> = groups.keys().cloned().collect();
        group_keys.sort();

        for key in group_keys {
            let Some(pendants) = groups.remove(&key) else { continue };
            let (_, anchor_id) = &key;

            let Some(&anchor_layered) = id_to_layered.get(anchor_id) else { continue };
            let Some(&anchor_var) = node_to_var.get(&anchor_layered) else { continue };

            if pendants.len() == 1 {
                // 单 pendant：直接对齐锚点
                let movable_id = &pendants[0];
                let Some(&movable_layered) = id_to_layered.get(movable_id) else { continue };
                let Some(&movable_var) = node_to_var.get(&movable_layered) else { continue };

                objectives.push(ObjectiveTerm {
                    priority: ObjectivePriority::P1,
                    coefficients: vec![(movable_var, 1.0), (anchor_var, -1.0)],
                    constant: 0.0,
                    weight: weights.pendant_align,
                    source: ConstraintSource {
                        kind: ConstraintSourceKind::NodeSeparation,
                        nodes: vec![movable_id.clone(), anchor_id.clone()],
                        note: "pendant_align_anchor",
                    },
                });
            } else {
                // 多 pendant：组质心对齐锚点
                // (Σ x[Pi]/n - x[A])² = (x[P1]/n + x[P2]/n + ... - x[A])²
                let n = pendants.len() as f64;
                let mut coefficients: Vec<(VarId, f64)> = Vec::new();
                let mut valid = true;
                for pid in &pendants {
                    let Some(&playout) = id_to_layered.get(pid) else { valid = false; break };
                    let Some(&pvar) = node_to_var.get(&playout) else { valid = false; break };
                    coefficients.push((pvar, 1.0 / n));
                }
                if !valid { continue; }
                coefficients.push((anchor_var, -1.0));

                objectives.push(ObjectiveTerm {
                    priority: ObjectivePriority::P1,
                    coefficients,
                    constant: 0.0,
                    weight: weights.pendant_align * 2.0, // 组质心权重更高
                    source: ConstraintSource {
                        kind: ConstraintSourceKind::NodeSeparation,
                        nodes: vec![anchor_id.clone()],
                        note: "pendant_centroid_pack",
                    },
                });
            }
        }
    }

    objectives
}
