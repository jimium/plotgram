//! Compose: weak components + partitions (BCC / single-cycle / custom) + circle order.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use tautcore_algo::bcc::biconnected_components;
use tautcore_algo::fiedler::fiedler_vector;
use tautcore_engine_api::LayoutError;
use tautcore_model::graph::{Graph, Node, NodeRole};

use super::circ_err;
use super::params::{CircleOrder, CircularParams, Partitioning};
use super::plan::{BackboneNode, CircComponent, CircPlan, EdgeRole, PartitionId};

pub fn compose(graph: &Graph, params: &CircularParams) -> Result<(CircPlan, Vec<String>), LayoutError> {
    match params.partitioning {
        Partitioning::SingleCycle
        | Partitioning::BccCompact
        | Partitioning::BccIsolated
        | Partitioning::Custom => {}
    }

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

    let mut undirected: BTreeMap<&str, BTreeSet<&str>> = nodes
        .iter()
        .map(|id| (id.as_str(), BTreeSet::new()))
        .collect();
    let mut pair_edges: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    let mut loops: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut all_edge_ids = Vec::new();
    let mut simple_decl: Vec<(String, String)> = Vec::new();

    for edge in graph.edges_in_declaration_order() {
        all_edge_ids.push(edge.id.clone());
        let s_ok = node_set.contains(edge.source.as_str());
        let t_ok = node_set.contains(edge.target.as_str());
        if !s_ok || !t_ok {
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
        let key = undirected_key(&edge.source, &edge.target);
        pair_edges.entry(key).or_default().push(edge.id.clone());
        simple_decl.push((edge.source.clone(), edge.target.clone()));
    }

    let neighbors: BTreeMap<&str, Vec<&str>> = undirected
        .iter()
        .map(|(&u, vs)| {
            let mut list: Vec<&str> = vs.iter().copied().collect();
            list.sort_by_key(|v| decl.get(v).copied().unwrap_or(usize::MAX));
            (u, list)
        })
        .collect();

    let mut visited: BTreeSet<&str> = BTreeSet::new();
    let mut components = Vec::new();
    let mut partitions: BTreeMap<PartitionId, Vec<String>> = BTreeMap::new();
    let mut partition_of: BTreeMap<String, PartitionId> = BTreeMap::new();
    let mut backbone: BTreeMap<PartitionId, BackboneNode> = BTreeMap::new();
    let mut cut_of: BTreeMap<(PartitionId, PartitionId), String> = BTreeMap::new();
    let mut next_pid: PartitionId = 0;
    let mut used_spectral = false;
    let mut warnings = Vec::new();

    for id in &nodes {
        if !visited.insert(id.as_str()) {
            continue;
        }
        let mut members = Vec::new();
        let mut q = VecDeque::new();
        q.push_back(id.as_str());
        while let Some(u) = q.pop_front() {
            members.push(u.to_string());
            for &v in neighbors.get(u).map(|v| v.as_slice()).unwrap_or(&[]) {
                if visited.insert(v) {
                    q.push_back(v);
                }
            }
        }
        members.sort_by_key(|m| decl.get(m.as_str()).copied().unwrap_or(usize::MAX));

        let labels = read_custom_labels(graph, &members)?;
        let apply_custom = matches!(
            params.partitioning,
            Partitioning::BccCompact | Partitioning::Custom
        ) && !labels.is_empty();
        if !labels.is_empty()
            && matches!(
                params.partitioning,
                Partitioning::SingleCycle | Partitioning::BccIsolated
            )
        {
            warnings.push(
                "circular: node `circle`/`partition` is ignored unless partitioning is `bcc-compact` or `custom`".to_string(),
            );
        }

        let built = if apply_custom {
            partition_custom(
                &members,
                &labels,
                &simple_decl,
                &neighbors,
                &decl,
                params.order,
                &mut next_pid,
            )?
        } else {
            match params.partitioning {
                Partitioning::SingleCycle => partition_single_cycle(
                    &members,
                    &neighbors,
                    &decl,
                    params.order,
                    &mut next_pid,
                )?,
                Partitioning::BccCompact | Partitioning::Custom => partition_bcc_compact(
                    &members,
                    &simple_decl,
                    &neighbors,
                    &decl,
                    params.order,
                    &mut next_pid,
                )?,
                Partitioning::BccIsolated => partition_bcc_isolated(
                    &members,
                    &simple_decl,
                    &neighbors,
                    &decl,
                    params.order,
                    &mut next_pid,
                )?,
            }
        };
        warnings.extend(built.warnings);
        if built.used_spectral {
            used_spectral = true;
        }
        for (pid, ordered) in built.partitions {
            for m in &ordered {
                partition_of.insert(m.clone(), pid);
            }
            partitions.insert(pid, ordered);
        }
        for (pid, node) in built.backbone {
            backbone.insert(pid, node);
        }
        for (k, v) in built.cut_of {
            cut_of.insert(k, v);
        }
        let cid = components.len() as u32;
        components.push(CircComponent {
            id: cid,
            nodes: members,
            partitions: built.component_partitions,
            root_partition: built.root,
        });
    }

    let mut edge_role = BTreeMap::new();
    for eids in loops.values() {
        for (i, eid) in eids.iter().enumerate() {
            edge_role.insert(
                eid.clone(),
                if i == 0 {
                    EdgeRole::Loop
                } else {
                    EdgeRole::Parallel
                },
            );
        }
    }
    for eids in pair_edges.values() {
        for (i, eid) in eids.iter().enumerate() {
            let role = if i == 0 {
                let edge = graph.find_edge(eid);
                match edge {
                    Some(e) => {
                        let a = partition_of.get(&e.source);
                        let b = partition_of.get(&e.target);
                        if a.is_some() && a == b {
                            EdgeRole::Intra
                        } else {
                            EdgeRole::Inter
                        }
                    }
                    None => EdgeRole::Intra,
                }
            } else {
                EdgeRole::Parallel
            };
            edge_role.insert(eid.clone(), role);
        }
    }
    for eid in &all_edge_ids {
        edge_role.entry(eid.clone()).or_insert(EdgeRole::Intra);
    }

    let order_method = match params.order {
        CircleOrder::Spectral if used_spectral || partitions.values().all(|p| p.len() < 3) => {
            CircleOrder::Spectral
        }
        CircleOrder::Spectral => CircleOrder::Bfs,
        other => other,
    };

    Ok((
        CircPlan {
            nodes,
            components,
            partitions,
            partition_of,
            backbone,
            cut_of,
            edge_role,
            order_method,
        },
        warnings,
    ))
}

struct BuiltPartitions {
    partitions: Vec<(PartitionId, Vec<String>)>,
    backbone: BTreeMap<PartitionId, BackboneNode>,
    cut_of: BTreeMap<(PartitionId, PartitionId), String>,
    component_partitions: Vec<PartitionId>,
    root: PartitionId,
    used_spectral: bool,
    warnings: Vec<String>,
}

fn partition_single_cycle(
    members: &[String],
    neighbors: &BTreeMap<&str, Vec<&str>>,
    decl: &BTreeMap<&str, usize>,
    method: CircleOrder,
    next_pid: &mut PartitionId,
) -> Result<BuiltPartitions, LayoutError> {
    let (ordered, used) = order_circle(members, neighbors, decl, method);
    let pid = *next_pid;
    *next_pid += 1;
    let mut backbone = BTreeMap::new();
    backbone.insert(
        pid,
        BackboneNode {
            parent: None,
            children: Vec::new(),
        },
    );
    Ok(BuiltPartitions {
        partitions: vec![(pid, ordered)],
        backbone,
        cut_of: BTreeMap::new(),
        component_partitions: vec![pid],
        root: pid,
        used_spectral: used == CircleOrder::Spectral,
        warnings: Vec::new(),
    })
}

fn partition_bcc_compact(
    members: &[String],
    simple_decl: &[(String, String)],
    neighbors: &BTreeMap<&str, Vec<&str>>,
    decl: &BTreeMap<&str, usize>,
    method: CircleOrder,
    next_pid: &mut PartitionId,
) -> Result<BuiltPartitions, LayoutError> {
    let local: BTreeMap<&str, usize> = members
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    let n = members.len();
    let mut local_edges: Vec<(usize, usize)> = Vec::new();
    for (a, b) in simple_decl {
        let Some(&i) = local.get(a.as_str()) else {
            continue;
        };
        let Some(&j) = local.get(b.as_str()) else {
            continue;
        };
        if i != j {
            local_edges.push((i, j));
        }
    }

    let forest = biconnected_components(n, &local_edges).map_err(|e| {
        circ_err(format!("circular: invariant: bcc failed: {e}"))
    })?;

    let mut blocks_of: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (bi, block) in forest.blocks.iter().enumerate() {
        for &v in &block.vertices {
            blocks_of[v].push(bi);
        }
    }
    let mut assign = vec![0usize; n];
    for v in 0..n {
        let Some(&bi) = blocks_of[v].iter().min() else {
            return Err(circ_err(format!(
                "circular: invariant: vertex {v} in no BCC block"
            )));
        };
        assign[v] = bi;
    }

    let mut verts_of_block: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for v in 0..n {
        verts_of_block.entry(assign[v]).or_default().push(v);
    }

    let mut pid_of_block: BTreeMap<usize, PartitionId> = BTreeMap::new();
    let mut partitions = Vec::new();
    let mut vertex_pid = vec![0 as PartitionId; n];
    for (bi, verts) in &verts_of_block {
        let pid = *next_pid;
        *next_pid += 1;
        pid_of_block.insert(*bi, pid);
        for &v in verts {
            vertex_pid[v] = pid;
        }
        let part_members: Vec<String> = verts.iter().map(|&v| members[v].clone()).collect();
        partitions.push((pid, part_members));
    }

    let mut undirected_adj: BTreeMap<PartitionId, BTreeSet<PartitionId>> = BTreeMap::new();
    let mut undirected_cut: BTreeMap<(PartitionId, PartitionId), String> = BTreeMap::new();
    for pid in pid_of_block.values() {
        undirected_adj.entry(*pid).or_default();
    }

    for (bi, block) in forest.blocks.iter().enumerate() {
        let mut parts: BTreeSet<PartitionId> = BTreeSet::new();
        for &v in &block.vertices {
            parts.insert(vertex_pid[v]);
        }
        if parts.len() <= 1 {
            continue;
        }
        let hub = pid_of_block
            .get(&bi)
            .copied()
            .or_else(|| parts.iter().copied().next())
            .unwrap();
        for &p in &parts {
            if p == hub {
                continue;
            }
            undirected_adj.entry(hub).or_default().insert(p);
            undirected_adj.entry(p).or_default().insert(hub);
            let cut = block
                .vertices
                .iter()
                .copied()
                .filter(|&v| vertex_pid[v] == p || vertex_pid[v] == hub)
                .filter(|&v| blocks_of[v].len() >= 2)
                .min_by_key(|&v| v)
                .map(|v| members[v].clone())
                .unwrap_or_else(|| members[block.vertices[0]].clone());
            let key = if hub < p { (hub, p) } else { (p, hub) };
            undirected_cut.entry(key).or_insert(cut);
        }
    }

    let n_part = pid_of_block.len();
    let n_edge: usize = undirected_adj.values().map(|s| s.len()).sum::<usize>() / 2;
    if n_part > 0 && n_edge != n_part - 1 {
        return Err(circ_err(format!(
            "circular: invariant: bcc-compact supergraph is not a tree ({n_part} partitions, {n_edge} edges)"
        )));
    }

    let root = vertex_pid[0];
    let (backbone, cut_of) = orient_backbone(root, n_part, &undirected_adj, &undirected_cut)?;
    finish_ordered(partitions, backbone, cut_of, root, neighbors, decl, method)
}

fn partition_bcc_isolated(
    members: &[String],
    simple_decl: &[(String, String)],
    neighbors: &BTreeMap<&str, Vec<&str>>,
    decl: &BTreeMap<&str, usize>,
    method: CircleOrder,
    next_pid: &mut PartitionId,
) -> Result<BuiltPartitions, LayoutError> {
    let local: BTreeMap<&str, usize> = members
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    let n = members.len();
    let mut local_edges: Vec<(usize, usize)> = Vec::new();
    for (a, b) in simple_decl {
        let Some(&i) = local.get(a.as_str()) else {
            continue;
        };
        let Some(&j) = local.get(b.as_str()) else {
            continue;
        };
        if i != j {
            local_edges.push((i, j));
        }
    }

    let forest = biconnected_components(n, &local_edges).map_err(|e| {
        circ_err(format!("circular: invariant: bcc failed: {e}"))
    })?;
    let cuts: BTreeSet<usize> = forest.cut_vertices.iter().copied().collect();

    let mut vertex_pid = vec![None; n];
    let mut partitions: Vec<(PartitionId, Vec<String>)> = Vec::new();

    for &v in &forest.cut_vertices {
        let pid = *next_pid;
        *next_pid += 1;
        vertex_pid[v] = Some(pid);
        partitions.push((pid, vec![members[v].clone()]));
    }

    let mut pid_of_block: Vec<Option<PartitionId>> = vec![None; forest.blocks.len()];
    for (bi, block) in forest.blocks.iter().enumerate() {
        let internals: Vec<usize> = block
            .vertices
            .iter()
            .copied()
            .filter(|v| !cuts.contains(v))
            .collect();
        if internals.is_empty() {
            continue;
        }
        let pid = *next_pid;
        *next_pid += 1;
        pid_of_block[bi] = Some(pid);
        for &v in &internals {
            vertex_pid[v] = Some(pid);
        }
        let part_members: Vec<String> = internals.iter().map(|&v| members[v].clone()).collect();
        partitions.push((pid, part_members));
    }

    for v in 0..n {
        if vertex_pid[v].is_none() {
            return Err(circ_err(format!(
                "circular: invariant: isolated vertex {v} has no partition"
            )));
        }
    }
    let vertex_pid: Vec<PartitionId> = vertex_pid.into_iter().map(|p| p.unwrap()).collect();

    let mut undirected_adj: BTreeMap<PartitionId, BTreeSet<PartitionId>> = BTreeMap::new();
    let mut undirected_cut: BTreeMap<(PartitionId, PartitionId), String> = BTreeMap::new();
    for (pid, _) in &partitions {
        undirected_adj.entry(*pid).or_default();
    }

    for (bi, block) in forest.blocks.iter().enumerate() {
        let block_cuts: Vec<usize> = block
            .vertices
            .iter()
            .copied()
            .filter(|v| cuts.contains(v))
            .collect();
        if let Some(ip) = pid_of_block[bi] {
            for &c in &block_cuts {
                link_parts(
                    &mut undirected_adj,
                    &mut undirected_cut,
                    ip,
                    vertex_pid[c],
                    members[c].clone(),
                );
            }
        } else if block_cuts.len() >= 2 {
            let hub = *block_cuts.iter().min().unwrap();
            for &c in &block_cuts {
                if c == hub {
                    continue;
                }
                link_parts(
                    &mut undirected_adj,
                    &mut undirected_cut,
                    vertex_pid[hub],
                    vertex_pid[c],
                    members[c].clone(),
                );
            }
        }
    }

    let n_part = partitions.len();
    let n_edge: usize = undirected_adj.values().map(|s| s.len()).sum::<usize>() / 2;
    if n_part > 0 && n_edge != n_part - 1 {
        return Err(circ_err(format!(
            "circular: invariant: bcc-isolated supergraph is not a tree ({n_part} partitions, {n_edge} edges)"
        )));
    }

    // Prefer a block partition as root so degree-2 cuts sit *between*
    // adjacent block disks (path geometry), not as a balloon hub.
    let root = (0..n)
        .filter(|&v| !cuts.contains(&v))
        .map(|v| vertex_pid[v])
        .next()
        .unwrap_or(vertex_pid[0]);
    let (backbone, cut_of) = orient_backbone(root, n_part, &undirected_adj, &undirected_cut)?;
    finish_ordered(partitions, backbone, cut_of, root, neighbors, decl, method)
}

fn link_parts(
    adj: &mut BTreeMap<PartitionId, BTreeSet<PartitionId>>,
    cut: &mut BTreeMap<(PartitionId, PartitionId), String>,
    a: PartitionId,
    b: PartitionId,
    cut_name: String,
) {
    if a == b {
        return;
    }
    adj.entry(a).or_default().insert(b);
    adj.entry(b).or_default().insert(a);
    let key = if a < b { (a, b) } else { (b, a) };
    cut.entry(key).or_insert(cut_name);
}

fn orient_backbone(
    root: PartitionId,
    n_part: usize,
    undirected_adj: &BTreeMap<PartitionId, BTreeSet<PartitionId>>,
    undirected_cut: &BTreeMap<(PartitionId, PartitionId), String>,
) -> Result<
    (
        BTreeMap<PartitionId, BackboneNode>,
        BTreeMap<(PartitionId, PartitionId), String>,
    ),
    LayoutError,
> {
    let mut backbone: BTreeMap<PartitionId, BackboneNode> = BTreeMap::new();
    let mut cut_of: BTreeMap<(PartitionId, PartitionId), String> = BTreeMap::new();
    let mut seen: BTreeSet<PartitionId> = BTreeSet::new();
    let mut q = VecDeque::new();
    q.push_back(root);
    seen.insert(root);
    backbone.insert(
        root,
        BackboneNode {
            parent: None,
            children: Vec::new(),
        },
    );
    while let Some(u) = q.pop_front() {
        let mut kids: Vec<PartitionId> = undirected_adj
            .get(&u)
            .into_iter()
            .flatten()
            .copied()
            .filter(|v| seen.insert(*v))
            .collect();
        kids.sort_unstable();
        for v in kids {
            backbone.entry(u).or_insert(BackboneNode {
                parent: None,
                children: Vec::new(),
            });
            backbone.get_mut(&u).unwrap().children.push(v);
            backbone.insert(
                v,
                BackboneNode {
                    parent: Some(u),
                    children: Vec::new(),
                },
            );
            let key = if u < v { (u, v) } else { (v, u) };
            if let Some(cut) = undirected_cut.get(&key) {
                cut_of.insert((u, v), cut.clone());
            }
            q.push_back(v);
        }
    }
    if seen.len() != n_part {
        return Err(circ_err(
            "circular: invariant: bcc backbone does not cover all partitions",
        ));
    }
    Ok((backbone, cut_of))
}

fn finish_ordered(
    partitions: Vec<(PartitionId, Vec<String>)>,
    backbone: BTreeMap<PartitionId, BackboneNode>,
    cut_of: BTreeMap<(PartitionId, PartitionId), String>,
    root: PartitionId,
    neighbors: &BTreeMap<&str, Vec<&str>>,
    decl: &BTreeMap<&str, usize>,
    method: CircleOrder,
) -> Result<BuiltPartitions, LayoutError> {
    let mut used_spectral = false;
    let mut ordered_partitions = Vec::new();
    let mut component_partitions = Vec::new();
    for (pid, raw) in partitions {
        let (ordered, used) = order_circle(&raw, neighbors, decl, method);
        if used == CircleOrder::Spectral {
            used_spectral = true;
        }
        component_partitions.push(pid);
        ordered_partitions.push((pid, ordered));
    }
    component_partitions.sort_unstable();
    Ok(BuiltPartitions {
        partitions: ordered_partitions,
        backbone,
        cut_of,
        component_partitions,
        root,
        used_spectral,
        warnings: Vec::new(),
    })
}

fn read_custom_labels(
    graph: &Graph,
    members: &[String],
) -> Result<BTreeMap<String, String>, LayoutError> {
    let mut labels = BTreeMap::new();
    for id in members {
        let Some(node) = graph.find_node(id) else {
            continue;
        };
        if let Some(atom) = circle_atom(node)? {
            labels.insert(id.clone(), atom);
        }
    }
    Ok(labels)
}

fn circle_atom(node: &Node) -> Result<Option<String>, LayoutError> {
    let circle = attr_atom(node, "circle")?;
    let part = attr_atom(node, "partition")?;
    match (circle, part) {
        (None, None) => Ok(None),
        (Some(a), Some(b)) if a != b => Err(circ_err(format!(
            "circular: node `{}`: `circle` (`{a}`) conflicts with `partition` (`{b}`)",
            node.id
        ))),
        (Some(id), _) | (_, Some(id)) => {
            if id.is_empty() {
                return Err(circ_err(format!(
                    "circular: node `{}`: empty `circle` / `partition` id",
                    node.id
                )));
            }
            if !is_node_id_atom(&id) {
                return Err(circ_err(format!(
                    "circular: node `{}`: `circle` / `partition` `{id}` is not a node-id atom",
                    node.id
                )));
            }
            Ok(Some(id))
        }
    }
}

fn attr_atom(node: &Node, key: &str) -> Result<Option<String>, LayoutError> {
    match node.attrs.get(key) {
        None => Ok(None),
        Some(v) => match v.as_str() {
            Some(s) => Ok(Some(s.to_string())),
            None => Err(circ_err(format!(
                "circular: node `{}`: `{key}` must be an atom",
                node.id
            ))),
        },
    }
}

fn is_node_id_atom(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {
            chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        }
        _ => false,
    }
}

