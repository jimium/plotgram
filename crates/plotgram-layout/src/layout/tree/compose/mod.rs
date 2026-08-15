//! Compose: root selection + spanning tree (first visit wins).

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use plotgram_engine_api::LayoutError;
use plotgram_model::graph::{Graph, NodeRole};

use super::params::TreeParams;
use super::plan::TreePlan;

pub fn compose(graph: &Graph, params: &TreeParams) -> Result<TreePlan, LayoutError> {
    let nodes: Vec<String> = graph
        .all_nodes()
        .into_iter()
        .filter(|n| n.role == NodeRole::Entity)
        .map(|n| n.id.clone())
        .collect();
    let node_set: BTreeSet<&str> = nodes.iter().map(|s| s.as_str()).collect();

    let mut outgoing: BTreeMap<&str, Vec<(&str, &str)>> = BTreeMap::new();
    let mut indeg: BTreeMap<&str, u32> = nodes.iter().map(|id| (id.as_str(), 0)).collect();
    let mut extra_edge_ids = Vec::new();

    for edge in graph.edges_in_declaration_order() {
        if !node_set.contains(edge.source.as_str()) || !node_set.contains(edge.target.as_str()) {
            extra_edge_ids.push(edge.id.clone());
            continue;
        }
        if edge.undirected || edge.source == edge.target {
            extra_edge_ids.push(edge.id.clone());
            continue;
        }
        outgoing
            .entry(edge.source.as_str())
            .or_default()
            .push((edge.id.as_str(), edge.target.as_str()));
        *indeg.entry(edge.target.as_str()).or_insert(0) += 1;
    }

    let roots: Vec<String> = if let Some(root) = &params.root {
        if !node_set.contains(root.as_str()) {
            return Err(LayoutError::message(format!(
                "tree: invalid: root `{root}` is not a participant node"
            )));
        }
        vec![root.clone()]
    } else {
        nodes
            .iter()
            .filter(|id| indeg.get(id.as_str()).copied().unwrap_or(0) == 0)
            .cloned()
            .collect()
    };

    if roots.is_empty() && !nodes.is_empty() {
        return Err(LayoutError::message(
            "tree: invalid: graph has no root (every entity has an incoming tree edge)",
        ));
    }

    let mut children: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut parent: BTreeMap<String, String> = BTreeMap::new();
    let mut depth: BTreeMap<String, u32> = BTreeMap::new();
    let mut tree_edge_ids = Vec::new();
    let mut visited: BTreeSet<String> = BTreeSet::new();
    let mut roots = roots;

    let walk_from = |root: &str,
                     visited: &mut BTreeSet<String>,
                     children: &mut BTreeMap<String, Vec<String>>,
                     parent: &mut BTreeMap<String, String>,
                     depth: &mut BTreeMap<String, u32>,
                     tree_edge_ids: &mut Vec<String>,
                     extra_edge_ids: &mut Vec<String>| {
        if !visited.insert(root.to_string()) {
            return;
        }
        depth.insert(root.to_string(), 0);
        let mut q = VecDeque::new();
        q.push_back(root.to_string());
        while let Some(u) = q.pop_front() {
            let d = *depth.get(&u).unwrap_or(&0);
            let outs = outgoing.get(u.as_str()).cloned().unwrap_or_default();
            for (eid, v) in outs {
                if visited.contains(v) {
                    extra_edge_ids.push(eid.to_string());
                    continue;
                }
                visited.insert(v.to_string());
                parent.insert(v.to_string(), u.clone());
                children.entry(u.clone()).or_default().push(v.to_string());
                depth.insert(v.to_string(), d + 1);
                tree_edge_ids.push(eid.to_string());
                q.push_back(v.to_string());
            }
        }
    };

    for root in roots.clone() {
        walk_from(
            &root,
            &mut visited,
            &mut children,
            &mut parent,
            &mut depth,
            &mut tree_edge_ids,
            &mut extra_edge_ids,
        );
    }
    // Unreached components become extra forest roots (declaration order).
    for id in &nodes {
        if visited.contains(id) {
            continue;
        }
        roots.push(id.clone());
        walk_from(
            id,
            &mut visited,
            &mut children,
            &mut parent,
            &mut depth,
            &mut tree_edge_ids,
            &mut extra_edge_ids,
        );
    }

    Ok(TreePlan {
        roots,
        nodes,
        children,
        parent,
        depth,
        tree_edge_ids,
        extra_edge_ids,
    })
}
