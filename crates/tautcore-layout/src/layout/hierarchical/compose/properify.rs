//! P3.5 properify: expand every working edge spanning >1 rank into a stable
//! dummy chain (composition.md §5.1). Produces the [`PlanGraph`] every
//! downstream phase (ordering, ports, coordinates, ink) reads.

use std::collections::BTreeMap;

use crate::layout::hierarchical::compose::rank::RankMap;
use crate::layout::hierarchical::model::RealGraph;
use crate::layout::hierarchical::model::{Elem, ElemKey, PlanGraph, RealEdge, Segment};

/// Partition undirected edges after ranking: zero-span ones move to
/// [`RealGraph::intra_layer`] (Ink routes them as side-links, invisible to
/// ordering/properify/channel); the rest stay in [`RealGraph::edges`] with
/// the working direction normalized downward, so [`properify`]'s `r1 > r0`
/// invariant holds without ever marking them `reversed` (arrowheads and
/// labels trace `original_*`, which are untouched).
pub fn split_intra_layer(graph: &mut RealGraph, rank: &RankMap) {
    let mut kept: Vec<RealEdge> = Vec::with_capacity(graph.edges.len());
    for mut e in std::mem::take(&mut graph.edges) {
        if !e.undirected {
            kept.push(e);
            continue;
        }
        let rs = rank[e.working_source];
        let rt = rank[e.working_target];
        if rs == rt {
            graph.intra_layer.push(e);
            continue;
        }
        if rs > rt {
            std::mem::swap(&mut e.working_source, &mut e.working_target);
        }
        kept.push(e);
    }
    graph.edges = kept;
}

