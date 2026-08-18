//! Variable Placement with Separation Constraints (1-D).
//!
//! Solves `min Σ wᵢ (xᵢ − dᵢ)²  s.t.  x[right] − x[left] ≥ gap` for a set of
//! separation constraints, via the Dwyer–Marriott–Stuckey active-set method
//! (block merge/split). Entry point: [`solve`]. Deterministic: same input →
//! bit-identical output.
//!
//! Scale: built for layout-sized instances (tens to a few hundred variables
//! per axis — intra-layer spacing, nudging, group borders). The bookkeeping
//! (most-violated scan) is O(m) per merge round; block traversals run in
//! O(A+B) via a per-call adjacency index (A = active constraints, B = block
//! vars). Do not use it as a large-graph overlap-removal engine without
//! first porting the libvpsc-style priority structures.

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

/// Constraint set preprocessed for repeated solves: validation and
/// feasibility are proven once, and the sorted processing order is frozen.
/// `solve_prepared(&p, vars)` is bit-identical to `solve(vars, original)`.
#[derive(Debug, Clone)]
pub struct Prepared {
    cons: Vec<Constraint>,
}

impl Prepared {
    /// Sorted constraint list (the exact order `solve` would use).
    pub fn constraints(&self) -> &[Constraint] {
        &self.cons
    }
}

