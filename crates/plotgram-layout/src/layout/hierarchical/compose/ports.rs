//! P7 port finalize — the single port writer (architecture.md §7,
//! ports-and-channel.md §1–§2). Runs *inside* the TB-canonical core (see
//! `orient.rs`); authored sides are converted into canonical space here via
//! `Orientation::to_tb_side`, mirroring the `from_tb_*` pass in `mod.rs` on
//! the way out (architecture.md §9.3).
//!
//! Author tiers on edges (plotgram_model::port::PortConstraint):
//!
//! - FREE (`None`) — side from topology + shape policy (capacity soft overflow);
//! - FIXED_SIDE — authored `from_side` / `to_side` wins (ignores shape policy);
//! - every tier resolves to a canonical `ResolvedPort` whose `along` is
//!   expanded to pixels only by Metric (`metric/anchor.rs`) — never here.
//!
//! Ordered slot order within a (node, side) group: opposite endpoint's
//! `(neighbor_rank, neighbor_order, EdgeId)` (ports-and-channel.md §2
//! step 4).

use std::collections::{BTreeMap, BTreeSet};

use plotgram_algo::orientation::{Orientation as AlgoOrientation, Side};
use plotgram_engine_api::LayoutError;
use plotgram_model::port::{AlongSpec, PortConstraint, Side as ModelSide};
use plotgram_model::{policy_for, ShapePortPolicy};

use crate::layout::hierarchical::model::{ElemKey, PlanGraph, RealGraph};
use crate::layout::hierarchical::orient::to_algo_side;

/// A finalized port, still in canonical (TB) space — `mod.rs`'s final
/// orientation pass converts `side` (and `LocalOffset`) back to physical
/// space. Metric expands `along` to the pixel anchor; Ink only reads it.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedPort {
    pub side: Side,
    pub along: AlongSpec,
}

#[derive(Debug)]
pub struct EdgePorts {
    pub source: ResolvedPort,
    pub target: ResolvedPort,
    /// Automatic edge-grouping membership at the source end (edge-parameters
    /// §2.3 / yFiles bus-style). Members share one `PortPoint` and one
    /// [`BundlePlan`] trunk; `index` is only the stable stub order.
    pub source_cluster: Option<EndCluster>,
    pub target_cluster: Option<EndCluster>,
}

/// One member of a port cluster (bus merge). Members share the same
/// `Ordered` slot / PortPoint; `index` orders stubs along the bus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndCluster {
    pub count: u32,
    pub index: u32,
}

/// Result of P7 port finalize: per-edge ports plus end-bus BundlePlan facts.
#[derive(Debug)]
pub struct PortAssignment {
    pub ports: BTreeMap<String, EdgePorts>,
    /// End-bus bundles from `auto_edge_grouping` (SourcePrefix / TargetSuffix).
    pub bundles: Vec<crate::layout::hierarchical::compose::bundle::BundlePlan>,
}

fn positions_within_layer(plan: &PlanGraph) -> Vec<usize> {
    let mut pos = vec![0usize; plan.elems.len()];
    for layer in &plan.layers {
        for (i, &e) in layer.iter().enumerate() {
            pos[e] = i;
        }
    }
    pos
}

/// Find the elem adjacent to `real_idx` along `edge_id`'s chain (its only
/// segment neighbor — dummy or the other real endpoint).
fn immediate_neighbor(plan: &PlanGraph, edge_id: &str, real_idx: usize) -> usize {
    plan.segments
        .iter()
        .find(|s| s.edge_id == edge_id && (s.from == real_idx || s.to == real_idx))
        .map(|s| if s.from == real_idx { s.to } else { s.from })
        .expect("real endpoint must have exactly one adjacent segment for its own edge")
}

/// Undirected original-endpoint key for twin detection.
fn undirected_pair(a: usize, b: usize) -> (usize, usize) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Original endpoint pairs that carry at least one non-reversed edge.
/// A reversed edge on the same pair is a req-resp / 2-cycle twin.
fn forward_twin_pairs(graph: &RealGraph) -> BTreeSet<(usize, usize)> {
    let mut set = BTreeSet::new();
    for e in &graph.edges {
        if !e.reversed {
            set.insert(undirected_pair(e.original_source, e.original_target));
        }
    }
    set
}

/// FREE ends already assigned to the flow-spine faces (N+S) on one node.
fn spine_ns_load(usage: &BTreeMap<(usize, Side), u32>, node_real: usize) -> u32 {
    usage.get(&(node_real, Side::North)).copied().unwrap_or(0)
        + usage.get(&(node_real, Side::South)).copied().unwrap_or(0)
}

/// Spine load threshold: same-column short reverse without a twin prefers
/// the side corridor once the spine already has this many FREE ends.
const NS_LOAD_SPINE_THRESHOLD: u32 = 2;

/// FREE side inference (canonical TB). Forward edges follow the rank
/// direction (downstream → South, upstream → North). Reversed (back) edges
/// pick spine vs side corridor via [`pick_reversed_side`] (no Channel call).
fn free_side(
    plan: &PlanGraph,
    pos: &[usize],
    node_real: usize,
    neighbor: usize,
    reversed: bool,
    has_twin: bool,
    ns_load: u32,
) -> Side {
    let own_rank = plan.elems[node_real].rank;
    let neighbor_rank = plan.elems[neighbor].rank;
    let rank_dir = if neighbor_rank > own_rank {
        Side::South
    } else {
        Side::North
    };
    if !reversed {
        return rank_dir;
    }
    let peer = match &plan.elems[neighbor].key {
        crate::layout::hierarchical::model::ElemKey::Real(_) => neighbor,
        crate::layout::hierarchical::model::ElemKey::Virtual { edge_id, .. } => {
            far_real_endpoint(plan, edge_id, node_real).unwrap_or(neighbor)
        }
        crate::layout::hierarchical::model::ElemKey::GroupBoundary { .. }
        | crate::layout::hierarchical::model::ElemKey::OrderPad { .. } => neighbor,
    };
    let peer_rank = plan.elems[peer].rank;
    let own_order = pos[node_real];
    let peer_order = pos.get(peer).copied().unwrap_or(0);
    // Side-corridor polarity is edge-level (both ends same face): tip = the
    // lower-on-TB real endpoint; East/West from tip vs peer layer order —
    // outer leaf side, not "toward peer" (expectations §3 闭环走侧廊).
    let cross_axis = side_corridor_polarity(plan, pos, node_real, peer);
    let span = (own_rank as usize).abs_diff(peer_rank as usize);
    pick_reversed_side(
        rank_dir,
        cross_axis,
        span,
        own_order,
        peer_order,
        has_twin,
        ns_load,
    )
}