pub fn properify(graph: &RealGraph, rank: &RankMap) -> PlanGraph {
    let mut elems: Vec<Elem> = Vec::with_capacity(graph.ids.len());
    let mut index_of: BTreeMap<ElemKey, usize> = BTreeMap::new();
    let mut decl_index: Vec<usize> = Vec::new();

    for (i, id) in graph.ids.iter().enumerate() {
        let key = ElemKey::Real(id.clone());
        index_of.insert(key.clone(), elems.len());
        elems.push(Elem {
            key,
            group_path: graph.group_path[i].clone(),
            rank: rank[i],
        });
        decl_index.push(i);
    }
    let mut next_decl = graph.ids.len();

    let mut segments = Vec::new();
    for e in &graph.edges {
        let r0 = rank[e.working_source];
        let r1 = rank[e.working_target];
        debug_assert!(r1 > r0, "properify requires a feasible ranking (span >= 1)");
        let span = r1 - r0;

        if span == 1 {
            let from = index_of[&ElemKey::Real(graph.ids[e.working_source].clone())];
            let to = index_of[&ElemKey::Real(graph.ids[e.working_target].clone())];
            segments.push(Segment {
                edge_id: e.edge_id.clone(),
                ordinal: 0,
                from,
                to,
            });
            continue;
        }

        let mut prev = index_of[&ElemKey::Real(graph.ids[e.working_source].clone())];
        for k in 0..(span - 1) {
            let key = ElemKey::Virtual {
                edge_id: e.edge_id.clone(),
                ordinal: k,
            };
            let idx = elems.len();
            index_of.insert(key.clone(), idx);
            elems.push(Elem {
                key,
                group_path: Vec::new(),
                rank: r0 + 1 + k,
            });
            decl_index.push(next_decl);
            next_decl += 1;

            segments.push(Segment {
                edge_id: e.edge_id.clone(),
                ordinal: k,
                from: prev,
                to: idx,
            });
            prev = idx;
        }
        let to = index_of[&ElemKey::Real(graph.ids[e.working_target].clone())];
        segments.push(Segment {
            edge_id: e.edge_id.clone(),
            ordinal: span - 1,
            from: prev,
            to,
        });
    }

    let max_rank = elems.iter().map(|e| e.rank).max().unwrap_or(0);
    let mut layers: Vec<Vec<usize>> = vec![Vec::new(); (max_rank as usize) + 1];
    // Populate in decl_index order so each layer's initial order is stable
    // and reals precede same-layer virtuals (virtuals get decl_index >= n).
    let mut order: Vec<usize> = (0..elems.len()).collect();
    order.sort_by_key(|&i| decl_index[i]);
    for i in order {
        layers[elems[i].rank as usize].push(i);
    }

    PlanGraph {
        elems,
        index_of,
        decl_index,
        segments,
        layers,
        partition_columns: Vec::new(),
        partition_cross_kind: Default::default(),
        partition_elem_col: Vec::new(),
        partition_rows: Vec::new(),
        partition_main_kind: Default::default(),
        partition_elem_row: Vec::new(),
        partition_row_intervals: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::hierarchical::model::RealEdge;
    use std::collections::BTreeMap as Map;

    fn graph(n: usize, edges: &[(usize, usize)]) -> RealGraph {
        let ids: Vec<String> = (0..n).map(|i| format!("n{i}")).collect();
        let index_of: Map<String, usize> = ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), i))
            .collect();
        let group_path = vec![Vec::new(); n];
        let edges = edges
            .iter()
            .enumerate()
            .map(|(i, &(s, t))| RealEdge {
                edge_id: format!("e{i}"),
                original_source: s,
                original_target: t,
                working_source: s,
                working_target: t,
                reversed: false,
                from_port: None,
                to_port: None,
                weight: 1.0,
                ..Default::default()
            })
            .collect();
        RealGraph {
            ids,
            index_of,
            group_path,
            shapes: vec![tautcore_model::NodeShape::DEFAULT; n],
            edges,
            self_loops: Vec::new(),
            ..Default::default()
        }
    }

    #[test]
    fn short_edge_gets_one_segment_no_dummy() {
        let g = graph(2, &[(0, 1)]);
        let rank = vec![0, 1];
        let plan = properify(&g, &rank);
        assert_eq!(plan.elems.len(), 2);
        assert_eq!(plan.segments.len(), 1);
        assert_eq!(plan.segments[0].from, 0);
        assert_eq!(plan.segments[0].to, 1);
    }

    #[test]
    fn long_edge_gets_dummy_chain() {
        let g = graph(2, &[(0, 1)]);
        let rank = vec![0, 3]; // span 3 -> 2 dummies, 3 segments
        let plan = properify(&g, &rank);
        assert_eq!(plan.elems.len(), 4);
        assert_eq!(plan.segments.len(), 3);
        assert_eq!(plan.layers.len(), 4);
        assert_eq!(plan.layers[0], vec![0]);
        assert_eq!(plan.layers[3], vec![1]);
        assert_eq!(plan.layers[1].len(), 1);
        assert_eq!(plan.layers[2].len(), 1);
        // chain connects source -> dummy -> dummy -> target
        let mut cur = plan.segments.iter().find(|s| s.from == 0).unwrap().to;
        let mut hops = 1;
        while cur != 1 {
            cur = plan.segments.iter().find(|s| s.from == cur).unwrap().to;
            hops += 1;
        }
        assert_eq!(hops, 3);
    }

    #[test]
    fn virtual_elems_have_empty_group_path() {
        let g = graph(2, &[(0, 1)]);
        let rank = vec![0, 2];
        let plan = properify(&g, &rank);
        let dummy = plan.elems.iter().find(|e| e.key.is_virtual()).unwrap();
        assert!(dummy.group_path.is_empty());
    }

    #[test]
    fn split_intra_layer_moves_zero_span_and_normalizes_downward() {
        // e0: undirected 0~1 (same rank) → intra_layer; e1: undirected 2~0
        // (rank 1 > 0) → kept, working direction swapped downward, never
        // `reversed`; e2: directed stays untouched.
        let mut g = graph(3, &[(0, 1), (2, 0), (0, 2)]);
        g.edges[0].undirected = true;
        g.edges[1].undirected = true;
        let rank = vec![0, 0, 1];
        split_intra_layer(&mut g, &rank);
        assert_eq!(g.intra_layer.len(), 1);
        assert_eq!(g.intra_layer[0].edge_id, "e0");
        assert_eq!(g.edges.len(), 2);
        let e1 = g.edges.iter().find(|e| e.edge_id == "e1").unwrap();
        assert_eq!(e1.working_source, 0);
        assert_eq!(e1.working_target, 2);
        assert!(!e1.reversed);
        assert_eq!(e1.original_source, 2);
        assert_eq!(e1.original_target, 0);
    }
}
