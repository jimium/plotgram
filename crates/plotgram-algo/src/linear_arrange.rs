//! One-dimensional linear arrangement (MinLA) with optional pins / before-order.
//!
//! Used by Sequence lifeline order (architecture.md §6.1 / axes.md §2).
//! Vertices are `0..n` in **declaration index** order. The result is a
//! permutation `perm` where `perm[position] = vertex`.
//!
//! Deterministic: greedy slot choice and local swaps use the key
//! `(MinLA, cutwidth, slot, vertex_index)` with `total_cmp` on floats.

use std::fmt;

/// How to produce the arrangement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinearArrangeMethod {
    /// Keep declaration order (`0..n`), then overlay pins.
    Identity,
    /// Greedy first-appearance insertion into free slots (19 §2.2 L2).
    Greedy,
    /// Greedy, then adjacent/pairwise swaps of unpinned vertices (L3).
    Local,
}

/// Input to [`arrange`].
#[derive(Debug, Clone)]
pub struct LinearArrangement {
    pub n: usize,
    /// `(u, v, weight)`; self-loops are ignored.
    pub edges: Vec<(usize, usize, f64)>,
    /// `(vertex, required_position)` — positions in `0..n`, unique.
    pub pins: Vec<(usize, usize)>,
    /// `(a, b)` means `pos[a] < pos[b]`.
    pub befores: Vec<(usize, usize)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArrangeError {
    Invalid(String),
    Infeasible(String),
}

impl fmt::Display for ArrangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(m) | Self::Infeasible(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for ArrangeError {}

/// Arrange vertices. `perm[i]` is the vertex at secondary-axis slot `i`.
pub fn arrange(
    problem: &LinearArrangement,
    method: LinearArrangeMethod,
) -> Result<Vec<usize>, ArrangeError> {
    validate(problem)?;
    let mut perm = match method {
        LinearArrangeMethod::Identity => overlay_pins(problem)?,
        LinearArrangeMethod::Greedy => greedy_slots(problem)?,
        LinearArrangeMethod::Local => {
            let mut perm = greedy_slots(problem)?;
            local_improve(problem, &mut perm);
            perm
        }
    };
    if !befores_ok(problem, &perm) {
        return Err(ArrangeError::Infeasible(
            "linear arrangement: before-order constraint violated".into(),
        ));
    }
    debug_assert_eq!(perm.len(), problem.n);
    Ok(std::mem::take(&mut perm))
}

fn validate(p: &LinearArrangement) -> Result<(), ArrangeError> {
    let n = p.n;
    for &(u, v, w) in &p.edges {
        if u >= n || v >= n {
            return Err(ArrangeError::Invalid(format!(
                "linear arrangement: edge ({u},{v}) out of range for n={n}"
            )));
        }
        if !w.is_finite() || w < 0.0 {
            return Err(ArrangeError::Invalid(
                "linear arrangement: edge weight must be finite and ≥ 0".into(),
            ));
        }
    }
    let mut seen_v = vec![false; n];
    let mut seen_p = vec![false; n];
    for &(v, pos) in &p.pins {
        if v >= n || pos >= n {
            return Err(ArrangeError::Invalid(format!(
                "linear arrangement: pin ({v} @ {pos}) out of range for n={n}"
            )));
        }
        if seen_v[v] {
            return Err(ArrangeError::Infeasible(format!(
                "linear arrangement: vertex {v} pinned twice"
            )));
        }
        if seen_p[pos] {
            return Err(ArrangeError::Infeasible(format!(
                "linear arrangement: two vertices pinned to slot {pos}"
            )));
        }
        seen_v[v] = true;
        seen_p[pos] = true;
    }
    for &(a, b) in &p.befores {
        if a >= n || b >= n {
            return Err(ArrangeError::Invalid(format!(
                "linear arrangement: before ({a},{b}) out of range for n={n}"
            )));
        }
        if a == b {
            return Err(ArrangeError::Infeasible(
                "linear arrangement: before-order on the same vertex".into(),
            ));
        }
    }
    // Pin vs before: if both ends pinned, they must already satisfy the order.
    let mut pin_pos = vec![None; n];
    for &(v, pos) in &p.pins {
        pin_pos[v] = Some(pos);
    }
    for &(a, b) in &p.befores {
        if let (Some(pa), Some(pb)) = (pin_pos[a], pin_pos[b]) {
            if pa >= pb {
                return Err(ArrangeError::Infeasible(format!(
                    "linear arrangement: pins place {a} at {pa} and {b} at {pb}, \
                     but before requires {a} left of {b}"
                )));
            }
        }
    }
    Ok(())
}

fn overlay_pins(p: &LinearArrangement) -> Result<Vec<usize>, ArrangeError> {
    let n = p.n;
    let mut slot: Vec<Option<usize>> = vec![None; n];
    let mut pinned = vec![false; n];
    for &(v, pos) in &p.pins {
        slot[pos] = Some(v);
        pinned[v] = true;
    }
    let mut rest: Vec<usize> = (0..n).filter(|&v| !pinned[v]).collect();
    for s in slot.iter_mut() {
        if s.is_none() {
            *s = Some(rest.remove(0));
        }
    }
    Ok(slot.into_iter().map(|x| x.expect("filled")).collect())
}

fn greedy_slots(p: &LinearArrangement) -> Result<Vec<usize>, ArrangeError> {
    let n = p.n;
    let mut assigned: Vec<Option<usize>> = vec![None; n]; // vertex → position
    let mut used = vec![false; n];
    let mut pinned_vert = vec![false; n];

    for &(v, pos) in &p.pins {
        assigned[v] = Some(pos);
        used[pos] = true;
        pinned_vert[v] = true;
    }

    let mut visit_order: Vec<usize> = Vec::new();
    for &(u, v, _) in &p.edges {
        if u != v {
            visit_order.push(u);
            visit_order.push(v);
        }
    }
    for v in 0..n {
        visit_order.push(v);
    }

    for v in visit_order {
        if assigned[v].is_some() {
            continue;
        }
        let mut best: Option<(f64, u64, usize)> = None;
        for pos in 0..n {
            if used[pos] {
                continue;
            }
            assigned[v] = Some(pos);
            if !befores_partial_ok(p, &assigned) {
                assigned[v] = None;
                continue;
            }
            let cost = minla_partial(p, &assigned);
            let cut = cutwidth_partial(p, &assigned);
            let key = (cost, cut, pos);
            let take = match best {
                None => true,
                Some(b) => cmp_key(key, b),
            };
            if take {
                best = Some(key);
            }
            assigned[v] = None;
        }
        let Some((_, _, pos)) = best else {
            return Err(ArrangeError::Infeasible(format!(
                "linear arrangement: no feasible slot for vertex {v}"
            )));
        };
        assigned[v] = Some(pos);
        used[pos] = true;
    }

    let mut perm = vec![0; n];
    for (v, pos) in assigned.into_iter().enumerate() {
        perm[pos.expect("assigned")] = v;
    }
    let _ = pinned_vert;
    Ok(perm)
}

fn local_improve(p: &LinearArrangement, perm: &mut [usize]) {
    let n = perm.len();
    let mut pinned_vert = vec![false; n];
    for &(v, _) in &p.pins {
        if v < n {
            pinned_vert[v] = true;
        }
    }
    let max_passes = n.saturating_mul(n).max(1);
    for _ in 0..max_passes {
        let mut improved = false;
        for i in 0..n {
            if pinned_vert[perm[i]] {
                continue;
            }
            for j in (i + 1)..n {
                if pinned_vert[perm[j]] {
                    continue;
                }
                let old_c = minla_perm(p, perm);
                let old_w = cutwidth_perm(p, perm);
                perm.swap(i, j);
                let better = befores_ok(p, perm)
                    && cmp_key(
                        (minla_perm(p, perm), cutwidth_perm(p, perm), 0),
                        (old_c, old_w, 0),
                    );
                if better {
                    improved = true;
                } else {
                    perm.swap(i, j);
                }
            }
        }
        if !improved {
            break;
        }
    }
}

fn cmp_key(a: (f64, u64, usize), b: (f64, u64, usize)) -> bool {
    match a.0.total_cmp(&b.0) {
        std::cmp::Ordering::Less => true,
        std::cmp::Ordering::Greater => false,
        std::cmp::Ordering::Equal => (a.1, a.2) < (b.1, b.2),
    }
}

fn minla_partial(p: &LinearArrangement, assigned: &[Option<usize>]) -> f64 {
    let mut s = 0.0;
    for &(u, v, w) in &p.edges {
        if u == v {
            continue;
        }
        if let (Some(pu), Some(pv)) = (assigned[u], assigned[v]) {
            s += w * (pu as i32 - pv as i32).unsigned_abs() as f64;
        }
    }
    s
}

fn cutwidth_partial(p: &LinearArrangement, assigned: &[Option<usize>]) -> u64 {
    let n = p.n;
    if n == 0 {
        return 0;
    }
    let mut cuts = vec![0u64; n.saturating_sub(1)];
    for &(u, v, _) in &p.edges {
        if u == v {
            continue;
        }
        let (Some(pu), Some(pv)) = (assigned[u], assigned[v]) else {
            continue;
        };
        let (lo, hi) = if pu < pv { (pu, pv) } else { (pv, pu) };
        for c in cuts.iter_mut().take(hi).skip(lo) {
            *c += 1;
        }
    }
    cuts.into_iter().max().unwrap_or(0)
}

fn pos_of(perm: &[usize]) -> Vec<usize> {
    let mut assigned = vec![0; perm.len()];
    for (pos, &v) in perm.iter().enumerate() {
        assigned[v] = pos;
    }
    assigned
}

fn minla_perm(p: &LinearArrangement, perm: &[usize]) -> f64 {
    let assigned: Vec<Option<usize>> = pos_of(perm).into_iter().map(Some).collect();
    minla_partial(p, &assigned)
}

fn cutwidth_perm(p: &LinearArrangement, perm: &[usize]) -> u64 {
    let assigned: Vec<Option<usize>> = pos_of(perm).into_iter().map(Some).collect();
    cutwidth_partial(p, &assigned)
}

fn befores_ok(p: &LinearArrangement, perm: &[usize]) -> bool {
    let assigned: Vec<Option<usize>> = pos_of(perm).into_iter().map(Some).collect();
    befores_partial_ok(p, &assigned)
}

fn befores_partial_ok(p: &LinearArrangement, assigned: &[Option<usize>]) -> bool {
    for &(a, b) in &p.befores {
        if let (Some(pa), Some(pb)) = (assigned[a], assigned[b]) {
            if pa >= pb {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(
        n: usize,
        edges: &[(usize, usize, f64)],
        pins: &[(usize, usize)],
        befores: &[(usize, usize)],
    ) -> LinearArrangement {
        LinearArrangement {
            n,
            edges: edges.to_vec(),
            pins: pins.to_vec(),
            befores: befores.to_vec(),
        }
    }

    #[test]
    fn table_identity_greedy_local_and_pins() {
        struct Case {
            name: &'static str,
            n: usize,
            edges: &'static [(usize, usize, f64)],
            pins: &'static [(usize, usize)],
            befores: &'static [(usize, usize)],
            method: LinearArrangeMethod,
            expect: &'static [usize],
        }
        let cases = [
            Case {
                name: "identity keeps declaration",
                n: 3,
                edges: &[(0, 2, 1.0)],
                pins: &[],
                befores: &[],
                method: LinearArrangeMethod::Identity,
                expect: &[0, 1, 2],
            },
            Case {
                name: "greedy pulls far pair together",
                n: 3,
                edges: &[(0, 2, 1.0)],
                pins: &[],
                befores: &[],
                method: LinearArrangeMethod::Greedy,
                expect: &[0, 2, 1],
            },
            Case {
                name: "pin holds leftmost under greedy",
                n: 3,
                edges: &[(0, 2, 1.0)],
                pins: &[(1, 0)],
                befores: &[],
                method: LinearArrangeMethod::Greedy,
                expect: &[1, 0, 2],
            },
            Case {
                name: "identity pin overlay preserves unpinned relative order",
                n: 3,
                edges: &[],
                pins: &[(2, 0)],
                befores: &[],
                method: LinearArrangeMethod::Identity,
                expect: &[2, 0, 1],
            },
        ];
        for c in cases {
            let got = arrange(&p(c.n, c.edges, c.pins, c.befores), c.method)
                .unwrap_or_else(|e| panic!("{}: {e}", c.name));
            assert_eq!(got, c.expect, "{}", c.name);
        }
    }

    #[test]
    fn pin_conflict_is_infeasible() {
        let err = arrange(
            &p(2, &[], &[(0, 0), (1, 0)], &[]),
            LinearArrangeMethod::Greedy,
        )
        .unwrap_err();
        assert!(matches!(err, ArrangeError::Infeasible(_)));
    }

    #[test]
    fn before_vs_pins_is_infeasible() {
        let err = arrange(
            &p(2, &[], &[(0, 1), (1, 0)], &[(0, 1)]),
            LinearArrangeMethod::Identity,
        )
        .unwrap_err();
        assert!(matches!(err, ArrangeError::Infeasible(_)));
    }

    #[test]
    fn local_is_deterministic() {
        let problem = p(4, &[(0, 3, 1.0), (1, 2, 1.0)], &[], &[]);
        let a = arrange(&problem, LinearArrangeMethod::Local).unwrap();
        let b = arrange(&problem, LinearArrangeMethod::Local).unwrap();
        assert_eq!(a, b);
    }
}
