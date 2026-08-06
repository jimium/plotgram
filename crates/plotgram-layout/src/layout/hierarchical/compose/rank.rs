//! P2 ranking: simplified Network Simplex (Gansner, Koutsofios, North, Vo 1993),
//! weight = 1 uniform (no dummies exist yet — properify runs after ranking).
//!
//! Deliberately O(V·(V+E)) per tighten/pivot step rather than the incremental
//! O(V+E) low/lim cut-value bookkeeping production implementations use — see
//! `docs/design/layout/hierarchical/notes/2026-08-02-mvp-scope.md` §2.1 for
//! why that's an acceptable trade at layout-fixture scale.

use std::collections::BTreeSet;

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

    Ok(normalize_dense(&rank))
}

/// Longest-path ranking over the working DAG (Kahn's algorithm). Feasible but
/// not necessarily tight/minimal — [`refine_component`] improves it.
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

/// Weakly-connected components over `graph.edges`, node indices sorted
/// ascending within each component, components sorted by minimum member.
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
    let mut groups: std::collections::BTreeMap<usize, Vec<usize>> =
        std::collections::BTreeMap::new();
    for v in 0..n {
        let r = find(&mut parent, v);
        groups.entry(r).or_default().push(v);
    }
    groups.into_values().collect()
}

fn slack_of(rank: &[i64], src: usize, tgt: usize) -> i64 {
    rank[tgt] - rank[src] - MIN_SPAN
}

/// Edges (indices into `graph.edges`) with both endpoints in `comp`, in
/// declaration order.
fn component_edges(graph: &RealGraph, comp: &BTreeSet<usize>) -> Vec<usize> {
    graph
        .edges
        .iter()
        .enumerate()
        .filter(|(_, e)| comp.contains(&e.working_source) && comp.contains(&e.working_target))
        .map(|(i, _)| i)
        .collect()
}

/// Build a feasible tight spanning tree over `comp`, then repeatedly pivot on
/// negative-cut-value tree edges until optimal (or the iteration budget is
/// exhausted). Mutates `rank` in place for nodes in `comp`.
fn refine_component(graph: &RealGraph, comp: &[usize], rank: &mut [i64]) {
    let comp_set: BTreeSet<usize> = comp.iter().copied().collect();
    let comp_edges = component_edges(graph, &comp_set);
    if comp_edges.is_empty() {
        return; // isolated nodes, nothing to tighten
    }

    let Some(mut tree_edges) = build_tight_tree(graph, comp, &comp_edges, rank) else {
        return; // defensive: should not happen for a connected component
    };

    let budget = 8 * (comp.len() + comp_edges.len()) + 64;
    let mut current_length = total_weighted_length(graph, &comp_edges, rank);

    for _ in 0..budget {
        let tree = TreeShape::build(comp[0], &tree_edges, graph);

        // Most-negative cut value wins; tie-break by the tree edge's index
        // into `graph.edges` (a stable proxy for declaration order — edges
        // are pushed in declaration order by `graph_index::build_real_graph`).
        let mut leave: Option<(
            usize, /*tree_edges idx*/
            i64,   /*cut value*/
            usize, /*edge_idx*/
        )> = None;
        for (ti, &edge_idx) in tree_edges.iter().enumerate() {
            let cv = cut_value(graph, &tree, edge_idx, &comp_edges);
            if cv >= 0 {
                continue;
            }
            let better = match leave {
                None => true,
                Some((_, best_cv, best_edge)) => {
                    cv < best_cv || (cv == best_cv && edge_idx < best_edge)
                }
            };
            if better {
                leave = Some((ti, cv, edge_idx));
            }
        }
        let Some((leave_ti, _, leave_edge_idx)) = leave else {
            break; // no negative cut value: optimal
        };
        let te = &graph.edges[leave_edge_idx];
        // Whichever endpoint is the DFS child of this tree edge owns the subtree.
        let head_is_child_subtree =
            tree.parent_edge.get(&te.working_target) == Some(&leave_edge_idx);
        let child = if head_is_child_subtree {
            te.working_target
        } else {
            te.working_source
        };
        let head_side_is_subtree = te.working_target == child;

        // Entering edge: crosses from head-side back to tail-side, minimal slack.
        let mut enter: Option<(usize, i64)> = None;
        for &ei in &comp_edges {
            if tree_edges.contains(&ei) {
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
            break; // defensive: shouldn't happen for a connected component
        };

        let tail_side_is_subtree = !head_side_is_subtree;
        for &v in comp {
            let in_subtree = tree.in_subtree(child, v);
            if in_subtree == tail_side_is_subtree {
                rank[v] -= delta;
            }
        }

        tree_edges[leave_ti] = enter_idx;

        let new_length = total_weighted_length(graph, &comp_edges, rank);
        if new_length > current_length {
            // Defensive: a correct pivot never increases total length; treat
            // as a stability guard against float/index edge cases and stop.
            break;
        }
        current_length = new_length;
    }
}

fn total_weighted_length(graph: &RealGraph, edges: &[usize], rank: &[i64]) -> i64 {
    edges
        .iter()
        .map(|&i| {
            let e = &graph.edges[i];
            rank[e.working_target] - rank[e.working_source]
        })
        .sum()
}

/// Grow a spanning tree over `comp` via the classic "grow tight, else shift
/// and retry" method. Returns the list of tree edges (indices into
/// `graph.edges`), or `None` if `comp` is not actually connected by
/// `comp_edges` (should not happen — `comp` comes from [`weak_components`]).
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
        // 1) grow via any tight edge with exactly one endpoint in the tree.
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
        // 2) stuck: shift the whole current tree by the minimal boundary slack.
        let mut best: Option<(usize, i64, bool)> = None; // (edge_idx, slack, head_in_tree)
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
            return None; // not connected — defensive
        };
        let delta = if head_in_tree { -s } else { s };
        for &v in &tree_nodes {
            rank[v] += delta;
        }
    }
    None
}

