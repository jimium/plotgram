//! P0 约束审计器。
//!
//! 在 solver 完成后验证所有硬约束（最小分离）是否满足。
//! 用于冻结点复核，确保 solver 输出可行。

use super::model::{CoordinateProblem, HardConstraint};

/// P0 审计结果。
#[derive(Debug, Clone, Default)]
pub struct AuditReport {
    /// 违反最小分离的节点对数量。
    pub separation_violations: usize,
    /// 最大违反量（像素）。
    pub max_violation: f64,
    /// 违反详情（描述字符串）。
    pub violations: Vec<String>,
}

impl AuditReport {
    /// 是否通过审计（无 P0 违反）。
    pub fn passed(&self) -> bool {
        self.separation_violations == 0
    }
}

/// 审计 solver 输出是否满足所有 P0 硬约束。
///
/// 检查：
/// 1. 层内最小分离（layers[i].separations）
/// 2. 跨层硬约束（hard 中的 MinSeparation）
pub fn audit_p0(problem: &CoordinateProblem, coordinates: &[f64]) -> AuditReport {
    let mut report = AuditReport::default();

    // 1. 检查层内最小分离
    for layer in &problem.layers {
        for i in 0..layer.vars.len().saturating_sub(1) {
            let left_var = layer.vars[i];
            let right_var = layer.vars[i + 1];
            let min_sep = layer.separations.get(i).copied().unwrap_or(0.0);

            let pos_left = coordinates[left_var];
            let pos_right = coordinates[right_var];
            let actual_sep = pos_right - pos_left;

            if actual_sep < min_sep - 1e-6 {
                let violation = min_sep - actual_sep;
                report.separation_violations += 1;
                report.max_violation = report.max_violation.max(violation);
                report.violations.push(format!(
                    "layer {} vars [{},{}]: actual={:.2} required={:.2}",
                    layer.rank, left_var, right_var, actual_sep, min_sep
                ));
            }
        }
    }

    // 2. 检查跨层硬约束
    for constraint in &problem.hard {
        if let HardConstraint::MinSeparation { left, right, distance, .. } = constraint {
            let pos_left = coordinates[*left];
            let pos_right = coordinates[*right];
            let actual_sep = pos_right - pos_left;

            if actual_sep < distance - 1e-6 {
                let violation = distance - actual_sep;
                report.separation_violations += 1;
                report.max_violation = report.max_violation.max(violation);
                report.violations.push(format!(
                    "hard MinSeparation [{},{}]: actual={:.2} required={:.2}",
                    left, right, actual_sep, distance
                ));
            }
        }
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::node::coordinate_solver::model::{
        ConstraintSource, ConstraintSourceKind, CoordinateSolverConfig, InitialCoordinates,
        LayerConstraintSet, NodeVariable, VarKind,
    };

    fn make_var(var_id: usize, rank: usize, order: usize) -> NodeVariable {
        NodeVariable {
            var_id,
            stable_id: format!("n{}", var_id),
            kind: VarKind::Real,
            rank,
            order,
            axis_size: 50.0,
            movable: true,
        }
    }

    fn make_problem(layers: Vec<LayerConstraintSet>) -> CoordinateProblem {
        let num_vars = layers.iter().map(|l| l.vars.len()).sum();
        CoordinateProblem {
            vars: (0..num_vars).map(|i| make_var(i, 0, i)).collect(),
            layers,
            hard: Vec::new(),
            objectives: Vec::new(),
            initial: InitialCoordinates {
                values: vec![0.0; num_vars],
            },
            config: CoordinateSolverConfig::default(),
        }
    }

    #[test]
    fn audit_passes_when_separation_satisfied() {
        let problem = make_problem(vec![LayerConstraintSet {
            rank: 0,
            vars: vec![0, 1, 2],
            separations: vec![50.0, 50.0],
        }]);
        let coords = vec![0.0, 100.0, 200.0];
        let report = audit_p0(&problem, &coords);
        assert!(report.passed());
        assert_eq!(report.separation_violations, 0);
    }

    #[test]
    fn audit_detects_separation_violation() {
        let problem = make_problem(vec![LayerConstraintSet {
            rank: 0,
            vars: vec![0, 1],
            separations: vec![150.0],
        }]);
        let coords = vec![0.0, 100.0];
        let report = audit_p0(&problem, &coords);
        assert!(!report.passed());
        assert_eq!(report.separation_violations, 1);
        assert!((report.max_violation - 50.0).abs() < 1e-6);
    }
}