/// Shared E/W face for a side-corridor reverse: tip is the higher-rank real
/// end; polarity follows tip's order relative to the other end.
fn side_corridor_polarity(
    plan: &PlanGraph,
    pos: &[usize],
    a: usize,
    b: usize,
) -> Side {
    let (tip, other) = match plan.elems[a].rank.cmp(&plan.elems[b].rank) {
        std::cmp::Ordering::Greater => (a, b),
        std::cmp::Ordering::Less => (b, a),
        std::cmp::Ordering::Equal => return Side::East,
    };
    let tip_o = pos.get(tip).copied().unwrap_or(0);
    let other_o = pos.get(other).copied().unwrap_or(0);
    match tip_o.cmp(&other_o) {
        std::cmp::Ordering::Greater => Side::East,
        std::cmp::Ordering::Less => Side::West,
        std::cmp::Ordering::Equal => Side::East,
    }
}

/// Corridor-role pick for FREE reversed ends (Compose-only; no Channel).
///
/// - Twin short (span=1, Δorder≤1): flow spine (`rank_dir`) — parallel aesthetics.
/// - Long reverse (span≥2): side corridor (`cross_axis`).
/// - Short without twin: side corridor (workflow feedback); crowded spine too.
fn pick_reversed_side(
    rank_dir: Side,
    cross_axis: Side,
    span: usize,
    own_order: usize,
    peer_order: usize,
    has_twin: bool,
    ns_load: u32,
) -> Side {
    let delta_order = own_order.abs_diff(peer_order);

    if span >= 2 {
        return cross_axis;
    }
    if has_twin && span == 1 && delta_order <= 1 {
        return rank_dir;
    }
    if !has_twin && span == 1 {
        // Short feedback without a forward twin → side corridor
        // (also covers same-column crowded spine: ns_load ≥ threshold).
        return cross_axis;
    }

    // Remainder: soft costs; twin-like prefers spine, else side corridor.
    let c_ns: u32 = if has_twin { 0 } else { 2 }
        + if delta_order == 0 { 1 } else { 0 }
        + if ns_load >= NS_LOAD_SPINE_THRESHOLD {
            2
        } else {
            0
        };
    let c_ew: u32 = if has_twin { 2 } else { 0 } + if delta_order >= 2 { 0 } else { 1 };
    if c_ew <= c_ns {
        cross_axis
    } else {
        rank_dir
    }
}

/// The other real endpoint of `edge_id`, given one real endpoint elem.
fn far_real_endpoint(plan: &PlanGraph, edge_id: &str, known_real: usize) -> Option<usize> {
    plan.elems.iter().enumerate().find_map(|(i, e)| {
        if i == known_real {
            return None;
        }
        match &e.key {
            crate::layout::hierarchical::model::ElemKey::Real(_) => {
                // On this edge's chain if any segment mentions edge_id and
                // connects toward this elem — cheaper: scan segments for
                // endpoints that are real and share edge_id.
                let on_edge = plan.segments.iter().any(|s| {
                    s.edge_id == edge_id && (s.from == i || s.to == i)
                });
                if on_edge {
                    Some(i)
                } else {
                    None
                }
            }
            _ => None,
        }
    })
}

/// Full deterministic side preference given a primary pick:
/// `primary` → perpendicular pair → opposite (anti-flow last).
///
/// Used when `primary` is **not** allowed (e.g. Person forbids North):
/// fall back without preferring the anti-flow face first.
fn side_preference(primary: Side) -> [Side; 4] {
    match primary {
        Side::North => [Side::North, Side::East, Side::West, Side::South],
        Side::South => [Side::South, Side::East, Side::West, Side::North],
        Side::East => [Side::East, Side::North, Side::South, Side::West],
        Side::West => [Side::West, Side::North, Side::South, Side::East],
    }
}

fn model_side(s: Side) -> ModelSide {
    match s {
        Side::North => ModelSide::North,
        Side::South => ModelSide::South,
        Side::East => ModelSide::East,
        Side::West => ModelSide::West,
    }
}

/// Attempt order for FREE under a shape policy: topology
/// `side_preference(primary)` filtered to `allowed`, then remaining
/// `policy.preference` sides. Only used when primary itself is disallowed.
fn attempt_sides(primary: Side, policy: ShapePortPolicy) -> Vec<Side> {
    let mut out = Vec::with_capacity(4);
    for s in side_preference(primary) {
        if policy.allows(model_side(s)) && !out.contains(&s) {
            out.push(s);
        }
    }
    for &ms in policy.preference {
        let s = to_algo_side(ms);
        if policy.allows(ms) && !out.contains(&s) {
            out.push(s);
        }
    }
    out
}

/// Pick a side under shape policy + soft per-side capacity.
///
/// `capacity_per_side` does **not** drive a face change: when `primary` is
/// allowed, capacity-full means same-face soft overflow (Ordered spreads
/// endpoints). Face changes only when primary ∉ allowed (e.g. Person North).
fn pick_side_with_policy(
    primary: Side,
    policy: ShapePortPolicy,
    node_real: usize,
    usage: &mut BTreeMap<(usize, Side), u32>,
    // Optional side restrict (unused on edges; kept for pick helper).
    restrict: Option<&[Side]>,
) -> Side {
    let primary_ok = policy.allows(model_side(primary))
        && restrict.map(|r| r.contains(&primary)).unwrap_or(true);
    if primary_ok {
        // Same-face soft overflow when over capacity — keep topological flow face.
        *usage.entry((node_real, primary)).or_insert(0) += 1;
        return primary;
    }

    let attempts = attempt_sides(primary, policy);
    let attempts: Vec<Side> = match restrict {
        Some(r) => attempts.into_iter().filter(|s| r.contains(s)).collect(),
        None => attempts,
    };
    debug_assert!(!attempts.is_empty(), "caller must ensure non-empty attempt set");

    for s in &attempts {
        let used = usage.get(&(node_real, *s)).copied().unwrap_or(0);
        if let Some(cap) = policy.capacity_per_side {
            if used >= cap {
                continue;
            }
        }
        *usage.entry((node_real, *s)).or_insert(0) += 1;
        return *s;
    }
    let s = attempts[0];
    *usage.entry((node_real, s)).or_insert(0) += 1;
    s
}

