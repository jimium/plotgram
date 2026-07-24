//! P0 约束审计器。
//!
//! 在 solver 完成后验证所有硬约束是否满足。
//! 用于冻结点复核，确保 solver 输出可行。
//!
//! 审计范围：
//! - 层内最小分离（layers[].separations）
//! - 跨层 MinSeparation
//! - LowerBound / UpperBound / Fixed
//! - movable == false 的变量是否被移动
//! - 非有限值（NaN/Infinity）

use super::model::{CoordinateProblem, HardConstraint};

/// P0 审计结果。
#[derive(Debug, Clone, Default)]
pub struct AuditReport {
    /// 违反最小分离的节点对数量。
    pub separation_violations: usize,
    /// 违反 bounds/fixed 约束的数量。
    pub bound_violations: usize,
    /// 非有限值数量。
    pub non_finite_count: usize,
    /// 最大违反量（像素）。
    pub max_violation: f64,
    /// 违反详情（描述字符串）。
    pub violations: Vec<String>,
}

impl AuditReport {
    /// 是否通过审计（无 P0 违反）。
    pub fn passed(&self) -> bool {
        self.separation_violations == 0
            && self.bound_violations == 0
            && self.non_finite_count == 0
    }

    /// 总违反数。
    pub fn total_violations(&self) -> usize {
        self.separation_violations + self.bound_violations + self.non_finite_count
    }
}

/// 审计 solver 输出是否满足所有 P0 硬约束。
///
/// 检查：
/// 1. 层内最小分离（layers[i].separations）
/// 2. 跨层 MinSeparation
/// 3. LowerBound / UpperBound / Fixed
/// 4. 非有限值（NaN/Infinity）
pub fn audit_p0(problem: &CoordinateProblem, coordinates: &[f64]) -> AuditReport {
    let mut report = AuditReport::default();

    // 0. 检查非有限值
    for (i, &val) in coordinates.iter().enumerate() {
        if !val.is_finite() {
            report.non_finite_count += 1;
            report.max_violation = f64::INFINITY;
            report.violations.push(format!(
                "var {}: non-finite value {:?}",
                i, val
            ));
        }
    }

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
        match constraint {
            HardConstraint::MinSeparation { left, right, distance, .. } => {
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
            HardConstraint::LowerBound { var, value, .. } => {
                if coordinates[*var] < *value - 1e-6 {
                    let violation = *value - coordinates[*var];
                    report.bound_violations += 1;
                    report.max_violation = report.max_violation.max(violation);
                    report.violations.push(format!(
                        "hard LowerBound var {}: actual={:.2} bound={:.2}",
                        var, coordinates[*var], value
                    ));
                }
            }
            HardConstraint::UpperBound { var, value, .. } => {
                if coordinates[*var] > *value + 1e-6 {
                    let violation = coordinates[*var] - *value;
                    report.bound_violations += 1;
                    report.max_violation = report.max_violation.max(violation);
                    report.violations.push(format!(
                        "hard UpperBound var {}: actual={:.2} bound={:.2}",
                        var, coordinates[*var], value
                    ));
                }
            }
            HardConstraint::Fixed { var, value, .. } => {
                let diff = (coordinates[*var] - *value).abs();
                if diff > 1e-6 {
                    report.bound_violations += 1;
                    report.max_violation = report.max_violation.max(diff);
                    report.violations.push(format!(
                        "hard Fixed var {}: actual={:.2} expected={:.2}",
                        var, coordinates[*var], value
                    ));
                }
            }
        }
    }

    report
}

