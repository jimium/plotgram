//! Greedy Feedback Arc Set (cycle removal / edge reversal).
//!
//! [`greedy_fas`] computes the set of edges to *mark as reversed* so the
//! remaining orientation is acyclic, via the Eades–Lin–Smyth 1993 greedy
//! skeleton with a declaration-order cycle rule (see below). Callers keep the
//! edges and only flip their direction for layering; arrows are still drawn
//! along the original direction (P5 restores them). Deterministic: stable
//! tie-breaks by smallest node id.
//!
//! Cycle-entry rule (cycle reroot): the author's declaration order is the
//! layout's narrative priority. After the standard greedy walk, if node 0
//! (earliest declared) is not yet a working source and has exactly one
//! unreversed original in-edge, the cut is rotated onto that in-edge:
//! one reversed edge is un-reversed in exchange, verified acyclic by an
//! explicit check. Node 0 then ranks at the top and the back edge lands on
//! the return hop, not on the entry hop. A rotation never changes the
//! reversal count, so the ≤ E/2 − V/6 bound still holds; the choice is
//! deterministic (smallest edge index first) and the hier_eval
//! `reversed_count` baseline guards against regressions.

use std::collections::BTreeSet;

/// Compute the edge indices to reverse so the graph becomes acyclic.
///
/// * `num_nodes`: nodes are `0..num_nodes`.
/// * `edges`: directed edges `(source, target)`; the index into this slice is
///   the edge id. Parallel edges and opposing pairs are allowed.
/// * Self-loops are skipped and never appear in the result (callers pull
///   them out separately for drawing).
///
/// Returns the indices of edges that point backwards in the greedy linear
/// order. Reversing exactly these edges yields a DAG. Parallel same-direction
/// edges are reversed (or kept) together; of an opposing pair exactly one is
/// reversed.
///
/// # Panics
///
/// If an edge endpoint is `>= num_nodes` (contract violation).
pub fn greedy_fas(num_nodes: usize, edges: &[(usize, usize)]) -> BTreeSet<usize> {
    for (i, &(u, v)) in edges.iter().enumerate() {
        assert!(
            u < num_nodes && v < num_nodes,
            "edge {i} = ({u}, {v}) out of range for {num_nodes} nodes"
        );
    }

    // Degree bookkeeping over the shrinking graph. Self-loops contribute to
    // neither degree (they never constrain the order).
    let mut outdeg = vec![0isize; num_nodes];
    let mut indeg = vec![0isize; num_nodes];
    // Adjacency by node with edge multiplicity implied by repetition.
    let mut out_adj: Vec<Vec<usize>> = vec![Vec::new(); num_nodes];
    let mut in_adj: Vec<Vec<usize>> = vec![Vec::new(); num_nodes];
    for &(u, v) in edges {
        if u == v {
            continue;
        }
        outdeg[u] += 1;
        indeg[v] += 1;
        out_adj[u].push(v);
        in_adj[v].push(u);
    }

    // Buckets over delta = outdeg − indeg. With parallel edges the delta is
    // bounded by the non-loop edge count, not the node count — shift by it.
    // BTreeSets keep every extraction deterministic (smallest id first).
    let shift: isize = edges.iter().filter(|&&(u, v)| u != v).count() as isize;
    let mut removed = vec![false; num_nodes];
    let mut sinks: BTreeSet<usize> = BTreeSet::new();
    let mut sources: BTreeSet<usize> = BTreeSet::new();
    let bucket_count = 2 * shift as usize + 1;
    let mut buckets: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); bucket_count];
    let bucket_of = |o: isize, i: isize, shift: isize| (o - i + shift) as usize;
    for v in 0..num_nodes {
        buckets[bucket_of(outdeg[v], indeg[v], shift)].insert(v);
        if outdeg[v] == 0 {
            sinks.insert(v);
        } else if indeg[v] == 0 {
            sources.insert(v);
        }
    }

    let mut s_left: Vec<usize> = Vec::with_capacity(num_nodes);
    let mut s_right_rev: Vec<usize> = Vec::with_capacity(num_nodes); // reversed S_right
    let mut remaining = num_nodes;

    // Remove `v` from the shrinking graph, updating neighbour degrees and
    // bucket membership.
    let remove_node = |v: usize,
                       removed: &mut Vec<bool>,
                       outdeg: &mut Vec<isize>,
                       indeg: &mut Vec<isize>,
                       sinks: &mut BTreeSet<usize>,
                       sources: &mut BTreeSet<usize>,
                       buckets: &mut Vec<BTreeSet<usize>>| {
        removed[v] = true;
        sinks.remove(&v);
        sources.remove(&v);
        buckets[bucket_of(outdeg[v], indeg[v], shift)].remove(&v);
        // v's out-edges lower each target's indeg; in-edges lower outdeg.
        for &w in &out_adj[v] {
            if removed[w] {
                continue;
            }
            buckets[bucket_of(outdeg[w], indeg[w], shift)].remove(&w);
            indeg[w] -= 1;
            buckets[bucket_of(outdeg[w], indeg[w], shift)].insert(w);
            if indeg[w] == 0 && outdeg[w] > 0 {
                sources.insert(w);
            }
        }
        for &w in &in_adj[v] {
            if removed[w] {
                continue;
            }
            buckets[bucket_of(outdeg[w], indeg[w], shift)].remove(&w);
            outdeg[w] -= 1;
            buckets[bucket_of(outdeg[w], indeg[w], shift)].insert(w);
            if outdeg[w] == 0 {
                sinks.insert(w);
                sources.remove(&w);
            }
        }
    };

    while remaining > 0 {
        // Drain sinks first (smallest id first), prepending to S_right.
        while let Some(&v) = sinks.iter().next() {
            remove_node(
                v,
                &mut removed,
                &mut outdeg,
                &mut indeg,
                &mut sinks,
                &mut sources,
                &mut buckets,
            );
            s_right_rev.push(v);
            remaining -= 1;
        }
        // Then sources, appending to S_left.
        while let Some(&v) = sources.iter().next() {
            remove_node(
                v,
                &mut removed,
                &mut outdeg,
                &mut indeg,
                &mut sinks,
                &mut sources,
                &mut buckets,
            );
            s_left.push(v);
            remaining -= 1;
        }
        if remaining == 0 {
            break;
        }
        // Strict ELS: only remove a max-delta node when no sink/source is
        // left. Draining above keeps both sets empty here (sink removal
        // never lowers an indegree; source removal never lowers an
        // outdegree), but the guard makes the invariant local and keeps it
        // safe under future edits to `remove_node`.
        if !sinks.is_empty() || !sources.is_empty() {
            continue;
        }
        // Highest outdeg − indeg among remaining nodes; ties by smallest id
        // (BTreeSet iteration order).
        let v = buckets
            .iter()
            .rev()
            .find_map(|b| b.iter().next().copied())
            .expect("remaining > 0 implies a non-empty bucket");
        remove_node(
            v,
            &mut removed,
            &mut outdeg,
            &mut indeg,
            &mut sinks,
            &mut sources,
            &mut buckets,
        );
        s_left.push(v);
        remaining -= 1;
    }

    // order = S_left ++ reverse(S_right_rev); backwards edges get reversed.
    let mut pos = vec![0usize; num_nodes];
    for (p, &v) in s_left.iter().chain(s_right_rev.iter().rev()).enumerate() {
        pos[v] = p;
    }
    let mut reversed: BTreeSet<usize> = edges
        .iter()
        .enumerate()
        .filter(|&(_, &(u, v))| u != v && pos[u] > pos[v])
        .map(|(i, _)| i)
        .collect();

    reroot_cycle_at(num_nodes, edges, &mut reversed);
    reversed
}

