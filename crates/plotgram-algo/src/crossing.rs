//! Crossing counting for layered bipartite graphs.
//!
//! [`count_bipartite_crossings`] counts edge crossings between two adjacent
//! layers with fixed left-to-right orders, via the Barth–Jünger–Mutzel
//! accumulation-tree method in `O(E log E)`. This is the oracle for ordering
//! optimization (P3 sweeps) and CI regression gates.

use std::collections::BTreeMap;

/// Fenwick tree (binary indexed tree) over `0..n`, prefix-sum queries.
/// Crate-internal so future transpose Δ-crossing checks can reuse it.
pub(crate) struct Fenwick {
    tree: Vec<u64>,
}

impl Fenwick {
    pub(crate) fn new(n: usize) -> Self {
        Self {
            tree: vec![0; n + 1],
        }
    }

    /// Add `delta` at index `i` (0-based).
    pub(crate) fn add(&mut self, i: usize, delta: u64) {
        let mut i = i + 1;
        while i < self.tree.len() {
            self.tree[i] += delta;
            i += i & i.wrapping_neg();
        }
    }

    /// Sum of values at indices `0..=i` (0-based).
    pub(crate) fn prefix_sum(&self, i: usize) -> u64 {
        let mut i = (i + 1).min(self.tree.len() - 1);
        let mut s = 0;
        while i > 0 {
            s += self.tree[i];
            i -= i & i.wrapping_neg();
        }
        s
    }

    /// Total of all values.
    pub(crate) fn total(&self) -> u64 {
        self.prefix_sum(self.tree.len().saturating_sub(2))
    }
}

