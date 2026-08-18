//! Build the dense [`RealGraph`] from `tautcore_model::graph::Graph`: node
//! declaration order, group scope paths, and self-loop extraction.

use std::collections::BTreeMap;

use tautcore_model::graph::{Graph, Group};
use tautcore_model::NodeShape;

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

    let shapes: Vec<NodeShape> = ids
        .iter()
        .map(|id| {
            graph
                .find_node(id)
                .and_then(|n| n.shape)
                .unwrap_or(NodeShape::DEFAULT)
        })
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
            from_port: e.from_port.clone(),
            to_port: e.to_port.clone(),
            weight: e.weight.unwrap_or(1.0),
            undirected: e.undirected,
        });
    }

    // PG-0: author partition facts enter Hier here (previously dropped).
    // `all_nodes()` walks the same declaration order as `all_node_ids()`.
    let partition = graph.partition.clone();
    let partition_cell: Vec<Option<tautcore_model::partition::PartitionCell>> = graph
        .all_nodes()
        .iter()
        .map(|n| n.partition_cell.clone())
        .collect();

    RealGraph {
        ids,
        index_of,
        group_path,
        shapes,
        edges,
        self_loops,
        intra_layer: Vec::new(),
        partition,
        partition_cell,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tautcore_model::attr::AttrMap;
    use tautcore_model::graph::{Arrow, Edge, Node, NodeRole};
    use tautcore_model::NodeShape;

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
            weight: None,
            undirected: false,
            attrs: AttrMap::new(),
        }
    }

    #[test]
    fn self_loops_are_extracted_and_group_paths_recorded() {
        let graph = Graph {
            nodes: vec![node("a")],
            edges: vec![edge("e0", "a", "a"), edge("e1", "a", "b")],
            groups: vec![tautcore_model::graph::Group {
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
        assert_eq!(rg.shapes, vec![NodeShape::DEFAULT, NodeShape::DEFAULT]);
        assert_eq!(rg.self_loops, vec![("e0".to_string(), rg.index_of["a"])]);
        assert_eq!(rg.edges.len(), 1);
        assert_eq!(rg.edges[0].edge_id, "e1");
        assert!(rg.group_path[rg.index_of["a"]].is_empty());
        assert_eq!(rg.group_path[rg.index_of["b"]], vec!["g".to_string()]);
        // No partition block: grid absent, cells empty (consumers gate on this).
        assert!(rg.partition.is_none());
        assert!(rg.partition_cell.iter().all(Option::is_none));
    }

    /// PG-0: grid + per-node cells survive into `RealGraph`, dense and in
    /// declaration order (top-level nodes, then group members depth-first).
    #[test]
    fn partition_grid_and_cells_are_carried_into_real_graph() {
        use tautcore_model::partition::{PartitionAxis, PartitionCell, PartitionGrid};

        let mut in_customer = node("place_order");
        in_customer.partition_cell = Some(PartitionCell::col("customer"));
        let mut in_sales = node("verify_order");
        in_sales.partition_cell = Some(PartitionCell::col("sales"));
        let unassigned = node("archive"); // no cell — free zone, allowed
        let graph = Graph {
            nodes: vec![in_customer, unassigned],
            edges: vec![],
            groups: vec![tautcore_model::graph::Group {
                id: "g".into(),
                label: None,
                attrs: AttrMap::new(),
                nodes: vec![in_sales],
                edges: vec![],
                groups: vec![],
            }],
            partition: Some(PartitionGrid {
                columns: vec![PartitionAxis::new("customer"), PartitionAxis::new("sales")],
                rows: vec![],
            }),
        };
        let rg = build_real_graph(&graph);
        assert_eq!(rg.ids, vec!["place_order", "archive", "verify_order"]);
        let grid = rg.partition.as_ref().expect("grid carried through");
        assert_eq!(
            grid.columns
                .iter()
                .map(|a| a.id.as_str())
                .collect::<Vec<_>>(),
            vec!["customer", "sales"]
        );
        assert_eq!(
            rg.partition_cell[rg.index_of["place_order"]],
            Some(PartitionCell::col("customer"))
        );
        assert_eq!(rg.partition_cell[rg.index_of["archive"]], None);
        assert_eq!(
            rg.partition_cell[rg.index_of["verify_order"]],
            Some(PartitionCell::col("sales"))
        );
    }
}
