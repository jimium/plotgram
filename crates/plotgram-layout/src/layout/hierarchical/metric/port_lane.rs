//! PortLaneWriter — corridor alignment + face-level absolute along.
//!
//! Contract: [`docs/design/layout/hierarchical/phases/port-lanes.md`]
//!
//! After node frames are known:
//! 1. Twin short N/S corridors get a shared world-space `lane_x` at both ends
//!    (yFiles PortAlignmentIds; **not** grid; **not** forced `axis ± pitch`).
//! 2. Every end on a touched `(node, side)` is rewritten to `LocalOffset`,
//!    monotone in Compose `Ordered` on twin faces; other shared faces follow
//!    the partner column (dummy trunk / peer center).

use std::collections::{BTreeMap, BTreeSet};

use plotgram_algo::orientation::Side;
use plotgram_model::geometry::{Point, Rect};
use plotgram_model::port::AlongSpec;

use crate::layout::hierarchical::compose::ports::EdgePorts;
use crate::layout::hierarchical::metric::anchor::port_anchor;
use crate::layout::hierarchical::model::{ElemKey, PlanGraph, RealEdge, RealGraph};

const PORT_MARGIN: f64 = 0.0;

/// Fixed-point budget for the N/S port alignment sweep, and the movement below
/// which the sweep is considered settled.
const PORT_ALIGN_ROUNDS: usize = 512;
const PORT_ALIGN_EPS: f64 = 1e-9;

#[derive(Clone, Debug)]
struct FaceEnd {
    edge_id: String,
    is_source: bool,
    order: u32,
}

#[derive(Clone, Debug)]
struct CorridorGroup {
    /// Corridor edge ids, sorted by Compose order (then edge id).
    edges: Vec<String>,
}

/// Align twin N/S corridors and adsorb Ordered N/S ports onto corridor x.
pub fn apply_port_lanes(
    plan: &PlanGraph,
    graph: &RealGraph,
    ports: &mut BTreeMap<String, EdgePorts>,
    frames: &[Rect],
    port_pitch: f64,
) {
    let groups = collect_corridor_groups(plan, graph, ports);
    let edge_of = graph.edge_index_map();
    let corridor_edges: BTreeSet<String> = groups
        .iter()
        .flat_map(|g| g.edges.iter().cloned())
        .collect();

    let face_ends = build_face_ends(plan, graph, ports);
    let mut touched: BTreeSet<(usize, Side)> = BTreeSet::new();
    for edge_id in &corridor_edges {
        let Some(ep) = ports.get(edge_id) else {
            continue;
        };
        let Some((src, tgt)) = endpoint_elems(plan, graph, &edge_of, edge_id) else {
            continue;
        };
        touched.insert((src, ep.source.side));
        touched.insert((tgt, ep.target.side));
    }

    // Provisional Ordered-isomorphic positions on touched faces.
    let mut provisional: BTreeMap<(String, bool), f64> = BTreeMap::new();
    for &face in &touched {
        let Some(ends) = face_ends.get(&face) else {
            continue;
        };
        let frame = &frames[face.0];
        let n = ends.len();
        if n == 0 {
            continue;
        }
        for (i, end) in ends.iter().enumerate() {
            let t = (i as f64 + 1.0) / (n as f64 + 1.0);
            let x = frame.x + PORT_MARGIN + t * (frame.width - 2.0 * PORT_MARGIN).max(0.0);
            provisional.insert((end.edge_id.clone(), end.is_source), x);
        }
    }

    // Shared lane per twin corridor edge.
    let mut lanes: BTreeMap<String, f64> = BTreeMap::new();
    for edge_id in &corridor_edges {
        let ps = provisional
            .get(&(edge_id.clone(), true))
            .copied()
            .unwrap_or(0.0);
        let pt = provisional
            .get(&(edge_id.clone(), false))
            .copied()
            .unwrap_or(0.0);
        lanes.insert(edge_id.clone(), 0.5 * (ps + pt));
    }

    // Enforce pitch within each twin group (keep Compose order → increasing x).
    for g in &groups {
        if g.edges.len() < 2 {
            continue;
        }
        let e0 = &g.edges[0];
        let e1 = &g.edges[1];
        let x0 = lanes[e0];
        let x1 = lanes[e1];
        let mid = 0.5 * (x0 + x1);
        let (left, right) = if x1 - x0 >= port_pitch {
            (x0.min(x1), x0.max(x1))
        } else {
            (mid - port_pitch / 2.0, mid + port_pitch / 2.0)
        };
        // Smaller Compose order stays on the left column.
        lanes.insert(e0.clone(), left);
        lanes.insert(e1.clone(), right);
    }

    // Clip each twin lane into the intersection of both endpoint frames.
    for edge_id in &corridor_edges {
        let Some((src, tgt)) = endpoint_elems(plan, graph, &edge_of, edge_id) else {
            continue;
        };
        let lo = frames[src].x.max(frames[tgt].x) + PORT_MARGIN;
        let hi = frames[src].right().min(frames[tgt].right()) - PORT_MARGIN;
        if let Some(lane) = lanes.get_mut(edge_id) {
            if hi >= lo {
                *lane = lane.clamp(lo, hi);
            }
        }
    }

    // Face-level LocalOffset for twin-touched faces.
    for &face in &touched {
        let Some(ends) = face_ends.get(&face) else {
            continue;
        };
        if ends.is_empty() {
            continue;
        }
        let frame = &frames[face.0];
        let side = face.1;
        let abs_x = pack_face_absolute(ends, &corridor_edges, &lanes, frame);

        for (i, end) in ends.iter().enumerate() {
            let along = local_on_side(frame, side, abs_x[i]);
            let Some(ep) = ports.get_mut(&end.edge_id) else {
                continue;
            };
            if end.is_source {
                ep.source.along = along;
            } else {
                ep.target.along = along;
            }
        }
    }

    align_ns_ports(plan, graph, ports, frames, port_pitch, &touched);
}