fn partition_custom(
    members: &[String],
    labels: &BTreeMap<String, String>,
    simple_decl: &[(String, String)],
    neighbors: &BTreeMap<&str, Vec<&str>>,
    decl: &BTreeMap<&str, usize>,
    method: CircleOrder,
    next_pid: &mut PartitionId,
) -> Result<BuiltPartitions, LayoutError> {
    let unlabeled: Vec<String> = members
        .iter()
        .filter(|m| !labels.contains_key(m.as_str()))
        .cloned()
        .collect();
    let unlabeled_set: BTreeSet<&str> = unlabeled.iter().map(|s| s.as_str()).collect();

    let mut partitions: Vec<(PartitionId, Vec<String>)> = Vec::new();
    let mut vertex_pid: BTreeMap<String, PartitionId> = BTreeMap::new();

    let mut seen_unlabeled: BTreeSet<&str> = BTreeSet::new();
    for id in &unlabeled {
        if !seen_unlabeled.insert(id.as_str()) {
            continue;
        }
        let mut chunk = Vec::new();
        let mut q = VecDeque::new();
        q.push_back(id.as_str());
        while let Some(u) = q.pop_front() {
            chunk.push(u.to_string());
            for &v in neighbors.get(u).map(|v| v.as_slice()).unwrap_or(&[]) {
                if unlabeled_set.contains(v) && seen_unlabeled.insert(v) {
                    q.push_back(v);
                }
            }
        }
        chunk.sort_by_key(|m| decl.get(m.as_str()).copied().unwrap_or(usize::MAX));
        let built = partition_bcc_compact(
            &chunk,
            simple_decl,
            neighbors,
            decl,
            method,
            next_pid,
        )?;
        for (pid, ordered) in built.partitions {
            for m in &ordered {
                vertex_pid.insert(m.clone(), pid);
            }
            partitions.push((pid, ordered));
        }
    }

    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for m in members {
        if let Some(atom) = labels.get(m) {
            groups.entry(atom.clone()).or_default().push(m.clone());
        }
    }
    let mut atoms: Vec<String> = groups.keys().cloned().collect();
    atoms.sort_by_key(|a| {
        groups[a]
            .iter()
            .map(|m| decl.get(m.as_str()).copied().unwrap_or(usize::MAX))
            .min()
            .unwrap_or(usize::MAX)
    });
    for atom in atoms {
        let raw = groups.remove(&atom).unwrap();
        let pid = *next_pid;
        *next_pid += 1;
        for m in &raw {
            vertex_pid.insert(m.clone(), pid);
        }
        partitions.push((pid, raw));
    }

    let mut undirected_adj: BTreeMap<PartitionId, BTreeSet<PartitionId>> = BTreeMap::new();
    let mut undirected_cut: BTreeMap<(PartitionId, PartitionId), String> = BTreeMap::new();
    for pid in vertex_pid.values() {
        undirected_adj.entry(*pid).or_default();
    }
    let member_set: BTreeSet<&str> = members.iter().map(|s| s.as_str()).collect();
    for (a, b) in simple_decl {
        if !member_set.contains(a.as_str()) || !member_set.contains(b.as_str()) {
            continue;
        }
        let pa = vertex_pid[a];
        let pb = vertex_pid[b];
        if pa == pb {
            continue;
        }
        let cut_name = if decl.get(a.as_str()) <= decl.get(b.as_str()) {
            a.clone()
        } else {
            b.clone()
        };
        link_parts(
            &mut undirected_adj,
            &mut undirected_cut,
            pa,
            pb,
            cut_name,
        );
    }

    let n_part = partitions.len();
    let n_edge: usize = undirected_adj.values().map(|s| s.len()).sum::<usize>() / 2;
    let mut warnings = Vec::new();
    if n_part > 1 && n_edge > n_part - 1 {
        warnings.push(
            "circular: custom partitions form a cycle; extra inter-edges kept as spokes"
                .to_string(),
        );
    }
    if n_part > 0 && n_edge < n_part - 1 {
        return Err(circ_err(format!(
            "circular: invariant: custom partition graph is disconnected ({n_part} partitions, {n_edge} edges)"
        )));
    }

    let root = vertex_pid[&members[0]];
    let (backbone, cut_of) = orient_backbone(root, n_part, &undirected_adj, &undirected_cut)?;
    let mut built = finish_ordered(partitions, backbone, cut_of, root, neighbors, decl, method)?;
    built.warnings = warnings;
    Ok(built)
}