/// Cycle reroot (cycle-entry rule, module doc): rotate the cut onto the
/// unique edge entering node 0, so node 0 becomes a working source and the
/// author's narrative start ranks at the top.
///
/// Applies only when node 0 has exactly one original in-edge and it is
/// currently unreversed (otherwise node 0 is already well-placed or the
/// rotation is ambiguous). Each candidate swap exchanges that in-edge for
/// one currently-reversed edge (smallest edge index first) and is accepted
/// only if the resulting orientation is acyclic — a rotation keeps the
/// reversal count intact. No acyclic swap means the ELS result stands.
fn reroot_cycle_at(num_nodes: usize, edges: &[(usize, usize)], reversed: &mut BTreeSet<usize>) {
    let in_edges: Vec<usize> = edges
        .iter()
        .enumerate()
        .filter(|&(_, &(u, v))| v == 0 && u != 0)
        .map(|(i, _)| i)
        .collect();
    if in_edges.len() != 1 {
        return;
    }
    let e_in = in_edges[0];
    if reversed.contains(&e_in) {
        return; // node 0 is already a working source
    }

    for &f in reversed.iter() {
        // BTreeSet iteration is ascending edge index — deterministic.
        let mut candidate = reversed.clone();
        candidate.remove(&f);
        candidate.insert(e_in);
        if is_acyclic(num_nodes, edges, &candidate) {
            *reversed = candidate;
            return;
        }
    }
}