/// Slide every Ordered N/S port inside its own face toward the column its
/// partner end already occupies.
///
/// Node centers are owned by the cross-axis solve and are frozen by the time we
/// get here; what is left is a sub-node-width misalignment that costs a jog at
/// each end. Twin faces keep Compose order (expectations §6.2). On other
/// shared faces the along order follows the partner column — dummy trunk, or
/// the peer node's center — because Compose's far-real `layer_order` is a
/// raw index and incommensurable across layers (mech n11: e29 leftmost on a
/// 2-node rank, reverse vertical then crossed by every left-going out-edge).
/// PAVA still enforces pitch and face bounds in that order. An end with
/// nothing to align to keeps its canonical even slot.
///
/// Faces the twin-corridor pass already wrote are skipped whole: a shared lane
/// is a stronger statement than per-edge alignment, and re-projecting the face
/// would let a neighbouring end push the twin columns apart again.
fn align_ns_ports(
    plan: &PlanGraph,
    graph: &RealGraph,
    ports: &mut BTreeMap<String, EdgePorts>,
    frames: &[Rect],
    pitch: f64,
    corridor_faces: &BTreeSet<(usize, Side)>,
) {
    let all_faces: BTreeMap<(usize, Side), Vec<FaceEnd>> = build_face_ends(plan, graph, ports)
        .into_iter()
        .filter(|(face, _)| is_ns(face.1))
        .collect();
    // Corridor faces still contribute their columns as fixed targets — a twin
    // lane is exactly the kind of settled column a free end wants to meet.
    //
    // A face carrying a single end is left alone: that port's column *is* the
    // node's column, which belongs to the cross-axis solve. Sliding it would
    // put the arrow head in a corner of the box to save a bend. Only a face
    // that is already shared — where the slots are arbitrary to begin with —
    // is PortLane's to redistribute.
    //
    // The unit is the Compose slot, not the end: a bundled fan shares one
    // `Ordered` slot and therefore one port point, and must move as one.
    let mut faces: BTreeMap<(usize, Side), Vec<Vec<FaceEnd>>> = all_faces
        .iter()
        .filter(|(face, _)| !corridor_faces.contains(face))
        .map(|(face, ends)| (*face, slot_groups(ends)))
        .filter(|(_, slots)| slots.len() >= 2)
        .collect();
    if faces.is_empty() {
        return;
    }

    let anchors = partner_anchors(plan, graph, ports, frames);
    for slots in faces.values_mut() {
        slots.sort_by(|a, b| {
            slot_partner_column(a, &anchors, plan, graph, frames)
                .total_cmp(&slot_partner_column(b, &anchors, plan, graph, frames))
                .then(a[0].edge_id.cmp(&b[0].edge_id))
        });
    }

    let frozen: BTreeSet<(String, bool)> = all_faces
        .iter()
        .filter(|(face, _)| !faces.contains_key(face))
        .flat_map(|(_, ends)| ends.iter().map(|e| (e.edge_id.clone(), e.is_source)))
        .collect();

    let mut at: BTreeMap<(String, bool), f64> = BTreeMap::new();
    for (&(elem, _), ends) in &all_faces {
        for end in ends {
            let Some(ep) = ports.get(&end.edge_id) else {
                continue;
            };
            let port = if end.is_source { ep.source } else { ep.target };
            at.insert(
                (end.edge_id.clone(), end.is_source),
                port_anchor(frames[elem], port).x,
            );
        }
    }

    // One projection is not a fixed point: faces couple through their edges, so
    // a face that moves re-targets its neighbours. Each round is a contraction;
    // run to a fixed point, because a residue of even half a pixel still costs
    // the edge two bends.
    for _ in 0..PORT_ALIGN_ROUNDS {
        let prev = at.clone();
        for (&(elem, _), slots) in &faces {
            let frame = &frames[elem];
            let targets: Vec<f64> = slots
                .iter()
                .map(|slot| {
                    let wants: Vec<f64> = slot
                        .iter()
                        .map(|end| {
                            let key = (end.edge_id.clone(), end.is_source);
                            let here = prev[&key];
                            match anchors.get(&key) {
                                Some(PartnerAnchor::Fixed(x)) => *x,
                                Some(PartnerAnchor::Peer(peer)) => match prev.get(peer) {
                                    // A frozen peer will not come to meet us, so
                                    // go all the way; otherwise both ends aim at
                                    // the midpoint — chasing the partner's last
                                    // position just swaps them.
                                    Some(&x) if frozen.contains(peer) => x,
                                    Some(&x) => 0.5 * (here + x),
                                    None => here,
                                },
                                None => here,
                            }
                        })
                        .collect();
                    median(&wants)
                })
                .collect();
            let placed = project_ordered(&targets, pitch, frame.x, frame.right());
            for (slot, x) in slots.iter().zip(placed) {
                for end in slot {
                    at.insert((end.edge_id.clone(), end.is_source), x);
                }
            }
        }
        let moved = at
            .iter()
            .map(|(k, x)| (x - prev[k]).abs())
            .fold(0.0f64, f64::max);
        if moved < PORT_ALIGN_EPS {
            break;
        }
    }

    for (&(elem, side), slots) in &faces {
        for end in slots.iter().flatten() {
            let Some(&x) = at.get(&(end.edge_id.clone(), end.is_source)) else {
                continue;
            };
            let along = local_on_side(&frames[elem], side, x);
            let Some(ep) = ports.get_mut(&end.edge_id) else {
                continue;
            };
            if end.is_source {
                ep.source.along = along;
            } else {
                ep.target.along = along;
            }
        }
    }
}