fn undirected_key(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

fn order_circle(
    members: &[String],
    neighbors: &BTreeMap<&str, Vec<&str>>,
    decl: &BTreeMap<&str, usize>,
    method: CircleOrder,
) -> (Vec<String>, CircleOrder) {
    if members.len() <= 1 {
        return (members.to_vec(), method);
    }
    match method {
        CircleOrder::Declaration => (members.to_vec(), CircleOrder::Declaration),
        CircleOrder::Bfs => (bfs_order(members, neighbors, decl), CircleOrder::Bfs),
        CircleOrder::Spectral => spectral_order(members, neighbors, decl),
    }
}

fn spectral_order(
    members: &[String],
    neighbors: &BTreeMap<&str, Vec<&str>>,
    decl: &BTreeMap<&str, usize>,
) -> (Vec<String>, CircleOrder) {
    if members.len() < 3 {
        return (bfs_order(members, neighbors, decl), CircleOrder::Bfs);
    }
    let local: BTreeMap<&str, usize> = members
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    let mut edges: Vec<(usize, usize)> = Vec::new();
    let mut edge_set: BTreeSet<(usize, usize)> = BTreeSet::new();
    for (i, a) in members.iter().enumerate() {
        for &b in neighbors.get(a.as_str()).map(|v| v.as_slice()).unwrap_or(&[]) {
            let Some(&j) = local.get(b) else {
                continue;
            };
            if i < j {
                edges.push((i, j));
                edge_set.insert((i, j));
            }
        }
    }
    let Ok(v) = fiedler_vector(members.len(), &edges) else {
        return (bfs_order(members, neighbors, decl), CircleOrder::Bfs);
    };
    if v.iter().any(|x| !x.is_finite()) {
        return (bfs_order(members, neighbors, decl), CircleOrder::Bfs);
    }
    let mut idx: Vec<usize> = (0..members.len()).collect();
    idx.sort_by(|a, b| {
        v[*a]
            .partial_cmp(&v[*b])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                decl.get(members[*a].as_str())
                    .cmp(&decl.get(members[*b].as_str()))
            })
    });
    let w = circular_adj_weight(&idx, &edge_set);
    let mut rev = idx.clone();
    rev.reverse();
    if circular_adj_weight(&rev, &edge_set) > w {
        idx = rev;
    }
    if let Some(pos) = idx.iter().position(|&i| i == 0) {
        idx.rotate_left(pos);
    }
    (
        idx.into_iter().map(|i| members[i].clone()).collect(),
        CircleOrder::Spectral,
    )
}

