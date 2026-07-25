//! 统一布局契约（工程收口：mindmap / 后续 recipe 语义编译层）。
//!
//! Recipe 只产出 [`LayoutContract`]；内核经 [`CoordinateProblem::from_contract`]
//! 构造求解问题。禁止 recipe 直接拼 `CoordinateProblem { ... }`。

use super::builder_common::{
    append_rank_layer_vars, build_adjacent_min_separations, RankNodeSpec,
};
use super::model::{
    CoordinateProblem, HardConstraint, InitialCoordinates, LayerConstraintSet, ObjectiveTerm,
    SolveAxis, VarKind,
};

/// 一层（rank）上的节点规格。
#[derive(Debug, Clone)]
pub struct ContractRankNode {
    pub stable_id: String,
    pub axis_size: f64,
    pub initial_center: f64,
    pub kind: VarKind,
}

/// 层内相邻分离（长度 = vars.len()-1）。
#[derive(Debug, Clone)]
pub struct ContractLayer {
    pub rank: usize,
    pub nodes: Vec<ContractRankNode>,
    /// 相邻间隙（不含半宽；由 builder 合成 MinSeparation）。
    pub adjacent_gaps: Vec<f64>,
}

/// 语义编译产物：与图类型无关的坐标求解契约。
#[derive(Debug, Clone)]
pub struct LayoutContract {
    pub layers: Vec<ContractLayer>,
    pub hard_constraints: Vec<HardConstraint>,
    pub objectives: Vec<ObjectiveTerm>,
    pub axis: SolveAxis,
}

impl CoordinateProblem {
    /// 由 [`LayoutContract`] 构造求解问题（G5 门面的 contract 入口）。
    pub fn from_contract(contract: LayoutContract) -> Self {
        let mut vars = Vec::new();
        let mut initial_values = Vec::new();
        let mut layer_constraints: Vec<LayerConstraintSet> = Vec::new();

        for layer in &contract.layers {
            let specs: Vec<RankNodeSpec<'_>> = layer
                .nodes
                .iter()
                .map(|n| RankNodeSpec {
                    stable_id: n.stable_id.as_str(),
                    axis_size: n.axis_size,
                    initial_center: n.initial_center,
                    kind: n.kind,
                })
                .collect();
            let layer_vars =
                append_rank_layer_vars(&mut vars, &mut initial_values, layer.rank, &specs);
            layer_constraints.push(build_adjacent_min_separations(
                layer.rank,
                layer_vars,
                &vars,
                &layer.adjacent_gaps,
            ));
        }

        CoordinateProblem::build(
            vars,
            layer_constraints,
            contract.hard_constraints,
            contract.objectives,
            InitialCoordinates {
                values: initial_values,
            },
            contract.axis,
        )
    }
}