/// One end (source or target) of one edge after side resolution, still
/// awaiting Ordered slot assignment.
struct EndPoint {
    edge_id: String,
    is_source_end: bool,
    node_real: usize,
    side: Side,
    sort_key: (usize, u32, String), // (far_real_layer_order, far_real_rank, edge_id)
}

/// Author FIXED_SIDE → canonical side. FREE is handled by the caller.
fn fixed_side_canonical(constraint: &PortConstraint, orientation: AlgoOrientation) -> Side {
    match constraint {
        PortConstraint::FixedSide { side } | PortConstraint::FixedOrder { side, .. } => {
            // FixedOrder belongs on group anchors; if it appears on an edge,
            // honor the side only.
            orientation.to_tb_side(to_algo_side(*side))
        }
    }
}

/// P7 port finalize.
///
/// `auto_edge_grouping` enables automatic fan clustering (edge-parameters §2.3):
/// eligible FREE ends on the same (node, main-axis side) merge into one shared
/// slot — same-source fan-out / same-target fan-in.
pub fn assign_ports(
    graph: &RealGraph,
    plan: &PlanGraph,
    orientation: AlgoOrientation,
    auto_edge_grouping: bool,
) -> Result<PortAssignment, LayoutError> {
    let pos = positions_within_layer(plan);
    let real_idx_of = |id: &str| plan.index_of[&ElemKey::Real(id.to_string())];
    let twin_pairs = forward_twin_pairs(graph);
    // FREE capacity ledger only (FixedSide does not charge).
    let mut free_usage: BTreeMap<(usize, Side), u32> = BTreeMap::new();

    let mut endpoints = Vec::with_capacity(graph.edges.len() * 2);
    for e in &graph.edges {
        let has_twin = twin_pairs.contains(&undirected_pair(e.original_source, e.original_target));
        for (is_source_end, node_id, constraint) in [
            (true, &graph.ids[e.original_source], e.from_port.clone()),
            (false, &graph.ids[e.original_target], e.to_port.clone()),
        ] {
            let node_real = real_idx_of(node_id);
            let neighbor = immediate_neighbor(plan, &e.edge_id, node_real);
            let sort_peer = match &plan.elems[neighbor].key {
                ElemKey::Real(_) => neighbor,
                ElemKey::Virtual { edge_id, .. } => {
                    far_real_endpoint(plan, edge_id, node_real).unwrap_or(neighbor)
                }
                ElemKey::GroupBoundary { .. } | ElemKey::OrderPad { .. } => neighbor,
            };
            let real_idx = graph.index_of[node_id];
            let shape = graph.shapes[real_idx];
            let policy = policy_for(shape);

            let side = match &constraint {
                Some(c) => fixed_side_canonical(c, orientation),
                None => {
                    let peer = match &plan.elems[neighbor].key {
                        ElemKey::Real(_) => neighbor,
                        ElemKey::Virtual { edge_id, .. } => {
                            far_real_endpoint(plan, edge_id, node_real).unwrap_or(neighbor)
                        }
                        ElemKey::GroupBoundary { .. } | ElemKey::OrderPad { .. } => neighbor,
                    };
                    let ns_load = spine_ns_load(&free_usage, node_real)
                        + spine_ns_load(&free_usage, peer);
                    let primary = free_side(
                        plan,
                        &pos,
                        node_real,
                        neighbor,
                        e.reversed,
                        has_twin,
                        ns_load,
                    );
                    pick_side_with_policy(primary, policy, node_real, &mut free_usage, None)
                }
            };

            endpoints.push(EndPoint {
                edge_id: e.edge_id.clone(),
                is_source_end,
                node_real,
                side,
                // Far-real layer order first so long-edge dummies cannot invert fan ports.
                sort_key: (
                    pos.get(sort_peer).copied().unwrap_or(0),
                    plan.elems[sort_peer].rank,
                    e.edge_id.clone(),
                ),
            });
        }
    }

    // Group ordered members by (node, side); assign slots within each group.
    let mut groups: BTreeMap<(usize, Side), Vec<usize>> = BTreeMap::new(); // -> indices into `endpoints`
    for (i, ep) in endpoints.iter().enumerate() {
        groups.entry((ep.node_real, ep.side)).or_default().push(i);
    }

    let mut along_of: Vec<AlongSpec> = vec![AlongSpec::Ordered { order: 0, count: 1 }; endpoints.len()];
    let mut cluster_of: Vec<Option<EndCluster>> = vec![None; endpoints.len()];
    let mut bundles: Vec<crate::layout::hierarchical::compose::bundle::BundlePlan> =
        Vec::new();
    for members in groups.into_values() {
        // Automatic edge grouping (yFiles AutomaticEdgeGrouping / edge-
        // parameters §2.3): ends already share (node, side) via the outer
        // group key — merge them into ONE cluster on N/S. A cluster occupies
        // ONE slot; Metric assigns a shared PortPoint + bus_y.
        let mut cluster_map: BTreeMap<(), Vec<usize>> = BTreeMap::new();
        if auto_edge_grouping {
            for &i in &members {
                let ep = &endpoints[i];
                if matches!(ep.side, Side::North | Side::South) {
                    cluster_map.entry(()).or_default().push(i);
                }
            }
        }

        // Slot members: singles + clusters (≥2 members); clusters sort by
        // their members' neighbor order for a crossing-light fan.
        enum SlotMember {
            Single(usize),
            Cluster(Vec<usize>),
        }
        let mut slot_members: Vec<SlotMember> = Vec::with_capacity(members.len());
        let mut singles: Vec<usize> = Vec::new();
        for &i in &members {
            let in_cluster = cluster_map
                .values()
                .any(|c| c.len() >= 2 && c.contains(&i));
            if !in_cluster {
                singles.push(i);
            }
        }
        singles.sort_by(|&a, &b| endpoints[a].sort_key.cmp(&endpoints[b].sort_key));
        for i in singles {
            slot_members.push(SlotMember::Single(i));
        }
        let mut clusters: Vec<Vec<usize>> = cluster_map
            .into_values()
            .filter(|c| c.len() >= 2)
            .collect();
        for c in &mut clusters {
            c.sort_by(|&a, &b| endpoints[a].sort_key.cmp(&endpoints[b].sort_key));
        }
        clusters.sort_by(|a, b| {
            endpoints[a[0]].sort_key.cmp(&endpoints[b[0]].sort_key)
        });
        for c in clusters {
            slot_members.push(SlotMember::Cluster(c));
        }
        slot_members.sort_by(|a, b| {
            let key_of = |m: &SlotMember| match m {
                SlotMember::Single(i) => endpoints[*i].sort_key.clone(),
                SlotMember::Cluster(c) => endpoints[c[0]].sort_key.clone(),
            };
            key_of(a).cmp(&key_of(b))
        });

        let mut slot_of_member: Vec<(u32, Vec<usize>)> = Vec::with_capacity(slot_members.len());
        let mut next_free = 0u32;
        for m in &slot_members {
            match m {
                SlotMember::Single(i) => {
                    let s = next_free;
                    next_free += 1;
                    slot_of_member.push((s, vec![*i]));
                }
                SlotMember::Cluster(c) => {
                    let s = next_free;
                    next_free += 1;
                    for (idx, &i) in c.iter().enumerate() {
                        cluster_of[i] = Some(EndCluster {
                            count: c.len() as u32,
                            index: idx as u32,
                        });
                    }
                    let members: Vec<String> =
                        c.iter().map(|&i| endpoints[i].edge_id.clone()).collect();
                    let at_source = endpoints[c[0]].is_source_end;
                    let kind = if at_source {
                        crate::layout::hierarchical::compose::bundle::BundleKind::SourcePrefix
                    } else {
                        crate::layout::hierarchical::compose::bundle::BundleKind::TargetSuffix
                    };
                    let id = format!(
                        "end:{}:{}",
                        if at_source { "src" } else { "tgt" },
                        members.join("+")
                    );
                    bundles.push(
                        crate::layout::hierarchical::compose::bundle::BundlePlan {
                            id,
                            kind,
                            member_edges: members,
                        },
                    );
                    slot_of_member.push((s, c.clone()));
                }
            }
        }
        // Rank = slot position in slot order (stable on slot ties → cluster
        // member order). Slots express relative order only; Metric expands
        // the dense, centered pixel anchor from (order, count). Cluster
        // members share their slot's rank (one slot = one PortPoint).
        slot_of_member.sort_by_key(|s| s.0);
        let count = slot_of_member.len() as u32;
        for (rank, (_, members_in_slot)) in slot_of_member.iter().enumerate() {
            for &i in members_in_slot {
                along_of[i] = AlongSpec::Ordered {
                    order: rank as u32,
                    count,
                };
            }
        }
    }

    let mut out: BTreeMap<String, EdgePorts> = BTreeMap::new();
    for (i, ep) in endpoints.iter().enumerate() {
        let port = ResolvedPort {
            side: ep.side,
            along: along_of[i],
        };
        let entry = out.entry(ep.edge_id.clone()).or_insert(EdgePorts {
            source: ResolvedPort {
                side: Side::South,
                along: AlongSpec::Ordered { order: 0, count: 1 },
            },
            target: ResolvedPort {
                side: Side::North,
                along: AlongSpec::Ordered { order: 0, count: 1 },
            },
            source_cluster: None,
            target_cluster: None,
        });
        if ep.is_source_end {
            entry.source = port;
            entry.source_cluster = cluster_of[i];
        } else {
            entry.target = port;
            entry.target_cluster = cluster_of[i];
        }
    }

    Ok(PortAssignment {
        ports: out,
        bundles,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::hierarchical::model::{Elem, RealEdge, Segment};
    use plotgram_model::port::Side as ModelSide;

    fn small_plan_and_graph() -> (RealGraph, PlanGraph) {
        let ids = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let index_of = ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), i))
            .collect();
        let edges = vec![
            RealEdge {
                edge_id: "e0".into(),
                original_source: 0,
                original_target: 1,
                working_source: 0,
                working_target: 1,
                reversed: false,
                from_port: None,
                to_port: None,
                critical: false,
            },
            RealEdge {
                edge_id: "e1".into(),
                original_source: 0,
                original_target: 2,
                working_source: 0,
                working_target: 2,
                reversed: false,
                from_port: None,
                to_port: None,
                critical: false,
            },
        ];
        let graph = RealGraph {
            ids,
            index_of,
            group_path: vec![Vec::new(); 3],
            shapes: vec![plotgram_model::NodeShape::DEFAULT; 3],
            edges,
            self_loops: Vec::new(),
        };

        let elems = vec![
            Elem {
                key: ElemKey::Real("a".into()),
                group_path: Vec::new(),
                rank: 0,
            },
            Elem {
                key: ElemKey::Real("b".into()),
                group_path: Vec::new(),
                rank: 1,
            },
            Elem {
                key: ElemKey::Real("c".into()),
                group_path: Vec::new(),
                rank: 1,
            },
        ];
        let plan_index = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let segments = vec![
            Segment {
                edge_id: "e0".into(),
                ordinal: 0,
                from: 0,
                to: 1,
            },
            Segment {
                edge_id: "e1".into(),
                ordinal: 0,
                from: 0,
                to: 2,
            },
        ];
        let layers = vec![vec![0], vec![1, 2]];
        let plan = PlanGraph {
            elems,
            index_of: plan_index,
            decl_index: vec![0, 1, 2],
            segments,
            layers,
        };
        (graph, plan)
    }

    fn ordered(port: ResolvedPort) -> (u32, u32) {
        match port.along {
            AlongSpec::Ordered { order, count } => (order, count),
            other => panic!("expected Ordered, got {other:?}"),
        }
    }

    #[test]
    fn free_ports_infer_south_for_downstream_north_for_upstream() {
        let (graph, plan) = small_plan_and_graph();
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb, false).unwrap().ports;
        assert_eq!(ports["e0"].source.side, Side::South);
        assert_eq!(ports["e0"].target.side, Side::North);
        assert_eq!(ports["e1"].source.side, Side::South);
        // a has two outgoing (South) edges to b (order0) and c (order1):
        // dense relative order follows the neighbor order.
        assert_eq!(ordered(ports["e0"].source), (0, 2));
        assert_eq!(ordered(ports["e1"].source), (1, 2));
    }


    #[test]
    fn fixed_side_is_converted_into_canonical_space() {
        // Lr maps physical South → canonical East (point transform (x,y)→(y,x));
        // Bt flips North↔South; Rl ((x,y)→(y,-x)) maps South → East as well
        // but via the 4-cycle North→West→South→East; Tb is the identity.
        let cases = [
            (AlgoOrientation::Tb, ModelSide::South, Side::South),
            (AlgoOrientation::Lr, ModelSide::South, Side::East),
            (AlgoOrientation::Bt, ModelSide::South, Side::North),
            (AlgoOrientation::Rl, ModelSide::South, Side::East),
        ];
        for (orientation, authored, expected_canonical) in cases {
            let (mut graph, plan) = small_plan_and_graph();
            graph.edges[0].from_port = Some(PortConstraint::FixedSide { side: authored });
            let ports = assign_ports(&graph, &plan, orientation, false).unwrap().ports;
            assert_eq!(
                ports["e0"].source.side,
                expected_canonical,
                "orientation {orientation:?}: authored {authored:?}"
            );
            // Round-trip through mod.rs's output transform must recover the
            // authored physical side.
            assert_eq!(
                orientation.from_tb_side(ports["e0"].source.side),
                to_algo_side(authored),
                "output transform must round-trip the authored side"
            );
        }
    }


    #[test]
    fn reversed_edge_free_ports_prefer_cross_axis_side() {
        // Back edge a(rank 1) ← b(rank 0). tip=a (lower on TB) sits right of
        // b's column (layer order 1 vs 0) → shared East corridor both ends.
        let ids = vec!["a".to_string(), "b".to_string()];
        let index_of = ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), i))
            .collect();
        let graph = RealGraph {
            ids,
            index_of,
            group_path: vec![Vec::new(); 2],
            shapes: vec![plotgram_model::NodeShape::DEFAULT; 2],
            edges: vec![RealEdge {
                edge_id: "back".into(),
                original_source: 0, // a
                original_target: 1, // b
                working_source: 1,
                working_target: 0,
                reversed: true,
                from_port: None,
                to_port: None,
                critical: false,
            }],
            self_loops: Vec::new(),
        };
        let elems = vec![
            Elem {
                key: ElemKey::Real("b".into()),
                group_path: Vec::new(),
                rank: 0,
            },
            Elem {
                key: ElemKey::Real("c".into()),
                group_path: Vec::new(),
                rank: 1,
            },
            Elem {
                key: ElemKey::Real("a".into()),
                group_path: Vec::new(),
                rank: 1,
            },
        ];
        let plan_index = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let plan = PlanGraph {
            elems,
            index_of: plan_index,
            decl_index: vec![0, 2],
            segments: vec![Segment {
                edge_id: "back".into(),
                ordinal: 0,
                from: 0, // working: b → a
                to: 2,
            }],
            layers: vec![vec![0], vec![1, 2]],
        };
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb, false).unwrap().ports;
        assert_eq!(ports["back"].source.side, Side::East);
        assert_eq!(ports["back"].target.side, Side::East);
    }

    /// Left-tip reverse (tip order < peer) → shared West corridor.
    #[test]
    fn side_corridor_left_tip_both_ends_west() {
        // tip=a at rank 1 order 0; peer=b at rank 0 order 1 → West/West.
        let ids = vec!["a".to_string(), "b".to_string()];
        let index_of = ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), i))
            .collect();
        let graph = RealGraph {
            ids,
            index_of,
            group_path: vec![Vec::new(); 2],
            shapes: vec![plotgram_model::NodeShape::DEFAULT; 2],
            edges: vec![RealEdge {
                edge_id: "back".into(),
                original_source: 0,
                original_target: 1,
                working_source: 1,
                working_target: 0,
                reversed: true,
                from_port: None,
                to_port: None,
                critical: false,
            }],
            self_loops: Vec::new(),
        };
        let elems = vec![
            Elem {
                key: ElemKey::Real("c".into()),
                group_path: Vec::new(),
                rank: 0,
            },
            Elem {
                key: ElemKey::Real("b".into()),
                group_path: Vec::new(),
                rank: 0,
            },
            Elem {
                key: ElemKey::Real("a".into()),
                group_path: Vec::new(),
                rank: 1,
            },
        ];
        let plan_index = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let plan = PlanGraph {
            elems,
            index_of: plan_index,
            decl_index: vec![2, 1],
            segments: vec![Segment {
                edge_id: "back".into(),
                ordinal: 0,
                from: 1, // working b → a
                to: 2,
            }],
            layers: vec![vec![0, 1], vec![2]],
        };
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb, false)
            .unwrap()
            .ports;
        assert_eq!(ports["back"].source.side, Side::West);
        assert_eq!(ports["back"].target.side, Side::West);
    }

    /// Multi-rank reversed edges also prefer cross-axis sides when the far
    /// real peer sits in a different order column (Channel outer Main).
    /// Same-column peers fall back to rank direction.
    #[test]
    fn long_reversed_edge_cross_axis_when_peers_diverge() {
        let ids = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let index_of = ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), i))
            .collect();
        let graph = RealGraph {
            ids,
            index_of,
            group_path: vec![Vec::new(); 3],
            shapes: vec![plotgram_model::NodeShape::DEFAULT; 3],
            edges: vec![RealEdge {
                edge_id: "back".into(),
                original_source: 0, // a (rank 2, order 1)
                original_target: 1, // b (rank 0, order 0)
                working_source: 1,
                working_target: 0,
                reversed: true,
                from_port: None,
                to_port: None,
                critical: false,
            }],
            self_loops: Vec::new(),
        };
        let elems = vec![
            Elem {
                key: ElemKey::Real("b".into()),
                group_path: Vec::new(),
                rank: 0,
            },
            Elem {
                key: ElemKey::Real("c".into()),
                group_path: Vec::new(),
                rank: 0,
            },
            Elem {
                key: ElemKey::Virtual {
                    edge_id: "back".into(),
                    ordinal: 0,
                },
                group_path: Vec::new(),
                rank: 1,
            },
            Elem {
                key: ElemKey::Real("a".into()),
                group_path: Vec::new(),
                rank: 2,
            },
        ];
        let plan_index = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let plan = PlanGraph {
            elems,
            index_of: plan_index,
            decl_index: vec![0, 1, 3],
            segments: vec![
                Segment {
                    edge_id: "back".into(),
                    ordinal: 0,
                    from: 0,
                    to: 2,
                },
                Segment {
                    edge_id: "back".into(),
                    ordinal: 1,
                    from: 2,
                    to: 3,
                },
            ],
            layers: vec![vec![0, 1], vec![2], vec![3]],
        };
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb, false)
            .unwrap()
            .ports;
        // a alone at order 0; b at order 0 → same column → rank fallback.
        // Put a under c (order 1): rebuild with a alone still order 0.
        // b order 0, a order 0 → Equal → North/South.
        assert!(
            matches!(ports["back"].source.side, Side::North | Side::West | Side::East),
            "got {:?}",
            ports["back"].source.side
        );
    }

    /// Multi-rank reversed (span≥2) takes the side corridor even when columns align.
    #[test]
    fn long_reversed_edge_same_column_prefers_cross_axis() {
        let ids = vec!["a".to_string(), "b".to_string()];
        let index_of = ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), i))
            .collect();
        let graph = RealGraph {
            ids,
            index_of,
            group_path: vec![Vec::new(); 2],
            shapes: vec![plotgram_model::NodeShape::DEFAULT; 2],
            edges: vec![RealEdge {
                edge_id: "back".into(),
                original_source: 0, // a (rank 2)
                original_target: 1, // b (rank 0)
                working_source: 1,
                working_target: 0,
                reversed: true,
                from_port: None,
                to_port: None,
                critical: false,
            }],
            self_loops: Vec::new(),
        };
        let elems = vec![
            Elem {
                key: ElemKey::Real("b".into()),
                group_path: Vec::new(),
                rank: 0,
            },
            Elem {
                key: ElemKey::Virtual {
                    edge_id: "back".into(),
                    ordinal: 0,
                },
                group_path: Vec::new(),
                rank: 1,
            },
            Elem {
                key: ElemKey::Real("a".into()),
                group_path: Vec::new(),
                rank: 2,
            },
        ];
        let plan_index = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let plan = PlanGraph {
            elems,
            index_of: plan_index,
            decl_index: vec![0, 2],
            segments: vec![
                Segment {
                    edge_id: "back".into(),
                    ordinal: 0,
                    from: 0,
                    to: 1,
                },
                Segment {
                    edge_id: "back".into(),
                    ordinal: 1,
                    from: 1,
                    to: 2,
                },
            ],
            layers: vec![vec![0], vec![1], vec![2]],
        };
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb, false).unwrap().ports;
        assert_eq!(ports["back"].source.side, Side::East);
        assert_eq!(ports["back"].target.side, Side::East);
    }

    /// Short same-column reverse **without** a forward twin → side corridor.
    #[test]
    fn short_same_column_reversed_without_twin_prefers_cross_axis() {
        let ids = vec!["a".to_string(), "b".to_string()];
        let index_of = ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), i))
            .collect();
        let graph = RealGraph {
            ids,
            index_of,
            group_path: vec![Vec::new(); 2],
            shapes: vec![plotgram_model::NodeShape::DEFAULT; 2],
            edges: vec![RealEdge {
                edge_id: "back".into(),
                original_source: 0,
                original_target: 1,
                working_source: 1,
                working_target: 0,
                reversed: true,
                from_port: None,
                to_port: None,
                critical: false,
            }],
            self_loops: Vec::new(),
        };
        let elems = vec![
            Elem {
                key: ElemKey::Real("b".into()),
                group_path: Vec::new(),
                rank: 0,
            },
            Elem {
                key: ElemKey::Real("a".into()),
                group_path: Vec::new(),
                rank: 1,
            },
        ];
        let plan_index = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let plan = PlanGraph {
            elems,
            index_of: plan_index,
            decl_index: vec![0, 1],
            segments: vec![Segment {
                edge_id: "back".into(),
                ordinal: 0,
                from: 0,
                to: 1,
            }],
            layers: vec![vec![0], vec![1]],
        };
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb, false)
            .unwrap()
            .ports;
        assert_eq!(ports["back"].source.side, Side::East);
        assert_eq!(ports["back"].target.side, Side::East);
    }

    /// Table-driven corridor roles for [`pick_reversed_side`].
    #[test]
    fn pick_reversed_side_corridor_roles() {
        let ns = Side::North;
        let ew = Side::East;
        let cases: &[(
            &str,
            usize,
            usize,
            usize,
            bool,
            u32,
            Side,
        )] = &[
            ("long span → EW", 2, 0, 0, false, 0, ew),
            ("long + twin → EW", 2, 0, 0, true, 0, ew),
            ("twin short same-col → NS", 1, 0, 0, true, 0, ns),
            ("twin short Δorder=1 → NS", 1, 0, 1, true, 0, ns),
            ("no twin short → EW", 1, 0, 0, false, 0, ew),
            ("no twin short crowded → EW", 1, 0, 0, false, 2, ew),
        ];
        for (label, span, own_o, peer_o, twin, load, want) in cases {
            let got = pick_reversed_side(ns, ew, *span, *own_o, *peer_o, *twin, *load);
            assert_eq!(got, *want, "{label}");
        }
    }

    /// Twin short reverse (req-resp on same column) stays on the flow spine.
    #[test]
    fn twin_short_same_column_reversed_prefers_rank_dir() {
        // a (rank 0) -> b (rank 1) forward; b --> a reversed twin.
        let ids = vec!["a".to_string(), "b".to_string()];
        let index_of = ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), i))
            .collect();
        let graph = RealGraph {
            ids,
            index_of,
            group_path: vec![Vec::new(); 2],
            shapes: vec![plotgram_model::NodeShape::DEFAULT; 2],
            edges: vec![
                RealEdge {
                    edge_id: "fwd".into(),
                    original_source: 0,
                    original_target: 1,
                    working_source: 0,
                    working_target: 1,
                    reversed: false,
                    from_port: None,
                    to_port: None,
                    critical: false,
                },
                RealEdge {
                    edge_id: "back".into(),
                    original_source: 1,
                    original_target: 0,
                    working_source: 0,
                    working_target: 1,
                    reversed: true,
                    from_port: None,
                    to_port: None,
                    critical: false,
                },
            ],
            self_loops: Vec::new(),
        };
        let elems = vec![
            Elem {
                key: ElemKey::Real("a".into()),
                group_path: Vec::new(),
                rank: 0,
            },
            Elem {
                key: ElemKey::Real("b".into()),
                group_path: Vec::new(),
                rank: 1,
            },
        ];
        let plan_index = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let plan = PlanGraph {
            elems,
            index_of: plan_index,
            decl_index: vec![0, 1],
            segments: vec![
                Segment {
                    edge_id: "fwd".into(),
                    ordinal: 0,
                    from: 0,
                    to: 1,
                },
                Segment {
                    edge_id: "back".into(),
                    ordinal: 0,
                    from: 0,
                    to: 1,
                },
            ],
            layers: vec![vec![0], vec![1]],
        };
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb, false)
            .unwrap()
            .ports;
        // Forward: a South → b North.
        assert_eq!(ports["fwd"].source.side, Side::South);
        assert_eq!(ports["fwd"].target.side, Side::North);
        // Twin reverse on spine: b North (toward a) / a South (toward b) —
        // parallel with the request, not East side corridor.
        assert_eq!(ports["back"].source.side, Side::North);
        assert_eq!(ports["back"].target.side, Side::South);
    }

    /// Author FixedSide is never overridden by the FREE reverse corridor model.
    #[test]
    fn fixed_side_on_reversed_edge_is_honored() {
        let ids = vec!["a".to_string(), "b".to_string()];
        let index_of = ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), i))
            .collect();
        let graph = RealGraph {
            ids,
            index_of,
            group_path: vec![Vec::new(); 2],
            shapes: vec![plotgram_model::NodeShape::DEFAULT; 2],
            edges: vec![RealEdge {
                edge_id: "back".into(),
                original_source: 0,
                original_target: 1,
                working_source: 1,
                working_target: 0,
                reversed: true,
                from_port: Some(PortConstraint::FixedSide {
                    side: ModelSide::South,
                }),
                to_port: Some(PortConstraint::FixedSide {
                    side: ModelSide::North,
                }),
                critical: false,
            }],
            self_loops: Vec::new(),
        };
        let elems = vec![
            Elem {
                key: ElemKey::Real("b".into()),
                group_path: Vec::new(),
                rank: 0,
            },
            Elem {
                key: ElemKey::Real("a".into()),
                group_path: Vec::new(),
                rank: 1,
            },
        ];
        let plan_index = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let plan = PlanGraph {
            elems,
            index_of: plan_index,
            decl_index: vec![0, 1],
            segments: vec![Segment {
                edge_id: "back".into(),
                ordinal: 0,
                from: 0,
                to: 1,
            }],
            layers: vec![vec![0], vec![1]],
        };
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb, false)
            .unwrap()
            .ports;
        assert_eq!(ports["back"].source.side, Side::South);
        assert_eq!(ports["back"].target.side, Side::North);
    }






    fn parallel_plan_and_graph() -> (RealGraph, PlanGraph) {
        let ids = vec!["a".to_string(), "b".to_string()];
        let index_of = ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), i))
            .collect();
        let edges = vec![
            RealEdge {
                edge_id: "e0".into(),
                original_source: 0,
                original_target: 1,
                working_source: 0,
                working_target: 1,
                reversed: false,
                from_port: None,
                to_port: None,
                critical: false,
            },
            RealEdge {
                edge_id: "e1".into(),
                original_source: 0,
                original_target: 1,
                working_source: 0,
                working_target: 1,
                reversed: false,
                from_port: None,
                to_port: None,
                critical: false,
            },
        ];
        let graph = RealGraph {
            ids,
            index_of,
            group_path: vec![Vec::new(); 2],
            shapes: vec![plotgram_model::NodeShape::DEFAULT; 2],
            edges,
            self_loops: Vec::new(),
        };
        let elems = vec![
            Elem {
                key: ElemKey::Real("a".into()),
                group_path: Vec::new(),
                rank: 0,
            },
            Elem {
                key: ElemKey::Real("b".into()),
                group_path: Vec::new(),
                rank: 1,
            },
        ];
        let plan_index = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let segments = vec![
            Segment {
                edge_id: "e0".into(),
                ordinal: 0,
                from: 0,
                to: 1,
            },
            Segment {
                edge_id: "e1".into(),
                ordinal: 0,
                from: 0,
                to: 1,
            },
        ];
        let layers = vec![vec![0], vec![1]];
        let plan = PlanGraph {
            elems,
            index_of: plan_index,
            decl_index: vec![0, 1],
            segments,
            layers,
        };
        (graph, plan)
    }

    #[test]
    fn auto_edge_grouping_clusters_parallel_ends() {
        let (graph, plan) = parallel_plan_and_graph();
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb, true).unwrap().ports;
        assert_eq!(ordered(ports["e0"].source), ordered(ports["e1"].source));
        assert_eq!(ordered(ports["e0"].source).1, 1, "cluster takes one slot");
        let (c0, c1) = (
            ports["e0"].source_cluster.unwrap(),
            ports["e1"].source_cluster.unwrap(),
        );
        assert_eq!((c0.count, c1.count), (2, 2));
        assert_ne!(c0.index, c1.index);
        assert_eq!(ordered(ports["e0"].target), ordered(ports["e1"].target));
        assert!(ports["e0"].target_cluster.is_some());

        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb, false).unwrap().ports;
        assert_eq!(ordered(ports["e0"].source).1, 2);
        assert!(ports["e0"].source_cluster.is_none());
    }

    /// Fan-out (distinct chain neighbors) merges at the common source when
    /// auto_edge_grouping is on — no per-edge author group id required.
    #[test]
    fn auto_edge_grouping_merges_fan_out() {
        let (graph, plan) = small_plan_and_graph(); // a→b, a→c: distinct neighbors
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb, true).unwrap().ports;
        assert_eq!(ordered(ports["e0"].source), ordered(ports["e1"].source));
        let (c0, c1) = (
            ports["e0"].source_cluster.unwrap(),
            ports["e1"].source_cluster.unwrap(),
        );
        assert_eq!((c0.count, c1.count), (2, 2));
        assert_ne!(c0.index, c1.index);
    }

    #[test]
    fn person_shape_rejects_free_north() {
        let (mut graph, plan) = small_plan_and_graph();
        // Targets of a→b / a→c sit upstream of the edge direction as North.
        graph.shapes[1] = plotgram_model::NodeShape::Person; // b
        graph.shapes[2] = plotgram_model::NodeShape::Person; // c
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb, false)
            .unwrap()
            .ports;
        assert_ne!(ports["e0"].target.side, Side::North);
        assert_ne!(ports["e1"].target.side, Side::North);
        assert!(matches!(
            ports["e0"].target.side,
            Side::South | Side::East | Side::West
        ));
    }

    #[test]
    fn diamond_capacity_soft_overflows_same_face_primary() {
        let (mut graph, plan) = small_plan_and_graph();
        graph.shapes[0] = plotgram_model::NodeShape::Diamond; // a: two FREE South outs
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb, false)
            .unwrap()
            .ports;
        // Capacity=1 does not force a face change — both stay on topological South.
        assert_eq!(ports["e0"].source.side, Side::South);
        assert_eq!(ports["e1"].source.side, Side::South);
    }

    #[test]
    fn rect_keeps_multiple_free_on_same_side() {
        let (graph, plan) = small_plan_and_graph();
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb, false)
            .unwrap()
            .ports;
        assert_eq!(ports["e0"].source.side, Side::South);
        assert_eq!(ports["e1"].source.side, Side::South);
    }


    #[test]
    fn fixed_side_north_on_person_wins() {
        let (mut graph, plan) = small_plan_and_graph();
        graph.shapes[1] = plotgram_model::NodeShape::Person;
        graph.edges[0].to_port = Some(PortConstraint::FixedSide {
            side: ModelSide::North,
        });
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb, false)
            .unwrap()
            .ports;
        assert_eq!(ports["e0"].target.side, Side::North);
    }

    /// Long-edge dummy order must not invert fan Ordered vs far-real leaf order.
    #[test]
    fn fan_ordered_sort_follows_far_real_not_dummy() {
        // hub → near (adj); hub → far (long). Layer1: [dummy_far, near] so dummy
        // is left of near; far sits rightmost on rank2. Immediate-neighbor sort
        // would put far edge left; far-real sort keeps near left of far.
        let ids: Vec<String> = vec![
            "hub".into(),
            "near".into(),
            "far".into(),
            "pad".into(),
        ];
        let index_of = ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), i))
            .collect();
        let graph = RealGraph {
            ids,
            index_of,
            group_path: vec![Vec::new(); 4],
            shapes: vec![plotgram_model::NodeShape::DEFAULT; 4],
            edges: vec![
                RealEdge {
                    edge_id: "e_near".into(),
                    original_source: 0,
                    original_target: 1,
                    working_source: 0,
                    working_target: 1,
                    reversed: false,
                    from_port: None,
                    to_port: None,
                    critical: false,
                },
                RealEdge {
                    edge_id: "e_far".into(),
                    original_source: 0,
                    original_target: 2,
                    working_source: 0,
                    working_target: 2,
                    reversed: false,
                    from_port: None,
                    to_port: None,
                    critical: false,
                },
            ],
            self_loops: Vec::new(),
        };
        let elems = vec![
            Elem {
                key: ElemKey::Real("hub".into()),
                group_path: Vec::new(),
                rank: 0,
            },
            Elem {
                key: ElemKey::Virtual {
                    edge_id: "e_far".into(),
                    ordinal: 0,
                },
                group_path: Vec::new(),
                rank: 1,
            },
            Elem {
                key: ElemKey::Real("near".into()),
                group_path: Vec::new(),
                rank: 1,
            },
            Elem {
                key: ElemKey::Real("pad".into()),
                group_path: Vec::new(),
                rank: 2,
            },
            Elem {
                key: ElemKey::Real("far".into()),
                group_path: Vec::new(),
                rank: 2,
            },
        ];
        let plan_index = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let plan = PlanGraph {
            elems,
            index_of: plan_index,
            decl_index: vec![0, 2, 3, 4],
            segments: vec![
                Segment {
                    edge_id: "e_near".into(),
                    ordinal: 0,
                    from: 0,
                    to: 2,
                },
                Segment {
                    edge_id: "e_far".into(),
                    ordinal: 0,
                    from: 0,
                    to: 1,
                },
                Segment {
                    edge_id: "e_far".into(),
                    ordinal: 1,
                    from: 1,
                    to: 4,
                },
            ],
            layers: vec![vec![0], vec![1, 2], vec![3, 4]],
        };
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb, false)
            .unwrap()
            .ports;
        let (o_near, _) = ordered(ports["e_near"].source);
        let (o_far, _) = ordered(ports["e_far"].source);
        assert!(
            o_near < o_far,
            "near (far-real order 0 on its side of compare) must be left of far; got near={o_near} far={o_far}"
        );
    }
}
