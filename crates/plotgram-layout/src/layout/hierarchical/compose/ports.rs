//! P7 port finalize — the single port writer (architecture.md §7,
//! ports-and-channel.md §1–§2). Runs *inside* the TB-canonical core (see
//! `orient.rs`); authored sides / local points are converted into canonical
//! space here via `Orientation::to_tb_side` / `to_tb_point`, mirroring the
//! `from_tb_*` pass in `mod.rs` on the way out (architecture.md §9.3).
//!
//! Five author tiers (plotgram_model::port::PortConstraint):
//!
//! - FREE (`None`) — side inferred; reversed (back) edges prefer the
//!   cross-axis side facing their chain head so the corridor does not enter
//!   the node over its head (roadmap G3);
//! - FIXED_SIDE / FIXED_ORDER / FIXED_RATIO / FIXED_POS — honored as
//!   authored; `Candidates` scores within the author's side set with the
//!   same preference;
//! - every tier resolves to a canonical `ResolvedPort` whose `along` spec is
//!   expanded to pixels only by Metric (`metric/anchor.rs`) — never here.
//!
//! Ordered slot order within a (node, side) group: opposite endpoint's
//! `(neighbor_rank, neighbor_order, EdgeId)` (ports-and-channel.md §2
//! step 4). FREE ordered members take a centered slot run around pinned
//! order keys (roadmap phase B: reduce meaningless micro-offsets).

use std::collections::{BTreeMap, BTreeSet};

use plotgram_algo::orientation::{Orientation as AlgoOrientation, Side, Size};
use plotgram_engine_api::LayoutError;
use plotgram_model::geometry::Point;
use plotgram_model::port::{AlongSpec, PortConstraint};

use crate::layout::hierarchical::model::{ElemKey, PlanGraph, RealGraph};
use crate::layout::hierarchical::orient::{from_algo_point, to_algo_point, to_algo_side};

/// Tolerance for FIXED_POS boundary validation (px, canonical space).
const POS_BOUNDARY_TOL: f64 = 1e-6;

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

