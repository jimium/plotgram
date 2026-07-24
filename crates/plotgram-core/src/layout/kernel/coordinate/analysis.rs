//! 问题结构分析：连通分量分解与稳定签名。
//!
//! - **Component decomposition**：通过 Union-Find 识别变量间的关联关系，
//!   将问题分解为独立子问题。独立分量可分别求解，互不影响。
//! - **Problem signature**：确定性哈希，用于调试和回归检测。

use super::model::*;

// ─── Union-Find ───────────────────────────────────────────────────────────────

struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            rank: vec![0; n],
        }
    }

    fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            self.parent[x] = self.find(self.parent[x]);
        }
        self.parent[x]
    }

    fn union(&mut self, a: usize, b: usize) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra == rb {
            return;
        }
        match self.rank[ra].cmp(&self.rank[rb]) {
            std::cmp::Ordering::Less => self.parent[ra] = rb,
            std::cmp::Ordering::Greater => self.parent[rb] = ra,
            std::cmp::Ordering::Equal => {
                self.parent[rb] = ra;
                self.rank[ra] += 1;
            }
        }
    }
}

// ─── Component Decomposition ──────────────────────────────────────────────────

/// 连通分量分析结果。
#[derive(Debug, Clone)]
pub struct ComponentAnalysis {
    /// 连通分量数量。
    pub count: usize,
    /// 每个变量所属的分量 id（0-based）。
    pub var_component: Vec<usize>,
    /// 每个分量包含的变量数。
    pub component_sizes: Vec<usize>,
}

/// 对 CoordinateProblem 执行连通分量分解。
///
/// 变量关联来源：
/// 1. 同层变量（层内 PAVA 耦合）
/// 2. 硬约束（MinSeparation 的 left/right；Bound/Fixed 的 var）
/// 3. 目标项（同一 ObjectiveTerm 中的所有 var）
pub fn analyze_components(problem: &CoordinateProblem) -> ComponentAnalysis {
    let n = problem.var_count();
    if n == 0 {
        return ComponentAnalysis {
            count: 0,
            var_component: vec![],
            component_sizes: vec![],
        };
    }

    let mut uf = UnionFind::new(n);

    // 1. 同层变量耦合
    for layer in &problem.layers {
        for window in layer.vars.windows(2) {
            uf.union(window[0], window[1]);
        }
    }

    // 2. 硬约束耦合
    for hc in &problem.hard {
        match hc {
            HardConstraint::MinSeparation { left, right, .. } => {
                uf.union(*left, *right);
            }
            HardConstraint::LowerBound { var, .. }
            | HardConstraint::UpperBound { var, .. }
            | HardConstraint::Fixed { var, .. } => {
                // 单变量约束不产生耦合（但标记为自身分量）
                let _ = var;
            }
        }
    }

    // 3. 目标项耦合
    for term in &problem.objectives {
        if term.coefficients.len() >= 2 {
            let first = term.coefficients[0].0;
            for &(var, _) in &term.coefficients[1..] {
                uf.union(first, var);
            }
        }
    }

    // 收集分量
    let mut root_to_id = std::collections::HashMap::new();
    let mut var_component = vec![0usize; n];
    let mut component_sizes: Vec<usize> = Vec::new();

    for i in 0..n {
        let root = uf.find(i);
        let id = *root_to_id.entry(root).or_insert_with(|| {
            component_sizes.push(0);
            component_sizes.len() - 1
        });
        var_component[i] = id;
        component_sizes[id] += 1;
    }

    ComponentAnalysis {
        count: component_sizes.len(),
        var_component,
        component_sizes,
    }
}

// ─── Problem Signature ────────────────────────────────────────────────────────