/// Split a face's ends (already in Compose order) into slots: ends sharing an
/// `Ordered` index share one port point and stay together.
fn slot_groups(ends: &[FaceEnd]) -> Vec<Vec<FaceEnd>> {
    let mut out: Vec<Vec<FaceEnd>> = Vec::new();
    for end in ends {
        match out.last_mut() {
            Some(slot) if slot[0].order == end.order => slot.push(end.clone()),
            _ => out.push(vec![end.clone()]),
        }
    }
    out
}

/// Column the slot should sit on: dummy trunk if the edge has one at this
/// end, otherwise the partner node's center. Not the far-real's layer index —
/// that is incommensurable across ranks.
fn slot_partner_column(
    slot: &[FaceEnd],
    anchors: &BTreeMap<(String, bool), PartnerAnchor>,
    plan: &PlanGraph,
    graph: &RealGraph,
    frames: &[Rect],
) -> f64 {
    let xs: Vec<f64> = slot
        .iter()
        .map(|end| match anchors.get(&(end.edge_id.clone(), end.is_source)) {
            Some(PartnerAnchor::Fixed(x)) => *x,
            _ => partner_node_center(end, plan, graph, frames).unwrap_or(0.0),
        })
        .collect();
    median(&xs)
}

fn partner_node_center(
    end: &FaceEnd,
    plan: &PlanGraph,
    graph: &RealGraph,
    frames: &[Rect],
) -> Option<f64> {
    let e = graph.edges.iter().find(|e| e.edge_id == end.edge_id)?;
    let peer = if end.is_source {
        e.original_target
    } else {
        e.original_source
    };
    let elem = *plan
        .index_of
        .get(&ElemKey::Real(graph.ids[peer].clone()))?;
    Some(frames[elem].x + frames[elem].width / 2.0)
}

