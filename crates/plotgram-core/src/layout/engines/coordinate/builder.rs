//! 从现有 Sugiyama layers/sizes 构建 CoordinateProblem。
//!
//! 首期（Phase 2）：只构建变量和层内硬约束（MinSeparation），
//! 不添加 objectives。BK 坐标作为 initial。
//! 后续阶段逐步添加 P1/P2/P3 objectives。

use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::HashMap;

use super::model::*;
use crate::layout::node::sugiyama_v2::graph::{LayerNode, LayerNodeKind};
use crate::layout::node::sugiyama_v2::preset::SugiyamaPreset;

/// 构建输出：包含问题 IR 和节点→变量映射。
pub(in crate::layout) struct BuildOutput {
    pub problem: CoordinateProblem,
    pub node_to_var: HashMap<NodeIndex, VarId>,
}

/// 从 Sugiyama 分层结果构建 CoordinateProblem。
///
/// - 每个 Real 节点和 Dummy 节点创建一个变量。
/// - 每层的相邻变量间编码 MinSeparation。
/// - BK 坐标作为 initial values。
pub(in crate::layout) fn build_coordinate_problem(
    layered_graph: &DiGraph<LayerNode, ()>,
    layers: &[Vec<NodeIndex>],
    sizes: &HashMap<NodeIndex, (f64, f64)>,
    centers: &HashMap<NodeIndex, f64>,
    preset: &SugiyamaPreset,
    horizontal: bool,
) -> CoordinateProblem {
    let (default_w, default_h) = preset.default_node_size();

    // 1. 创建变量：按层顺序、层内顺序遍历，保证确定性
    let mut vars: Vec<NodeVariable> = Vec::new();
    let mut node_to_var: HashMap<NodeIndex, VarId> = HashMap::new();
    let mut layer_var_ids: Vec<Vec<VarId>> = Vec::with_capacity(layers.len());

    for (rank, layer) in layers.iter().enumerate() {
        let mut layer_vars = Vec::new();
        for (order, node) in layer.iter().enumerate() {
            let (w, h) = sizes.get(node).copied().unwrap_or((default_w, default_h));
            let axis_size = if horizontal { h } else { w };

            let kind = match &layered_graph[*node].kind {
                LayerNodeKind::Real(_) => VarKind::Real,
                LayerNodeKind::Dummy { .. } => VarKind::Dummy,
            };

            let stable_id = match &layered_graph[*node].kind {
                LayerNodeKind::Real(_) => format!("n{}", node.index()),
                LayerNodeKind::Dummy { .. } => format!("d{}", node.index()),
            };

            let var_id = vars.len();
            vars.push(NodeVariable {
                var_id,
                stable_id,
                kind,
                rank,
                order,
                axis_size,
                movable: true,
            });
            node_to_var.insert(*node, var_id);
            layer_vars.push(var_id);
        }
        layer_var_ids.push(layer_vars);
    }

    // 2. 构建层约束集：相邻变量的最小分离
    let mut layer_constraints: Vec<LayerConstraintSet> = Vec::with_capacity(layers.len());
    for (rank, layer_vars) in layer_var_ids.iter().enumerate() {
        let mut separations = Vec::with_capacity(layer_vars.len().saturating_sub(1));
        for i in 0..layer_vars.len().saturating_sub(1) {
            let left_var = layer_vars[i];
            let right_var = layer_vars[i + 1];
            let left_size = vars[left_var].axis_size;
            let right_size = vars[right_var].axis_size;
            // 最小分离 = 左半宽 + 右半宽 + gap
            let gap = preset.node_gap;
            let sep = left_size / 2.0 + right_size / 2.0 + gap;
            separations.push(sep);
        }
        layer_constraints.push(LayerConstraintSet {
            rank,
            vars: layer_vars.clone(),
            separations,
        });
    }

    // 3. 初值：BK 坐标
    let mut initial_values = vec![0.0f64; vars.len()];
    for (node, &var_id) in &node_to_var {
        if let Some(&center) = centers.get(node) {
            initial_values[var_id] = center;
        }
    }

    CoordinateProblem {
        vars,
        layers: layer_constraints,
        hard: Vec::new(), // 首期无跨层硬约束
        objectives: Vec::new(), // 首期无 objectives（Phase 3 添加）
        initial: InitialCoordinates {
            values: initial_values,
        },
        config: CoordinateSolverConfig::default(),
    }
}

