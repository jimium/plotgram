//! Compose: spanning forest + placer assignment + connectors.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use plotgram_engine_api::LayoutError;
use plotgram_model::graph::{Graph, NodeRole};
use plotgram_model::port::Side;

use super::params::{
    BusSlot, PlacerAtom, PlacerId, SplitPolicy, SplitSide, SubtreeTransform, TreeParams,
};
use super::plan::TreePlan;
use super::tree_err;

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
            return Err(tree_err(format!(
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
        return Err(tree_err(
            "tree: invalid: graph has no root (every entity has an incoming tree edge)",
        ));
    }

    let mut children: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut parent: BTreeMap<String, String> = BTreeMap::new();
    let mut depth: BTreeMap<String, u32> = BTreeMap::new();
    let mut tree_edge_ids = Vec::new();
    let mut edge_of_child: BTreeMap<String, String> = BTreeMap::new();
    let mut visited: BTreeSet<String> = BTreeSet::new();
    let mut roots = roots;

    let walk_from = |root: &str,
                     visited: &mut BTreeSet<String>,
                     children: &mut BTreeMap<String, Vec<String>>,
                     parent: &mut BTreeMap<String, String>,
                     depth: &mut BTreeMap<String, u32>,
                     tree_edge_ids: &mut Vec<String>,
                     edge_of_child: &mut BTreeMap<String, String>,
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
                edge_of_child.insert(v.to_string(), eid.to_string());
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
            &mut edge_of_child,
            &mut extra_edge_ids,
        );
    }
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
            &mut edge_of_child,
            &mut extra_edge_ids,
        );
    }

    let mut explicit_placer = BTreeMap::new();
    let mut split_side = BTreeMap::new();
    let mut assistants = BTreeSet::new();
    for n in graph.all_nodes() {
        if n.role != NodeRole::Entity {
            continue;
        }
        if n.attrs.get("assistant").and_then(|v| v.as_bool()) == Some(true) {
            assistants.insert(n.id.clone());
        }
        if let Some(raw) = n.attrs.get("subtree_placer").and_then(|v| v.as_str()) {
            match PlacerId::from_atom(raw) {
                Ok(p) => {
                    explicit_placer.insert(n.id.clone(), p);
                }
                Err(PlacerAtom::Unsupported(name)) => {
                    return Err(LayoutError::unsupported(format!("tree: placer `{name}`")));
                }
                Err(PlacerAtom::Unknown(name)) => {
                    return Err(tree_err(format!(
                        "tree: invalid: subtree_placer `{name}` on `{id}`",
                        id = n.id
                    )));
                }
            }
        }
        if let Some(raw) = n.attrs.get("split_side").and_then(|v| v.as_str()) {
            let side = match raw {
                "primary" | "left" => SplitSide::Primary,
                "secondary" | "right" => SplitSide::Secondary,
                other => {
                    return Err(tree_err(format!(
                        "tree: invalid: split_side `{other}` on `{}`",
                        n.id
                    )));
                }
            };
            split_side.insert(n.id.clone(), side);
        }
    }

    let mut placer_of: BTreeMap<String, PlacerId> = BTreeMap::new();
    let mut transform_of: BTreeMap<String, SubtreeTransform> = BTreeMap::new();
    for id in &nodes {
        placer_of.insert(id.clone(), params.placer);
        transform_of.insert(id.clone(), SubtreeTransform::None);
    }
    for (id, p) in &explicit_placer {
        placer_of.insert(id.clone(), *p);
    }

    apply_split_processor(
        &nodes,
        &children,
        &explicit_placer,
        params.split_policy,
        &split_side,
        &mut placer_of,
        &mut transform_of,
    );
    apply_assistant_processor(&nodes, &children, &assistants, &mut placer_of);

    let mut bus_slot = BTreeMap::new();
    let mut child_connectors = BTreeMap::new();
    for id in &nodes {
        let kids = children.get(id).map(|v| v.as_slice()).unwrap_or(&[]);
        assign_bus_slots(
            placer_of.get(id).copied().unwrap_or_default(),
            kids,
            &assistants,
            &mut bus_slot,
        );
        for (i, child) in kids.iter().enumerate() {
            let side = connector_side(
                placer_of.get(id).copied().unwrap_or_default(),
                transform_of.get(child).copied().unwrap_or_default(),
                split_side.get(child).copied(),
                bus_slot.get(child).copied(),
                params.split_policy,
                i,
                kids.len(),
            );
            child_connectors.insert(child.clone(), side);
        }
    }

    Ok(TreePlan {
        roots,
        nodes,
        children,
        parent,
        depth,
        tree_edge_ids,
        edge_of_child,
        extra_edge_ids,
        placer_of,
        transform_of,
        child_connectors,
        split_side,
        bus_slot,
        explicit_placer,
        assistants,
    })
}

fn apply_split_processor(
    nodes: &[String],
    children: &BTreeMap<String, Vec<String>>,
    explicit: &BTreeMap<String, PlacerId>,
    policy: SplitPolicy,
    split_side: &BTreeMap<String, SplitSide>,
    placer_of: &mut BTreeMap<String, PlacerId>,
    transform_of: &mut BTreeMap<String, SubtreeTransform>,
) {
    for id in nodes {
        if placer_of.get(id).copied() != Some(PlacerId::SingleSplitLayered) {
            continue;
        }
        let kids = children.get(id).map(|v| v.as_slice()).unwrap_or(&[]);
        let (left, right) = partition_children(kids, policy, split_side);
        for k in left {
            stamp_side(
                k,
                children,
                explicit,
                PlacerId::LevelAligned,
                SubtreeTransform::RotateLeft,
                placer_of,
                transform_of,
            );
        }
        for k in right {
            stamp_side(
                k,
                children,
                explicit,
                PlacerId::LevelAligned,
                SubtreeTransform::RotateRight,
                placer_of,
                transform_of,
            );
        }
    }
}

