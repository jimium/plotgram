//! PAVA 硬约束投影器。
//!
//! 实现层内最小分离约束的精确投影：
//! 给定一层节点的候选坐标和最小分离距离，
//! 使用 Pool Adjacent Violators Algorithm (PAVA) 在线性时间内
//! 将坐标投影到可行域（所有相邻节点满足最小分离）。
//!
//! 数学原理：
//! 约束 `x[i+1] - x[i] >= d[i]` 通过累计距离变换
//! `y[i] = x[i] - s[i]`（其中 `s[i] = Σ d[0..i-1]`）
//! 转化为单调约束 `y[i+1] >= y[i]`，
//! 即加权 isotonic regression，可用 PAVA 精确求解。
//!
//! ## Dykstra 交替投影
//!
//! 层内 PAVA 与 bounds/fixed/MinSeparation 可能互相冲突。
//! 使用固定轮次 Dykstra 交替投影：
//! ```text
//! loop {
//!   project_all_layers (PAVA)
//!   project_bounds_and_fixed
//!   project_cross_layer_min_separation
//! } until converged or max_rounds
//! ```

use super::model::{CoordinateProblem, HardConstraint, LayerConstraintSet};

/// 对单层执行 PAVA 投影：确保所有相邻变量满足最小分离。
///
/// 输入：`coords` 为当前坐标（会被原地修改），`layer` 为层约束。
/// 返回：active block 数量（被绑定为最小间距的相邻对数）。
pub fn project_layer_separation(
    coords: &mut [f64],
    layer: &LayerConstraintSet,
) -> usize {
    let n = layer.vars.len();
    if n <= 1 {
        return 0;
    }

    // 累计最小距离 s[i]
    let mut s = vec![0.0f64; n];
    for i in 1..n {
        s[i] = s[i - 1] + layer.separations[i - 1];
    }

    // 变换：y[i] = x[var[i]] - s[i]
    let mut y: Vec<f64> = layer.vars.iter().enumerate()
        .map(|(i, &var)| coords[var] - s[i])
        .collect();

    // PAVA：投影到 y[0] <= y[1] <= ... <= y[n-1]
    // 使用 block 结构：每个 block 有 (weighted_sum, weight, start, end)
    let weights = vec![1.0f64; n]; // 等权重（首期）
    let active_count = pava_isotonic(&mut y, &weights);

    // 反变换：x[var[i]] = y[i] + s[i]
    for (i, &var) in layer.vars.iter().enumerate() {
        coords[var] = y[i] + s[i];
    }

    active_count
}

/// PAVA isotonic regression：将 y 投影到 y[0] <= y[1] <= ... <= y[n-1]。
///
/// 返回 active constraint 数量（被合并的相邻对数）。
fn pava_isotonic(y: &mut [f64], weights: &[f64]) -> usize {
    let n = y.len();
    if n <= 1 {
        return 0;
    }

    // Block 结构：(weighted_value_sum, weight_sum, start_index, end_index)
    let mut blocks: Vec<(f64, f64, usize, usize)> = Vec::with_capacity(n);

    for i in 0..n {
        // 新 block
        blocks.push((y[i] * weights[i], weights[i], i, i));

        // 合并违反单调性的相邻 block
        while blocks.len() >= 2 {
            let len = blocks.len();
            let prev_mean = blocks[len - 2].0 / blocks[len - 2].1;
            let curr_mean = blocks[len - 1].0 / blocks[len - 1].1;
            if prev_mean <= curr_mean + 1e-12 {
                break; // 满足单调性
            }
            // 合并：加权平均
            let merged_sum = blocks[len - 2].0 + blocks[len - 1].0;
            let merged_weight = blocks[len - 2].1 + blocks[len - 1].1;
            let start = blocks[len - 2].2;
            let end = blocks[len - 1].3;
            blocks.pop();
            blocks.pop();
            blocks.push((merged_sum, merged_weight, start, end));
        }
    }

    // 回写：每个 block 内所有元素取 block 均值
    let mut active = 0;
    for &(_, weight, start, end) in &blocks {
        let mean = blocks.iter()
            .find(|&&(_, _, s, e)| s == start && e == end)
            .map(|&(sum, w, _, _)| sum / w)
            .unwrap_or(0.0);
        if end > start {
            active += end - start; // 被绑定的相邻对数
        }
        for i in start..=end {
            y[i] = mean;
        }
        let _ = weight; // suppress unused warning
    }

    active
}