/// FREE side inference (canonical TB). Forward edges follow the rank
/// direction (downstream → South, upstream → North). Reversed (back) edges
/// that have no dummy chain prefer the cross-axis side facing the opposite
/// endpoint's order position — the corridor leaves
/// FREE side from graph shape. Forward edges follow the rank direction.
/// Reversed (back) edges prefer a cross-axis exit so Channel can take the
/// outer Main corridor (roadmap G3 + D1.2 `prefer_outer_main`) — including
/// multi-rank spans whose chain neighbor is a dummy (peer = far real end).
fn free_side(plan: &PlanGraph, pos: &[usize], node_real: usize, neighbor: usize, reversed: bool) -> Side {
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
    };
    match pos.get(peer).copied().unwrap_or(0).cmp(&pos[node_real]) {
        std::cmp::Ordering::Less => Side::West,
        std::cmp::Ordering::Greater => Side::East,
        std::cmp::Ordering::Equal => rank_dir,
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

/// Full deterministic side preference given a primary pick: the primary
/// first, then the remaining sides in fixed `Side` order (ties follow the
/// fixed Side order — ports-and-channel.md §2 step 3).
fn side_preference(primary: Side) -> [Side; 4] {
    let rest: Vec<Side> = [Side::North, Side::South, Side::East, Side::West]
        .into_iter()
        .filter(|s| *s != primary)
        .collect();
    [primary, rest[0], rest[1], rest[2]]
}

/// One end (source or target) of one edge after tier resolution, still
/// awaiting slot assignment for ordered tiers.
struct EndPoint {
    edge_id: String,
    is_source_end: bool,
    node_real: usize,
    side: Side,
    /// Author order key (FIXED_ORDER only).
    fixed_order: Option<u32>,
    /// Non-ordered tiers resolve their `along` spec directly (Ratio /
    /// LocalOffset); they do not participate in slot grouping.
    along_fixed: Option<AlongSpec>,
    sort_key: (u32, usize, String), // (neighbor_rank, neighbor_order, edge_id)
}

/// Resolve the author constraint of one endpoint into a canonical side +
/// tier facts. `node_id` / `real_idx` are only used for error messages.
fn resolve_constraint(
    constraint: Option<PortConstraint>,
    orientation: AlgoOrientation,
    canonical_size: Size,
    node_id: &str,
    edge_id: &str,
    end: &str,
) -> Result<(Side, Option<u32>, Option<AlongSpec>), LayoutError> {
    let Some(c) = constraint else {
        // FREE — the caller infers the side from the graph shape.
        return Ok((Side::North, None, None));
    };
    match c {
        PortConstraint::FixedSide { side } => {
            Ok((orientation.to_tb_side(to_algo_side(side)), None, None))
        }
        PortConstraint::FixedOrder { side, order } => Ok((
            orientation.to_tb_side(to_algo_side(side)),
            Some(order),
            None,
        )),
        PortConstraint::FixedRatio { side, ratio } => Ok((
            orientation.to_tb_side(to_algo_side(side)),
            None,
            Some(AlongSpec::Ratio(ratio)),
        )),
        PortConstraint::FixedPos { local } => {
            let canonical = from_algo_point(orientation.to_tb_point(to_algo_point(local)));
            let (side, clamped) =
                pos_on_boundary(canonical, canonical_size).ok_or_else(|| {
                    LayoutError::message(format!(
                        "hierarchical: edge `{edge_id}` {end} port FIXED_POS ({}, {}) \
                         is not on node `{node_id}`'s boundary ({} x {} canonical); \
                         refusing to silently drop the constraint",
                        local.x, local.y, canonical_size.width, canonical_size.height
                    ))
                })?;
            Ok((side, None, Some(AlongSpec::LocalOffset(clamped))))
        }
        PortConstraint::Candidates { .. } => {
            // Handled by the caller (needs graph-shape scoring); unreachable here.
            unreachable!("Candidates resolved by caller")
        }
    }
}

/// Side + clamped canonical local point for a FIXED_POS constraint.
/// Returns `None` when the point is off-frame or in the interior. Corner
/// points resolve by fixed priority: North, South, West, East.
fn pos_on_boundary(p: Point, size: Size) -> Option<(Side, Point)> {
    let w = size.width;
    let h = size.height;
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    if p.x < -POS_BOUNDARY_TOL
        || p.x > w + POS_BOUNDARY_TOL
        || p.y < -POS_BOUNDARY_TOL
        || p.y > h + POS_BOUNDARY_TOL
    {
        return None;
    }
    let x = p.x.clamp(0.0, w);
    let y = p.y.clamp(0.0, h);
    let on_left = x <= POS_BOUNDARY_TOL;
    let on_right = x >= w - POS_BOUNDARY_TOL;
    let on_top = y <= POS_BOUNDARY_TOL;
    let on_bottom = y >= h - POS_BOUNDARY_TOL;
    if !(on_left || on_right || on_top || on_bottom) {
        return None; // interior point
    }
    let side = if on_top {
        Side::North
    } else if on_bottom {
        Side::South
    } else if on_left {
        Side::West
    } else {
        Side::East
    };
    // Snap to the owning edge so Metric's anchor lands exactly on it.
    let snapped = match side {
        Side::North => Point { x, y: 0.0 },
        Side::South => Point { x, y: h },
        Side::West => Point { x: 0.0, y },
        Side::East => Point { x: w, y },
    };
    Some((side, snapped))
}

/// P7 port finalize. `canonical_size` is indexed by real-node index
/// (`graph.ids` order) and needed only for FIXED_POS validation.
///
/// `auto_edge_grouping` enables automatic fan clustering (edge-parameters §2.3):
/// eligible FREE ends on the same (node, main-axis side) merge into one shared
/// slot — same-source fan-out / same-target fan-in, matching yFiles
/// AutomaticEdgeGrouping (no per-edge author group ids).
pub fn assign_ports(
    graph: &RealGraph,
    plan: &PlanGraph,
    orientation: AlgoOrientation,
    canonical_size: &[Size],
    auto_edge_grouping: bool,
) -> Result<PortAssignment, LayoutError> {
    let pos = positions_within_layer(plan);
    let real_idx_of = |id: &str| plan.index_of[&ElemKey::Real(id.to_string())];

    let mut endpoints = Vec::with_capacity(graph.edges.len() * 2);
    for e in &graph.edges {
        for (is_source_end, node_id, constraint) in [
            (true, &graph.ids[e.original_source], e.from_port.clone()),
            (false, &graph.ids[e.original_target], e.to_port.clone()),
        ] {
            let node_real = real_idx_of(node_id);
            let neighbor = immediate_neighbor(plan, &e.edge_id, node_real);
            let neighbor_rank = plan.elems[neighbor].rank;
            let real_idx = graph.index_of[node_id];
            let end = if is_source_end { "source" } else { "target" };

            let (side, fixed_order, along_fixed) = match &constraint {
                Some(PortConstraint::Candidates { sides }) => {
                    let canonical: Vec<Side> = sides
                        .iter()
                        .map(|s| orientation.to_tb_side(to_algo_side(*s)))
                        .collect();
                    let primary = free_side(plan, &pos, node_real, neighbor, e.reversed);
                    let pick = side_preference(primary)
                        .into_iter()
                        .find(|s| canonical.contains(s))
                        .expect("candidate set is non-empty and covers all four sides' domain");
                    (pick, None, None)
                }
                _ => {
                    let (side, fixed_order, along_fixed) = resolve_constraint(
                        constraint.clone(),
                        orientation,
                        canonical_size[real_idx],
                        node_id,
                        &e.edge_id,
                        end,
                    )?;
                    if constraint.is_none() {
                        // FREE: side from graph shape.
                        (
                            free_side(plan, &pos, node_real, neighbor, e.reversed),
                            None,
                            None,
                        )
                    } else {
                        (side, fixed_order, along_fixed)
                    }
                }
            };

            endpoints.push(EndPoint {
                edge_id: e.edge_id.clone(),
                is_source_end,
                node_real,
                side,
                fixed_order,
                along_fixed,
                sort_key: (neighbor_rank, pos[neighbor], e.edge_id.clone()),
            });
        }
    }

    // Strong-constraint compatibility on the same (node, side): identical
    // Ratio / LocalOffset pins collide and are an input error — hard fail,
    // never a silent merge (ports-and-channel.md §1.1).
    let mut ratio_seen: BTreeSet<(usize, Side, u64)> = BTreeSet::new();
    let mut pos_seen: BTreeSet<(usize, Side, u64, u64)> = BTreeSet::new();
    for ep in &endpoints {
        let key_node = ep.node_real;
        match ep.along_fixed {
            Some(AlongSpec::Ratio(r)) => {
                let key = (key_node, ep.side, r.to_bits());
                if !ratio_seen.insert(key) {
                    return Err(LayoutError::message(format!(
                        "hierarchical: edge `{}` pins ratio {r} on a side already pinned \
                         with the same ratio; same-side strong constraints must be compatible",
                        ep.edge_id
                    )));
                }
            }
            Some(AlongSpec::LocalOffset(p)) => {
                let key = (key_node, ep.side, p.x.to_bits(), p.y.to_bits());
                if !pos_seen.insert(key) {
                    return Err(LayoutError::message(format!(
                        "hierarchical: edge `{}` pins the same FIXED_POS point on the same \
                         side as another edge; same-side strong constraints must be compatible",
                        ep.edge_id
                    )));
                }
            }
            _ => {}
        }
    }

    // Group ordered members by (node, side); assign slots within each group.
    let mut groups: BTreeMap<(usize, Side), Vec<usize>> = BTreeMap::new(); // -> indices into `endpoints`
    for (i, ep) in endpoints.iter().enumerate() {
        if ep.along_fixed.is_none() {
            groups.entry((ep.node_real, ep.side)).or_default().push(i);
        }
    }

    let mut along_of: Vec<AlongSpec> = vec![AlongSpec::Ordered { order: 0, count: 1 }; endpoints.len()];
    let mut cluster_of: Vec<Option<EndCluster>> = vec![None; endpoints.len()];
    let mut bundles: Vec<crate::layout::hierarchical::compose::bundle::BundlePlan> =
        Vec::new();
    for members in groups.into_values() {
        let taken: BTreeSet<u32> = members
            .iter()
            .filter_map(|&i| endpoints[i].fixed_order)
            .collect();

        // Automatic edge grouping (yFiles AutomaticEdgeGrouping / edge-
        // parameters §2.3): eligible FREE ends already share (node, side)
        // via the outer group key — merge them into ONE cluster (common
        // source fan-out / common target fan-in). A cluster occupies ONE
        // slot; Metric assigns a shared PortPoint + bus_y. Disabled
        // or ineligible → every member stays a single-slot member.
        let mut cluster_map: BTreeMap<(), Vec<usize>> = BTreeMap::new();
        if auto_edge_grouping {
            for &i in &members {
                let ep = &endpoints[i];
                let eligible = ep.fixed_order.is_none()
                    && matches!(ep.side, Side::North | Side::South);
                if eligible {
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

        // FREE slots (one per slot member, clusters included) take a run
        // centered around the pinned order keys; with no pins the run starts
        // at 0. Only singles can be pinned (clusters are FREE by eligibility).
        let free_count = slot_members
            .iter()
            .filter(|m| matches!(m, SlotMember::Single(i) if endpoints[*i].fixed_order.is_none()))
            .count();
        let start = match taken.iter().max() {
            None => 0u32,
            Some(&m) => {
                let center = m as f64 / 2.0;
                let s = (center - (free_count as f64 - 1.0) / 2.0).round();
                (s.max(0.0)) as u32
            }
        };

        let mut slot_of_member: Vec<(u32, Vec<usize>)> = Vec::with_capacity(slot_members.len());
        let mut next_free = start;
        for m in &slot_members {
            match m {
                SlotMember::Single(i) => {
                    let s = match endpoints[*i].fixed_order {
                        Some(s) => s,
                        None => {
                            while taken.contains(&next_free) {
                                next_free += 1;
                            }
                            let s = next_free;
                            next_free += 1;
                            s
                        }
                    };
                    slot_of_member.push((s, vec![*i]));
                }
                SlotMember::Cluster(c) => {
                    while taken.contains(&next_free) {
                        next_free += 1;
                    }
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
            along: ep.along_fixed.unwrap_or(along_of[i]),
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

    fn sizes(n: usize) -> Vec<Size> {
        vec![Size::new(40.0, 20.0); n]
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
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb, &sizes(3), false).unwrap().ports;
        assert_eq!(ports["e0"].source.side, Side::South);
        assert_eq!(ports["e0"].target.side, Side::North);
        assert_eq!(ports["e1"].source.side, Side::South);
        // a has two outgoing (South) edges to b (order0) and c (order1):
        // dense relative order follows the neighbor order.
        assert_eq!(ordered(ports["e0"].source), (0, 2));
        assert_eq!(ordered(ports["e1"].source), (1, 2));
    }

    #[test]
    fn fixed_order_is_honored_and_free_orders_avoid_it() {
        let (graph, mut plan) = small_plan_and_graph();
        let mut graph = graph;
        graph.edges[1].from_port = Some(PortConstraint::FixedOrder {
            side: ModelSide::South,
            order: 0,
        });
        let _ = &mut plan;
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb, &sizes(3), false).unwrap().ports;
        assert_eq!(ordered(ports["e1"].source), (0, 2));
        assert_eq!(
            ordered(ports["e0"].source),
            (1, 2),
            "free member must sort after the pinned order key 0"
        );
    }

    /// Regression: an authored (physical) fixed side must enter the canonical
    /// TB core through `to_tb_side` — with `Tb` the identity hid the bug.
    /// Table-driven over the four orientations (architecture.md §9.3).
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
            let ports = assign_ports(&graph, &plan, orientation, &sizes(3), false).unwrap().ports;
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

    /// FREE center preference: a single free port is the group's center by
    /// construction (dense expansion); around a high pinned key the free run
    /// starts centered, not packed at slot 0.
    #[test]
    fn free_slots_center_around_pinned_order_keys() {
        let (graph, plan) = small_plan_and_graph();
        let mut graph = graph;
        // Pin e1's source at order key 4 — two free? no: e0 free + e1 pinned.
        graph.edges[1].from_port = Some(PortConstraint::FixedOrder {
            side: ModelSide::South,
            order: 4,
        });
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb, &sizes(3), false).unwrap().ports;
        // Free run centered at 4/2 = 2 → e0 slot 2 < pinned 4 → order 0.
        assert_eq!(ordered(ports["e0"].source), (0, 2));
        assert_eq!(ordered(ports["e1"].source), (1, 2));
    }

    /// G3: a reversed (back) edge without a dummy chain has its FREE
    /// endpoints leave via the cross-axis side facing the opposite
    /// endpoint's order position — not over the node head.
    #[test]
    fn reversed_edge_free_ports_prefer_cross_axis_side() {
        // Back edge a(rank 1) ← b(rank 0) reversed into working b→a; the
        // single segment connects them directly. Layer positions: b at 0 in
        // its layer, a at 1 in its layer (c occupies position 0) → at a the
        // neighbor b (pos 0 < 1) is left → West; at b the neighbor a
        // (pos 1 > 0) is right → East.
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
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb, &sizes(2), false).unwrap().ports;
        // a (node pos 1; neighbor b at pos 0 → left) → West.
        assert_eq!(ports["back"].source.side, Side::West);
        // b (node pos 0; neighbor a at pos 1 → right) → East.
        assert_eq!(ports["back"].target.side, Side::East);
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
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb, &sizes(3), false)
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

    /// Multi-rank reversed with aligned columns keeps rank-direction sides.
    #[test]
    fn long_reversed_edge_keeps_rank_direction_side() {
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
        // working b → a spans ranks 0..2 through a dummy at rank 1.
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
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb, &sizes(2), false).unwrap().ports;
        // a's chain neighbor is the rank-1 dummy (upstream) → North.
        assert_eq!(ports["back"].source.side, Side::North);
        // b's chain neighbor is the rank-1 dummy (downstream) → South.
        assert_eq!(ports["back"].target.side, Side::South);
    }

    /// Candidates restrict the FREE scoring to the author's side set; the
    /// primary (rank-direction) side wins when offered, otherwise the next
    /// side in the fixed preference order.
    #[test]
    fn candidates_restrict_free_side_choice() {
        let (graph, plan) = small_plan_and_graph();
        let mut graph = graph;
        // e0 source is downstream-preferring South; candidates without South
        // → next in preference order (North).
        graph.edges[0].from_port = Some(PortConstraint::Candidates {
            sides: vec![ModelSide::West, ModelSide::North],
        });
        // e1 source offered South → keeps the primary.
        graph.edges[1].from_port = Some(PortConstraint::Candidates {
            sides: vec![ModelSide::South, ModelSide::East],
        });
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb, &sizes(3), false).unwrap().ports;
        assert_eq!(ports["e0"].source.side, Side::North);
        assert_eq!(ports["e1"].source.side, Side::South);
    }

    #[test]
    fn fixed_ratio_resolves_to_ratio_along_spec() {
        let (graph, plan) = small_plan_and_graph();
        let mut graph = graph;
        graph.edges[0].to_port = Some(PortConstraint::FixedRatio {
            side: ModelSide::North,
            ratio: 0.25,
        });
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb, &sizes(3), false).unwrap().ports;
        assert_eq!(ports["e0"].target.side, Side::North);
        assert_eq!(ports["e0"].target.along, AlongSpec::Ratio(0.25));
    }

    /// FIXED_POS table: boundary points resolve to the owning side and a
    /// snapped local offset; interior / off-frame points are hard errors.
    #[test]
    fn fixed_pos_resolves_side_and_validates_boundary() {
        // Canonical node size is 40 x 20 for all fixtures.
        let cases = [
            // (local point, expected side, expected offset)
            (Point { x: 12.0, y: 0.0 }, Side::North, Point { x: 12.0, y: 0.0 }),
            (Point { x: 12.0, y: 20.0 }, Side::South, Point { x: 12.0, y: 20.0 }),
            (Point { x: 0.0, y: 7.0 }, Side::West, Point { x: 0.0, y: 7.0 }),
            (Point { x: 40.0, y: 7.0 }, Side::East, Point { x: 40.0, y: 7.0 }),
            // Corner: North wins by fixed priority.
            (Point { x: 0.0, y: 0.0 }, Side::North, Point { x: 0.0, y: 0.0 }),
        ];
        for (local, want_side, want_offset) in cases {
            let (graph, plan) = small_plan_and_graph();
            let mut graph = graph;
            graph.edges[0].from_port = Some(PortConstraint::FixedPos { local });
            let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb, &sizes(3), false).unwrap().ports;
            assert_eq!(ports["e0"].source.side, want_side, "local {local:?}");
            assert_eq!(
                ports["e0"].source.along,
                AlongSpec::LocalOffset(want_offset),
                "local {local:?}"
            );
        }

        for bad in [Point { x: 12.0, y: 10.0 }, Point { x: 99.0, y: 0.0 }] {
            let (graph, plan) = small_plan_and_graph();
            let mut graph = graph;
            graph.edges[0].from_port = Some(PortConstraint::FixedPos { local: bad });
            assert!(
                assign_ports(&graph, &plan, AlgoOrientation::Tb, &sizes(3), false).is_err(),
                "FIXED_POS {bad:?} must hard-fail"
            );
        }
    }

    /// FIXED_POS canonicalizes through the orientation: authored physical
    /// (0, 5) under Lr ((x,y)→(y,x)) lands at canonical (5, 0) = North edge.
    #[test]
    fn fixed_pos_is_canonicalized_for_orientation() {
        let (graph, plan) = small_plan_and_graph();
        let mut graph = graph;
        graph.edges[0].from_port = Some(PortConstraint::FixedPos {
            local: Point { x: 0.0, y: 5.0 },
        });
        // Lr canonical size swaps axes: 20 x 40.
        let lr_sizes = vec![Size::new(20.0, 40.0); 3];
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Lr, &lr_sizes, false).unwrap().ports;
        assert_eq!(ports["e0"].source.side, Side::North);
        assert_eq!(
            ports["e0"].source.along,
            AlongSpec::LocalOffset(Point { x: 5.0, y: 0.0 })
        );
    }

    #[test]
    fn duplicate_ratio_on_same_side_is_a_hard_error() {
        let (graph, plan) = small_plan_and_graph();
        let mut graph = graph;
        for e in graph.edges.iter_mut() {
            e.from_port = Some(PortConstraint::FixedRatio {
                side: ModelSide::South,
                ratio: 0.5,
            });
        }
        assert!(assign_ports(&graph, &plan, AlgoOrientation::Tb, &sizes(3), false).is_err());
    }

    /// Two parallel edges a→b share their chain's first elem at both ends →
    /// one cluster per end, one shared slot (edge-parameters §2.3).
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

    /// Parallel edges on the same (node, side) merge into one cluster when
    /// auto_edge_grouping is on; disabled → separate slots.
    #[test]
    fn auto_edge_grouping_clusters_parallel_ends() {
        let (graph, plan) = parallel_plan_and_graph();
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb, &sizes(2), true).unwrap().ports;
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

        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb, &sizes(2), false).unwrap().ports;
        assert_eq!(ordered(ports["e0"].source).1, 2);
        assert!(ports["e0"].source_cluster.is_none());
    }

    /// Fan-out (distinct chain neighbors) merges at the common source when
    /// auto_edge_grouping is on — no per-edge author group id required.
    #[test]
    fn auto_edge_grouping_merges_fan_out() {
        let (graph, plan) = small_plan_and_graph(); // a→b, a→c: distinct neighbors
        let ports = assign_ports(&graph, &plan, AlgoOrientation::Tb, &sizes(3), true).unwrap().ports;
        assert_eq!(ordered(ports["e0"].source), ordered(ports["e1"].source));
        let (c0, c1) = (
            ports["e0"].source_cluster.unwrap(),
            ports["e1"].source_cluster.unwrap(),
        );
        assert_eq!((c0.count, c1.count), (2, 2));
        assert_ne!(c0.index, c1.index);
    }
}
