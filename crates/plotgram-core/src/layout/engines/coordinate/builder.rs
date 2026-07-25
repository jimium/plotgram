//! 从现有 Sugiyama layers/sizes 构建 CoordinateProblem。
//!
//! 首期（Phase 2）：只构建变量和层内硬约束（MinSeparation），
//! 不添加 objectives。BK 坐标作为 initial。
//! 后续阶段逐步添加 P1/P2/P3 objectives。

use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::HashMap;

use super::model::*;
use crate::layout::engines::layered::graph::{LayerNode, LayerNodeKind};
use crate::layout::engines::layered::preset::SugiyamaPreset;
use crate::layout::kernel::coordinate::builder::{
    append_rank_layer_vars, build_adjacent_min_separations, RankNodeSpec,
};

/// 构建输出：包含问题 IR 和节点→变量映射。
pub(in crate::layout) struct BuildOutput {
    pub problem: CoordinateProblem,
    pub node_to_var: HashMap<NodeIndex, VarId>,
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
    let mut initial_values: Vec<f64> = Vec::new();
    let mut layer_constraints: Vec<LayerConstraintSet> = Vec::with_capacity(layers.len());

    for (rank, layer) in layers.iter().enumerate() {
        // stable_id 缓冲：RankNodeSpec 只借引用。
        let ids: Vec<String> = layer
            .iter()
            .map(|node| match &layered_graph[*node].kind {
                LayerNodeKind::Real(_) => format!("n{}", node.index()),
                LayerNodeKind::Dummy { .. } => format!("d{}", node.index()),
            })
            .collect();
        let rank_specs: Vec<RankNodeSpec<'_>> = layer
            .iter()
            .enumerate()
            .map(|(i, node)| {
                let (w, h) = sizes.get(node).copied().unwrap_or((default_w, default_h));
                let axis_size = if horizontal { h } else { w };
                let kind = match &layered_graph[*node].kind {
                    LayerNodeKind::Real(_) => VarKind::Real,
                    LayerNodeKind::Dummy { .. } => VarKind::Dummy,
                };
                RankNodeSpec {
                    stable_id: &ids[i],
                    axis_size,
                    initial_center: centers.get(node).copied().unwrap_or(0.0),
                    kind,
                }
            })
            .collect();
        let layer_vars = append_rank_layer_vars(&mut vars, &mut initial_values, rank, &rank_specs);
        for (node, &var_id) in layer.iter().zip(layer_vars.iter()) {
            node_to_var.insert(*node, var_id);
        }
        let gaps: Vec<f64> = (0..layer_vars.len().saturating_sub(1))
            .map(|_| preset.node_gap)
            .collect();
        layer_constraints.push(build_adjacent_min_separations(
            rank,
            layer_vars,
            &vars,
            &gaps,
        ));
    }

    BuildOutput {
        problem: CoordinateProblem::build(
            vars,
            layer_constraints,
            Vec::new(),
            Vec::new(),
            InitialCoordinates {
                values: initial_values,
            },
            SolveAxis::Cross,
        ),
        node_to_var,
    }
}
