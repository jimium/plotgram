//! Build the dense [`RealGraph`] from `plotgram_model::graph::Graph`: node
//! declaration order, group scope paths, and self-loop extraction.

use std::collections::BTreeMap;

use plotgram_model::graph::{Graph, Group};

use crate::layout::hierarchical::model::{RealEdge, RealGraph};

/// Walk `graph.groups` recording each node id's root..leaf group path.
fn index_group_paths(groups: &[Group], prefix: &[String], out: &mut BTreeMap<String, Vec<String>>) {
    for g in groups {
        let mut path = prefix.to_vec();
        path.push(g.id.clone());
        for n in &g.nodes {
            out.insert(n.id.clone(), path.clone());
        }
        index_group_paths(&g.groups, &path, out);
    }
}

pub fn build_real_graph(graph: &Graph) -> RealGraph {
    let ids = graph.all_node_ids();
    let index_of: BTreeMap<String, usize> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.clone(), i))
        .collect();

    let mut paths: BTreeMap<String, Vec<String>> = BTreeMap::new();
    index_group_paths(&graph.groups, &[], &mut paths);
    let group_path: Vec<Vec<String>> = ids
        .iter()
        .map(|id| paths.get(id).cloned().unwrap_or_default())
        .collect();

    let mut edges = Vec::new();
    let mut self_loops = Vec::new();
    for e in graph.edges_in_declaration_order() {
        let s = index_of[&e.source];
        let t = index_of[&e.target];
        if s == t {
            self_loops.push((e.id.clone(), s));
            continue;
        }
        edges.push(RealEdge {
            edge_id: e.id.clone(),
            original_source: s,
            original_target: t,
            working_source: s,
            working_target: t,
            reversed: false,
            from_port: e.from_port,
            to_port: e.to_port,
        });
    }

    RealGraph {
        ids,
        index_of,
        group_path,
        edges,
        self_loops,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plotgram_model::attr::AttrMap;
    use plotgram_model::graph::{Arrow, Edge, Node, NodeRole};

    fn node(id: &str) -> Node {
        Node {
            id: id.to_string(),
            label: None,
            shape: None,
            role: NodeRole::Entity,
            host_group: None,
            anchor: None,
            partition_cell: None,
            attrs: AttrMap::new(),
        }
    }

    fn edge(id: &str, source: &str, target: &str) -> Edge {
        Edge {
            id: id.to_string(),
            source: source.to_string(),
            target: target.to_string(),
            arrow: Arrow::Forward,
            label: None,
            head_label: None,
            tail_label: None,
            from_port: None,
            to_port: None,
            edge_group: None,
            attrs: AttrMap::new(),
        }
    }

    #[test]
    fn self_loops_are_extracted_and_group_paths_recorded() {
        let graph = Graph {
            nodes: vec![node("a")],
            edges: vec![edge("e0", "a", "a"), edge("e1", "a", "b")],
            groups: vec![plotgram_model::graph::Group {
                id: "g".into(),
                label: None,
                attrs: AttrMap::new(),
                nodes: vec![node("b")],
                edges: vec![],
                groups: vec![],
            }],
            partition: None,
        };
        let rg = build_real_graph(&graph);
        assert_eq!(rg.ids, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(rg.self_loops, vec![("e0".to_string(), rg.index_of["a"])]);
        assert_eq!(rg.edges.len(), 1);
        assert_eq!(rg.edges[0].edge_id, "e1");
        assert!(rg.group_path[rg.index_of["a"]].is_empty());
        assert_eq!(rg.group_path[rg.index_of["b"]], vec!["g".to_string()]);
    }
}
