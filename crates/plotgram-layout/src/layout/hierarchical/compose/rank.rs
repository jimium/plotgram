//! Ranking via Network Simplex (Gansner, Koutsofios, North, Vo 1993),
//! weight = 1 uniform (no dummies exist yet — properify runs after ranking).
//!
//! Cut values use GKNV leaf-to-root accumulation (O(V+E) init) and path-only
//! updates on pivot; `(low, lim)` membership tests are O(1).

use std::collections::{BTreeMap, BTreeSet};

use plotgram_engine_api::LayoutError;

use crate::layout::hierarchical::model::RealGraph;

const MIN_SPAN: i64 = 1;

/// node index -> rank (dense, starts at 0).
pub type RankMap = Vec<u32>;

pub fn assign_ranks(graph: &RealGraph) -> Result<RankMap, LayoutError> {
    let n = graph.ids.len();
    if n == 0 {
        return Ok(Vec::new());
    }

    let mut rank: Vec<i64> = longest_path_ranks(graph);

    for comp in weak_components(graph) {
        if comp.len() > 1 {
            refine_component(graph, &comp, &mut rank);
        }
    }

    // P3-4 (dot `balance` + per-component / non-compressing normalize) is
    // deferred: wiring it regresses showcase bend hard-gates while the
    // performance goals are already met by incremental NS / crossings / indexes.
    Ok(normalize_dense(&rank))
}

fn longest_path_ranks(graph: &RealGraph) -> Vec<i64> {
    let n = graph.ids.len();
    let mut succ: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut indeg = vec![0u32; n];
    for e in &graph.edges {
        succ[e.working_source].push(e.working_target);
        indeg[e.working_target] += 1;
    }
    for adj in &mut succ {
        adj.sort_unstable();
    }

    let mut rank = vec![0i64; n];
    let mut remaining = indeg.clone();
    let mut queue: Vec<usize> = (0..n).filter(|&v| remaining[v] == 0).collect();
    let mut head = 0usize;
    while head < queue.len() {
        let u = queue[head];
        head += 1;
        let ru = rank[u];
        for &v in &succ[u] {
            if ru + 1 > rank[v] {
                rank[v] = ru + 1;
            }
            remaining[v] -= 1;
            if remaining[v] == 0 {
                queue.push(v);
            }
        }
    }
    rank
}

fn weak_components(graph: &RealGraph) -> Vec<Vec<usize>> {
    let n = graph.ids.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], x: usize) -> usize {
        if parent[x] != x {
            parent[x] = find(parent, parent[x]);
        }
        parent[x]
    }
    for e in &graph.edges {
        let (a, b) = (
            find(&mut parent, e.working_source),
            find(&mut parent, e.working_target),
        );
        if a != b {
            parent[a.max(b)] = a.min(b);
        }
    }
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for v in 0..n {
        let r = find(&mut parent, v);
        groups.entry(r).or_default().push(v);
    }
    groups.into_values().collect()
}

fn slack_of(rank: &[i64], src: usize, tgt: usize) -> i64 {
    rank[tgt] - rank[src] - MIN_SPAN
}

fn component_edges(graph: &RealGraph, comp: &BTreeSet<usize>) -> Vec<usize> {
    graph
        .edges
        .iter()
        .enumerate()
        .filter(|(_, e)| comp.contains(&e.working_source) && comp.contains(&e.working_target))
        .map(|(i, _)| i)
        .collect()
}

fn incident_edges(graph: &RealGraph, n: usize, comp_edges: &[usize]) -> Vec<Vec<usize>> {
    let mut inc = vec![Vec::new(); n];
    for &ei in comp_edges {
        let e = &graph.edges[ei];
        inc[e.working_source].push(ei);
        if e.working_source != e.working_target {
            inc[e.working_target].push(ei);
        }
    }
    inc
}

