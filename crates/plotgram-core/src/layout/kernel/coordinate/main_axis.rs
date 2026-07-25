//! Phase 5 / D4：Main 轴（rank / y）二次求解。
//!
//! 每层一个变量（层顶 y），相邻层 `MinSeparation` = 上层高 + rank 缝。
//! 不与 Cross 同矩阵联合；固定顺序 Cross → Main。

use std::collections::HashMap;

use crate::layout::kernel::coordinate::model::{
    ConstraintSource, ConstraintSourceKind, CoordinateProblem, CoordinateSolverConfig,
    HardConstraint, InitialCoordinates, LayerConstraintSet, NodeVariable, SolveAxis, VarKind,
};

/// 求解各层顶边 y，返回 `layer_y_offsets`（与旧启发式同语义）。
pub fn solve_main_axis_layer_tops(
    layer_heights: &[f64],
    per_layer_gaps: &[f64],
    first_top: f64,
    default_gap: f64,
) -> Vec<f64> {
    let n = layer_heights.len();
    if n == 0 {
        return Vec::new();
    }

    let mut vars = Vec::with_capacity(n);
    let mut initial = Vec::with_capacity(n);
    let mut cursor = first_top;
    for (i, &h) in layer_heights.iter().enumerate() {
        vars.push(NodeVariable {
            var_id: i,
            stable_id: format!("rank#{i}"),
            kind: VarKind::Axis,
            rank: i,
            order: 0,
            axis_size: h,
            movable: i > 0,
        });
        initial.push(cursor);
        if i + 1 < n {
            let gap = per_layer_gaps.get(i).copied().unwrap_or(default_gap);
            cursor += h + gap;
        }
    }

    let mut hard = Vec::new();
    // 首层顶固定
    hard.push(HardConstraint::Fixed {
        var: 0,
        value: first_top,
        source: ConstraintSource {
            kind: ConstraintSourceKind::UserConstraint,
            nodes: vec![],
            note: "main-axis first rank top",
        },
    });
    for i in 0..n.saturating_sub(1) {
        let gap = per_layer_gaps.get(i).copied().unwrap_or(default_gap);
        let distance = layer_heights[i] + gap;
        hard.push(HardConstraint::MinSeparation {
            left: i,
            right: i + 1,
            distance,
            source: ConstraintSource {
                kind: ConstraintSourceKind::SpaceBudget,
                nodes: vec![],
                note: "vertical rank gap (main axis)",
            },
        });
    }

    // 单层一层变量——空 layers，全靠 hard MinSeparation
    let mut problem = CoordinateProblem::build(
        vars,
        vec![LayerConstraintSet {
            rank: 0,
            vars: (0..n).collect(),
            separations: (0..n.saturating_sub(1))
                .map(|i| {
                    let gap = per_layer_gaps.get(i).copied().unwrap_or(default_gap);
                    layer_heights[i] + gap
                })
                .collect(),
        }],
        hard,
        vec![],
        InitialCoordinates { values: initial.clone() },
        SolveAxis::Main,
    );
    problem.config = CoordinateSolverConfig {
        max_iter_p1: 20,
        max_iter_p2: 10,
        max_iter_p3: 10,
        ..CoordinateSolverConfig::default()
    };

    let result = crate::layout::kernel::coordinator::CoordinateKernel::solve(
        "architecture-main-axis",
        &problem,
    );
    if result.coordinates.len() >= n {
        result.coordinates[..n].to_vec()
    } else {
        initial
    }
}

/// 由层顶 y + 层高得到节点中心 y 映射（按 layers）。
pub fn layer_center_ys_from_tops(
    layers: &[Vec<String>],
    layer_tops: &[f64],
    layer_heights: &[f64],
) -> HashMap<String, f64> {
    let mut out = HashMap::new();
    for (i, layer) in layers.iter().enumerate() {
        let top = layer_tops.get(i).copied().unwrap_or(0.0);
        let h = layer_heights.get(i).copied().unwrap_or(0.0);
        let cy = top + h * 0.5;
        for id in layer {
            out.insert(id.clone(), cy);
        }
    }
    out
}