fn stamp_side(
    id: &str,
    children: &BTreeMap<String, Vec<String>>,
    explicit: &BTreeMap<String, PlacerId>,
    placer: PlacerId,
    transform: SubtreeTransform,
    placer_of: &mut BTreeMap<String, PlacerId>,
    transform_of: &mut BTreeMap<String, SubtreeTransform>,
) {
    let mut stack = vec![id.to_string()];
    while let Some(cur) = stack.pop() {
        if !explicit.contains_key(&cur) {
            placer_of.insert(cur.clone(), placer);
        }
        transform_of.insert(cur.clone(), transform);
        if let Some(kids) = children.get(&cur) {
            for k in kids.iter().rev() {
                stack.push(k.clone());
            }
        }
    }
}

pub(crate) fn partition_children<'a>(
    kids: &'a [String],
    policy: SplitPolicy,
    split_side: &BTreeMap<String, SplitSide>,
) -> (Vec<&'a str>, Vec<&'a str>) {
    let mut left = Vec::new();
    let mut right = Vec::new();
    let n = kids.len();
    let half = n.div_ceil(2);
    for (i, k) in kids.iter().enumerate() {
        let side = split_side.get(k).copied().unwrap_or(match policy {
            SplitPolicy::Half => {
                if i < half {
                    SplitSide::Primary
                } else {
                    SplitSide::Secondary
                }
            }
            SplitPolicy::Alternate => {
                if i % 2 == 0 {
                    SplitSide::Primary
                } else {
                    SplitSide::Secondary
                }
            }
        });
        match side {
            SplitSide::Primary => left.push(k.as_str()),
            SplitSide::Secondary => right.push(k.as_str()),
        }
    }
    (left, right)
}

fn apply_assistant_processor(
    nodes: &[String],
    children: &BTreeMap<String, Vec<String>>,
    assistants: &BTreeSet<String>,
    placer_of: &mut BTreeMap<String, PlacerId>,
) {
    for id in nodes {
        stamp_barren_assistant(id, children, assistants, placer_of);
    }
}

fn stamp_barren_assistant(
    id: &str,
    children: &BTreeMap<String, Vec<String>>,
    assistants: &BTreeSet<String>,
    placer_of: &mut BTreeMap<String, PlacerId>,
) {
    if placer_of.get(id).copied() != Some(PlacerId::Assistant) {
        return;
    }
    let kids = children.get(id).map(|v| v.as_slice()).unwrap_or(&[]);
    let has_asst = kids.iter().any(|k| assistants.contains(k));
    if !has_asst {
        placer_of.insert(id.to_string(), PlacerId::SingleLayer);
    }
    for k in kids {
        stamp_barren_assistant(k, children, assistants, placer_of);
    }
}

fn assign_bus_slots(
    placer: PlacerId,
    kids: &[String],
    assistants: &BTreeSet<String>,
    bus_slot: &mut BTreeMap<String, BusSlot>,
) {
    match placer {
        PlacerId::LeftRight | PlacerId::Bus => {
            let n = kids.len();
            for (i, k) in kids.iter().enumerate() {
                let slot = if placer == PlacerId::Bus && n > 2 && i == n - 1 {
                    BusSlot::Bottom
                } else if i % 2 == 0 {
                    BusSlot::Left
                } else {
                    BusSlot::Right
                };
                bus_slot.insert(k.clone(), slot);
            }
        }
        PlacerId::Assistant | PlacerId::Compact => {
            let asst: Vec<&String> = kids.iter().filter(|k| assistants.contains(*k)).collect();
            for (i, k) in asst.iter().enumerate() {
                let slot = if i % 2 == 0 {
                    BusSlot::Left
                } else {
                    BusSlot::Right
                };
                bus_slot.insert((*k).clone(), slot);
            }
        }
        _ => {}
    }
}

fn connector_side(
    parent_placer: PlacerId,
    child_transform: SubtreeTransform,
    child_split: Option<SplitSide>,
    bus_slot: Option<BusSlot>,
    policy: SplitPolicy,
    index: usize,
    n_kids: usize,
) -> Side {
    if let Some(slot) = bus_slot {
        return match slot {
            BusSlot::Left => Side::East,
            BusSlot::Right => Side::West,
            BusSlot::Bottom => Side::North,
        };
    }
    match parent_placer {
        PlacerId::SingleSplitLayered => {
            let side = child_split.unwrap_or(match policy {
                SplitPolicy::Half => {
                    if index < n_kids.div_ceil(2) {
                        SplitSide::Primary
                    } else {
                        SplitSide::Secondary
                    }
                }
                SplitPolicy::Alternate => {
                    if index % 2 == 0 {
                        SplitSide::Primary
                    } else {
                        SplitSide::Secondary
                    }
                }
            });
            match side {
                SplitSide::Primary => Side::East,
                SplitSide::Secondary => Side::West,
            }
        }
        _ => match child_transform {
            SubtreeTransform::RotateLeft => Side::East,
            SubtreeTransform::RotateRight => Side::West,
            SubtreeTransform::None => Side::North,
        },
    }
}
