//! 共享 CoordinateProblem 构造辅助（Phase 4.C）。
//!
//! flat Sugiyama / architecture / intra 共用「按层建变量 + 相邻 MinSeparation」逻辑。
//! mindmap 经 `layout_contract::LayoutContract` 编译后走同一路径。

use super::model::{LayerConstraintSet, NodeVariable, VarId, VarKind};

/// 一层节点的尺寸与初始中心。
#[derive(Debug, Clone, Copy)]
pub struct RankNodeSpec<'a> {
    pub stable_id: &'a str,
    pub axis_size: f64,
    pub initial_center: f64,
    pub kind: VarKind,
}

/// 按层追加节点变量，返回每层的 `VarId` 序列。
///
/// `vars` / `initial_values` 就地增长；调用方自行维护 id→var 映射。
pub fn append_rank_layer_vars(
    vars: &mut Vec<NodeVariable>,
    initial_values: &mut Vec<f64>,
    rank: usize,
    nodes: &[RankNodeSpec<'_>],
) -> Vec<VarId> {
    let mut layer_vars = Vec::with_capacity(nodes.len());
    for (order, spec) in nodes.iter().enumerate() {
        let var_id = vars.len();
        vars.push(NodeVariable {
            var_id,
            stable_id: spec.stable_id.to_string(),
            kind: spec.kind,
            rank,
            order,
            axis_size: spec.axis_size,
            movable: true,
        });
        initial_values.push(spec.initial_center);
        layer_vars.push(var_id);
    }
    layer_vars
}

/// 由相邻半宽 + gap 生成层内 `LayerConstraintSet`。
pub fn build_adjacent_min_separations(
    rank: usize,
    layer_vars: Vec<VarId>,
    vars: &[NodeVariable],
    gaps: &[f64],
) -> LayerConstraintSet {
    debug_assert_eq!(
        gaps.len(),
        layer_vars.len().saturating_sub(1),
        "gaps must cover adjacent pairs"
    );
    let mut separations = Vec::with_capacity(gaps.len());
    for i in 0..gaps.len() {
        let left = layer_vars[i];
        let right = layer_vars[i + 1];
        let sep = vars[left].axis_size / 2.0 + gaps[i] + vars[right].axis_size / 2.0;
        separations.push(sep);
    }
    LayerConstraintSet {
        rank,
        vars: layer_vars,
        separations,
    }
}