/// DFS shape of the current tree (rooted arbitrarily at `root`), giving O(1)
/// subtree-membership queries via preorder (`tin`/`tout`) ranges.
struct TreeShape {
    node: Vec<NodeSpan>,
    parent_edge: std::collections::BTreeMap<usize, usize>,
}

#[derive(Clone, Copy)]
struct NodeSpan {
    tin: usize,
    tout: usize,
}

impl TreeShape {
    fn build(root: usize, tree_edges: &[usize], graph: &RealGraph) -> Self {
        let mut adj: std::collections::BTreeMap<usize, Vec<(usize, usize)>> =
            std::collections::BTreeMap::new();
        for &ei in tree_edges {
            let e = &graph.edges[ei];
            adj.entry(e.working_source)
                .or_default()
                .push((e.working_target, ei));
            adj.entry(e.working_target)
                .or_default()
                .push((e.working_source, ei));
        }
        for v in adj.values_mut() {
            v.sort_unstable();
        }

        let n = graph.ids.len();
        let mut node = vec![
            NodeSpan {
                tin: usize::MAX,
                tout: 0
            };
            n
        ];
        let mut parent_edge = std::collections::BTreeMap::new();
        let mut timer = 0usize;

        // Explicit-stack DFS (bounded graph size; avoids recursion-depth worries).
        // Order is a valid preorder because children are only pushed once,
        // immediately after their parent is popped.
        let mut order: Vec<usize> = Vec::new();
        let mut visited = vec![false; n];
        let mut children: std::collections::BTreeMap<usize, Vec<usize>> =
            std::collections::BTreeMap::new();
        let mut dfs_stack = vec![root];
        visited[root] = true;
        while let Some(u) = dfs_stack.pop() {
            order.push(u);
            if let Some(neis) = adj.get(&u) {
                for &(v, ei) in neis {
                    if !visited[v] {
                        visited[v] = true;
                        parent_edge.insert(v, ei);
                        children.entry(u).or_default().push(v);
                        dfs_stack.push(v);
                    }
                }
            }
        }
        for &u in &order {
            node[u].tin = timer;
            timer += 1;
        }
        // tout via reverse-order accumulation (post-order max over subtree).
        for &u in order.iter().rev() {
            let mut hi = node[u].tin;
            if let Some(ch) = children.get(&u) {
                for &c in ch {
                    hi = hi.max(node[c].tout);
                }
            }
            node[u].tout = hi;
        }

        TreeShape { node, parent_edge }
    }

    fn in_subtree(&self, subtree_root: usize, x: usize) -> bool {
        let r = self.node[subtree_root];
        let n = self.node[x];
        n.tin != usize::MAX && r.tin <= n.tin && n.tin <= r.tout
    }
}

fn cut_value(
    graph: &RealGraph,
    tree: &TreeShape,
    tree_edge_idx: usize,
    comp_edges: &[usize],
) -> i64 {
    let te = &graph.edges[tree_edge_idx];
    let head_is_child = tree.parent_edge.get(&te.working_target) == Some(&tree_edge_idx);
    let child = if head_is_child {
        te.working_target
    } else {
        te.working_source
    };
    let head_side_is_subtree = te.working_target == child;

    let mut cv = 0i64;
    for &ei in comp_edges {
        let e = &graph.edges[ei];
        let src_in = tree.in_subtree(child, e.working_source);
        let tgt_in = tree.in_subtree(child, e.working_target);
        if src_in == tgt_in {
            continue;
        }
        // +1 if this edge goes tail-side -> head-side (same direction as the
        // tree edge), -1 if head-side -> tail-side.
        let goes_tail_to_head = if head_side_is_subtree {
            !src_in && tgt_in
        } else {
            src_in && !tgt_in
        };
        cv += if goes_tail_to_head { 1 } else { -1 };
    }
    cv
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
    use std::collections::BTreeMap;

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
        // s -> b; s -> {a1->a2->t1, b1->b2->t2, c1->c2->t3} (three length-3
        // chains from s); b -> t1, b -> t2, b -> t3.
        // Longest-path pins b at rank 1 (as early as feasible). Minimizing
        // total edge length (3 outgoing from b + 1 incoming to b) strictly
        // prefers b at rank 2 (see notes doc §2.1 derivation): objective
        // 3*(3-b) + b = 9-2b, minimized at the upper feasible bound b=2.
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