/// Kahn's algorithm over the working orientation (self-loops ignored).
fn is_acyclic(num_nodes: usize, edges: &[(usize, usize)], reversed: &BTreeSet<usize>) -> bool {
    let mut indeg = vec![0usize; num_nodes];
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); num_nodes];
    for (i, &(u, v)) in edges.iter().enumerate() {
        if u == v {
            continue;
        }
        let (s, t) = if reversed.contains(&i) { (v, u) } else { (u, v) };
        adj[s].push(t);
        indeg[t] += 1;
    }
    let mut queue: Vec<usize> = (0..num_nodes).filter(|&v| indeg[v] == 0).collect();
    let mut seen = 0;
    while let Some(v) = queue.pop() {
        seen += 1;
        for &w in &adj[v] {
            indeg[w] -= 1;
            if indeg[w] == 0 {
                queue.push(w);
            }
        }
    }
    seen == num_nodes
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hard assertion: after reversing the reported edges, the graph must be
    /// acyclic (self-loops excluded). Kahn's algorithm.
    fn assert_acyclic_after_reversal(
        num_nodes: usize,
        edges: &[(usize, usize)],
        rev: &BTreeSet<usize>,
    ) {
        let mut indeg = vec![0usize; num_nodes];
        let mut adj: Vec<Vec<usize>> = vec![Vec::new(); num_nodes];
        for (i, &(u, v)) in edges.iter().enumerate() {
            if u == v {
                assert!(!rev.contains(&i), "self-loop {i} must never be reversed");
                continue;
            }
            let (s, t) = if rev.contains(&i) { (v, u) } else { (u, v) };
            adj[s].push(t);
            indeg[t] += 1;
        }
        let mut queue: Vec<usize> = (0..num_nodes).filter(|&v| indeg[v] == 0).collect();
        let mut seen = 0;
        while let Some(v) = queue.pop() {
            seen += 1;
            for &w in &adj[v] {
                indeg[w] -= 1;
                if indeg[w] == 0 {
                    queue.push(w);
                }
            }
        }
        assert_eq!(seen, num_nodes, "cycle remains after reversing {rev:?}");
    }

    #[test]
    fn hand_crafted_cases() {
        struct Case {
            name: &'static str,
            num_nodes: usize,
            edges: Vec<(usize, usize)>,
            /// Exact expected reversal set, when deterministic and small.
            expect: Option<Vec<usize>>,
        }
        let cases = vec![
            Case {
                name: "empty graph",
                num_nodes: 0,
                edges: vec![],
                expect: Some(vec![]),
            },
            Case {
                name: "DAG stays untouched",
                num_nodes: 4,
                edges: vec![(0, 1), (0, 2), (1, 3), (2, 3)],
                expect: Some(vec![]),
            },
            Case {
                name: "2-cycle reverses exactly one edge",
                num_nodes: 2,
                edges: vec![(0, 1), (1, 0)],
                // ELS flips (1,0); node 0's in-edge is already the cut, so
                // the reroot leaves it — node 0 is a working source.
                expect: Some(vec![1]),
            },
            Case {
                name: "3-cycle reverses exactly one edge",
                num_nodes: 3,
                edges: vec![(0, 1), (1, 2), (2, 0)],
                expect: Some(vec![2]),
            },
            Case {
                name: "opposing pair plus parallel edges move together",
                num_nodes: 2,
                // two parallel 0→1, two parallel 1→0: one direction flips whole
                edges: vec![(0, 1), (0, 1), (1, 0), (1, 0)],
                expect: Some(vec![2, 3]),
            },
            Case {
                name: "two disjoint cycles",
                num_nodes: 6,
                edges: vec![(0, 1), (1, 2), (2, 0), (3, 4), (4, 5), (5, 3)],
                expect: Some(vec![2, 5]),
            },
            Case {
                name: "self-loops are ignored",
                num_nodes: 3,
                edges: vec![(0, 0), (0, 1), (1, 1), (1, 2), (2, 2)],
                expect: Some(vec![]),
            },
            Case {
                name: "fully backwards chain flips nothing extra (chain is a DAG)",
                num_nodes: 4,
                edges: vec![(3, 2), (2, 1), (1, 0)],
                // A backwards-labelled chain is still acyclic → no reversal.
                expect: Some(vec![]),
            },
            Case {
                name: "pipeline + retry loop reroots onto the return hop",
                // software-release shape: 0..12 main chain, 13→0 retry
                // loop. ELS alone cuts the cycle at (1,9); the reroot then
                // rotates the cut onto the return hop (13,0), so node 0
                // becomes the working source and ranks at the top.
                num_nodes: 14,
                edges: vec![
                    (0, 1),
                    (1, 9),
                    (9, 2),
                    (9, 13),
                    (2, 10),
                    (10, 3),
                    (10, 13),
                    (3, 4),
                    (4, 11),
                    (11, 5),
                    (11, 13),
                    (5, 12),
                    (12, 6),
                    (12, 7),
                    (6, 8),
                    (7, 13),
                    (13, 0),
                ],
                expect: Some(vec![16]),
            },
        ];
        for case in &cases {
            let rev = greedy_fas(case.num_nodes, &case.edges);
            assert_acyclic_after_reversal(case.num_nodes, &case.edges, &rev);
            if let Some(exp) = &case.expect {
                let got: Vec<usize> = rev.iter().copied().collect();
                assert_eq!(&got, exp, "{}", case.name);
            }
        }
    }

    #[test]
    #[should_panic(expected = "out of range")]
    fn out_of_range_endpoint_panics() {
        greedy_fas(2, &[(0, 2)]);
    }

    /// Deterministic LCG (same generator as the other module tests).
    struct Lcg(u64);
    impl Lcg {
        fn next_u64(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            self.0
        }
        fn usize_below(&mut self, n: usize) -> usize {
            (self.next_u64() % n as u64) as usize
        }
    }

    #[test]
    fn random_instances_acyclic_and_bounded() {
        let mut rng = Lcg(0xfa5_2026_0731);
        for round in 0..30 {
            let n = 2 + rng.usize_below(39); // 2..=40
            let e = rng.usize_below(161); // 0..=160, self-loops/parallel allowed
            let edges: Vec<(usize, usize)> = (0..e)
                .map(|_| (rng.usize_below(n), rng.usize_below(n)))
                .collect();
            let rev = greedy_fas(n, &edges);
            assert_acyclic_after_reversal(n, &edges, &rev);
            let non_loops = edges.iter().filter(|&&(u, v)| u != v).count();
            assert!(
                rev.len() <= non_loops / 2 + 1,
                "round {round}: reversed {} of {} edges exceeds greedy bound",
                rev.len(),
                non_loops
            );
            // Determinism: identical rerun.
            assert_eq!(rev, greedy_fas(n, &edges), "round {round}: rerun differs");
        }
    }

    /// Exact minimum FAS size for tiny graphs: minimum backward-edge count
    /// over all node orderings (every FAS corresponds to some linear order,
    /// so this is the true optimum). Self-loops excluded. The cycle reroot
    /// rotates an existing cut without changing the reversal count, so the
    /// ELS-quality gap assertions still apply.
    fn exact_min_fas(num_nodes: usize, edges: &[(usize, usize)]) -> usize {
        fn recurse(
            order: &mut Vec<usize>,
            used: &mut [bool],
            num_nodes: usize,
            edges: &[(usize, usize)],
            best: &mut usize,
        ) {
            if order.len() == num_nodes {
                let mut pos = vec![0usize; num_nodes];
                for (p, &v) in order.iter().enumerate() {
                    pos[v] = p;
                }
                let back = edges
                    .iter()
                    .filter(|&&(u, v)| u != v && pos[u] > pos[v])
                    .count();
                *best = (*best).min(back);
                return;
            }
            for v in 0..num_nodes {
                if !used[v] {
                    used[v] = true;
                    order.push(v);
                    recurse(order, used, num_nodes, edges, best);
                    order.pop();
                    used[v] = false;
                }
            }
        }
        let mut best = usize::MAX;
        recurse(
            &mut Vec::new(),
            &mut vec![false; num_nodes],
            num_nodes,
            edges,
            &mut best,
        );
        best
    }

    #[test]
    fn tiny_instances_close_to_exact_optimum() {
        let mut rng = Lcg(0xe15_0dcc);
        for round in 0..40 {
            let n = 2 + rng.usize_below(4); // 2..=5
            let e = rng.usize_below(13); // 0..=12, self-loops/parallel allowed
            let edges: Vec<(usize, usize)> = (0..e)
                .map(|_| (rng.usize_below(n), rng.usize_below(n)))
                .collect();
            let rev = greedy_fas(n, &edges);
            assert_acyclic_after_reversal(n, &edges, &rev);
            let exact = exact_min_fas(n, &edges);
            assert!(
                rev.len() >= exact,
                "round {round}: greedy {} below exact minimum {exact} — oracle broken",
                rev.len()
            );
            // Quality: on these tiny dense multigraphs the greedy stays
            // within a small additive gap of the optimum.
            assert!(
                rev.len() <= exact + 2,
                "round {round}: greedy {} too far above exact minimum {exact} (edges {edges:?})",
                rev.len()
            );
        }
    }
}