fn circular_adj_weight(order: &[usize], edges: &BTreeSet<(usize, usize)>) -> usize {
    if order.len() < 2 {
        return 0;
    }
    let mut w = 0;
    for i in 0..order.len() {
        let a = order[i];
        let b = order[(i + 1) % order.len()];
        let key = if a < b { (a, b) } else { (b, a) };
        if edges.contains(&key) {
            w += 1;
        }
    }
    w
}

fn bfs_order(
    members: &[String],
    neighbors: &BTreeMap<&str, Vec<&str>>,
    decl: &BTreeMap<&str, usize>,
) -> Vec<String> {
    let member_set: BTreeSet<&str> = members.iter().map(|s| s.as_str()).collect();
    let start = members[0].as_str();
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut q = VecDeque::new();
    let mut out = Vec::new();
    q.push_back(start);
    seen.insert(start);
    while let Some(u) = q.pop_front() {
        out.push(u.to_string());
        for &v in neighbors.get(u).map(|v| v.as_slice()).unwrap_or(&[]) {
            if member_set.contains(v) && seen.insert(v) {
                q.push_back(v);
            }
        }
    }
    let mut rest: Vec<String> = members
        .iter()
        .filter(|m| !seen.contains(m.as_str()))
        .cloned()
        .collect();
    rest.sort_by_key(|m| decl.get(m.as_str()).copied().unwrap_or(usize::MAX));
    out.extend(rest);
    out
}