fn refine_component(graph: &RealGraph, comp: &[usize], rank: &mut [i64]) {
    let comp_set: BTreeSet<usize> = comp.iter().copied().collect();
    let comp_edges = component_edges(graph, &comp_set);
    if comp_edges.is_empty() {
        return;
    }

    let Some(tree_edges) = build_tight_tree(graph, comp, &comp_edges, rank) else {
        return;
    };

    let n = graph.ids.len();
    let inc = incident_edges(graph, n, &comp_edges);
    let mut tree = NsTree::build(comp[0], &tree_edges, graph, n);
    tree.init_cut_values(graph, &inc);

    let budget = 8 * (comp.len() + comp_edges.len()) + 64;

    for _ in 0..budget {
        let mut leave: Option<(usize, i64)> = None;
        for &edge_idx in &tree.tree_edges {
            let cv = tree.cut.get(&edge_idx).copied().unwrap_or(0);
            if cv >= 0 {
                continue;
            }
            let better = match leave {
                None => true,
                Some((best_edge, best_cv)) => {
                    cv < best_cv || (cv == best_cv && edge_idx < best_edge)
                }
            };
            if better {
                leave = Some((edge_idx, cv));
            }
        }
        let Some((leave_edge_idx, leave_cut)) = leave else {
            break;
        };

        let te = &graph.edges[leave_edge_idx];
        let head_is_child_subtree =
            tree.parent_edge[te.working_target] == Some(leave_edge_idx);
        let child = if head_is_child_subtree {
            te.working_target
        } else {
            te.working_source
        };
        let head_side_is_subtree = te.working_target == child;

        let mut enter: Option<(usize, i64)> = None;
        for &ei in &comp_edges {
            if tree.tree_edge_set.contains(&ei) {
                continue;
            }
            let e = &graph.edges[ei];
            let src_in_subtree = tree.in_subtree(child, e.working_source);
            let tgt_in_subtree = tree.in_subtree(child, e.working_target);
            let crosses_reverse = if head_side_is_subtree {
                src_in_subtree && !tgt_in_subtree
            } else {
                !src_in_subtree && tgt_in_subtree
            };
            if !crosses_reverse {
                continue;
            }
            let s = slack_of(rank, e.working_source, e.working_target);
            let better = match enter {
                None => true,
                Some((best_ei, best_s)) => s < best_s || (s == best_s && ei < best_ei),
            };
            if better {
                enter = Some((ei, s));
            }
        }
        let Some((enter_idx, delta)) = enter else {
            break;
        };

        let tail_side_is_subtree = !head_side_is_subtree;
        for &v in comp {
            if tree.in_subtree(child, v) == tail_side_is_subtree {
                rank[v] -= delta;
            }
        }

        let f_tail = graph.edges[enter_idx].working_source;
        let f_head = graph.edges[enter_idx].working_target;
        let lca = tree.treeupdate(graph, f_tail, f_head, leave_cut, true);
        let lca2 = tree.treeupdate(graph, f_head, f_tail, leave_cut, false);
        debug_assert_eq!(lca, lca2);

        tree.cut.insert(enter_idx, -leave_cut);
        tree.cut.remove(&leave_edge_idx);
        tree.exchange_edge(leave_edge_idx, enter_idx, graph);
        let lca_low = tree.low[lca];
        let par_lca = tree.parent_edge[lca];
        tree.dfs_range(lca, par_lca, lca_low);
    }
}

fn build_tight_tree(
    graph: &RealGraph,
    comp: &[usize],
    comp_edges: &[usize],
    rank: &mut [i64],
) -> Option<Vec<usize>> {
    let mut tree_nodes: BTreeSet<usize> = BTreeSet::new();
    tree_nodes.insert(comp[0]);
    let mut tree_edges = Vec::with_capacity(comp.len().saturating_sub(1));

    let outer_budget = 2 * comp.len() + 8;
    for _ in 0..outer_budget {
        if tree_nodes.len() == comp.len() {
            return Some(tree_edges);
        }
        let mut grown = false;
        for &ei in comp_edges {
            let e = &graph.edges[ei];
            let a_in = tree_nodes.contains(&e.working_source);
            let b_in = tree_nodes.contains(&e.working_target);
            if a_in == b_in {
                continue;
            }
            if slack_of(rank, e.working_source, e.working_target) == 0 {
                tree_nodes.insert(e.working_source);
                tree_nodes.insert(e.working_target);
                tree_edges.push(ei);
                grown = true;
                break;
            }
        }
        if grown {
            continue;
        }
        let mut best: Option<(usize, i64, bool)> = None;
        for &ei in comp_edges {
            let e = &graph.edges[ei];
            let a_in = tree_nodes.contains(&e.working_source);
            let b_in = tree_nodes.contains(&e.working_target);
            if a_in == b_in {
                continue;
            }
            let s = slack_of(rank, e.working_source, e.working_target);
            let better = match best {
                None => true,
                Some((best_ei, best_s, _)) => s < best_s || (s == best_s && ei < best_ei),
            };
            if better {
                best = Some((ei, s, b_in));
            }
        }
        let Some((_, s, head_in_tree)) = best else {
            return None;
        };
        let delta = if head_in_tree { -s } else { s };
        for &v in &tree_nodes {
            rank[v] += delta;
        }
    }
    None
}

struct NsTree {
    root: usize,
    parent_edge: Vec<Option<usize>>,
    low: Vec<usize>,
    lim: Vec<usize>,
    tree_adj: Vec<Vec<(usize, usize)>>,
    tree_edges: Vec<usize>,
    tree_edge_set: BTreeSet<usize>,
    cut: BTreeMap<usize, i64>,
}