/// 对整个问题的所有层执行 PAVA 投影。
///
/// 返回总 active separation 数量。
pub fn project_all_layers(
    coords: &mut [f64],
    problem: &CoordinateProblem,
) -> usize {
    let mut total_active = 0;
    for layer in &problem.layers {
        total_active += project_layer_separation(coords, layer);
    }
    total_active
}

/// 应用 Fixed / LowerBound / UpperBound 硬约束投影。
///
/// 在 PAVA 之后调用，处理跨层/全局约束。
/// 注意：此函数不处理 MinSeparation，需用 `project_hard_constraints`。
pub fn project_bounds(
    coords: &mut [f64],
    problem: &CoordinateProblem,
) {
    for hc in &problem.hard {
        match hc {
            HardConstraint::Fixed { var, value, .. } => {
                coords[*var] = *value;
            }
            HardConstraint::LowerBound { var, value, .. } => {
                if coords[*var] < *value {
                    coords[*var] = *value;
                }
            }
            HardConstraint::UpperBound { var, value, .. } => {
                if coords[*var] > *value {
                    coords[*var] = *value;
                }
            }
            HardConstraint::MinSeparation { .. } => {}
        }
    }
}

/// 投影跨层 MinSeparation 硬约束。
///
/// 对每个 `x[right] - x[left] >= distance` 约束：
/// 若违反，将 right 右推（优先），或 left 左推（若 right 不可移动）。
pub fn project_cross_layer_min_separation(
    coords: &mut [f64],
    problem: &CoordinateProblem,
) -> usize {
    let mut violations_fixed = 0;
    for hc in &problem.hard {
        if let HardConstraint::MinSeparation { left, right, distance, .. } = hc {
            let actual = coords[*right] - coords[*left];
            if actual < *distance - 1e-9 {
                let deficit = *distance - actual;
                let left_movable = problem.vars.get(*left).map(|v| v.movable).unwrap_or(true);
                let right_movable = problem.vars.get(*right).map(|v| v.movable).unwrap_or(true);
                match (left_movable, right_movable) {
                    (true, true) => {
                        // 双方各承担一半
                        coords[*left] -= deficit / 2.0;
                        coords[*right] += deficit / 2.0;
                    }
                    (false, true) => {
                        coords[*right] += deficit;
                    }
                    (true, false) => {
                        coords[*left] -= deficit;
                    }
                    (false, false) => {
                        // 双方都不可移动，无法修复（auditor 会报告）
                    }
                }
                violations_fixed += 1;
            }
        }
    }
    violations_fixed
}

/// Dykstra 交替投影：统一处理所有 P0 硬约束。
///
/// 交替执行：
/// 1. 层内 PAVA 分离投影
/// 2. bounds/fixed 投影
/// 3. 跨层 MinSeparation 投影
///
/// 固定最大轮次，收敛后提前退出。
/// 返回 `(total_rounds, converged)`。
pub fn project_hard_constraints(
    coords: &mut [f64],
    problem: &CoordinateProblem,
) -> (usize, bool) {
    const MAX_ROUNDS: usize = 8;
    const CONVERGENCE_EPS: f64 = 1e-6;

    let has_cross_constraints = problem.hard.iter().any(|hc| {
        matches!(hc, HardConstraint::MinSeparation { .. }
            | HardConstraint::LowerBound { .. }
            | HardConstraint::UpperBound { .. }
            | HardConstraint::Fixed { .. })
    });

    // 无跨层约束时，单次 PAVA + bounds 即可
    if !has_cross_constraints {
        project_all_layers(coords, problem);
        return (1, true);
    }

    let mut prev_coords = coords.to_vec();
    for round in 0..MAX_ROUNDS {
        // Step 1: 层内 PAVA
        project_all_layers(coords, problem);
        // Step 2: bounds/fixed
        project_bounds(coords, problem);
        // Step 3: 跨层 MinSeparation
        project_cross_layer_min_separation(coords, problem);

        // 收敛检测
        let max_delta = coords.iter()
            .zip(prev_coords.iter())
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f64, f64::max);

        if max_delta < CONVERGENCE_EPS {
            return (round + 1, true);
        }
        prev_coords.copy_from_slice(coords);
    }

    (MAX_ROUNDS, false)
}