/// 构建 CoordinateProblem 并返回 node_to_var 映射。
pub(in crate::layout) fn build_with_mapping(
    layered_graph: &DiGraph<LayerNode, ()>,
    layers: &[Vec<NodeIndex>],
    sizes: &HashMap<NodeIndex, (f64, f64)>,
    centers: &HashMap<NodeIndex, f64>,
    preset: &SugiyamaPreset,
    horizontal: bool,
) -> BuildOutput {
    let (default_w, default_h) = preset.default_node_size();

    let mut vars: Vec<NodeVariable> = Vec::new();
    let mut node_to_var: HashMap<NodeIndex, VarId> = HashMap::new();
    let mut layer_var_ids: Vec<Vec<VarId>> = Vec::with_capacity(layers.len());

    for (rank, layer) in layers.iter().enumerate() {
        let mut layer_vars = Vec::new();
        for (order, node) in layer.iter().enumerate() {
            let (w, h) = sizes.get(node).copied().unwrap_or((default_w, default_h));
            let axis_size = if horizontal { h } else { w };

            let kind = match &layered_graph[*node].kind {
                LayerNodeKind::Real(_) => VarKind::Real,
                LayerNodeKind::Dummy { .. } => VarKind::Dummy,
            };

            let stable_id = match &layered_graph[*node].kind {
                LayerNodeKind::Real(_) => format!("n{}", node.index()),
                LayerNodeKind::Dummy { .. } => format!("d{}", node.index()),
            };

            let var_id = vars.len();
            vars.push(NodeVariable {
                var_id,
                stable_id,
                kind,
                rank,
                order,
                axis_size,
                movable: true,
            });
            node_to_var.insert(*node, var_id);
            layer_vars.push(var_id);
        }
        layer_var_ids.push(layer_vars);
    }

    let mut layer_constraints: Vec<LayerConstraintSet> = Vec::with_capacity(layers.len());
    for (rank, layer_vars) in layer_var_ids.iter().enumerate() {
        let mut separations = Vec::with_capacity(layer_vars.len().saturating_sub(1));
        for i in 0..layer_vars.len().saturating_sub(1) {
            let left_var = layer_vars[i];
            let right_var = layer_vars[i + 1];
            let left_size = vars[left_var].axis_size;
            let right_size = vars[right_var].axis_size;
            let gap = preset.node_gap;
            let sep = left_size / 2.0 + right_size / 2.0 + gap;
            separations.push(sep);
        }
        layer_constraints.push(LayerConstraintSet {
            rank,
            vars: layer_vars.clone(),
            separations,
        });
    }

    let mut initial_values = vec![0.0f64; vars.len()];
    for (node, &var_id) in &node_to_var {
        if let Some(&center) = centers.get(node) {
            initial_values[var_id] = center;
        }
    }

    BuildOutput {
        problem: CoordinateProblem {
            vars,
            layers: layer_constraints,
            hard: Vec::new(),
            objectives: Vec::new(),
            initial: InitialCoordinates { values: initial_values },
            config: CoordinateSolverConfig::default(),
        },
        node_to_var,
    }
}

/// 从 CoordinateProblem 求解结果中提取 Real 节点的中心坐标。
///
/// 返回 NodeIndex → center 的映射，可直接替换旧 centers。
pub(in crate::layout) fn extract_centers(
    problem: &CoordinateProblem,
    coords: &[f64],
    layered_graph: &DiGraph<LayerNode, ()>,
    layers: &[Vec<NodeIndex>],
) -> HashMap<NodeIndex, f64> {
    let mut result = HashMap::new();
    for layer in layers {
        for node in layer {
            if matches!(&layered_graph[*node].kind, LayerNodeKind::Real(_)) {
                // 找到对应的 var_id
                for var in &problem.vars {
                    if var.stable_id == format!("n{}", node.index()) {
                        result.insert(*node, coords[var.var_id]);
                        break;
                    }
                }
            }
        }
    }
    result
}
