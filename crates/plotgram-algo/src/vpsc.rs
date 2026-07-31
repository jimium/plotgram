//! Variable Placement with Separation Constraints (1-D).
//!
//! Solves `min Σ wᵢ (xᵢ − dᵢ)²  s.t.  x[right] − x[left] ≥ gap` for a set of
//! separation constraints, via the Dwyer–Marriott–Stuckey active-set method
//! (block merge/split). Entry point: [`solve`]. Deterministic: same input →
//! bit-identical output.
//!
//! Scale: built for layout-sized instances (tens to a few hundred variables
//! per axis — intra-layer spacing, nudging, group borders). The bookkeeping
//! (most-violated scan, multiplier checks) is O(n·m) per round rather than
//! event-queue optimized; do not use it as a large-graph overlap-removal
//! engine without first porting the libvpsc-style priority structures.

/// One solver variable: desired position and (positive) weight.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Variable {
    pub desired: f64,
    pub weight: f64,
}

impl Variable {
    /// Weight-1 variable at `desired`.
    pub fn new(desired: f64) -> Self {
        Self {
            desired,
            weight: 1.0,
        }
    }
}

/// Separation constraint `x[right] - x[left] >= gap`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Constraint {
    pub left: usize,
    pub right: usize,
    pub gap: f64,
}