/// Validate + feasibility-check + freeze the processing order of a
/// constraint set. Reuse the returned [`Prepared`] across solves that share
/// the same constraints (only desired/weights change) to skip the O(m log m)
/// sort and the Bellman–Ford feasibility scan per solve.
pub fn prepare(n_vars: usize, constraints: &[Constraint]) -> Result<Prepared, VpscError> {
    // Variable-side validation happens per solve (desired/weights change);
    // here we only check the constraint side against `n_vars`.
    for (i, c) in constraints.iter().enumerate() {
        if c.left >= n_vars || c.right >= n_vars {
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
    check_feasible(n_vars, constraints)?;
    // Stable processing order: by (left, right, input index). Keys are
    // unique (i is), so the unstable sort yields the identical order.
    let mut order: Vec<usize> = (0..constraints.len()).collect();
    order.sort_unstable_by_key(|&i| (constraints[i].left, constraints[i].right, i));
    let cons: Vec<Constraint> = order.iter().map(|&i| constraints[i]).collect();
    Ok(Prepared { cons })
}

/// Solve using a preprocessed constraint set. See [`prepare`].
pub fn solve_prepared(p: &Prepared, vars: &[Variable]) -> Result<Vec<f64>, VpscError> {
    solve_inner(vars, &p.cons)
}

/// Reusable working memory for repeated solves (see
/// [`solve_prepared_with`]): variable/block state and traversal scratch,
/// plus a pool of index vectors so the hot merge/split loop runs without
/// per-solve allocations. Reset on every call; iteration orders and
/// arithmetic are identical to [`solve_prepared`].
#[derive(Default)]
pub struct SolveBufs {
    vs: Vec<Var>,
    blocks: Vec<Block>,
    scratch: Scratch,
}

/// [`solve_prepared`] over caller-owned buffers, for loops that solve the
/// same (or different) prepared sets many times. Bit-identical results;
/// only allocation behavior differs.
pub fn solve_prepared_with(
    p: &Prepared,
    vars: &[Variable],
    bufs: &mut SolveBufs,
) -> Result<Vec<f64>, VpscError> {
    solve_inner_with(vars, &p.cons, bufs)
}

/// Solve the VPSC problem. Returns final positions, one per variable.
pub fn solve(vars: &[Variable], constraints: &[Constraint]) -> Result<Vec<f64>, VpscError> {
    let p = prepare(vars.len(), constraints)?;
    solve_inner(vars, &p.cons)
}

fn solve_inner(vars: &[Variable], cons: &[Constraint]) -> Result<Vec<f64>, VpscError> {
    let mut bufs = SolveBufs {
        vs: Vec::with_capacity(vars.len()),
        blocks: Vec::with_capacity(vars.len()),
        scratch: Scratch::new(vars.len()),
    };
    solve_inner_with(vars, cons, &mut bufs)
}

fn solve_inner_with(
    vars: &[Variable],
    cons: &[Constraint],
    bufs: &mut SolveBufs,
) -> Result<Vec<f64>, VpscError> {
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

    let n = vars.len();
    let SolveBufs {
        vs,
        blocks,
        scratch,
    } = bufs;
    // Reset per-solve state, recycling the previous call's buffers:
    // block var/active lists go back to the pool (cleared on reuse), the
    // flat caches are zeroed for this call's variable count.
    for b in blocks.drain(..) {
        scratch.give_pool(b.vars);
        scratch.give_pool(b.active);
    }
    vs.clear();
    vs.reserve(n);
    for v in vars {
        vs.push(Var {
            desired: v.desired,
            weight: v.weight,
            block: usize::MAX,
            offset: 0.0,
        });
    }
    blocks.reserve(n);
    for i in 0..n {
        vs[i].block = i;
        let mut vars_i = scratch.pool.pop().unwrap_or_default();
        vars_i.clear();
        vars_i.push(i);
        let (desired, weight) = (vs[i].desired, vs[i].weight);
        blocks.push(Block {
            vars: vars_i,
            active: Vec::new(),
            position: desired,
            weight_sum: weight,
            wd_sum: weight * desired,
        });
    }
    scratch.reset(n);
    for i in 0..n {
        scratch.pos[i] = blocks[i].position;
    }

    // Explicit budget: guards the merge/split loops against pathological
    // cycling (AGENTS: no silent infinite loops).
    let budget = 8 * (n + cons.len()) * (cons.len() + 1) + 64;
    let mut used = 0usize;
    const EPS: f64 = 1e-9;

    // Outer fixpoint: satisfy all constraints, then deactivate one active
    // constraint with a negative Lagrange multiplier (the block prefers to
    // stretch there) and re-satisfy. No violation + no negative multiplier
    // ⇒ global optimum of the convex QP.
    loop {
        satisfy(vs, blocks, cons, EPS, &mut used, budget, scratch)?;
        match find_split(vs, blocks, cons, EPS, scratch) {
            Some((b, ai)) => {
                split_block(vs, blocks, cons, b, ai, scratch);
                used += 1;
                if used > budget {
                    return Err(VpscError::IterationLimit);
                }
            }
            None => {
                let mut out = vec![0.0; n];
                for (i, v) in vs.iter().enumerate() {
                    out[i] = blocks[v.block].position + v.offset;
                }
                return Ok(out);
            }
        }
    }
}

/// Reusable per-solve buffers: adjacency lists, visited flags and traversal
/// stacks. Shared across `satisfy` / `find_split` / `split_block` so the hot
/// merge/split loop performs no per-round allocations. Semantics are
/// untouched: every traversal walks the same neighbor order and every sum
/// runs over the same sorted component as the from-scratch version.
#[derive(Default)]
struct Scratch {
    adj: Vec<Vec<usize>>,
    seen: Vec<bool>,
    comp: Vec<usize>,
    stack: Vec<usize>,
    /// Temp for the sorted-merge in [`merge_blocks`].
    merge_buf: Vec<usize>,
    /// Cached per-var absolute position `block.position + offset`, kept in
    /// sync by [`merge_blocks`] / [`split_block`] so the satisfy scan reads
    /// flat memory instead of chasing two pointers per endpoint.
    pos: Vec<f64>,
    /// Per-block "verified negative-multiplier-free" flag (see
    /// [`find_split`]). Cleared whenever the block's state changes.
    clean: Vec<bool>,
    /// Recycled index vectors (block `vars` / `active` lists). Buffers are
    /// cleared on checkout ([`Scratch::take_pool`]) so reuse is invisible
    /// to the algorithm.
    pool: Vec<Vec<usize>>,
}

impl Scratch {
    fn new(n: usize) -> Self {
        Self {
            adj: vec![Vec::new(); n],
            seen: vec![false; n],
            comp: Vec::new(),
            stack: Vec::new(),
            merge_buf: Vec::new(),
            pos: vec![0.0; n],
            clean: vec![false; n],
            pool: Vec::new(),
        }
    }

    /// Reset the per-call flat caches to `n` variables. Traversal buffers
    /// (`comp` / `stack` / `merge_buf` / `adj`) are empty by exit invariant;
    /// error paths can leave `adj` dirty, so clear defensively.
    fn reset(&mut self, n: usize) {
        // Error paths can leave `adj` lists filled; clear defensively and
        // size every flat cache to this call's variable count.
        self.adj.truncate(n);
        self.adj.resize_with(n, Vec::new);
        for slot in self.adj.iter_mut() {
            slot.clear();
        }
        self.seen.clear();
        self.seen.resize(n, false);
        self.pos.clear();
        self.pos.resize(n, 0.0);
        self.clean.clear();
        self.clean.resize(n, false);
        self.comp.clear();
        self.stack.clear();
        self.merge_buf.clear();
    }

    fn take_pool(&mut self) -> Vec<usize> {
        let mut v = self.pool.pop().unwrap_or_default();
        v.clear();
        v
    }

    fn give_pool(&mut self, v: Vec<usize>) {
        if !v.is_empty() {
            self.pool.push(v);
        }
    }
}

/// Fill `adj` for `block`'s active tree: per-variable lists of active
/// constraint indices, pushed in `block.active`'s sorted order — exactly the
/// neighbor order the legacy from-scratch build produced, keeping paths and
/// multiplier values bit-identical while costing O(A) instead of O(n).
/// Callers must [`clear_adj`] the same block when done.
fn fill_adj(scratch: &mut Scratch, block: &Block, cons: &[Constraint]) {
    for &ai in &block.active {
        let c = &cons[ai];
        scratch.adj[c.left].push(ai);
        scratch.adj[c.right].push(ai);
    }
}

/// Reset only the per-variable lists [`fill_adj`] touched (capacity is kept,
/// so the hot merge/split loop runs allocation-free).
fn clear_adj(adj: &mut [Vec<usize>], active: &[usize], cons: &[Constraint]) {
    for &ai in active {
        let c = &cons[ai];
        adj[c.left].clear();
        adj[c.right].clear();
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
    scratch: &mut Scratch,
) -> Result<(), VpscError> {
    loop {
        *used += 1;
        if *used > budget {
            return Err(VpscError::IterationLimit);
        }
        // Most-violated constraint (stable tie-break by sorted-order index).
        // `pos` is the flat per-var cache — same values, same fold order.
        let pos = &scratch.pos;
        let mut worst: Option<(f64, usize)> = None;
        for (ci, c) in cons.iter().enumerate() {
            let viol = pos[c.left] + c.gap - pos[c.right];
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
            let path = active_path(&blocks[bl], cons, c.left, c.right, scratch);
            fill_adj(scratch, &blocks[bl], cons);
            let Scratch {
                adj, seen, stack, ..
            } = scratch;
            let weakest = path
                .into_iter()
                .filter(|&(_, forward)| forward)
                .map(|(ai, _)| (ai, multiplier(vs, &blocks[bl], cons, adj, seen, stack, ai)))
                .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap().then(a.0.cmp(&b.0)))
                .map(|(ai, _)| ai);
            clear_adj(adj, &blocks[bl].active, cons);
            let Some(weakest) = weakest else {
                // A feasible system always has a forward edge here; reaching
                // this branch means a positive-gap cycle below the 1e-12
                // feasibility tolerance slipped past check_feasible. Fail
                // closed with an error instead of panicking.
                return Err(VpscError::IterationLimit);
            };
            split_block(vs, blocks, cons, bl, weakest, scratch);
            let (nbl, nbr) = (vs[c.left].block, vs[c.right].block);
            debug_assert_ne!(
                nbl, nbr,
                "splitting a path edge must separate the endpoints"
            );
            merge_blocks(vs, blocks, cons, nbl, nbr, ci, scratch);
        } else {
            merge_blocks(vs, blocks, cons, bl, br, ci, scratch);
        }
    }
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
/// endpoint when walking `from` → `to`. BFS discovery order (neighbors in
/// `active` order) and the resulting path are identical to the legacy
/// from-scratch version; the back-walk follows parent queue indices instead
/// of re-scanning the queue (O(path) instead of O(path²)).
fn active_path(
    block: &Block,
    cons: &[Constraint],
    from: usize,
    to: usize,
    scratch: &mut Scratch,
) -> Vec<(usize, bool)> {
    fill_adj(scratch, block, cons);
    let Scratch { adj, seen, .. } = scratch;
    seen[from] = true;
    // (var, parent queue index, via ai); `from` sits at index 0.
    let mut via: Vec<(usize, usize, usize)> = vec![(from, usize::MAX, usize::MAX)];
    let mut head = 0;
    while head < via.len() {
        let (v, _, _) = via[head];
        let parent = head;
        head += 1;
        if v == to {
            break;
        }
        for &ai in &adj[v] {
            let c = &cons[ai];
            let other = if c.left == v {
                c.right
            } else if c.right == v {
                c.left
            } else {
                continue;
            };
            if seen[other] {
                continue;
            }
            seen[other] = true;
            via.push((other, parent, ai));
        }
    }
    // Walk back from `to` by parent index. Traversal direction on the path
    // is parent → child, so the edge is forward iff cons[ai].left == parent.
    let mut path = Vec::new();
    let mut cur = to;
    while cur != from {
        let &(_, parent, ai) = via
            .iter()
            .find(|&&(v, _, _)| v == cur)
            .expect("path endpoint must be reachable in active tree");
        path.push((ai, cons[ai].left == via[parent].0));
        cur = via[parent].0;
    }
    path.reverse();
    for &(v, _, _) in &via {
        seen[v] = false;
    }
    clear_adj(adj, &block.active, cons);
    path
}

/// Merge two sorted, disjoint index lists into `buf` (left in arbitrary
/// order). Identical result to concatenation + sort for unique elements,
/// but linear.
fn merge_sorted(buf: &mut Vec<usize>, a: &[usize], b: &[usize]) {
    buf.clear();
    buf.reserve(a.len() + b.len());
    let (mut i, mut j) = (0usize, 0usize);
    while i < a.len() && j < b.len() {
        if a[i] < b[j] {
            buf.push(a[i]);
            i += 1;
        } else {
            buf.push(b[j]);
            j += 1;
        }
    }
    buf.extend_from_slice(&a[i..]);
    buf.extend_from_slice(&b[j..]);
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
    scratch: &mut Scratch,
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

    // Both sides keep `vars` / `active` sorted (merge invariant; `ci` is a
    // fresh index), so a linear merge reproduces the sorted order exactly.
    {
        let Scratch { merge_buf, .. } = scratch;
        let kb = &mut blocks[keep];
        merge_sorted(merge_buf, &kb.vars, &drop_vars);
        kb.vars.clear();
        kb.vars.extend_from_slice(merge_buf);
        merge_sorted(merge_buf, &kb.active, &drop_active);
        let at = merge_buf.partition_point(|&a| a < ci);
        merge_buf.insert(at, ci);
        kb.active.clear();
        kb.active.extend_from_slice(merge_buf);
        kb.weight_sum = 0.0;
        kb.wd_sum = 0.0;
        for &v in &kb.vars {
            kb.weight_sum += vs[v].weight;
            kb.wd_sum += vs[v].weight * (vs[v].desired - vs[v].offset);
        }
        kb.position = kb.wd_sum / kb.weight_sum;
    }
    // Recycle the dropped block's buffers (cleared on next checkout).
    scratch.give_pool(drop_vars);
    scratch.give_pool(drop_active);
    let keep_pos = blocks[keep].position;
    for &v in &blocks[keep].vars {
        scratch.pos[v] = keep_pos + vs[v].offset;
    }
    scratch.clean[keep] = false;
}

/// Lagrange multipliers of the active constraints in block `b`.
///
/// With the active tree, removing constraint `ai` splits the block into the
/// component containing `right` (call it S). Optimality of x wrt the tree
/// gives λ_ai = Σ_{k∈S} w_k (x_k − d_k) (up to sign convention): a negative
/// value means the constraint "wants to stretch" and should be deactivated.
/// `adj` is the shared adjacency index of the block (see [`fill_adj`]); the
/// sum walks the sorted component (pool order), so values are bit-identical
/// to a from-scratch traversal. The pool pass fuses summation and
/// seen-reset: no component list is materialized.
fn multiplier(
    vs: &[Var],
    block: &Block,
    cons: &[Constraint],
    adj: &[Vec<usize>],
    seen: &mut Vec<bool>,
    stack: &mut Vec<usize>,
    ai: usize,
) -> f64 {
    mark_component(adj, cons, ai, cons[ai].right, seen, stack);
    let mut lambda = 0.0;
    for &v in block.vars.iter() {
        if seen[v] {
            seen[v] = false;
            let x = block.position + vs[v].offset;
            lambda += vs[v].weight * (x - vs[v].desired);
        }
    }
    lambda
}

/// Mark members reachable from `seed` through the adjacency index `adj`,
/// excluding constraint `skip`, into `seen` (DFS). Callers must clear the
/// marks they consume. Shared marking core of [`component_of`] and
/// [`multiplier`].
fn mark_component(
    adj: &[Vec<usize>],
    cons: &[Constraint],
    skip: usize,
    seed: usize,
    seen: &mut Vec<bool>,
    stack: &mut Vec<usize>,
) {
    stack.clear();
    seen[seed] = true;
    stack.push(seed);
    while let Some(v) = stack.pop() {
        for &ai in &adj[v] {
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
            if !seen[other] {
                seen[other] = true;
                stack.push(other);
            }
        }
    }
    stack.clear();
}

/// Members reachable from `seed` through the adjacency index `adj`,
/// excluding constraint `skip`. Left sorted in `comp` for deterministic
/// summation; visited marks are reset on exit so the buffers can be reused.
///
/// `pool` is the owning block's sorted `vars` list: every DFS-reachable
/// member is a block member, so collecting by filtering the sorted pool
/// yields the same sorted output as the old collect-then-sort without the
/// O(k log k) sort.
fn component_of(
    adj: &[Vec<usize>],
    cons: &[Constraint],
    skip: usize,
    seed: usize,
    pool: &[usize],
    comp: &mut Vec<usize>,
    seen: &mut Vec<bool>,
    stack: &mut Vec<usize>,
) {
    comp.clear();
    mark_component(adj, cons, skip, seed, seen, stack);
    for &v in pool {
        if seen[v] {
            seen[v] = false;
            comp.push(v);
        }
    }
}

/// First (in stable order) active constraint with a negative multiplier.
///
/// Blocks carry a `clean` flag: a block whose vars/offsets/position/active
/// set were verified negative-free is skipped until its next mutation
/// (merge/split clear the flag). Multiplier values are a pure function of
/// that state, so skipping cannot change which constraint is found first.
fn find_split(
    vs: &[Var],
    blocks: &[Block],
    cons: &[Constraint],
    eps: f64,
    scratch: &mut Scratch,
) -> Option<(usize, usize)> {
    if scratch.clean.len() < blocks.len() {
        scratch.clean.resize(blocks.len(), false);
    }
    for (b, block) in blocks.iter().enumerate() {
        if block.active.is_empty() || scratch.clean[b] {
            continue;
        }
        fill_adj(scratch, block, cons);
        let Scratch {
            adj, seen, stack, ..
        } = scratch;
        let mut hit = None;
        for &ai in &block.active {
            if multiplier(vs, block, cons, adj, seen, stack, ai) < -eps {
                hit = Some(ai);
                break;
            }
        }
        clear_adj(adj, &block.active, cons);
        match hit {
            Some(ai) => return Some((b, ai)),
            None => scratch.clean[b] = true,
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
    scratch: &mut Scratch,
) {
    // Split lists come off the pool before `scratch` is destructured for
    // the component walk; the distributed source buffers return below.
    let mut left_vars = scratch.take_pool();
    let mut right_vars = scratch.take_pool();
    let mut left_active = scratch.take_pool();
    let mut right_active = scratch.take_pool();
    fill_adj(scratch, &blocks[b], cons);
    let Scratch {
        adj,
        comp,
        seen,
        stack,
        ..
    } = scratch;
    let pool = &blocks[b].vars;
    component_of(adj, cons, ai, cons[ai].right, pool, comp, seen, stack);
    let right_side: &[usize] = comp;
    let old_active = std::mem::take(&mut blocks[b].active);
    let all_vars = std::mem::take(&mut blocks[b].vars);
    let old_pos = blocks[b].position;

    for v in all_vars.iter() {
        if right_side.binary_search(v).is_ok() {
            right_vars.push(*v);
        } else {
            left_vars.push(*v);
        }
    }
    for a in old_active.iter().copied() {
        if a == ai {
            continue;
        }
        if right_side.binary_search(&cons[a].left).is_ok() {
            right_active.push(a);
        } else {
            left_active.push(a);
        }
    }
    clear_adj(adj, &old_active, cons);
    // The distributed source buffers go back to the pool; the halves'
    // buffers return on the next solve reset (they live in the blocks).
    scratch.give_pool(all_vars);
    scratch.give_pool(old_active);

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
    let left_pos = blocks[b].position;
    for &v in &blocks[b].vars {
        scratch.pos[v] = left_pos + vs[v].offset;
    }
    let right_pos = blocks[new_id].position;
    for &v in &blocks[new_id].vars {
        scratch.pos[v] = right_pos + vs[v].offset;
    }
    scratch.clean[b] = false;
    if scratch.clean.len() <= new_id {
        scratch.clean.push(false);
    } else {
        scratch.clean[new_id] = false;
    }
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
