//! Architecture 布局的 CoordinateProblem 构建器。
//!
//! Phase A1: 从 architecture 的 layers/sizes/graph 构建 CoordinateProblem，
//! 替代旧的 `assign_coordinates` 中的 resolve_x_overlaps 前扫+后扫。

use std::collections::{HashMap, HashSet};

use crate::layout::kernel::coordinate::model::*;
use crate::layout::kernel::coordinate::builder::{
    append_rank_layer_vars, build_adjacent_min_separations, RankNodeSpec,
};
use crate::layout::recipes::architecture::layout::constants::NODE_GAP;
use crate::layout::recipes::architecture::layout::types::{GraphIndex, GroupMap};

/// 构建输出：包含问题 IR 和节点→变量映射。
pub(in crate::layout::recipes) struct ArchBuildOutput {
    pub problem: CoordinateProblem,
    pub node_to_var: HashMap<String, VarId>,
}

/// 从 architecture 的分层结果构建 CoordinateProblem。
///
/// - 每个节点创建一个变量（kind=Real）。
/// - 每层的相邻变量间编码 MinSeparation（NODE_GAP 或 edge_band_demand gap）。
/// - BK 坐标作为 initial values。
/// - Objectives: P2 边拉直 + P3 BK 位置保持。
/// - Phase 5：可选挂 Group IR；SpacingDemandStore / RouteDemand 前馈抬缝。
pub(in crate::layout::recipes) fn build_arch_coordinate_problem(
    layers: &[Vec<String>],
    sizes: &HashMap<String, (f64, f64)>,
    centers: &HashMap<String, f64>,
    graph: &GraphIndex,
    reversed: &HashSet<(String, String)>,
    relations: &[crate::ast::Relation],
    diagram_type: &crate::types::DiagramType,
    has_groups: bool,
    group_map: &GroupMap,
    diagram: Option<&crate::ast::Diagram>,
) -> ArchBuildOutput {
    let mut vars: Vec<NodeVariable> = Vec::new();
    let mut node_to_var: HashMap<String, VarId> = HashMap::new();
    let mut layer_constraints: Vec<LayerConstraintSet> = Vec::new();
    let mut initial_values: Vec<f64> = Vec::new();

    // edge_band_demand 参数
    let parallel_gap =
        crate::layout::routing::segment_pair::parallel_gap_for_diagram(diagram_type.clone());
    let profile = crate::layout::demand::band::EdgeBandDemandProfile::for_diagram(
        diagram_type.clone(),
        has_groups,
    );

    // 1. 创建变量 + 层约束（共享 builder_common）
    for (rank, layer) in layers.iter().enumerate() {
        let rank_specs: Vec<RankNodeSpec<'_>> = layer
            .iter()
            .map(|node_id| {
                let (w, _h) = sizes.get(node_id).copied().unwrap_or((
                    crate::layout::constants::DEFAULT_NODE_WIDTH,
                    crate::layout::constants::DEFAULT_NODE_HEIGHT,
                ));
                RankNodeSpec {
                    stable_id: node_id.as_str(),
                    axis_size: w,
                    initial_center: centers.get(node_id).copied().unwrap_or(0.0),
                    kind: VarKind::Real,
                }
            })
            .collect();
        let layer_vars = append_rank_layer_vars(&mut vars, &mut initial_values, rank, &rank_specs);
        for (node_id, &var_id) in layer.iter().zip(layer_vars.iter()) {
            node_to_var.insert(node_id.clone(), var_id);
        }

        let mut gaps = Vec::with_capacity(layer_vars.len().saturating_sub(1));
        for order in 1..layer.len() {
            let prev_id = &layer[order - 1];
            let node_id = &layer[order];
            let gap = if profile.horizontal_max_extra > 0.0 {
                let layer_ids: HashSet<&str> = layer.iter().map(|s| s.as_str()).collect();
                crate::layout::demand::band::adjacent_rank_gap(
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
            // Phase 5 / D4-6：RouteDemand 前馈——反馈边邻对额外抬缝（不再 route 前二次 solve）
            let route_extra = if reversed.contains(&(prev_id.clone(), node_id.clone()))
                || reversed.contains(&(node_id.clone(), prev_id.clone()))
            {
                12.0
            } else {
                0.0
            };
            gaps.push(gap + route_extra);
        }
        layer_constraints.push(build_adjacent_min_separations(
            rank,
            layer_vars,
            &vars,
            &gaps,
        ));
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
                    if !crate::layout::recipes::architecture::layout::acyclic::is_effective_edge(
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
                                crate::layout::recipes::architecture::layout::acyclic::is_effective_edge(
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
                                crate::layout::recipes::architecture::layout::acyclic::is_effective_edge(
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

    // Phase I: P1 基础设施层居中 objective（L4: 替代 post-layout mutation）
    // 对于无组归属的基础设施层，将每个节点拉向其邻居（上下游）的质心
    for (rank, layer) in layers.iter().enumerate() {
        let is_infra = !layer.is_empty()
            && layer.iter().all(|n| !group_map.node_to_top_group.contains_key(n));
        if !is_infra {
            continue;
        }

        for node_id in layer {
            let Some(&node_var) = node_to_var.get(node_id) else {
                continue;
            };

            // 收集所有 effective 邻居（上游 + 下游）
            let mut neighbors: Vec<VarId> = Vec::new();

            // 上游邻居
            if let Some(preds) = graph.in_edges.get(node_id) {
                for pred in preds {
                    if crate::layout::recipes::architecture::layout::acyclic::is_effective_edge(
                        pred, node_id, reversed,
                    ) {
                        if let Some(&pv) = node_to_var.get(pred) {
                            neighbors.push(pv);
                        }
                    }
                }
            }

            // 下游邻居
            if let Some(succs) = graph.out_edges.get(node_id) {
                for succ in succs {
                    if crate::layout::recipes::architecture::layout::acyclic::is_effective_edge(
                        node_id, succ, reversed,
                    ) {
                        if let Some(&sv) = node_to_var.get(succ) {
                            neighbors.push(sv);
                        }
                    }
                }
            }

            if !neighbors.is_empty() {
                // (x[node] - avg(neighbors))²
                let n = neighbors.len() as f64;
                let mut coeffs: Vec<(VarId, f64)> = vec![(node_var, 1.0)];
                for &nv in &neighbors {
                    coeffs.push((nv, -1.0 / n));
                }
                objectives.push(ObjectiveTerm {
                    priority: ObjectivePriority::P1,
                    coefficients: coeffs,
                    constant: 0.0,
                    weight: 2.5, // 略低于 hub centering，高于 edge straightening
                    source: ConstraintSource {
                        kind: ConstraintSourceKind::NodeSeparation,
                        nodes: vec![node_id.clone()],
                        note: "infrastructure layer centering",
                    },
                });
            }
        }
    }

    let mut problem = CoordinateProblem::build(
        vars,
        layer_constraints,
        vec![],
        objectives,
        InitialCoordinates { values: initial_values },
        SolveAxis::Cross,
    );

    // Phase 5 / D4-2：Group IR 进 Cross 轴求解（G1 shadow；不写回生产框）
    if has_groups {
        if let Some(d) = diagram {
            let pad =
                crate::layout::kernel::common::group_bounds::GroupPadding::architecture();
            let sibling_gap = 40.0;
            crate::layout::kernel::coordinate::group_ir::attach_group_ir(
                &mut problem,
                d,
                &node_to_var,
                pad,
                sibling_gap,
            );
            // G4：跨组边负载 → H-G3 RouteDemand（无几何走廊时用拓扑估计）
            let pair_gaps =
                crate::layout::kernel::coordinate::group_ir::pair_gaps_from_cross_group_edge_loads(
                    d,
                    sibling_gap,
                    crate::layout::demand::CORRIDOR_LANE_PITCH,
                );
            crate::layout::kernel::coordinate::group_ir::draft_boost_h_g3_from_pair_gaps(
                &mut problem,
                &pair_gaps,
            );
        }
    }

    // Phase 5：SpacingDemandStore pair gaps → 抬高层内分离（前馈）
    if let Some(d) = diagram {
        let demand = crate::layout::demand::space_budget::SpaceBudget::from_diagram(d)
            .to_spacing_demand();
        if demand.has_custom_demands() {
            for layer in &mut problem.layers {
                for i in 0..layer.separations.len() {
                    let left_id = &problem.vars[layer.vars[i]].stable_id;
                    let right_id = &problem.vars[layer.vars[i + 1]].stable_id;
                    let need = demand.min_gap(left_id, right_id);
                    let half_sum = problem.vars[layer.vars[i]].axis_size * 0.5
                        + problem.vars[layer.vars[i + 1]].axis_size * 0.5;
                    // separations 存的是中心距；pair_gaps 是外缘距 → 中心距 = half_sum + gap
                    layer.separations[i] = layer.separations[i].max(half_sum + need);
                }
            }
        }
    }

    ArchBuildOutput {
        problem,
        node_to_var,
    }
}
