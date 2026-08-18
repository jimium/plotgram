//! Compose: weak components + undirected simple adjacency + edge roles.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use plotgram_engine_api::LayoutError;
use plotgram_model::graph::{Graph, NodeRole};

use super::plan::{EdgeRole, OrgComponent, OrganicPlan};

pub fn compose(graph: &Graph) -> Result<(OrganicPlan, Vec<String>), LayoutError> {
    let nodes: Vec<String> = graph
        .all_nodes()
        .into_iter()
        .filter(|n| n.role == NodeRole::Entity)
        .map(|n| n.id.clone())
        .collect();
    let decl: BTreeMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    let node_set: BTreeSet<&str> = nodes.iter().map(|s| s.as_str()).collect();

    // Undirected simple adjacency + declaration-ordered edge stream.
    let mut undirected: BTreeMap<&str, BTreeSet<&str>> = nodes
        .iter()
        .map(|id| (id.as_str(), BTreeSet::new()))
        .collect();
    let mut pair_edges: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    let mut loops: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for edge in graph.edges_in_declaration_order() {
        if !node_set.contains(edge.source.as_str())
            || !node_set.contains(edge.target.as_str())
        {
            continue;
        }
        if edge.source == edge.target {
            loops
                .entry(edge.source.clone())
                .or_default()
                .push(edge.id.clone());
            continue;
        }
        undirected
            .entry(edge.source.as_str())
            .or_default()
            .insert(edge.target.as_str());
        undirected
            .entry(edge.target.as_str())
            .or_default()
            .insert(edge.source.as_str());
        let key = if edge.source <= edge.target {
            (edge.source.clone(), edge.target.clone())
        } else {
            (edge.target.clone(), edge.source.clone())
        };
        pair_edges.entry(key).or_default().push(edge.id.clone());
    }

    // Edge roles: first edge of a pair is Plain, extras are Parallel; self
    // loops are Loop (first) / Parallel (rest).
    let mut edge_role: BTreeMap<String, EdgeRole> = BTreeMap::new();
    for eid in loops.values().flatten() {
        edge_role.insert(eid.clone(), EdgeRole::Loop);
    }
    for eids in pair_edges.values() {
        for (i, eid) in eids.iter().enumerate() {
            let role = if i == 0 {
                EdgeRole::Plain
            } else {
                EdgeRole::Parallel
            };
            edge_role.insert(eid.clone(), role);
        }
    }

    // Weak components, seeded in declaration order.
    let neighbors: BTreeMap<&str, Vec<&str>> = undirected
        .iter()
        .map(|(&u, vs)| {
            let mut list: Vec<&str> = vs.iter().copied().collect();
            list.sort_by_key(|v| decl.get(v).copied().unwrap_or(usize::MAX));
            (u, list)
        })
        .collect();

    let mut visited: BTreeSet<&str> = BTreeSet::new();
    let mut components: Vec<OrgComponent> = Vec::new();
    let mut node_of: BTreeMap<String, (u32, usize)> = BTreeMap::new();
    let mut warnings = Vec::new();

    for id in &nodes {
        if !visited.insert(id.as_str()) {
            continue;
        }
        let mut members: Vec<&str> = Vec::new();
        let mut q = VecDeque::new();
        q.push_back(id.as_str());
        while let Some(u) = q.pop_front() {
            members.push(u);
            for &v in neighbors.get(u).map(|v| v.as_slice()).unwrap_or(&[]) {
                if visited.insert(v) {
                    q.push_back(v);
                }
            }
        }
        members.sort_by_key(|m| decl.get(m).copied().unwrap_or(usize::MAX));

        let local: BTreeMap<&str, usize> = members
            .iter()
            .enumerate()
            .map(|(i, m)| (*m, i))
            .collect();
        let adjacency: Vec<Vec<usize>> = members
            .iter()
            .map(|m| {
                let mut list: Vec<usize> = neighbors
                    .get(m)
                    .map(|v| v.as_slice())
                    .unwrap_or(&[])
                    .iter()
                    .filter_map(|v| local.get(v).copied())
                    .collect();
                list.sort_unstable();
                list
            })
            .collect();

        let cid = components.len() as u32;
        for (i, m) in members.iter().enumerate() {
            node_of.insert(m.to_string(), (cid, i));
        }
        components.push(OrgComponent {
            id: cid,
            nodes: members.iter().map(|m| m.to_string()).collect(),
            adjacency,
        });
    }

    if nodes.len() > 1 && components.len() > 1 {
        warnings.push(format!(
            "organic: graph has {} disconnected components; each is laid out \
             separately and shelf-packed",
            components.len()
        ));
    }

    Ok((
        OrganicPlan {
            nodes,
            components,
            node_of,
            edge_role,
        },
        warnings,
    ))
}