/// 计算问题的确定性签名（FNV-1a 64-bit hash）。
///
/// 签名覆盖：变量数、层结构、硬约束数与类型、目标数与优先级分布。
/// 用于调试确定性问题和回归检测。
pub fn problem_signature(problem: &CoordinateProblem) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET;

    let mut feed = |bytes: &[u8]| {
        for &b in bytes {
            hash ^= b as u64;
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    };

    // 变量数和结构
    feed(&(problem.vars.len() as u64).to_le_bytes());
    for var in &problem.vars {
        feed(&(var.rank as u64).to_le_bytes());
        feed(&(var.order as u64).to_le_bytes());
        feed(&(var.axis_size.to_bits()).to_le_bytes());
        feed(&[var.movable as u8]);
    }

    // 层结构
    feed(&(problem.layers.len() as u64).to_le_bytes());
    for layer in &problem.layers {
        feed(&(layer.vars.len() as u64).to_le_bytes());
        for sep in &layer.separations {
            feed(&(sep.to_bits()).to_le_bytes());
        }
    }

    // 硬约束
    feed(&(problem.hard.len() as u64).to_le_bytes());
    for hc in &problem.hard {
        match hc {
            HardConstraint::MinSeparation { left, right, distance, .. } => {
                feed(&[1u8]);
                feed(&(*left as u64).to_le_bytes());
                feed(&(*right as u64).to_le_bytes());
                feed(&(distance.to_bits()).to_le_bytes());
            }
            HardConstraint::LowerBound { var, value, .. } => {
                feed(&[2u8]);
                feed(&(*var as u64).to_le_bytes());
                feed(&(value.to_bits()).to_le_bytes());
            }
            HardConstraint::UpperBound { var, value, .. } => {
                feed(&[3u8]);
                feed(&(*var as u64).to_le_bytes());
                feed(&(value.to_bits()).to_le_bytes());
            }
            HardConstraint::Fixed { var, value, .. } => {
                feed(&[4u8]);
                feed(&(*var as u64).to_le_bytes());
                feed(&(value.to_bits()).to_le_bytes());
            }
        }
    }

    // 目标
    feed(&(problem.objectives.len() as u64).to_le_bytes());
    for term in &problem.objectives {
        feed(&[term.priority as u8]);
        feed(&(term.coefficients.len() as u64).to_le_bytes());
        for &(var, coeff) in &term.coefficients {
            feed(&(var as u64).to_le_bytes());
            feed(&(coeff.to_bits()).to_le_bytes());
        }
        feed(&(term.constant.to_bits()).to_le_bytes());
        feed(&(term.weight.to_bits()).to_le_bytes());
    }

    // 初值
    for v in &problem.initial.values {
        feed(&(v.to_bits()).to_le_bytes());
    }

    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_problem() -> CoordinateProblem {
        CoordinateProblem {
            vars: (0..4).map(|i| NodeVariable {
                var_id: i,
                stable_id: format!("n{}", i),
                kind: VarKind::Real,
                rank: i / 2,
                order: i % 2,
                axis_size: 40.0,
                movable: true,
            }).collect(),
            layers: vec![
                LayerConstraintSet { rank: 0, vars: vec![0, 1], separations: vec![50.0] },
                LayerConstraintSet { rank: 1, vars: vec![2, 3], separations: vec![50.0] },
            ],
            hard: vec![],
            objectives: vec![
                ObjectiveTerm {
                    priority: ObjectivePriority::P2,
                    coefficients: vec![(0, 1.0), (2, -1.0)],
                    constant: 0.0,
                    weight: 1.0,
                    source: ConstraintSource { kind: ConstraintSourceKind::NodeSeparation, nodes: vec![], note: "edge" },
                },
            ],
            initial: InitialCoordinates { values: vec![0.0, 60.0, 10.0, 70.0] },
            config: CoordinateSolverConfig::default(),
            axis: Default::default(),
        }
    }

    #[test]
    fn component_single_when_all_linked() {
        let problem = make_test_problem();
        let analysis = analyze_components(&problem);
        // 层内耦合 + 目标耦合 → 所有 4 个变量在同一分量
        assert_eq!(analysis.count, 1);
        assert_eq!(analysis.component_sizes, vec![4]);
    }

    #[test]
    fn component_multiple_when_disconnected() {
        // 两个独立层，无跨层目标
        let problem = CoordinateProblem {
            vars: (0..4).map(|i| NodeVariable {
                var_id: i,
                stable_id: format!("n{}", i),
                kind: VarKind::Real,
                rank: i / 2,
                order: i % 2,
                axis_size: 40.0,
                movable: true,
            }).collect(),
            layers: vec![
                LayerConstraintSet { rank: 0, vars: vec![0, 1], separations: vec![50.0] },
                LayerConstraintSet { rank: 1, vars: vec![2, 3], separations: vec![50.0] },
            ],
            hard: vec![],
            objectives: vec![], // 无跨层目标
            initial: InitialCoordinates { values: vec![0.0, 60.0, 0.0, 60.0] },
            config: CoordinateSolverConfig::default(),
            axis: Default::default(),
        };
        let analysis = analyze_components(&problem);
        assert_eq!(analysis.count, 2);
        assert_eq!(analysis.component_sizes, vec![2, 2]);
    }

    #[test]
    fn signature_deterministic() {
        let p = make_test_problem();
        let s1 = problem_signature(&p);
        let s2 = problem_signature(&p);
        assert_eq!(s1, s2);
        assert_ne!(s1, 0);
    }

    #[test]
    fn signature_changes_with_input() {
        let mut p1 = make_test_problem();
        let s1 = problem_signature(&p1);
        p1.initial.values[0] = 999.0;
        let s2 = problem_signature(&p1);
        assert_ne!(s1, s2);
    }
}