impl NsTree {
    fn build(root: usize, tree_edges: &[usize], graph: &RealGraph, n: usize) -> Self {
        let mut tree_adj: Vec<Vec<(usize, usize)>> = vec![Vec::new(); n];
        let mut tree_edge_set = BTreeSet::new();
        for &ei in tree_edges {
            let e = &graph.edges[ei];
            tree_adj[e.working_source].push((e.working_target, ei));
            tree_adj[e.working_target].push((e.working_source, ei));
            tree_edge_set.insert(ei);
        }
        for v in &mut tree_adj {
            v.sort_unstable();
        }

        let mut tree = Self {
            root,
            parent_edge: vec![None; n],
            low: vec![0; n],
            lim: vec![0; n],
            tree_adj,
            tree_edges: tree_edges.to_vec(),
            tree_edge_set,
            cut: BTreeMap::new(),
        };
        tree.dfs_range(root, None, 1);
        tree
    }

    fn in_subtree(&self, subtree_root: usize, x: usize) -> bool {
        let lo = self.low[subtree_root];
        let hi = self.lim[subtree_root];
        let lx = self.lim[x];
        lo <= lx && lx <= hi
    }

    fn dfs_range(&mut self, v: usize, par: Option<usize>, low: usize) -> usize {
        self.parent_edge[v] = par;
        self.low[v] = low;
        let mut next = low;
        let children: Vec<(usize, usize)> = self.tree_adj[v]
            .iter()
            .copied()
            .filter(|&(_, ei)| Some(ei) != par)
            .collect();
        for (w, ei) in children {
            next = self.dfs_range(w, Some(ei), next);
        }
        self.lim[v] = next;
        next + 1
    }

    fn init_cut_values(&mut self, graph: &RealGraph, inc: &[Vec<usize>]) {
        self.cut.clear();
        self.dfs_cutval(self.root, None, graph, inc);
    }

    fn dfs_cutval(
        &mut self,
        v: usize,
        par: Option<usize>,
        graph: &RealGraph,
        inc: &[Vec<usize>],
    ) {
        let children: Vec<(usize, usize)> = self.tree_adj[v]
            .iter()
            .copied()
            .filter(|&(_, ei)| Some(ei) != par)
            .collect();
        for (w, ei) in children {
            self.dfs_cutval(w, Some(ei), graph, inc);
        }
        if let Some(pe) = par {
            let cv = self.x_cutval(pe, graph, inc);
            self.cut.insert(pe, cv);
        }
    }

    fn x_cutval(&self, f: usize, graph: &RealGraph, inc: &[Vec<usize>]) -> i64 {
        let e = &graph.edges[f];
        let (v, dir) = if self.parent_edge[e.working_source] == Some(f) {
            (e.working_source, 1i32)
        } else {
            (e.working_target, -1i32)
        };
        let mut sum = 0i64;
        for &ei in &inc[v] {
            sum += self.x_val(ei, v, dir, graph);
        }
        sum
    }

    fn x_val(&self, ei: usize, v: usize, dir: i32, graph: &RealGraph) -> i64 {
        let e = &graph.edges[ei];
        let other = if e.working_source == v {
            e.working_target
        } else {
            e.working_source
        };
        let weight = 1i64;
        let (f_cross, mut rv) = if !self.in_subtree(v, other) {
            (true, weight)
        } else {
            let rv = if self.tree_edge_set.contains(&ei) {
                self.cut.get(&ei).copied().unwrap_or(0) - weight
            } else {
                -weight
            };
            (false, rv)
        };
        let mut d = if dir > 0 {
            if e.working_target == v {
                1
            } else {
                -1
            }
        } else if e.working_source == v {
            1
        } else {
            -1
        };
        if f_cross {
            d = -d;
        }
        if d < 0 {
            rv = -rv;
        }
        rv
    }

    /// Graphviz `treeupdate`: walk from `v` toward LCA with `w`.
    fn treeupdate(
        &mut self,
        graph: &RealGraph,
        mut v: usize,
        w: usize,
        cutvalue: i64,
        dir: bool,
    ) -> usize {
        while !(self.low[v] <= self.lim[w] && self.lim[w] <= self.lim[v]) {
            let ei = self.parent_edge[v].expect("treeupdate walked past root");
            let e = &graph.edges[ei];
            let d = if v == e.working_source { dir } else { !dir };
            let entry = self.cut.entry(ei).or_insert(0);
            if d {
                *entry += cutvalue;
            } else {
                *entry -= cutvalue;
            }
            v = if self.lim[e.working_source] > self.lim[e.working_target] {
                e.working_source
            } else {
                e.working_target
            };
        }
        v
    }