impl Constraint {
    pub fn new(left: usize, right: usize, gap: f64) -> Self {
        Self { left, right, gap }
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum VpscError {
    /// Contradictory constraints: a positive-gap cycle in the constraint
    /// graph. `constraints` are input indices of the constraints on one
    /// such cycle.
    #[error("infeasible: positive-gap constraint cycle via constraints {constraints:?}")]
    Infeasible { constraints: Vec<usize> },
    /// The merge/split loop exceeded its explicit iteration budget.
    #[error("vpsc iteration limit exceeded")]
    IterationLimit,
    /// Bad input: out-of-range variable index, non-positive / non-finite
    /// weight, non-finite desired position or gap, or self-loop constraint.
    #[error("invalid input: {reason} (constraint/variable index {index})")]
    InvalidInput { reason: &'static str, index: usize },
}

/// Per-variable solver state. Variables in the same block move rigidly:
/// `position(v) = blocks[v.block].position + v.offset`.
#[derive(Debug, Clone)]
struct Var {
    desired: f64,
    weight: f64,
    block: usize,
    offset: f64,
}

/// A block of variables chained together by active constraints.
#[derive(Debug, Clone, Default)]
struct Block {
    /// Member variable indices (kept sorted for determinism).
    vars: Vec<usize>,
    /// Indices (into the sorted constraint list) of active constraints
    /// forming a spanning tree over `vars`.
    active: Vec<usize>,
    position: f64,
    weight_sum: f64,
    /// Σ w_k (desired_k − offset_k); optimal position = wd_sum / weight_sum.
    wd_sum: f64,
}

fn violation(c: &Constraint, pos: impl Fn(usize) -> f64) -> f64 {
    pos(c.left) + c.gap - pos(c.right)
}

/// Solve the VPSC problem. Returns final positions, one per variable.
pub fn solve(vars: &[Variable], constraints: &[Constraint]) -> Result<Vec<f64>, VpscError> {
    validate(vars, constraints)?;
    check_feasible(vars.len(), constraints)?;

    // Stable processing order: by (left, right, input index).
    let mut order: Vec<usize> = (0..constraints.len()).collect();
    order.sort_by_key(|&i| (constraints[i].left, constraints[i].right, i));
    let cons: Vec<Constraint> = order.iter().map(|&i| constraints[i]).collect();

    let mut vs: Vec<Var> = vars
        .iter()
        .map(|v| Var {
            desired: v.desired,
            weight: v.weight,
            block: usize::MAX,
            offset: 0.0,
        })
        .collect();
    let mut blocks: Vec<Block> = Vec::with_capacity(vs.len());
    for (i, v) in vs.iter_mut().enumerate() {
        v.block = i;
        blocks.push(Block {
            vars: vec![i],
            active: Vec::new(),
            position: v.desired,
            weight_sum: v.weight,
            wd_sum: v.weight * v.desired,
        });
    }

    // Explicit budget: guards the merge/split loops against pathological
    // cycling (AGENTS: no silent infinite loops).
    let budget = 8 * (vs.len() + cons.len()) * (cons.len() + 1) + 64;
    let mut used = 0usize;
    const EPS: f64 = 1e-9;

    // Outer fixpoint: satisfy all constraints, then deactivate one active
    // constraint with a negative Lagrange multiplier (the block prefers to
    // stretch there) and re-satisfy. No violation + no negative multiplier
    // ⇒ global optimum of the convex QP.
    loop {
        satisfy(&mut vs, &mut blocks, &cons, EPS, &mut used, budget)?;
        match find_split(&vs, &blocks, &cons, EPS) {
            Some((b, ai)) => {
                split_block(&mut vs, &mut blocks, &cons, b, ai);
                used += 1;
                if used > budget {
                    return Err(VpscError::IterationLimit);
                }
            }
            None => {
                let mut out = vec![0.0; vs.len()];
                for (i, v) in vs.iter().enumerate() {
                    out[i] = blocks[v.block].position + v.offset;
                }
                return Ok(out);
            }
        }
    }
}

/// Merge blocks until no separation constraint is violated.
fn satisfy(
    vs: &mut Vec<Var>,
    blocks: &mut Vec<Block>,
    cons: &[Constraint],
    eps: f64,
    used: &mut usize,
    budget: usize,
) -> Result<(), VpscError> {
    loop {
        *used += 1;
        if *used > budget {
            return Err(VpscError::IterationLimit);
        }
        // Most-violated constraint (stable tie-break by sorted-order index).
        let pos = |v: usize| blocks[vs[v].block].position + vs[v].offset;
        let mut worst: Option<(f64, usize)> = None;
        for (ci, c) in cons.iter().enumerate() {
            let viol = violation(c, pos);
            if viol > eps && worst.map_or(true, |(w, _)| viol > w + eps) {
                worst = Some((viol, ci));
            }
        }
        let Some((_, ci)) = worst else {
            return Ok(());
        };

        let c = &cons[ci];
        let bl = vs[c.left].block;
        let br = vs[c.right].block;
        if bl == br {
            // Violated constraint inside one block ("expand"): break the
            // active path between its endpoints, then immediately re-merge
            // across the violated constraint. Only *forward-oriented* path
            // edges qualify: removing a backward edge and shifting the right
            // side would re-violate it and cycle forever. A forward edge
            // always exists here, otherwise the constraints would form a
            // positive-gap cycle already rejected by the feasibility check.
            let path = active_path(&blocks[bl], cons, c.left, c.right);
            let weakest = path
                .into_iter()
                .filter(|&(_, forward)| forward)
                .map(|(ai, _)| (ai, multiplier(vs, blocks, cons, bl, ai)))
                .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap().then(a.0.cmp(&b.0)))
                .map(|(ai, _)| ai);
            let Some(weakest) = weakest else {
                // A feasible system always has a forward edge here; reaching
                // this branch means a positive-gap cycle below the 1e-12
                // feasibility tolerance slipped past check_feasible. Fail
                // closed with an error instead of panicking.
                return Err(VpscError::IterationLimit);
            };
            split_block(vs, blocks, cons, bl, weakest);
            let (nbl, nbr) = (vs[c.left].block, vs[c.right].block);
            debug_assert_ne!(
                nbl, nbr,
                "splitting a path edge must separate the endpoints"
            );
            merge_blocks(vs, blocks, cons, nbl, nbr, ci);
        } else {
            merge_blocks(vs, blocks, cons, bl, br, ci);
        }
    }
}

fn validate(vars: &[Variable], constraints: &[Constraint]) -> Result<(), VpscError> {
    for (i, v) in vars.iter().enumerate() {
        if !v.weight.is_finite() || v.weight <= 0.0 {
            return Err(VpscError::InvalidInput {
                reason: "weight must be finite and > 0",
                index: i,
            });
        }
        if !v.desired.is_finite() {
            return Err(VpscError::InvalidInput {
                reason: "desired must be finite",
                index: i,
            });
        }
    }
    for (i, c) in constraints.iter().enumerate() {
        if c.left >= vars.len() || c.right >= vars.len() {
            return Err(VpscError::InvalidInput {
                reason: "variable index out of range",
                index: i,
            });
        }
        if c.left == c.right {
            return Err(VpscError::InvalidInput {
                reason: "self-loop constraint",
                index: i,
            });
        }
        if !c.gap.is_finite() {
            return Err(VpscError::InvalidInput {
                reason: "gap must be finite",
                index: i,
            });
        }
    }
    Ok(())
}

/// Detect positive-gap cycles (infeasible) via Bellman–Ford style longest-path
/// relaxation on the constraint graph, then extract one offending cycle.
fn check_feasible(n: usize, constraints: &[Constraint]) -> Result<(), VpscError> {
    if constraints.is_empty() {
        return Ok(());
    }
    // Longest path distances from a virtual source (dist 0 everywhere).
    let mut dist = vec![0.0f64; n];
    let mut pred_con = vec![usize::MAX; n];
    let mut changed_node = usize::MAX;
    for round in 0..n {
        let mut changed = false;
        for (ci, c) in constraints.iter().enumerate() {
            if dist[c.left] + c.gap > dist[c.right] + 1e-12 {
                dist[c.right] = dist[c.left] + c.gap;
                pred_con[c.right] = ci;
                changed = true;
                changed_node = c.right;
            }
        }
        if !changed {
            return Ok(());
        }
        if round == n - 1 {
            // Positive-cycle analogue of Bellman–Ford negative-cycle
            // detection: walk predecessors to land inside a cycle, then
            // collect it. Defensive against dangling predecessors.
            let mut node = changed_node;
            for _ in 0..n {
                if pred_con[node] == usize::MAX {
                    break;
                }
                node = constraints[pred_con[node]].left;
            }
            let start = node;
            let mut cycle = Vec::new();
            let mut cur = node;
            loop {
                let ci = pred_con[cur];
                if ci == usize::MAX || cycle.len() > n {
                    break;
                }
                cycle.push(ci);
                cur = constraints[ci].left;
                if cur == start {
                    break;
                }
            }
            if cycle.is_empty() {
                cycle.push(pred_con[changed_node]);
            }
            cycle.reverse();
            return Err(VpscError::Infeasible { constraints: cycle });
        }
    }
    Ok(())
}

/// Active constraints on the (unique) active-tree path between `from` and
/// `to` inside `block`. Each entry is `(constraint index, forward)` where
/// `forward` means the edge is traversed from its `left` to its `right`
/// endpoint when walking `from` → `to`.
fn active_path(block: &Block, cons: &[Constraint], from: usize, to: usize) -> Vec<(usize, bool)> {
    // BFS over active constraints as undirected edges, remembering the
    // constraint used to reach each variable.
    let mut via: Vec<(usize, usize, usize)> = vec![(from, usize::MAX, usize::MAX)]; // (var, prev var, via ai)
    let mut head = 0;
    while head < via.len() {
        let (v, _, _) = via[head];
        head += 1;
        for &ai in &block.active {
            let c = &cons[ai];
            let other = if c.left == v {
                c.right
            } else if c.right == v {
                c.left
            } else {
                continue;
            };
            if via.iter().any(|&(seen, _, _)| seen == other) {
                continue;
            }
            via.push((other, v, ai));
        }
    }
    // Walk back from `to`. Traversal direction on the path is prev → cur,
    // so the edge is forward iff cons[ai].left == prev.
    let mut path = Vec::new();
    let mut cur = to;
    while cur != from {
        let &(_, prev, ai) = via
            .iter()
            .find(|&&(v, _, _)| v == cur)
            .expect("path endpoint must be reachable in active tree");
        path.push((ai, cons[ai].left == prev));
        cur = prev;
    }
    path.reverse();
    path
}

/// Merge blocks `bl` (containing c.left) and `br` (containing c.right) so
/// that constraint `ci` becomes tight/active.
fn merge_blocks(
    vs: &mut [Var],
    blocks: &mut [Block],
    cons: &[Constraint],
    bl: usize,
    br: usize,
    ci: usize,
) {
    let c = &cons[ci];
    // Shift applied to br members' offsets so that the constraint is tight
    // relative to bl's frame: offset(right) = offset(left) + gap + (old
    // in-block offsets adjustment).
    let shift = vs[c.left].offset + c.gap - vs[c.right].offset;
    let (keep, drop) = (bl, br);
    let drop_vars = std::mem::take(&mut blocks[drop].vars);
    let drop_active = std::mem::take(&mut blocks[drop].active);
    for &v in &drop_vars {
        vs[v].block = keep;
        vs[v].offset += shift;
    }
    blocks[drop].weight_sum = 0.0;
    blocks[drop].wd_sum = 0.0;

    let kb = &mut blocks[keep];
    kb.vars.extend(drop_vars);
    kb.vars.sort_unstable();
    kb.active.extend(drop_active);
    kb.active.push(ci);
    kb.active.sort_unstable();
    kb.weight_sum = 0.0;
    kb.wd_sum = 0.0;
    for &v in &kb.vars {
        kb.weight_sum += vs[v].weight;
        kb.wd_sum += vs[v].weight * (vs[v].desired - vs[v].offset);
    }
    kb.position = kb.wd_sum / kb.weight_sum;
}

/// Lagrange multipliers of the active constraints in block `b`.
///
/// With the active tree, removing constraint `ai` splits the block into the
/// component containing `right` (call it S). Optimality of x wrt the tree
/// gives λ_ai = Σ_{k∈S} w_k (x_k − d_k) (up to sign convention): a negative
/// value means the constraint "wants to stretch" and should be deactivated.
fn multiplier(vs: &[Var], blocks: &[Block], cons: &[Constraint], b: usize, ai: usize) -> f64 {
    let block = &blocks[b];
    let side = component_of(block, cons, ai, cons[ai].right);
    let mut lambda = 0.0;
    for &v in &side {
        let x = block.position + vs[v].offset;
        lambda += vs[v].weight * (x - vs[v].desired);
    }
    lambda
}

/// Members of `block` reachable from `seed` through active constraints,
/// excluding constraint `skip`.
fn component_of(block: &Block, cons: &[Constraint], skip: usize, seed: usize) -> Vec<usize> {
    let mut comp = vec![seed];
    let mut stack = vec![seed];
    while let Some(v) = stack.pop() {
        for &ai in &block.active {
            if ai == skip {
                continue;
            }
            let c = &cons[ai];
            let other = if c.left == v {
                c.right
            } else if c.right == v {
                c.left
            } else {
                continue;
            };
            if !comp.contains(&other) {
                comp.push(other);
                stack.push(other);
            }
        }
    }
    comp.sort_unstable();
    comp
}

/// First (in stable order) active constraint with a negative multiplier.
fn find_split(
    vs: &[Var],
    blocks: &[Block],
    cons: &[Constraint],
    eps: f64,
) -> Option<(usize, usize)> {
    for (b, block) in blocks.iter().enumerate() {
        for &ai in &block.active {
            if multiplier(vs, blocks, cons, b, ai) < -eps {
                return Some((b, ai));
            }
        }
    }
    None
}

/// Split block `b` by deactivating active constraint `ai`. The component on
/// the `right` side becomes a new block; both blocks move to their optima.
fn split_block(
    vs: &mut Vec<Var>,
    blocks: &mut Vec<Block>,
    cons: &[Constraint],
    b: usize,
    ai: usize,
) {
    let right_side = component_of(&blocks[b], cons, ai, cons[ai].right);
    let old_active = std::mem::take(&mut blocks[b].active);
    let all_vars = std::mem::take(&mut blocks[b].vars);
    let old_pos = blocks[b].position;

    let mut left_vars = Vec::new();
    let mut right_vars = Vec::new();
    for v in all_vars {
        if right_side.binary_search(&v).is_ok() {
            right_vars.push(v);
        } else {
            left_vars.push(v);
        }
    }
    let mut left_active = Vec::new();
    let mut right_active = Vec::new();
    for a in old_active {
        if a == ai {
            continue;
        }
        if right_side.binary_search(&cons[a].left).is_ok() {
            right_active.push(a);
        } else {
            left_active.push(a);
        }
    }

    let new_id = blocks.len();
    let rebuilt = |vars: Vec<usize>, active: Vec<usize>, vs: &[Var]| -> Block {
        let mut weight_sum = 0.0;
        let mut wd_sum = 0.0;
        for &v in &vars {
            weight_sum += vs[v].weight;
            wd_sum += vs[v].weight * (vs[v].desired - vs[v].offset);
        }
        Block {
            vars,
            active,
            position: wd_sum / weight_sum,
            weight_sum,
            wd_sum,
        }
    };
    // Keep old positions as the frame; positions are recomputed from
    // desired/offset so both halves land on their own optima.
    let _ = old_pos;
    for &v in &right_vars {
        vs[v].block = new_id;
    }
    let left_block = rebuilt(left_vars, left_active, vs);
    let right_block = rebuilt(right_vars, right_active, vs);
    blocks[b] = left_block;
    blocks.push(right_block);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn positions_ok(vars: &[Variable], cons: &[Constraint], xs: &[f64], eps: f64) {
        for (i, c) in cons.iter().enumerate() {
            assert!(
                xs[c.right] - xs[c.left] >= c.gap - eps,
                "constraint {i} violated: x[{}]={} x[{}]={} gap={}",
                c.left,
                xs[c.left],
                c.right,
                xs[c.right],
                c.gap
            );
        }
        assert_eq!(xs.len(), vars.len());
    }

    fn objective(vars: &[Variable], xs: &[f64]) -> f64 {
        vars.iter()
            .zip(xs)
            .map(|(v, &x)| v.weight * (x - v.desired) * (x - v.desired))
            .sum()
    }

    /// Exact oracle for tiny instances: enumerate subsets of constraints as
    /// the active (tight) set, solve the equality-constrained least squares
    /// via block reasoning, keep feasible candidates, return best objective.
    fn brute_force_optimum(vars: &[Variable], cons: &[Constraint]) -> Option<f64> {
        let n = vars.len();
        let m = cons.len();
        let mut best: Option<f64> = None;
        for mask in 0u32..(1 << m) {
            // Union blocks over the active subset.
            let mut parent: Vec<usize> = (0..n).collect();
            fn find(p: &mut Vec<usize>, i: usize) -> usize {
                if p[i] != i {
                    let r = find(p, p[i]);
                    p[i] = r;
                }
                p[i]
            }
            let active: Vec<usize> = (0..m).filter(|i| mask >> i & 1 == 1).collect();
            // Build offsets via union-find: offset[right] = offset[left] + gap.
            // Inconsistent active sets (cycle with mismatched gaps) → skip mask.
            let mut ok = true;
            let mut off = vec![0.0f64; n];
            for &ci in &active {
                let c = &cons[ci];
                let (rl, rr) = (find(&mut parent, c.left), find(&mut parent, c.right));
                let want = off[c.left] + c.gap; // desired off for right in left's frame
                if rl == rr {
                    if (off[c.right] - want).abs() > 1e-9 {
                        ok = false;
                        break;
                    }
                } else {
                    // attach rr's tree under rl: shift all nodes with root rr
                    let delta = want - off[c.right];
                    for v in 0..n {
                        if find(&mut parent, v) == rr {
                            off[v] += delta;
                        }
                    }
                    parent[rr] = rl;
                }
            }
            if !ok {
                continue;
            }
            // Optimal position per block root.
            let mut xs = vec![0.0f64; n];
            let roots: Vec<usize> = (0..n).map(|v| find(&mut parent, v)).collect();
            let mut uniq = roots.clone();
            uniq.sort_unstable();
            uniq.dedup();
            for &r in &uniq {
                let mut wsum = 0.0;
                let mut wd = 0.0;
                for v in 0..n {
                    if roots[v] == r {
                        wsum += vars[v].weight;
                        wd += vars[v].weight * (vars[v].desired - off[v]);
                    }
                }
                let p = wd / wsum;
                for v in 0..n {
                    if roots[v] == r {
                        xs[v] = p + off[v];
                    }
                }
            }
            // Feasibility of ALL constraints.
            if cons
                .iter()
                .all(|c| xs[c.right] - xs[c.left] >= c.gap - 1e-9)
            {
                let obj = objective(vars, &xs);
                if best.map_or(true, |b| obj < b) {
                    best = Some(obj);
                }
            }
        }
        best
    }

    /// Deterministic LCG for reproducible pseudo-random instances.
    struct Lcg(u64);
    impl Lcg {
        fn next_u64(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            self.0
        }
        fn f64_in(&mut self, lo: f64, hi: f64) -> f64 {
            let u = (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
            lo + u * (hi - lo)
        }
        fn usize_below(&mut self, n: usize) -> usize {
            (self.next_u64() % n as u64) as usize
        }
    }

    #[test]
    fn hand_crafted_cases() {
        struct Case {
            name: &'static str,
            vars: Vec<Variable>,
            cons: Vec<Constraint>,
            /// Expected exact positions (if fully determined).
            expect: Option<Vec<f64>>,
        }
        let cases = vec![
            Case {
                name: "no violation → desired unchanged",
                vars: vec![Variable::new(0.0), Variable::new(10.0)],
                cons: vec![Constraint::new(0, 1, 5.0)],
                expect: Some(vec![0.0, 10.0]),
            },
            Case {
                name: "two vars pushed apart symmetrically",
                vars: vec![Variable::new(0.0), Variable::new(0.0)],
                cons: vec![Constraint::new(0, 1, 4.0)],
                expect: Some(vec![-2.0, 2.0]),
            },
            Case {
                name: "chain squeeze: three coincident vars, gaps 2",
                vars: vec![Variable::new(0.0), Variable::new(0.0), Variable::new(0.0)],
                cons: vec![Constraint::new(0, 1, 2.0), Constraint::new(1, 2, 2.0)],
                expect: Some(vec![-2.0, 0.0, 2.0]),
            },
            Case {
                name: "equality via opposing constraints",
                vars: vec![Variable::new(0.0), Variable::new(6.0)],
                cons: vec![Constraint::new(0, 1, 3.0), Constraint::new(1, 0, -3.0)],
                expect: Some(vec![1.5, 4.5]),
            },
            Case {
                name: "weighted average dominates",
                vars: vec![
                    Variable {
                        desired: 0.0,
                        weight: 3.0,
                    },
                    Variable {
                        desired: 0.0,
                        weight: 1.0,
                    },
                ],
                cons: vec![Constraint::new(0, 1, 4.0)],
                // block optimum p = (3*0 + 1*(0-4))/4 = -1 → x = [-1, 3]
                expect: Some(vec![-1.0, 3.0]),
            },
            Case {
                name: "redundant constraint requiring split",
                // Classic: 0→1 gap 3, 1→2 gap 3, 0→2 gap 3 (redundant).
                // desired [0, 0, 9]: merging greedily can over-chain; the
                // optimal keeps 2 free at 9 once 0,1 settle at [-1.5, 1.5].
                vars: vec![Variable::new(0.0), Variable::new(0.0), Variable::new(9.0)],
                cons: vec![
                    Constraint::new(0, 1, 3.0),
                    Constraint::new(1, 2, 3.0),
                    Constraint::new(0, 2, 3.0),
                ],
                expect: Some(vec![-1.5, 1.5, 9.0]),
            },
        ];
        for case in cases {
            let xs = solve(&case.vars, &case.cons)
                .unwrap_or_else(|e| panic!("{}: solver error {e}", case.name));
            positions_ok(&case.vars, &case.cons, &xs, 1e-9);
            if let Some(exp) = &case.expect {
                for (i, (&got, &want)) in xs.iter().zip(exp).enumerate() {
                    assert!(
                        (got - want).abs() < 1e-9,
                        "{}: x[{i}] = {got}, expected {want} (all: {xs:?})",
                        case.name
                    );
                }
            }
            // objective must match the exact oracle
            let best = brute_force_optimum(&case.vars, &case.cons).expect("feasible");
            let obj = objective(&case.vars, &xs);
            assert!(
                (obj - best).abs() < 1e-9,
                "{}: objective {obj} vs oracle {best} (xs {xs:?})",
                case.name
            );
        }
    }

    #[test]
    fn infeasible_cycle_reported() {
        // x1 - x0 >= 1 and x0 - x1 >= 1 → positive cycle.
        let vars = vec![Variable::new(0.0), Variable::new(0.0)];
        let cons = vec![Constraint::new(0, 1, 1.0), Constraint::new(1, 0, 1.0)];
        match solve(&vars, &cons) {
            Err(VpscError::Infeasible { constraints }) => {
                assert!(!constraints.is_empty());
                for ci in constraints {
                    assert!(ci < cons.len());
                }
            }
            other => panic!("expected Infeasible, got {other:?}"),
        }
    }

    #[test]
    fn random_infeasible_cycles_detected_and_reported() {
        let mut rng = Lcg(0xbad_c1c1e);
        for round in 0..30 {
            let n = 3 + rng.usize_below(10); // 3..=12
            let vars: Vec<Variable> = (0..n)
                .map(|_| Variable::new(rng.f64_in(-10.0, 10.0)))
                .collect();
            // Plant a directed cycle with strictly positive gap sum over a
            // shuffled node subset (Fisher–Yates with the LCG).
            let mut nodes: Vec<usize> = (0..n).collect();
            for i in (1..n).rev() {
                let j = rng.usize_below(i + 1);
                nodes.swap(i, j);
            }
            let k = 2 + rng.usize_below(n - 1); // cycle length 2..=n
            let mut cons = Vec::new();
            for w in 0..k {
                cons.push(Constraint::new(
                    nodes[w],
                    nodes[(w + 1) % k],
                    rng.f64_in(-1.0, 2.0),
                ));
            }
            let sum: f64 = cons.iter().map(|c| c.gap).sum();
            if sum < 0.1 {
                cons.last_mut().unwrap().gap += 0.1 - sum;
            }
            // Acyclic noise constraints on top (left < right cannot cancel
            // the planted infeasibility).
            for _ in 0..rng.usize_below(6) {
                let a = rng.usize_below(n);
                let b = rng.usize_below(n);
                if a == b {
                    continue;
                }
                let (l, r) = if a < b { (a, b) } else { (b, a) };
                cons.push(Constraint::new(l, r, rng.f64_in(0.0, 3.0)));
            }
            match solve(&vars, &cons) {
                Err(VpscError::Infeasible { constraints }) => {
                    // The reported indices must form a genuine positive-gap
                    // cycle: consecutively chained (right == next left,
                    // wrapping) with gap sum > 0.
                    assert!(
                        constraints.len() >= 2,
                        "round {round}: reported cycle too short: {constraints:?}"
                    );
                    let mut gap_sum = 0.0;
                    for w in 0..constraints.len() {
                        let c = &cons[constraints[w]];
                        let next = &cons[constraints[(w + 1) % constraints.len()]];
                        assert_eq!(
                            c.right, next.left,
                            "round {round}: reported edges not chained: {constraints:?}"
                        );
                        gap_sum += c.gap;
                    }
                    assert!(
                        gap_sum > 0.0,
                        "round {round}: reported cycle gap sum {gap_sum} not positive"
                    );
                }
                other => panic!("round {round}: expected Infeasible, got {other:?}"),
            }
        }
    }

    #[test]
    fn invalid_inputs_rejected() {
        let v = Variable::new(0.0);
        let cases: Vec<(Vec<Variable>, Vec<Constraint>)> = vec![
            (
                vec![Variable {
                    desired: 0.0,
                    weight: 0.0,
                }],
                vec![],
            ),
            (
                vec![Variable {
                    desired: f64::NAN,
                    weight: 1.0,
                }],
                vec![],
            ),
            (vec![v, v], vec![Constraint::new(0, 2, 1.0)]),
            (vec![v, v], vec![Constraint::new(1, 1, 1.0)]),
            (vec![v, v], vec![Constraint::new(0, 1, f64::INFINITY)]),
        ];
        for (vars, cons) in cases {
            assert!(matches!(
                solve(&vars, &cons),
                Err(VpscError::InvalidInput { .. })
            ));
        }
    }

    #[test]
    fn random_instances_match_oracle_and_are_feasible() {
        let mut rng = Lcg(0x5eed_2026_0731);
        // Tiny instances: exact oracle comparison.
        for round in 0..60 {
            let n = 2 + rng.usize_below(4); // 2..=5
            let vars: Vec<Variable> = (0..n)
                .map(|_| Variable {
                    desired: rng.f64_in(-10.0, 10.0),
                    weight: rng.f64_in(0.5, 3.0),
                })
                .collect();
            let m = rng.usize_below(6);
            let mut cons = Vec::new();
            for _ in 0..m {
                let a = rng.usize_below(n);
                let b = rng.usize_below(n);
                if a == b {
                    continue;
                }
                // left < right keeps the system acyclic → always feasible.
                let (l, r) = if a < b { (a, b) } else { (b, a) };
                cons.push(Constraint::new(l, r, rng.f64_in(0.0, 5.0)));
            }
            let xs = solve(&vars, &cons).unwrap_or_else(|e| panic!("round {round}: {e}"));
            positions_ok(&vars, &cons, &xs, 1e-9);
            let best = brute_force_optimum(&vars, &cons).expect("feasible");
            let obj = objective(&vars, &xs);
            assert!(
                (obj - best).abs() < 1e-7 * (1.0 + best.abs()),
                "round {round}: objective {obj} vs oracle {best}\nvars {vars:?}\ncons {cons:?}\nxs {xs:?}"
            );
        }
        // Larger instances: feasibility only.
        for round in 0..20 {
            let n = 10 + rng.usize_below(41); // 10..=50
            let vars: Vec<Variable> = (0..n)
                .map(|_| Variable {
                    desired: rng.f64_in(-100.0, 100.0),
                    weight: rng.f64_in(0.5, 4.0),
                })
                .collect();
            let m = 2 * n;
            let mut cons = Vec::new();
            for _ in 0..m {
                let a = rng.usize_below(n);
                let b = rng.usize_below(n);
                if a == b {
                    continue;
                }
                let (l, r) = if a < b { (a, b) } else { (b, a) };
                cons.push(Constraint::new(l, r, rng.f64_in(0.0, 8.0)));
            }
            let xs = solve(&vars, &cons).unwrap_or_else(|e| panic!("big round {round}: {e}"));
            positions_ok(&vars, &cons, &xs, 1e-7);
        }
    }

    #[test]
    fn deterministic_bit_identical_reruns() {
        let mut rng = Lcg(42);
        let n = 30;
        let vars: Vec<Variable> = (0..n)
            .map(|_| Variable {
                desired: rng.f64_in(-50.0, 50.0),
                weight: rng.f64_in(0.5, 2.0),
            })
            .collect();
        let mut cons = Vec::new();
        for _ in 0..60 {
            let a = rng.usize_below(n);
            let b = rng.usize_below(n);
            if a == b {
                continue;
            }
            let (l, r) = if a < b { (a, b) } else { (b, a) };
            cons.push(Constraint::new(l, r, rng.f64_in(0.0, 6.0)));
        }
        let x1 = solve(&vars, &cons).unwrap();
        let x2 = solve(&vars, &cons).unwrap();
        let b1: Vec<u64> = x1.iter().map(|f| f.to_bits()).collect();
        let b2: Vec<u64> = x2.iter().map(|f| f.to_bits()).collect();
        assert_eq!(b1, b2, "solver output must be bit-identical across reruns");
    }
}