/// 验证 CoordinateProblem IR 结构合法性（solve 前调用）。
///
/// 检查：
/// - var_id == index
/// - initial 长度匹配
/// - layer vars 不重复
/// - separation 长度正确
/// - coefficient var id 合法
/// - 无 NaN/Infinity/负尺寸/负权重
pub fn validate_problem(problem: &CoordinateProblem) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    let n = problem.vars.len();

    // var_id == index
    for (i, var) in problem.vars.iter().enumerate() {
        if var.var_id != i {
            errors.push(format!("var[{}].var_id = {} (expected {})", i, var.var_id, i));
        }
        if !var.axis_size.is_finite() || var.axis_size < 0.0 {
            errors.push(format!("var[{}] invalid axis_size={}", i, var.axis_size));
        }
    }

    // initial 长度
    if problem.initial.values.len() != n {
        errors.push(format!(
            "initial.values len {} != vars len {}",
            problem.initial.values.len(), n
        ));
    }

    // initial 值有限
    for (i, &v) in problem.initial.values.iter().enumerate() {
        if !v.is_finite() {
            errors.push(format!("initial[{}] is not finite", i));
        }
    }

    // layer vars 不重复 + separation 长度
    let mut seen_vars = vec![false; n];
    for layer in &problem.layers {
        if layer.separations.len() != layer.vars.len().saturating_sub(1) {
            errors.push(format!(
                "layer {} separations len {} != vars len {} - 1",
                layer.rank, layer.separations.len(), layer.vars.len()
            ));
        }
        for &var in &layer.vars {
            if var >= n {
                errors.push(format!("layer {} var {} out of range", layer.rank, var));
            } else if seen_vars[var] {
                errors.push(format!("layer {} var {} duplicated", layer.rank, var));
            } else {
                seen_vars[var] = true;
            }
        }
        for (i, &sep) in layer.separations.iter().enumerate() {
            if !sep.is_finite() || sep < 0.0 {
                errors.push(format!("layer {} sep[{}] invalid: {}", layer.rank, i, sep));
            }
        }
    }

    // objective coefficients 合法
    for (ti, term) in problem.objectives.iter().enumerate() {
        if !term.weight.is_finite() || term.weight < 0.0 {
            errors.push(format!("objective[{}] invalid weight={}", ti, term.weight));
        }
        for &(var, coeff) in &term.coefficients {
            if var >= n {
                errors.push(format!("objective[{}] coeff var {} out of range", ti, var));
            }
            if !coeff.is_finite() {
                errors.push(format!("objective[{}] coeff for var {} not finite", ti, var));
            }
        }
        if !term.constant.is_finite() {
            errors.push(format!("objective[{}] constant not finite", ti));
        }
    }

    // hard constraints var id 合法
    for (hi, hc) in problem.hard.iter().enumerate() {
        let check_var = |v: usize, errors: &mut Vec<String>| {
            if v >= n {
                errors.push(format!("hard[{}] var {} out of range", hi, v));
            }
        };
        match hc {
            HardConstraint::MinSeparation { left, right, distance, .. } => {
                check_var(*left, &mut errors);
                check_var(*right, &mut errors);
                if !distance.is_finite() || *distance < 0.0 {
                    errors.push(format!("hard[{}] invalid distance={}", hi, distance));
                }
            }
            HardConstraint::LowerBound { var, value, .. } => {
                check_var(*var, &mut errors);
                if !value.is_finite() {
                    errors.push(format!("hard[{}] invalid lower bound={}", hi, value));
                }
            }
            HardConstraint::UpperBound { var, value, .. } => {
                check_var(*var, &mut errors);
                if !value.is_finite() {
                    errors.push(format!("hard[{}] invalid upper bound={}", hi, value));
                }
            }
            HardConstraint::Fixed { var, value, .. } => {
                check_var(*var, &mut errors);
                if !value.is_finite() {
                    errors.push(format!("hard[{}] invalid fixed value={}", hi, value));
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::kernel::coordinate::model::{
        ConstraintSource, ConstraintSourceKind, CoordinateSolverConfig, InitialCoordinates,
        LayerConstraintSet, NodeVariable, ObjectiveTerm, ObjectivePriority, VarKind,
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
            axis: Default::default(),
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

    #[test]
    fn audit_detects_bound_violations() {
        let mut problem = make_problem(vec![LayerConstraintSet {
            rank: 0,
            vars: vec![0, 1],
            separations: vec![10.0],
        }]);
        let src = ConstraintSource { kind: ConstraintSourceKind::ContainerBound, nodes: vec![], note: "test" };
        problem.hard.push(HardConstraint::LowerBound { var: 0, value: 50.0, source: src.clone() });
        problem.hard.push(HardConstraint::UpperBound { var: 1, value: 80.0, source: src.clone() });
        problem.hard.push(HardConstraint::Fixed { var: 0, value: 60.0, source: src });

        // var 0 = 0.0 violates LowerBound(50) and Fixed(60)
        // var 1 = 100.0 violates UpperBound(80)
        let coords = vec![0.0, 100.0];
        let report = audit_p0(&problem, &coords);
        assert!(!report.passed());
        assert_eq!(report.bound_violations, 3);
    }

    #[test]
    fn audit_detects_non_finite() {
        let problem = make_problem(vec![LayerConstraintSet {
            rank: 0,
            vars: vec![0, 1],
            separations: vec![10.0],
        }]);
        let coords = vec![f64::NAN, 100.0];
        let report = audit_p0(&problem, &coords);
        assert!(!report.passed());
        assert_eq!(report.non_finite_count, 1);
    }

    #[test]
    fn validate_problem_ok() {
        let problem = make_problem(vec![LayerConstraintSet {
            rank: 0,
            vars: vec![0, 1, 2],
            separations: vec![50.0, 50.0],
        }]);
        assert!(validate_problem(&problem).is_ok());
    }

    #[test]
    fn validate_problem_detects_bad_initial_len() {
        let mut problem = make_problem(vec![LayerConstraintSet {
            rank: 0,
            vars: vec![0, 1],
            separations: vec![50.0],
        }]);
        problem.initial.values = vec![0.0]; // wrong length
        let errs = validate_problem(&problem).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("initial.values len")));
    }

    #[test]
    fn validate_problem_detects_negative_weight() {
        let mut problem = make_problem(vec![LayerConstraintSet {
            rank: 0,
            vars: vec![0, 1],
            separations: vec![50.0],
        }]);
        problem.objectives.push(ObjectiveTerm {
            priority: ObjectivePriority::P1,
            coefficients: vec![(0, 1.0)],
            constant: 0.0,
            weight: -1.0,
            source: ConstraintSource { kind: ConstraintSourceKind::LayerOrder, nodes: vec![], note: "" },
        });
        let errs = validate_problem(&problem).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("invalid weight")));
    }
}