/// What an end aligns to: a frozen column (its long edge's dummy trunk) or the
/// other end of a rank-adjacent edge, which is still moving.
enum PartnerAnchor {
    Fixed(f64),
    Peer((String, bool)),
}

fn partner_anchors(
    plan: &PlanGraph,
    graph: &RealGraph,
    ports: &BTreeMap<String, EdgePorts>,
    frames: &[Rect],
) -> BTreeMap<(String, bool), PartnerAnchor> {
    let segs_by_edge = plan.segments_by_edge();
    let mut out = BTreeMap::new();
    for e in &graph.edges {
        let Some(ep) = ports.get(&e.edge_id) else {
            continue;
        };
        let Some((src, tgt)) = endpoint_elems_direct(plan, graph, e) else {
            continue;
        };
        if src == tgt {
            continue;
        }
        let Some(segs) = segs_by_edge.get(&e.edge_id) else {
            continue;
        };
        for (elem, is_source, peer_side) in
            [(src, true, ep.target.side), (tgt, false, ep.source.side)]
        {
            // The trunk column wins when the edge has one: it is already placed
            // and every other end on this face is negotiating against it.
            let trunk = segs
                .iter()
                .map(|&i| &plan.segments[i])
                .find(|s| s.from == elem || s.to == elem)
                .map(|s| if s.from == elem { s.to } else { s.from })
                .filter(|&nb| plan.elems[nb].key.is_virtual())
                .map(|nb| frames[nb].x + frames[nb].width / 2.0);
            let anchor = match trunk {
                Some(x) => PartnerAnchor::Fixed(x),
                None if is_ns(peer_side) => PartnerAnchor::Peer((e.edge_id.clone(), !is_source)),
                None => continue,
            };
            out.insert((e.edge_id.clone(), is_source), anchor);
        }
    }
    out
}

fn endpoint_elems_direct(
    plan: &PlanGraph,
    graph: &RealGraph,
    edge: &RealEdge,
) -> Option<(usize, usize)> {
    let src = *plan
        .index_of
        .get(&ElemKey::Real(graph.ids[edge.original_source].clone()))?;
    let tgt = *plan
        .index_of
        .get(&ElemKey::Real(graph.ids[edge.original_target].clone()))?;
    Some((src, tgt))
}

/// L1 projection of `targets` onto `x[i] + pitch ≤ x[i+1]` inside `[lo, hi]` —
/// pool-adjacent-violators on the pitch-shifted sequence.
///
/// The pooled value is the median, not the mean: when two ends of one face
/// cannot both reach their partner, least squares splits the difference and
/// leaves *both* edges with a sub-pixel jog, which still costs two bends each.
/// The median hands one of them exact alignment.
fn project_ordered(targets: &[f64], pitch: f64, lo: f64, hi: f64) -> Vec<f64> {
    let n = targets.len();
    if n == 0 {
        return Vec::new();
    }
    if (n as f64 - 1.0) * pitch > hi - lo {
        // No room for the pitch: fall back to the canonical even slots.
        return (0..n)
            .map(|i| lo + (i as f64 + 1.0) / (n as f64 + 1.0) * (hi - lo))
            .collect();
    }

    let mut blocks: Vec<Vec<f64>> = Vec::with_capacity(n);
    for (i, &t) in targets.iter().enumerate() {
        blocks.push(vec![t.clamp(lo, hi) - i as f64 * pitch]);
        while blocks.len() >= 2 {
            let last = median(&blocks[blocks.len() - 1]);
            let prev = median(&blocks[blocks.len() - 2]);
            if prev <= last {
                break;
            }
            let merged = blocks.pop().unwrap();
            blocks.last_mut().unwrap().extend(merged);
        }
    }

    let mut out: Vec<f64> = Vec::with_capacity(n);
    for block in &blocks {
        let v = median(block);
        for _ in 0..block.len() {
            out.push(v + out.len() as f64 * pitch);
        }
    }
    // The pooled run is rigid, so fitting it inside the face is a plain shift.
    let shift = (lo - out[0]).max(0.0) + (hi - out[n - 1]).min(0.0);
    out.iter().map(|x| x + shift).collect()
}

