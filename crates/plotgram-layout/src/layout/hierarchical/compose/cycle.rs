//! P1 cycle removal: `plotgram_algo::fas::greedy_fas` wrapper.
//!
//! Writer: working direction / `reversed` only. `original_source`/`target`
//! (set by [`super::graph_index::build_real_graph`]) are never touched here —
//! ports, arrows and labels must trace back to them, never to `working_*`.

use crate::layout::hierarchical::model::RealGraph;

/// Run Greedy-FAS over `graph.edges` and set `working_*` / `reversed` in
/// place. Deterministic: `plotgram_algo::fas::greedy_fas` ties break by
/// smallest node index, which here is declaration order (`RealGraph::ids`).
pub fn remove_cycles(graph: &mut RealGraph) {
    let pairs: Vec<(usize, usize)> = graph
        .edges
        .iter()
        .map(|e| (e.original_source, e.original_target))
        .collect();
    let reversed = plotgram_algo::fas::greedy_fas(graph.ids.len(), &pairs);

    for (i, e) in graph.edges.iter_mut().enumerate() {
        if reversed.contains(&i) {
            e.working_source = e.original_target;
            e.working_target = e.original_source;
            e.reversed = true;
        } else {
            e.working_source = e.original_source;
            e.working_target = e.original_target;
            e.reversed = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn graph(ids: &[&str], edges: &[(usize, usize)]) -> RealGraph {
        let ids: Vec<String> = ids.iter().map(|s| s.to_string()).collect();
        let index_of: BTreeMap<String, usize> = ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), i))
            .collect();
        let group_path = vec![Vec::new(); ids.len()];
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
                },
            )
            .collect();
        RealGraph {
            ids,
            index_of,
            group_path,
            edges,
            self_loops: Vec::new(),
        }
    }

    #[test]
    fn dag_untouched() {
        let mut g = graph(&["a", "b", "c"], &[(0, 1), (1, 2)]);
        remove_cycles(&mut g);
        assert!(g.edges.iter().all(|e| !e.reversed));
    }

    #[test]
    fn two_cycle_reverses_one_edge_keeps_original_semantics() {
        let mut g = graph(&["a", "b"], &[(0, 1), (1, 0)]);
        remove_cycles(&mut g);
        // exactly one of the two edges is reversed; original fields untouched
        let reversed_count = g.edges.iter().filter(|e| e.reversed).count();
        assert_eq!(reversed_count, 1);
        assert_eq!(g.edges[0].original_source, 0);
        assert_eq!(g.edges[0].original_target, 1);
        assert_eq!(g.edges[1].original_source, 1);
        assert_eq!(g.edges[1].original_target, 0);
        for e in &g.edges {
            if e.reversed {
                assert_eq!(e.working_source, e.original_target);
                assert_eq!(e.working_target, e.original_source);
            } else {
                assert_eq!(e.working_source, e.original_source);
                assert_eq!(e.working_target, e.original_target);
            }
        }
    }

    #[test]
    fn deterministic_rerun() {
        let mut g1 = graph(&["a", "b", "c", "d"], &[(0, 1), (1, 2), (2, 0), (2, 3)]);
        let mut g2 = graph(&["a", "b", "c", "d"], &[(0, 1), (1, 2), (2, 0), (2, 3)]);
        remove_cycles(&mut g1);
        remove_cycles(&mut g2);
        let r1: Vec<bool> = g1.edges.iter().map(|e| e.reversed).collect();
        let r2: Vec<bool> = g2.edges.iter().map(|e| e.reversed).collect();
        assert_eq!(r1, r2);
    }
}