    fn exchange_edge(&mut self, leave: usize, enter: usize, graph: &RealGraph) {
        let le = &graph.edges[leave];
        self.tree_adj[le.working_source].retain(|&(_, e)| e != leave);
        self.tree_adj[le.working_target].retain(|&(_, e)| e != leave);
        self.tree_edge_set.remove(&leave);
        self.tree_edges.retain(|&e| e != leave);

        let ee = &graph.edges[enter];
        self.tree_adj[ee.working_source].push((ee.working_target, enter));
        self.tree_adj[ee.working_target].push((ee.working_source, enter));
        self.tree_adj[ee.working_source].sort_unstable();
        self.tree_adj[ee.working_target].sort_unstable();
        self.tree_edge_set.insert(enter);
        self.tree_edges.push(enter);
    }
}

fn normalize_dense(rank: &[i64]) -> RankMap {
    if rank.is_empty() {
        return Vec::new();
    }
    let mut distinct: Vec<i64> = rank.to_vec();
    distinct.sort_unstable();
    distinct.dedup();
    let index_of = |r: i64| distinct.binary_search(&r).unwrap() as u32;
    rank.iter().map(|&r| index_of(r)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph(n: usize, edges: &[(usize, usize)]) -> RealGraph {
        let ids: Vec<String> = (0..n).map(|i| format!("n{i}")).collect();
        let index_of: BTreeMap<String, usize> = ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), i))
            .collect();
        let group_path = vec![Vec::new(); n];
        let edges = edges
            .iter()
            .enumerate()
            .map(
                |(i, &(s, t))| crate::layout::hierarchical::model::RealEdge {
                    edge_id: format!("e{i}"),
                    original_source: s,
                    original_target: t,
                    working_source: s,
                    working_target: t,
                    reversed: false,
                    from_port: None,
                    to_port: None,
                    critical: false,
                },
            )
            .collect();
        RealGraph {
            ids,
            index_of,
            group_path,
            shapes: vec![plotgram_model::NodeShape::DEFAULT; n],
            edges,
            self_loops: Vec::new(),
        }
    }

    fn assert_feasible(g: &RealGraph, rank: &RankMap) {
        for e in &g.edges {
            assert!(
                rank[e.working_target] as i64 - rank[e.working_source] as i64 >= MIN_SPAN,
                "edge {} -> {} not feasible: rank {} -> {}",
                e.working_source,
                e.working_target,
                rank[e.working_source],
                rank[e.working_target]
            );
        }
    }

    #[test]
    fn simple_chain() {
        let g = graph(4, &[(0, 1), (1, 2), (2, 3)]);
        let rank = assign_ranks(&g).unwrap();
        assert_eq!(rank, vec![0, 1, 2, 3]);
    }

    #[test]
    fn empty_graph() {
        let g = graph(0, &[]);
        assert!(assign_ranks(&g).unwrap().is_empty());
    }

    #[test]
    fn disconnected_components_each_start_at_zero() {
        let g = graph(4, &[(0, 1), (2, 3)]);
        let rank = assign_ranks(&g).unwrap();
        assert_eq!(rank[0], 0);
        assert_eq!(rank[1], 1);
        assert_eq!(rank[2], 0);
        assert_eq!(rank[3], 1);
    }

    #[test]
    fn network_simplex_balances_beyond_longest_path() {
        let s = 0;
        let b = 1;
        let (a1, a2, t1) = (2, 3, 4);
        let (b1, b2, t2) = (5, 6, 7);
        let (c1, c2, t3) = (8, 9, 10);
        let edges = [
            (s, b),
            (s, a1),
            (a1, a2),
            (a2, t1),
            (s, b1),
            (b1, b2),
            (b2, t2),
            (s, c1),
            (c1, c2),
            (c2, t3),
            (b, t1),
            (b, t2),
            (b, t3),
        ];
        let g = graph(11, &edges);
        let rank = assign_ranks(&g).unwrap();
        assert_feasible(&g, &rank);
        assert_eq!(rank[s], 0);
        assert_eq!(rank[t1], 3);
        assert_eq!(rank[t2], 3);
        assert_eq!(rank[t3], 3);
        assert_eq!(
            rank[b], 2,
            "NS should pull b later than longest-path's rank 1"
        );
    }

    #[test]
    fn deterministic_rerun() {
        let g = graph(6, &[(0, 1), (0, 2), (1, 3), (2, 3), (3, 4), (3, 5)]);
        let r1 = assign_ranks(&g).unwrap();
        let r2 = assign_ranks(&g).unwrap();
        assert_eq!(r1, r2);
    }

    #[test]
    fn diamond_is_already_optimal_longest_path() {
        let g = graph(4, &[(0, 1), (0, 2), (1, 3), (2, 3)]);
        let rank = assign_ranks(&g).unwrap();
        assert_eq!(rank, vec![0, 1, 1, 2]);
    }
}