/// Lower median — deterministic, and it lands on an actual target.
fn median(values: &[f64]) -> f64 {
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    v[(v.len() - 1) / 2]
}

fn pack_face_absolute(
    ends: &[FaceEnd],
    corridor_edges: &BTreeSet<String>,
    lanes: &BTreeMap<String, f64>,
    frame: &Rect,
) -> Vec<f64> {
    let n = ends.len();
    let mut abs_x = vec![0.0; n];
    let face_lo = frame.x + PORT_MARGIN;
    let face_hi = frame.right() - PORT_MARGIN;

    let mut corr_idx = Vec::new();
    for (i, end) in ends.iter().enumerate() {
        if corridor_edges.contains(&end.edge_id) {
            let lane = lanes
                .get(&end.edge_id)
                .copied()
                .unwrap_or(center_x(frame))
                .clamp(face_lo, face_hi);
            abs_x[i] = lane;
            corr_idx.push(i);
        }
    }

    if corr_idx.is_empty() {
        // Should not happen on touched faces; fall back to Ordered-isomorphic.
        for (i, _) in ends.iter().enumerate() {
            let t = (i as f64 + 1.0) / (n as f64 + 1.0);
            abs_x[i] = face_lo + t * (face_hi - face_lo).max(0.0);
        }
        return abs_x;
    }

    let i_min = *corr_idx.iter().min().unwrap();
    let block_lo = corr_idx
        .iter()
        .map(|&i| abs_x[i])
        .fold(f64::INFINITY, f64::min);
    let block_hi = corr_idx
        .iter()
        .map(|&i| abs_x[i])
        .fold(f64::NEG_INFINITY, f64::max);

    let mut left = Vec::new();
    let mut right = Vec::new();
    for i in 0..n {
        if corridor_edges.contains(&ends[i].edge_id) {
            continue;
        }
        if i < i_min {
            left.push(i);
        } else {
            // Outside or sandwiched in Compose order → right of twin block.
            right.push(i);
        }
    }

    space_evenly(&mut abs_x, &left, face_lo, block_lo);
    space_evenly(&mut abs_x, &right, block_hi, face_hi);
    abs_x
}

/// Place `indices` strictly inside `(lo, hi)` with Ordered-isomorphic spacing.
fn space_evenly(abs_x: &mut [f64], indices: &[usize], lo: f64, hi: f64) {
    let n = indices.len();
    if n == 0 {
        return;
    }
    let span = (hi - lo).max(0.0);
    if span <= 1e-12 {
        let x = 0.5 * (lo + hi);
        for &i in indices {
            abs_x[i] = x;
        }
        return;
    }
    for (j, &i) in indices.iter().enumerate() {
        let t = (j as f64 + 1.0) / (n as f64 + 1.0);
        abs_x[i] = lo + t * span;
    }
}