/// Count crossings between two adjacent layers.
///
/// * `order_a` / `order_b`: node ids of the two layers, left to right. Ids
///   are arbitrary `usize` (need not be contiguous) but must be unique
///   within each layer.
/// * `edges`: cross-layer edges as `(a_id, b_id)`. Parallel edges are
///   allowed and never cross each other.
///
/// Two edges cross iff their layer positions strictly invert. Pure counting:
/// the result is independent of input edge order.
///
/// # Panics
///
/// If an edge endpoint does not appear in the respective order, or an id is
/// duplicated within a layer (contract violation).
pub fn count_bipartite_crossings(
    order_a: &[usize],
    order_b: &[usize],
    edges: &[(usize, usize)],
) -> u64 {
    let index_of = |order: &[usize], layer: &str| -> BTreeMap<usize, usize> {
        let mut map = BTreeMap::new();
        for (pos, &id) in order.iter().enumerate() {
            if map.insert(id, pos).is_some() {
                panic!("duplicate node id {id} in layer {layer}");
            }
        }
        map
    };
    let pos_a = index_of(order_a, "a");
    let pos_b = index_of(order_b, "b");

    // Map edges to (posA, posB) and sort lexicographically; then a pair of
    // edges crosses iff the earlier one (in this order) has a strictly
    // larger posB. Sweep with a Fenwick tree over posB.
    let mut pairs: Vec<(usize, usize)> = edges
        .iter()
        .map(|&(a, b)| {
            let pa = *pos_a
                .get(&a)
                .unwrap_or_else(|| panic!("edge endpoint {a} not in layer a"));
            let pb = *pos_b
                .get(&b)
                .unwrap_or_else(|| panic!("edge endpoint {b} not in layer b"));
            (pa, pb)
        })
        .collect();
    pairs.sort_unstable();

    let mut fen = Fenwick::new(order_b.len());
    let mut crossings = 0u64;
    for &(_, pb) in &pairs {
        // Edges already inserted have posA ≤ current; those with posB > pb
        // strictly invert → cross. Equal posA pairs sorted by posB ascending
        // never see a larger inserted posB among same-posA edges, so shared
        // upper endpoints are handled for free.
        crossings += fen.total() - fen.prefix_sum(pb);
        fen.add(pb, 1);
    }
    crossings
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Naive O(E²) oracle: count strictly inverting pairs.
    fn naive(order_a: &[usize], order_b: &[usize], edges: &[(usize, usize)]) -> u64 {
        let pos = |order: &[usize], id: usize| order.iter().position(|&x| x == id).unwrap();
        let mapped: Vec<(usize, usize)> = edges
            .iter()
            .map(|&(a, b)| (pos(order_a, a), pos(order_b, b)))
            .collect();
        let mut c = 0u64;
        for i in 0..mapped.len() {
            for j in i + 1..mapped.len() {
                let (a1, b1) = mapped[i];
                let (a2, b2) = mapped[j];
                if (a1 < a2 && b1 > b2) || (a1 > a2 && b1 < b2) {
                    c += 1;
                }
            }
        }
        c
    }

    #[test]
    fn hand_crafted_cases() {
        struct Case {
            name: &'static str,
            order_a: Vec<usize>,
            order_b: Vec<usize>,
            edges: Vec<(usize, usize)>,
            expect: u64,
        }
        let cases = vec![
            Case {
                name: "empty graph",
                order_a: vec![],
                order_b: vec![],
                edges: vec![],
                expect: 0,
            },
            Case {
                name: "single edge",
                order_a: vec![0],
                order_b: vec![10],
                edges: vec![(0, 10)],
                expect: 0,
            },
            Case {
                name: "X crossing",
                order_a: vec![0, 1],
                order_b: vec![10, 11],
                edges: vec![(0, 11), (1, 10)],
                expect: 1,
            },
            Case {
                name: "parallel straight edges do not cross",
                order_a: vec![0, 1],
                order_b: vec![10, 11],
                edges: vec![(0, 10), (1, 11)],
                expect: 0,
            },
            Case {
                name: "K2,2 has 1 crossing",
                order_a: vec![0, 1],
                order_b: vec![10, 11],
                edges: vec![(0, 10), (0, 11), (1, 10), (1, 11)],
                expect: 1,
            },
            Case {
                name: "K3,3 has 9 crossings",
                order_a: vec![0, 1, 2],
                order_b: vec![10, 11, 12],
                edges: vec![
                    (0, 10),
                    (0, 11),
                    (0, 12),
                    (1, 10),
                    (1, 11),
                    (1, 12),
                    (2, 10),
                    (2, 11),
                    (2, 12),
                ],
                expect: 9,
            },
            Case {
                name: "multi-edges never cross each other but cross others",
                order_a: vec![0, 1],
                order_b: vec![10, 11],
                // two parallel (0,11) edges each cross (1,10) once
                edges: vec![(0, 11), (0, 11), (1, 10)],
                expect: 2,
            },
            Case {
                name: "shared upper endpoint fan does not self-cross",
                order_a: vec![0],
                order_b: vec![10, 11, 12],
                edges: vec![(0, 10), (0, 11), (0, 12)],
                expect: 0,
            },
            Case {
                name: "non-contiguous ids, reversed visual order",
                order_a: vec![100, 7],
                order_b: vec![55, 3],
                edges: vec![(100, 3), (7, 55)],
                expect: 1,
            },
        ];
        for case in &cases {
            let got = count_bipartite_crossings(&case.order_a, &case.order_b, &case.edges);
            assert_eq!(got, case.expect, "{}", case.name);
            assert_eq!(
                got,
                naive(&case.order_a, &case.order_b, &case.edges),
                "{}: naive oracle disagrees",
                case.name
            );
        }
    }

    #[test]
    #[should_panic(expected = "not in layer")]
    fn unknown_endpoint_panics() {
        count_bipartite_crossings(&[0], &[1], &[(0, 99)]);
    }

    #[test]
    #[should_panic(expected = "duplicate node id")]
    fn duplicate_id_panics() {
        count_bipartite_crossings(&[0, 0], &[1], &[]);
    }

    /// Deterministic LCG (same generator as vpsc tests).
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
    fn random_instances_match_naive_oracle() {
        let mut rng = Lcg(0xc0de_2026_0731);
        for round in 0..40 {
            let na = 1 + rng.usize_below(30);
            let nb = 1 + rng.usize_below(30);
            // Shuffled, non-contiguous ids (Fisher–Yates with LCG).
            let mut order_a: Vec<usize> = (0..na).map(|i| i * 3 + 1000).collect();
            let mut order_b: Vec<usize> = (0..nb).map(|i| i * 7 + 2000).collect();
            for arr in [&mut order_a, &mut order_b] {
                for i in (1..arr.len()).rev() {
                    let j = rng.usize_below(i + 1);
                    arr.swap(i, j);
                }
            }
            let e = rng.usize_below(201); // 0..=200, duplicates allowed
            let edges: Vec<(usize, usize)> = (0..e)
                .map(|_| (order_a[rng.usize_below(na)], order_b[rng.usize_below(nb)]))
                .collect();
            let fast = count_bipartite_crossings(&order_a, &order_b, &edges);
            let slow = naive(&order_a, &order_b, &edges);
            assert_eq!(fast, slow, "round {round}: fenwick {fast} vs naive {slow}");
        }
    }
}