/// 检查所有层内分离约束是否满足，返回最大违反量。
pub fn max_separation_violation(
    coords: &[f64],
    problem: &CoordinateProblem,
) -> f64 {
    let mut max_violation = 0.0f64;
    for layer in &problem.layers {
        for i in 0..layer.vars.len().saturating_sub(1) {
            let left = coords[layer.vars[i]];
            let right = coords[layer.vars[i + 1]];
            let required = layer.separations[i];
            let actual = right - left;
            if actual < required - 1e-9 {
                max_violation = max_violation.max(required - actual);
            }
        }
    }
    max_violation
}

/// 检查所有硬约束（含 bounds/fixed/MinSeparation）的最大违反量。
pub fn max_hard_violation(
    coords: &[f64],
    problem: &CoordinateProblem,
) -> f64 {
    let mut max_v = max_separation_violation(coords, problem);

    for hc in &problem.hard {
        let v = match hc {
            HardConstraint::MinSeparation { left, right, distance, .. } => {
                let actual = coords[*right] - coords[*left];
                if actual < *distance - 1e-9 { *distance - actual } else { 0.0 }
            }
            HardConstraint::LowerBound { var, value, .. } => {
                if coords[*var] < *value - 1e-9 { *value - coords[*var] } else { 0.0 }
            }
            HardConstraint::UpperBound { var, value, .. } => {
                if coords[*var] > *value + 1e-9 { coords[*var] - *value } else { 0.0 }
            }
            HardConstraint::Fixed { var, value, .. } => {
                (coords[*var] - *value).abs()
            }
        };
        max_v = max_v.max(v);
    }

    max_v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::kernel::coordinate::model::*;

    fn make_layer(vars: Vec<VarId>, separations: Vec<f64>) -> LayerConstraintSet {
        LayerConstraintSet {
            rank: 0,
            vars,
            separations,
        }
    }

    #[test]
    fn pava_no_violation() {
        let layer = make_layer(vec![0, 1, 2], vec![10.0, 10.0]);
        let mut coords = vec![0.0, 20.0, 40.0];
        let active = project_layer_separation(&mut coords, &layer);
        assert_eq!(active, 0);
        assert!((coords[0] - 0.0).abs() < 1e-9);
        assert!((coords[1] - 20.0).abs() < 1e-9);
        assert!((coords[2] - 40.0).abs() < 1e-9);
    }

    #[test]
    fn pava_simple_violation() {
        let layer = make_layer(vec![0, 1, 2], vec![10.0, 10.0]);
        // 节点 1 和 2 太近
        let mut coords = vec![0.0, 15.0, 20.0];
        let active = project_layer_separation(&mut coords, &layer);
        assert!(active > 0);
        // 验证分离约束满足
        assert!(coords[1] - coords[0] >= 10.0 - 1e-9);
        assert!(coords[2] - coords[1] >= 10.0 - 1e-9);
    }

    #[test]
    fn pava_all_overlap() {
        let layer = make_layer(vec![0, 1, 2], vec![10.0, 10.0]);
        // 全部重叠
        let mut coords = vec![5.0, 5.0, 5.0];
        project_layer_separation(&mut coords, &layer);
        assert!(coords[1] - coords[0] >= 10.0 - 1e-9);
        assert!(coords[2] - coords[1] >= 10.0 - 1e-9);
        // 质心应保持不变（PAVA 保均值）
        let mean = (coords[0] + coords[1] + coords[2]) / 3.0;
        assert!((mean - 5.0).abs() < 1e-9);
    }

    #[test]
    fn pava_deterministic() {
        let layer = make_layer(vec![0, 1, 2, 3], vec![8.0, 12.0, 8.0]);
        let input = vec![0.0, 5.0, 10.0, 30.0];
        let mut c1 = input.clone();
        let mut c2 = input.clone();
        project_layer_separation(&mut c1, &layer);
        project_layer_separation(&mut c2, &layer);
        assert_eq!(c1, c2);
    }
}