fn collect_corridor_groups(
    plan: &PlanGraph,
    graph: &RealGraph,
    ports: &BTreeMap<String, EdgePorts>,
) -> Vec<CorridorGroup> {
    let twin_pairs = twin_undirected_pairs(graph);
    let mut groups = Vec::new();
    for &(a, b) in &twin_pairs {
        let Some(ea) = plan
            .index_of
            .get(&ElemKey::Real(graph.ids[a].clone()))
            .copied()
        else {
            continue;
        };
        let Some(eb) = plan
            .index_of
            .get(&ElemKey::Real(graph.ids[b].clone()))
            .copied()
        else {
            continue;
        };
        let span = (plan.elems[ea].rank as usize).abs_diff(plan.elems[eb].rank as usize);
        if span != 1 {
            continue;
        }

        let mut corridor: Vec<(String, u32)> = Vec::new();
        for e in &graph.edges {
            let pair = undirected_pair(e.original_source, e.original_target);
            if pair != (a, b) {
                continue;
            }
            let Some(ep) = ports.get(&e.edge_id) else {
                continue;
            };
            if !is_ns(ep.source.side) || !is_ns(ep.target.side) {
                continue;
            }
            let order = corridor_order(ep);
            corridor.push((e.edge_id.clone(), order));
        }
        if corridor.len() < 2 {
            continue;
        }
        corridor.sort_by(|x, y| x.1.cmp(&y.1).then(x.0.cmp(&y.0)));
        groups.push(CorridorGroup {
            edges: corridor.into_iter().map(|(id, _)| id).collect(),
        });
    }
    groups
}

fn build_face_ends(
    plan: &PlanGraph,
    graph: &RealGraph,
    ports: &BTreeMap<String, EdgePorts>,
) -> BTreeMap<(usize, Side), Vec<FaceEnd>> {
    let mut faces: BTreeMap<(usize, Side), Vec<FaceEnd>> = BTreeMap::new();
    for e in &graph.edges {
        let Some(ep) = ports.get(&e.edge_id) else {
            continue;
        };
        let Some(&src) = plan
            .index_of
            .get(&ElemKey::Real(graph.ids[e.original_source].clone()))
        else {
            continue;
        };
        let Some(&tgt) = plan
            .index_of
            .get(&ElemKey::Real(graph.ids[e.original_target].clone()))
        else {
            continue;
        };
        faces
            .entry((src, ep.source.side))
            .or_default()
            .push(FaceEnd {
                edge_id: e.edge_id.clone(),
                is_source: true,
                order: ordered_order(ep.source.along),
            });
        faces
            .entry((tgt, ep.target.side))
            .or_default()
            .push(FaceEnd {
                edge_id: e.edge_id.clone(),
                is_source: false,
                order: ordered_order(ep.target.along),
            });
    }
    for ends in faces.values_mut() {
        ends.sort_by(|a, b| a.order.cmp(&b.order).then(a.edge_id.cmp(&b.edge_id)));
    }
    faces
}

fn endpoint_elems(
    plan: &PlanGraph,
    graph: &RealGraph,
    edge_of: &BTreeMap<String, usize>,
    edge_id: &str,
) -> Option<(usize, usize)> {
    let &ei = edge_of.get(edge_id)?;
    let edge = &graph.edges[ei];
    let src = *plan
        .index_of
        .get(&ElemKey::Real(graph.ids[edge.original_source].clone()))?;
    let tgt = *plan
        .index_of
        .get(&ElemKey::Real(graph.ids[edge.original_target].clone()))?;
    Some((src, tgt))
}

fn twin_undirected_pairs(graph: &RealGraph) -> BTreeSet<(usize, usize)> {
    let mut forward = BTreeSet::new();
    let mut reversed = BTreeSet::new();
    for e in &graph.edges {
        let p = undirected_pair(e.original_source, e.original_target);
        if e.reversed {
            reversed.insert(p);
        } else {
            forward.insert(p);
        }
    }
    forward.intersection(&reversed).copied().collect()
}

fn undirected_pair(a: usize, b: usize) -> (usize, usize) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

fn is_ns(side: Side) -> bool {
    matches!(side, Side::North | Side::South)
}

fn corridor_order(ep: &EdgePorts) -> u32 {
    ordered_order(ep.source.along).min(ordered_order(ep.target.along))
}

fn ordered_order(along: AlongSpec) -> u32 {
    match along {
        AlongSpec::Ordered { order, .. } => order,
        AlongSpec::LocalOffset(_) => u32::MAX,
    }
}

fn center_x(frame: &Rect) -> f64 {
    frame.x + frame.width / 2.0
}

