//! Architecture 布局的 CoordinateProblem 构建器。
//!
//! Phase A1: 从 architecture 的 layers/sizes/graph 构建 CoordinateProblem，
//! 替代旧的 `assign_coordinates` 中的 resolve_x_overlaps 前扫+后扫。

use std::collections::{HashMap, HashSet};

use crate::layout::kernel::coordinate::model::*;
use crate::layout::node::architecture_v2::layout::constants::NODE_GAP;
use crate::layout::node::architecture_v2::layout::types::{GraphIndex, GroupMap};

/// 构建输出：包含问题 IR 和节点→变量映射。
pub(in crate::layout::node) struct ArchBuildOutput {
    pub problem: CoordinateProblem,
    pub node_to_var: HashMap<String, VarId>,
}

/// 从 architecture 的分层结果构建 CoordinateProblem。
///
/// - 每个节点创建一个变量（kind=Real）。
/// - 每层的相邻变量间编码 MinSeparation（NODE_GAP 或 edge_band_demand gap）。
/// - BK 坐标作为 initial values。
/// - Objectives: P2 边拉直 + P3 BK 位置保持。
pub(in crate::layout::node) fn build_arch_coordinate_problem(
    layers: &[Vec<String>],
    sizes: &HashMap<String, (f64, f64)>,
    centers: &HashMap<String, f64>,
    graph: &GraphIndex,
    reversed: &HashSet<(String, String)>,
    relations: &[crate::ast::Relation],
    diagram_type: &crate::types::DiagramType,
    has_groups: bool,
    group_map: &GroupMap,
) -> ArchBuildOutput {
    let mut vars: Vec<NodeVariable> = Vec::new();
    let mut node_to_var: HashMap<String, VarId> = HashMap::new();
    let mut layer_constraints: Vec<LayerConstraintSet> = Vec::new();
    let mut initial_values: Vec<f64> = Vec::new();

    // edge_band_demand 参数
    let parallel_gap =
        crate::layout::edge::segment_pair::parallel_gap_for_diagram(diagram_type.clone());
    let profile = crate::layout::edge_band_demand::EdgeBandDemandProfile::for_diagram(
        diagram_type.clone(),
        has_groups,
    );

    // 1. 创建变量 + 层约束
    for (rank, layer) in layers.iter().enumerate() {
        let mut layer_vars: Vec<VarId> = Vec::new();
        let mut separations: Vec<f64> = Vec::new();

        for (order, node_id) in layer.iter().enumerate() {
            let var_id = vars.len();
            let (w, _h) = sizes
                .get(node_id)
                .copied()
                .unwrap_or((crate::layout::constants::DEFAULT_NODE_WIDTH, crate::layout::constants::DEFAULT_NODE_HEIGHT));

            vars.push(NodeVariable {
                var_id,
                stable_id: node_id.clone(),
                kind: VarKind::Real,
                rank,
                order,
                axis_size: w,
                movable: true,
            });

            let cx = centers.get(node_id).copied().unwrap_or(0.0);
            initial_values.push(cx);
            node_to_var.insert(node_id.clone(), var_id);
            layer_vars.push(var_id);

            // 计算与前一节点的最小分离
            if order > 0 {
                let prev_id = &layer[order - 1];
                let prev_w = sizes
                    .get(prev_id)
                    .map(|(w, _)| *w)
                    .unwrap_or(crate::layout::constants::DEFAULT_NODE_WIDTH);
                let curr_w = w;

                // 使用 edge_band_demand 的 adjacent_rank_gap（如果启用）
                let gap = if profile.horizontal_max_extra > 0.0 {
                    let layer_ids: HashSet<&str> = layer.iter().map(|s| s.as_str()).collect();
                    crate::layout::edge_band_demand::adjacent_rank_gap(
                        prev_id,
                        node_id,
                        &layer_ids,
                        relations,
                        NODE_GAP,
                        parallel_gap,
                        profile,
                    )
                } else {
                    NODE_GAP
                };

                // separation = prev_half_width + gap + curr_half_width
                let sep = prev_w / 2.0 + gap + curr_w / 2.0;
                separations.push(sep);
            }
        }

        layer_constraints.push(LayerConstraintSet {
            rank,
            vars: layer_vars,
            separations,
        });
    }

    // 2. 构建 objectives
    let mut objectives: Vec<ObjectiveTerm> = Vec::new();

    // P3: PreferBKPosition（保持 BK 初值）
    for (var_id, &init_cx) in initial_values.iter().enumerate() {
        objectives.push(ObjectiveTerm {
            priority: ObjectivePriority::P3,
            coefficients: vec![(var_id, 1.0)],
            constant: -init_cx,
            weight: 1.0,
            source: ConstraintSource {
                kind: ConstraintSourceKind::LayerOrder,
                nodes: vec![vars[var_id].stable_id.clone()],
                note: "prefer BK position",
            },
        });
    }

    // P2: PreferShortHorizontalEdge（边拉直：相邻层连接节点对齐）
    for (rank, layer) in layers.iter().enumerate() {
        if rank + 1 >= layers.len() {
            continue;
        }
        let lower_set: HashSet<&str> = layers[rank + 1].iter().map(|s| s.as_str()).collect();

        for node_id in layer {
            let Some(&var_a) = node_to_var.get(node_id) else {
                continue;
            };
            // 找 effective 下游邻居
            if let Some(succs) = graph.out_edges.get(node_id) {
                for succ in succs {
                    if !lower_set.contains(succ.as_str()) {
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
                    // (x[a] - x[b])² → coefficients: a=1, b=-1, constant=0
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

    // Phase G: P1 hub 居中 + client 对齐 objectives
    if has_groups {
        for (rank, layer) in layers.iter().enumerate() {
            if rank + 1 >= layers.len() {
                continue;
            }
            // 跳过基础设施层（无组归属的层）
            let is_infra = !layer.is_empty()
                && layer.iter().all(|n| !group_map.node_to_top_group.contains_key(n));
            if is_infra {
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
                                    && group_map.node_to_top_group.get(*s) == Some(gid)
                            })
                            .filter_map(|s| node_to_var.get(s).copied())
                            .collect()
                    })
                    .unwrap_or_default();

                if children.len() >= 2 {
                    // (x[hub] - avg(children))²
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

            // P1: client 对齐到唯一 hub 目标
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
                                    && group_map.node_to_top_group.get(client_id)
                                        == group_map.node_to_top_group.get(*s)
                            })
                            .filter_map(|s| node_to_var.get(s).copied())
                            .collect()
                    })
                    .unwrap_or_default();

                if hubs.len() == 1 {
                    // (x[client] - x[hub])²
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
    }

    let problem = CoordinateProblem {
        vars,
        layers: layer_constraints,
        hard: vec![],
        objectives,
        initial: InitialCoordinates { values: initial_values },
        config: CoordinateSolverConfig::default(),
    };

    ArchBuildOutput {
        problem,
        node_to_var,
    }
}
