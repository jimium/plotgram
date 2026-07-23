//! 组内布局的 CoordinateProblem 构建器。
//!
//! 将组内子图（layers + sizes + graph）编译为 CoordinateProblem，
//! 替代旧的 `assign_coordinates_intra`（迭代 neighbor-pull + resolve_x_overlaps）。
//!
//! 与 `arch_builder` 的区别：
//! - 不使用 edge_band_demand（组内节点少，无需通道感知）
//! - 分离距离固定为 NODE_GAP
//! - P1 objectives 包含 hub 居中 + client 对齐（组内语义）

use std::collections::{HashMap, HashSet};

use crate::layout::kernel::coordinate::model::*;
use crate::layout::node::architecture_v2::layout::constants::NODE_GAP;
use crate::layout::node::architecture_v2::layout::types::{GraphIndex, GroupMap};

/// 组内构建输出。
pub(super) struct IntraBuildOutput {
    pub problem: CoordinateProblem,
    pub node_to_var: HashMap<String, VarId>,
}

/// 从组内分层结果构建 CoordinateProblem。
///
/// - 每个节点创建一个变量（kind=Real）。
/// - 每层相邻变量间编码 MinSeparation = prev_half + gap + curr_half（gap 来自 SpaceBudget）。
/// - 初值 = uniform 分布（与旧 assign_coordinates_intra 的 uniform_initial_positions 一致）。
/// - Objectives:
///   - P1: hub 居中到同组子节点质心
///   - P1: client 对齐到唯一 hub
///   - P2: 边拉直（相邻层连接节点对齐）
///   - P3: 保持初值
pub(super) fn build_intra_coordinate_problem(
    layers: &[Vec<String>],
    sizes: &HashMap<String, (f64, f64)>,
    graph: &GraphIndex,
    group_map: &GroupMap,
    member_set: &HashSet<String>,
    reversed: &HashSet<(String, String)>,
    budget: Option<&crate::layout::space_budget::SpaceBudget>,
) -> IntraBuildOutput {
    let mut vars: Vec<NodeVariable> = Vec::new();
    let mut node_to_var: HashMap<String, VarId> = HashMap::new();
    let mut layer_constraints: Vec<LayerConstraintSet> = Vec::new();
    let mut initial_values: Vec<f64> = Vec::new();

    // 1. 创建变量 + 层约束
    for (rank, layer) in layers.iter().enumerate() {
        let mut layer_vars: Vec<VarId> = Vec::new();
        let mut separations: Vec<f64> = Vec::new();

        for (order, node_id) in layer.iter().enumerate() {
            let var_id = vars.len();
            let (w, _h) = sizes
                .get(node_id)
                .copied()
                .unwrap_or((
                    crate::layout::constants::DEFAULT_NODE_WIDTH,
                    crate::layout::constants::DEFAULT_NODE_HEIGHT,
                ));

            vars.push(NodeVariable {
                var_id,
                stable_id: node_id.clone(),
                kind: VarKind::Real,
                rank,
                order,
                axis_size: w,
                movable: true,
            });

            node_to_var.insert(node_id.clone(), var_id);
            layer_vars.push(var_id);

            // 与前一节点的最小分离（使用 SpaceBudget 的 per-pair gap）
            if order > 0 {
                let prev_id = &layer[order - 1];
                let prev_w = sizes
                    .get(prev_id)
                    .map(|(w, _)| *w)
                    .unwrap_or(crate::layout::constants::DEFAULT_NODE_WIDTH);
                let gap = budget
                    .map(|b| b.min_gap(prev_id, node_id))
                    .unwrap_or(NODE_GAP);
                let sep = prev_w / 2.0 + gap + w / 2.0;
                separations.push(sep);
            }
        }

        layer_constraints.push(LayerConstraintSet {
            rank,
            vars: layer_vars,
            separations,
        });
    }

    // 2. 计算初值：uniform 分布（与旧逻辑一致）
    for layer in layers {
        let total_width: f64 = layer
            .iter()
            .map(|n| sizes.get(n).map(|(w, _)| *w).unwrap_or(crate::layout::constants::DEFAULT_NODE_WIDTH))
            .sum::<f64>()
            + NODE_GAP * (layer.len().saturating_sub(1)) as f64;

        let mut cursor = -total_width / 2.0;
        for node_id in layer {
            let w = sizes
                .get(node_id)
                .map(|(w, _)| *w)
                .unwrap_or(crate::layout::constants::DEFAULT_NODE_WIDTH);
            let center = cursor + w / 2.0;
            if let Some(&var_id) = node_to_var.get(node_id) {
                initial_values.resize(var_id + 1, 0.0);
                initial_values[var_id] = center;
            }
            cursor += w + NODE_GAP;
        }
    }

    // 3. 构建 objectives
    let mut objectives: Vec<ObjectiveTerm> = Vec::new();

    // P3: 保持初值
    for (var_id, &init_cx) in initial_values.iter().enumerate() {
        objectives.push(ObjectiveTerm {
            priority: ObjectivePriority::P3,
            coefficients: vec![(var_id, 1.0)],
            constant: -init_cx,
            weight: 1.0,
            source: ConstraintSource {
                kind: ConstraintSourceKind::LayerOrder,
                nodes: vec![vars[var_id].stable_id.clone()],
                note: "prefer initial position",
            },
        });
    }

    // P2: 边拉直（相邻层连接节点对齐）
    for (rank, layer) in layers.iter().enumerate() {
        if rank + 1 >= layers.len() {
            continue;
        }
        let lower_set: HashSet<&str> = layers[rank + 1].iter().map(|s| s.as_str()).collect();

        for node_id in layer {
            let Some(&var_a) = node_to_var.get(node_id) else {
                continue;
            };
            if let Some(succs) = graph.out_edges.get(node_id) {
                for succ in succs {
                    if !lower_set.contains(succ.as_str()) {
                        continue;
                    }
                    if !member_set.contains(succ) {
                        continue;
                    }
                    if !crate::layout::node::architecture_v2::layout::acyclic::is_effective_edge(
                        node_id, succ, reversed,
                    ) {
                        continue;
                    }
                    let Some(&var_b) = node_to_var.get(succ) else {
                        continue;
                    };
                    objectives.push(ObjectiveTerm {
                        priority: ObjectivePriority::P2,
                        coefficients: vec![(var_a, 1.0), (var_b, -1.0)],
                        constant: 0.0,
                        weight: 2.0,
                        source: ConstraintSource {
                            kind: ConstraintSourceKind::NodeSeparation,
                            nodes: vec![node_id.clone(), succ.clone()],
                            note: "edge straightening",
                        },
                    });
                }
            }
        }
    }

    // P1: hub 居中 + client 对齐（组内语义）
    for (rank, layer) in layers.iter().enumerate() {
        if rank + 1 >= layers.len() {
            continue;
        }
        let lower_set: HashSet<&str> = layers[rank + 1].iter().map(|s| s.as_str()).collect();

        // P1: hub 居中到同组子节点质心
        for hub_id in layer {
            let Some(gid) = group_map.node_to_top_group.get(hub_id) else {
                continue;
            };
            let Some(&hub_var) = node_to_var.get(hub_id) else {
                continue;
            };
            let children: Vec<VarId> = graph
                .out_edges
                .get(hub_id)
                .map(|succs| {
                    succs
                        .iter()
                        .filter(|s| {
                            crate::layout::node::architecture_v2::layout::acyclic::is_effective_edge(
                                hub_id, s, reversed,
                            ) && lower_set.contains(s.as_str())
                                && member_set.contains(*s)
                                && group_map.node_to_top_group.get(*s) == Some(gid)
                        })
                        .filter_map(|s| node_to_var.get(s).copied())
                        .collect()
                })
                .unwrap_or_default();

            if children.len() >= 2 {
                let n = children.len() as f64;
                let mut coeffs: Vec<(VarId, f64)> = vec![(hub_var, 1.0)];
                for &cv in &children {
                    coeffs.push((cv, -1.0 / n));
                }
                objectives.push(ObjectiveTerm {
                    priority: ObjectivePriority::P1,
                    coefficients: coeffs,
                    constant: 0.0,
                    weight: 3.0,
                    source: ConstraintSource {
                        kind: ConstraintSourceKind::NodeSeparation,
                        nodes: vec![hub_id.clone()],
                        note: "hub centering over group children",
                    },
                });
            }
        }

        // P1: client 对齐到唯一 hub
        for client_id in layer {
            let Some(&client_var) = node_to_var.get(client_id) else {
                continue;
            };
            let hubs: Vec<VarId> = graph
                .out_edges
                .get(client_id)
                .map(|succs| {
                    succs
                        .iter()
                        .filter(|s| {
                            crate::layout::node::architecture_v2::layout::acyclic::is_effective_edge(
                                client_id, s, reversed,
                            ) && lower_set.contains(s.as_str())
                                && member_set.contains(*s)
                                && group_map.node_to_top_group.get(client_id)
                                    == group_map.node_to_top_group.get(*s)
                        })
                        .filter_map(|s| node_to_var.get(s).copied())
                        .collect()
                })
                .unwrap_or_default();

            if hubs.len() == 1 {
                objectives.push(ObjectiveTerm {
                    priority: ObjectivePriority::P1,
                    coefficients: vec![(client_var, 1.0), (hubs[0], -1.0)],
                    constant: 0.0,
                    weight: 2.0,
                    source: ConstraintSource {
                        kind: ConstraintSourceKind::NodeSeparation,
                        nodes: vec![client_id.clone()],
                        note: "client align to hub",
                    },
                });
            }
        }
    }

    let problem = CoordinateProblem {
        vars,
        layers: layer_constraints,
        hard: vec![],
        objectives,
        initial: InitialCoordinates { values: initial_values },
        config: CoordinateSolverConfig::default(),
    };

    IntraBuildOutput {
        problem,
        node_to_var,
    }
}