fn local_on_side(frame: &Rect, side: Side, lane_x: f64) -> AlongSpec {
    let x = lane_x.clamp(frame.x + PORT_MARGIN, frame.right() - PORT_MARGIN);
    let y = match side {
        Side::North => 0.0,
        Side::South => frame.height,
        Side::West | Side::East => frame.height / 2.0,
    };
    AlongSpec::LocalOffset(Point { x: x - frame.x, y })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::hierarchical::compose::ports::ResolvedPort;
    use crate::layout::hierarchical::metric::anchor::port_anchor;
    use crate::layout::hierarchical::model::{Elem, RealEdge};

    fn real(id: &str, rank: u32) -> Elem {
        Elem {
            key: ElemKey::Real(id.into()),
            group_path: Vec::new(),
            rank,
        }
    }

    fn graph_and_plan() -> (RealGraph, PlanGraph, BTreeMap<String, EdgePorts>, Vec<Rect>) {
        // client=0 (narrow), api=1 (wide); forward + reverse twin.
        let elems = vec![real("client", 0), real("api", 1)];
        let index_of = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let plan = PlanGraph {
            elems,
            index_of,
            decl_index: vec![0, 1],
            segments: vec![crate::layout::hierarchical::model::Segment {
                edge_id: "fwd".into(),
                ordinal: 0,
                from: 0,
                to: 1,
            }],
            layers: vec![vec![0], vec![1]],
            ..Default::default()
        };
        let graph = RealGraph {
            ids: vec!["client".into(), "api".into()],
            index_of: [("client".into(), 0usize), ("api".into(), 1)]
                .into_iter()
                .collect(),
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
                    weight: 1.0,
                    ..Default::default()
                },
                RealEdge {
                    edge_id: "rev".into(),
                    original_source: 1,
                    original_target: 0,
                    working_source: 0,
                    working_target: 1,
                    reversed: true,
                    from_port: None,
                    to_port: None,
                    weight: 1.0,
                    ..Default::default()
                },
            ],
            self_loops: Vec::new(),
            ..Default::default()
        };
        let mut ports = BTreeMap::new();
        ports.insert(
            "fwd".into(),
            EdgePorts {
                source: ResolvedPort {
                    side: Side::South,
                    along: AlongSpec::Ordered { order: 0, count: 2 },
                },
                target: ResolvedPort {
                    side: Side::North,
                    along: AlongSpec::Ordered { order: 0, count: 2 },
                },
                source_cluster: None,
                target_cluster: None,
            },
        );
        ports.insert(
            "rev".into(),
            EdgePorts {
                source: ResolvedPort {
                    side: Side::North,
                    along: AlongSpec::Ordered { order: 1, count: 2 },
                },
                target: ResolvedPort {
                    side: Side::South,
                    along: AlongSpec::Ordered { order: 1, count: 2 },
                },
                source_cluster: None,
                target_cluster: None,
            },
        );
        // Centers both at 50; widths 40 vs 80 → Ordered would diverge.
        let frames = vec![
            Rect::new(30.0, 0.0, 40.0, 20.0),
            Rect::new(10.0, 40.0, 80.0, 20.0),
        ];
        (graph, plan, ports, frames)
    }

    #[test]
    fn twin_unequal_width_ports_share_lane_x() {
        let (graph, plan, mut ports, frames) = graph_and_plan();
        let pitch = 12.0;
        apply_port_lanes(&plan, &graph, &mut ports, &frames, pitch);

        let fwd = &ports["fwd"];
        let rev = &ports["rev"];
        let fwd_sx = port_anchor(frames[0], fwd.source).x;
        let fwd_tx = port_anchor(frames[1], fwd.target).x;
        let rev_sx = port_anchor(frames[1], rev.source).x;
        let rev_tx = port_anchor(frames[0], rev.target).x;

        assert!(
            (fwd_sx - fwd_tx).abs() < 1e-9,
            "forward ends must share x: {fwd_sx} vs {fwd_tx}"
        );
        assert!(
            (rev_sx - rev_tx).abs() < 1e-9,
            "reverse ends must share x: {rev_sx} vs {rev_tx}"
        );
        // Compose order: fwd left of rev; pitch floor.
        assert!(
            fwd_sx < rev_sx,
            "corridor order must stay left→right: {fwd_sx} vs {rev_sx}"
        );
        assert!(
            (rev_sx - fwd_sx) + 1e-9 >= pitch,
            "corridor separation must be ≥ pitch: {}",
            rev_sx - fwd_sx
        );
    }

    /// Hub South: twin×2 + outer leaf — leaf must not sit between twin lanes.
    #[test]
    fn face_with_outer_leaf_keeps_compose_order() {
        let elems = vec![real("hub", 0), real("twin", 1), real("leaf", 1)];
        let index_of = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let plan = PlanGraph {
            elems,
            index_of,
            decl_index: vec![0, 1, 2],
            segments: vec![],
            layers: vec![vec![0], vec![1, 2]],
            ..Default::default()
        };
        let graph = RealGraph {
            ids: vec!["hub".into(), "twin".into(), "leaf".into()],
            index_of: [
                ("hub".into(), 0usize),
                ("twin".into(), 1),
                ("leaf".into(), 2),
            ]
            .into_iter()
            .collect(),
            group_path: vec![Vec::new(); 3],
            shapes: vec![plotgram_model::NodeShape::DEFAULT; 3],
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
                    weight: 1.0,
                    ..Default::default()
                },
                RealEdge {
                    edge_id: "rev".into(),
                    original_source: 1,
                    original_target: 0,
                    working_source: 0,
                    working_target: 1,
                    reversed: true,
                    from_port: None,
                    to_port: None,
                    weight: 1.0,
                    ..Default::default()
                },
                RealEdge {
                    edge_id: "out".into(),
                    original_source: 0,
                    original_target: 2,
                    working_source: 0,
                    working_target: 2,
                    reversed: false,
                    from_port: None,
                    to_port: None,
                    weight: 1.0,
                    ..Default::default()
                },
            ],
            self_loops: Vec::new(),
            ..Default::default()
        };
        let mut ports = BTreeMap::new();
        ports.insert(
            "fwd".into(),
            EdgePorts {
                source: ResolvedPort {
                    side: Side::South,
                    along: AlongSpec::Ordered { order: 0, count: 3 },
                },
                target: ResolvedPort {
                    side: Side::North,
                    along: AlongSpec::Ordered { order: 0, count: 2 },
                },
                source_cluster: None,
                target_cluster: None,
            },
        );
        ports.insert(
            "rev".into(),
            EdgePorts {
                source: ResolvedPort {
                    side: Side::North,
                    along: AlongSpec::Ordered { order: 1, count: 2 },
                },
                target: ResolvedPort {
                    side: Side::South,
                    along: AlongSpec::Ordered { order: 1, count: 3 },
                },
                source_cluster: None,
                target_cluster: None,
            },
        );
        ports.insert(
            "out".into(),
            EdgePorts {
                source: ResolvedPort {
                    side: Side::South,
                    along: AlongSpec::Ordered { order: 2, count: 3 },
                },
                target: ResolvedPort {
                    side: Side::North,
                    along: AlongSpec::Ordered { order: 0, count: 1 },
                },
                source_cluster: None,
                target_cluster: None,
            },
        );
        // Hub centered; twin and leaf below.
        let frames = vec![
            Rect::new(0.0, 0.0, 100.0, 20.0),
            Rect::new(20.0, 40.0, 40.0, 20.0),
            Rect::new(80.0, 40.0, 40.0, 20.0),
        ];
        apply_port_lanes(&plan, &graph, &mut ports, &frames, 12.0);

        let hub = frames[0];
        let fwd_x = port_anchor(hub, ports["fwd"].source).x;
        let rev_x = port_anchor(hub, ports["rev"].target).x;
        let out_x = port_anchor(hub, ports["out"].source).x;
        let twin_lo = fwd_x.min(rev_x);
        let twin_hi = fwd_x.max(rev_x);
        assert!(
            out_x > twin_hi + 1e-6,
            "outer leaf must sit right of twin block: out={out_x}, twin=[{twin_lo},{twin_hi}]"
        );
        assert!(
            (port_anchor(frames[1], ports["fwd"].target).x - fwd_x).abs() < 1e-9,
            "fwd must stay aligned"
        );
        assert!(
            (port_anchor(frames[1], ports["rev"].source).x - rev_x).abs() < 1e-9,
            "rev must stay aligned"
        );
    }
}
