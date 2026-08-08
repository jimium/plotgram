//! SymmetryAxisWriter — explicit Metric sub-step for spine∩fan co-linearity.
//!
//! Contract: [`docs/design/layout/hierarchical/phases/symmetry-axis.md`]
//!
//! After pass-1 (BK ideal + soft VPSC), this writer emits:
//! - [`SymmetryAxis`] per fan hub (inherit unique upstream axis when present;
//!   else odd/even formula from pass-1 neighbors)
//! - [`RigidColumnClass`] membership (hub + non-fan-side 1:1 chain; downward
//!   absorb of a unique child fan hub)
//! - [`FanPack`] leaf slots (`axis ± k·pitch`) covering BK ideal on pass-2
//!
//! Fan adjacency is **forward real endpoints** on [`RealGraph`] (non-reversed
//! edges): long edges still count as one hop, so a decision that spans ranks
//! via dummies remains a fan. Reversed back-edges are excluded so a return
//! path does not invent a spurious fan at the loop head.
//!
//! **Twin spine privilege**: a forward neighbor that also has a reversed edge
//! on the same undirected pair (req–resp / 2-cycle) joins the rigid column on
//! the axis instead of taking a mirrored FanPack slot — matching yFiles and
//! expectations §6 parallel fold-back (see `mech.constrain-sink`).
//!
//! Downstream constraints / desired must **only consume** these tables.

use std::collections::BTreeSet;

use plotgram_algo::orientation::Size;

use crate::layout::hierarchical::model::{ElemKey, PlanGraph, RealGraph};

/// One fan hub's cross-axis symmetry target (canonical TB).
#[derive(Debug, Clone, PartialEq)]
pub struct SymmetryAxis {
    pub hub: usize,
    pub coord: f64,
}

/// Elements that must share one cross-axis column with a fan axis.
#[derive(Debug, Clone, PartialEq)]
pub struct RigidColumnClass {
    pub axis_hub: usize,
    /// Sorted by rank, then elem index.
    pub members: Vec<usize>,
}

/// Absolute cross-axis desired for one fan leaf.
#[derive(Debug, Clone, PartialEq)]
pub struct FanPackSlot {
    pub elem: usize,
    pub desired: f64,
}

/// Fan leaves packed symmetrically about a hub's axis.
#[derive(Debug, Clone, PartialEq)]
pub struct FanPack {
    pub hub: usize,
    pub slots: Vec<FanPackSlot>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SymmetryPlan {
    pub axes: Vec<SymmetryAxis>,
    pub classes: Vec<RigidColumnClass>,
    pub packs: Vec<FanPack>,
}

impl SymmetryPlan {
    /// `Some(class_index)` if `elem` belongs to a rigid column class.
    pub fn class_of(&self, elem: usize) -> Option<usize> {
        self.classes.iter().position(|c| c.members.contains(&elem))
    }

    /// Axis coordinate for a class member, if any.
    pub fn axis_coord_for(&self, elem: usize) -> Option<f64> {
        let ci = self.class_of(elem)?;
        let hub = self.classes[ci].axis_hub;
        self.axes.iter().find(|a| a.hub == hub).map(|a| a.coord)
    }

    /// FanPack absolute desired for a leaf, if any.
    pub fn fan_desired_for(&self, elem: usize) -> Option<f64> {
        for pack in &self.packs {
            for slot in &pack.slots {
                if slot.elem == elem {
                    return Some(slot.desired);
                }
            }
        }
        None
    }
}

/// Forward (non-reversed) real–real adjacency: long edges count as one hop
/// between endpoints; dummies do not hide a fan.
pub fn forward_real_adjacency(
    plan: &PlanGraph,
    graph: &RealGraph,
) -> (Vec<Vec<usize>>, Vec<Vec<usize>>) {
    let n = plan.elems.len();
    let (mut down, mut up) = (vec![Vec::new(); n], vec![Vec::new(); n]);
    for e in &graph.edges {
        if e.reversed {
            continue;
        }
        let src_id = &graph.ids[e.working_source];
        let tgt_id = &graph.ids[e.working_target];
        let Some(&src) = plan.index_of.get(&ElemKey::Real(src_id.clone())) else {
            continue;
        };
        let Some(&tgt) = plan.index_of.get(&ElemKey::Real(tgt_id.clone())) else {
            continue;
        };
        down[src].push(tgt);
        up[tgt].push(src);
    }
    for v in &mut down {
        v.sort_unstable();
        v.dedup();
    }
    for v in &mut up {
        v.sort_unstable();
        v.dedup();
    }
    (down, up)
}

pub fn degrees_of(nbs: &[Vec<usize>]) -> Vec<usize> {
    nbs.iter().map(|v| v.len()).collect()
}

fn undirected_real_pair(a: usize, b: usize) -> (usize, usize) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Undirected original-endpoint pairs that carry both a forward and a reversed
/// edge (2-cycle / req–resp twin).
fn twin_real_pairs(graph: &RealGraph) -> BTreeSet<(usize, usize)> {
    let mut fwd = BTreeSet::new();
    let mut rev = BTreeSet::new();
    for e in &graph.edges {
        let p = undirected_real_pair(e.original_source, e.original_target);
        if e.reversed {
            rev.insert(p);
        } else {
            fwd.insert(p);
        }
    }
    fwd.into_iter().filter(|p| rev.contains(p)).collect()
}

/// Map twin pairs from RealGraph id indices → PlanGraph elem indices.
fn twin_plan_pairs(plan: &PlanGraph, graph: &RealGraph) -> BTreeSet<(usize, usize)> {
    let mut out = BTreeSet::new();
    for (a, b) in twin_real_pairs(graph) {
        let Some(&ea) = plan
            .index_of
            .get(&ElemKey::Real(graph.ids[a].clone()))
        else {
            continue;
        };
        let Some(&eb) = plan
            .index_of
            .get(&ElemKey::Real(graph.ids[b].clone()))
        else {
            continue;
        };
        out.insert(undirected_real_pair(ea, eb));
    }
    out
}

fn is_twin_peer(hub: usize, peer: usize, twins: &BTreeSet<(usize, usize)>) -> bool {
    twins.contains(&undirected_real_pair(hub, peer))
}

/// Unique min-span non-twin real neighbor on one side — primary arm on spine.
/// Tied minima → `None` (keep even/odd FanPack mirror).
fn unique_min_span_primary(
    hub: usize,
    neighbors: &[usize],
    plan: &PlanGraph,
    twins: &BTreeSet<(usize, usize)>,
) -> Option<usize> {
    let hub_rank = plan.elems[hub].rank as i64;
    let mut best_span: Option<i64> = None;
    let mut best: Option<usize> = None;
    let mut tied = false;
    for &nb in neighbors {
        if plan.elems[nb].key.is_virtual() || is_twin_peer(hub, nb, twins) {
            continue;
        }
        let span = (plan.elems[nb].rank as i64 - hub_rank).abs();
        match best_span {
            None => {
                best_span = Some(span);
                best = Some(nb);
                tied = false;
            }
            Some(s) if span < s => {
                best_span = Some(span);
                best = Some(nb);
                tied = false;
            }
            Some(s) if span == s => {
                tied = true;
            }
            _ => {}
        }
    }
    if tied {
        None
    } else {
        best
    }
}

/// Build symmetry axes, rigid column classes, and FanPack slots from pass-1.
pub fn compute_symmetry_plan(
    plan: &PlanGraph,
    graph: &RealGraph,
    pass1: &[f64],
    dummy_aligned: &[bool],
    size_of: &dyn Fn(usize) -> Size,
    node_gap: f64,
) -> SymmetryPlan {
    let n = plan.elems.len();
    let (down_nbs, up_nbs) = forward_real_adjacency(plan, graph);
    let down_deg = degrees_of(&down_nbs);
    let up_deg = degrees_of(&up_nbs);
    let twin_pairs = twin_plan_pairs(plan, graph);

    let mut axes = Vec::new();
    let mut classes = Vec::new();
    let mut packs = Vec::new();

    // Top-down: rank then elem — upstream axes exist before downstream hubs inherit.
    let mut hubs: Vec<usize> = (0..n)
        .filter(|&e| {
            !plan.elems[e].key.is_virtual() && (down_deg[e] >= 2 || up_deg[e] >= 2)
        })
        .collect();
    hubs.sort_by(|&a, &b| {
        plan.elems[a]
            .rank
            .cmp(&plan.elems[b].rank)
            .then(a.cmp(&b))
    });

    let mut claimed = vec![false; n];
    let mut fan_claimed = vec![false; n];

    for &hub in &hubs {
        let already_claimed = claimed[hub];

        let coord = inherited_axis_coord(hub, &up_nbs, &up_deg, &classes, &axes)
            .unwrap_or_else(|| {
                let mut centers = Vec::new();
                if down_deg[hub] >= 2 {
                    centers.push(axis_coord_for_side(
                        hub,
                        &down_nbs[hub],
                        pass1,
                        &twin_pairs,
                    ));
                }
                if up_deg[hub] >= 2 {
                    centers.push(axis_coord_for_side(
                        hub,
                        &up_nbs[hub],
                        pass1,
                        &twin_pairs,
                    ));
                }
                if centers.is_empty() {
                    pass1[hub]
                } else {
                    centers.iter().sum::<f64>() / centers.len() as f64
                }
            });
        axes.push(SymmetryAxis { hub, coord });

        if !already_claimed {
            let mut members = vec![hub];
            // Non-fan side walks: fan below → walk up; fan above → walk down
            // (downward may absorb a unique child fan hub onto this spine).
            if down_deg[hub] >= 2 {
                walk_chain(
                    hub,
                    /*toward_up=*/ true,
                    /*absorb_unique_child_fan=*/ false,
                    plan,
                    &down_nbs,
                    &up_nbs,
                    &down_deg,
                    &up_deg,
                    dummy_aligned,
                    &mut members,
                );
            }
            if up_deg[hub] >= 2 {
                walk_chain(
                    hub,
                    /*toward_up=*/ false,
                    /*absorb_unique_child_fan=*/ true,
                    plan,
                    &down_nbs,
                    &up_nbs,
                    &down_deg,
                    &up_deg,
                    dummy_aligned,
                    &mut members,
                );
            }
            // Twin forward peers sit on the spine (not FanPack-mirrored).
            let mut twin_cands: Vec<usize> = down_nbs[hub]
                .iter()
                .chain(up_nbs[hub].iter())
                .copied()
                .filter(|&nb| {
                    !plan.elems[nb].key.is_virtual() && is_twin_peer(hub, nb, &twin_pairs)
                })
                .collect();
            twin_cands.sort_by(|&a, &b| {
                let da = (pass1[a] - pass1[hub]).abs();
                let db = (pass1[b] - pass1[hub]).abs();
                da.partial_cmp(&db)
                    .unwrap()
                    .then(compose_order_key(plan, a).cmp(&compose_order_key(plan, b)))
            });
            twin_cands.dedup();
            for nb in twin_cands {
                let r = plan.elems[nb].rank;
                if members
                    .iter()
                    .any(|&m| m != hub && plan.elems[m].rank == r)
                {
                    continue;
                }
                members.push(nb);
            }

            // Primary arm: unique min-span non-twin on a *fan* side joins the
            // spine (yFiles: adjacent "yes" on axis, long "no" aside). Do not
            // promote the unique 1:1 back-neighbor (would undo dummy-aligned
            // / chain truncation).
            let mut primary_cands = Vec::new();
            if down_deg[hub] >= 2 {
                if let Some(nb) =
                    unique_min_span_primary(hub, &down_nbs[hub], plan, &twin_pairs)
                {
                    primary_cands.push(nb);
                }
            }
            if up_deg[hub] >= 2 {
                if let Some(nb) =
                    unique_min_span_primary(hub, &up_nbs[hub], plan, &twin_pairs)
                {
                    primary_cands.push(nb);
                }
            }
            for nb in primary_cands {
                let r = plan.elems[nb].rank;
                if members
                    .iter()
                    .any(|&m| m != hub && plan.elems[m].rank == r)
                {
                    continue;
                }
                if members.contains(&nb) {
                    continue;
                }
                members.push(nb);
            }

            members.retain(|&e| e == hub || !claimed[e]);
            for &e in &members {
                claimed[e] = true;
            }
            members.sort_by(|&a, &b| {
                plan.elems[a]
                    .rank
                    .cmp(&plan.elems[b].rank)
                    .then(a.cmp(&b))
            });
            members.dedup();
            classes.push(RigidColumnClass {
                axis_hub: hub,
                members,
            });
        }

        // FanPack even when hub was absorbed into an upstream class.
        let mut slots = Vec::new();
        if down_deg[hub] >= 2 {
            append_fan_slots(
                plan,
                hub,
                &down_nbs[hub],
                /*toward_down=*/ true,
                coord,
                pass1,
                size_of,
                node_gap,
                &down_nbs,
                &up_nbs,
                &down_deg,
                &up_deg,
                &claimed,
                &mut fan_claimed,
                &mut slots,
            );
        }
        if up_deg[hub] >= 2 {
            append_fan_slots(
                plan,
                hub,
                &up_nbs[hub],
                /*toward_down=*/ false,
                coord,
                pass1,
                size_of,
                node_gap,
                &down_nbs,
                &up_nbs,
                &down_deg,
                &up_deg,
                &claimed,
                &mut fan_claimed,
                &mut slots,
            );
        }
        if !slots.is_empty() {
            slots.sort_by_key(|s| s.elem);
            packs.push(FanPack { hub, slots });
        }
    }

    SymmetryPlan {
        axes,
        classes,
        packs,
    }
}

/// Inherit spine axis when this hub has a unique upstream real with a known axis.
fn inherited_axis_coord(
    hub: usize,
    up_nbs: &[Vec<usize>],
    up_deg: &[usize],
    classes: &[RigidColumnClass],
    axes: &[SymmetryAxis],
) -> Option<f64> {
    if up_deg.get(hub).copied().unwrap_or(0) != 1 {
        return None;
    }
    let u = *up_nbs.get(hub)?.first()?;
    if let Some(c) = classes.iter().find(|c| c.members.contains(&u)) {
        return axes.iter().find(|a| a.hub == c.axis_hub).map(|a| a.coord);
    }
    axes.iter().find(|a| a.hub == u).map(|a| a.coord)
}

/// With a twin on this side, keep the hub's pass-1 column (spine); otherwise
/// odd/even neighbor formula.
fn axis_coord_for_side(
    hub: usize,
    neighbors: &[usize],
    pass1: &[f64],
    twins: &BTreeSet<(usize, usize)>,
) -> f64 {
    if neighbors
        .iter()
        .any(|&nb| is_twin_peer(hub, nb, twins))
    {
        pass1[hub]
    } else {
        axis_from_neighbors(neighbors, pass1)
    }
}

/// Odd → median neighbor center; even → midpoint of two middle neighbors.
pub fn axis_from_neighbors(neighbors: &[usize], pass1: &[f64]) -> f64 {
    let mut nbs: Vec<usize> = neighbors.to_vec();
    nbs.sort_by(|&a, &b| {
        pass1[a]
            .partial_cmp(&pass1[b])
            .unwrap()
            .then(a.cmp(&b))
    });
    let m = nbs.len() / 2;
    if nbs.len() % 2 == 1 {
        pass1[nbs[m]]
    } else {
        (pass1[nbs[m - 1]] + pass1[nbs[m]]) / 2.0
    }
}

fn is_fan(e: usize, down_deg: &[usize], up_deg: &[usize]) -> bool {
    down_deg[e] >= 2 || up_deg[e] >= 2
}

/// Walk exclusive 1:1 chain on the non-fan side of `start` (already in members).
///
/// When walking down (`toward_up == false`) with `absorb_unique_child_fan`, a
/// unique child that is itself a fan hub is included then the walk stops —
/// spine inheritance without pulling fan leaves into the class.
fn walk_chain(
    start: usize,
    toward_up: bool,
    absorb_unique_child_fan: bool,
    plan: &PlanGraph,
    down_nbs: &[Vec<usize>],
    up_nbs: &[Vec<usize>],
    down_deg: &[usize],
    up_deg: &[usize],
    dummy_aligned: &[bool],
    members: &mut Vec<usize>,
) {
    let mut cur = start;
    loop {
        let nbs = if toward_up {
            &up_nbs[cur]
        } else {
            &down_nbs[cur]
        };
        // Chain step: exactly one real neighbor in the walk direction.
        if nbs.len() != 1 {
            break;
        }
        let next = nbs[0];
        if plan.elems[next].key.is_virtual() {
            break;
        }
        if is_fan(next, down_deg, up_deg) {
            if absorb_unique_child_fan
                && !toward_up
                && down_deg[cur] == 1
                && up_deg[next] == 1
            {
                members.push(next);
            }
            break;
        }
        if dummy_aligned.get(next).copied().unwrap_or(false) {
            break;
        }
        // Next must remain chain-like: total real degree ≤ 2 (one back + one forward),
        // or degree 1 (leaf end of spine).
        let deg = down_deg[next] + up_deg[next];
        if deg > 2 {
            break;
        }
        members.push(next);
        cur = next;
    }
}

/// Compose layer order key: `(rank, index_in_layer, elem)`.
fn compose_order_key(plan: &PlanGraph, e: usize) -> (u32, usize, usize) {
    let rank = plan.elems[e].rank;
    let order = plan
        .layers
        .get(rank as usize)
        .and_then(|layer| layer.iter().position(|&x| x == e))
        .unwrap_or(usize::MAX);
    (rank, order, e)
}

/// Relative slot offsets: `i - (n-1)/2` in pitch units (odd median 0; even ±0.5…).
pub fn slot_multipliers(n: usize) -> Vec<f64> {
    if n == 0 {
        return Vec::new();
    }
    let center = (n as f64 - 1.0) / 2.0;
    (0..n).map(|i| i as f64 - center).collect()
}

/// Pitch large enough for layer sep and not tighter than pass-1 adjacent gaps.
pub fn fan_pitch(
    leaves: &[usize],
    pass1: &[f64],
    size_of: &dyn Fn(usize) -> Size,
    node_gap: f64,
) -> f64 {
    if leaves.len() < 2 {
        return node_gap;
    }
    let mut width_need = 0.0_f64;
    let mut pass1_gaps = Vec::with_capacity(leaves.len() - 1);
    for w in leaves.windows(2) {
        let (a, b) = (w[0], w[1]);
        width_need = width_need.max(node_gap + size_of(a).width / 2.0 + size_of(b).width / 2.0);
        pass1_gaps.push((pass1[b] - pass1[a]).abs());
    }
    pass1_gaps.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mid = pass1_gaps.len() / 2;
    let from_pass1 = if pass1_gaps.len() % 2 == 1 {
        pass1_gaps[mid]
    } else {
        (pass1_gaps[mid - 1] + pass1_gaps[mid]) / 2.0
    };
    width_need.max(from_pass1).max(node_gap)
}

fn append_fan_slots(
    plan: &PlanGraph,
    hub: usize,
    neighbors: &[usize],
    toward_down: bool,
    axis: f64,
    pass1: &[f64],
    size_of: &dyn Fn(usize) -> Size,
    node_gap: f64,
    down_nbs: &[Vec<usize>],
    up_nbs: &[Vec<usize>],
    down_deg: &[usize],
    up_deg: &[usize],
    class_claimed: &[bool],
    fan_claimed: &mut [bool],
    slots: &mut Vec<FanPackSlot>,
) {
    let mut leaves: Vec<usize> = neighbors
        .iter()
        .copied()
        .filter(|&e| {
            !plan.elems[e].key.is_virtual() && !class_claimed[e] && !fan_claimed[e]
        })
        .collect();
    if leaves.is_empty() {
        return;
    }
    leaves.sort_by(|&a, &b| compose_order_key(plan, a).cmp(&compose_order_key(plan, b)));

    // Twin peers already occupy the axis via RigidColumnClass. A single
    // remaining free leaf still packs off-axis (yFiles: spine twin + offset sink).
    if leaves.len() == 1 {
        let leaf = leaves[0];
        let pitch = (node_gap + size_of(hub).width / 2.0 + size_of(leaf).width / 2.0).max(node_gap);
        let sign = if pass1[leaf] + 1e-9 < axis {
            -1.0
        } else {
            1.0
        };
        fan_claimed[leaf] = true;
        let desired = axis + sign * pitch;
        slots.push(FanPackSlot {
            elem: leaf,
            desired,
        });
        append_leaf_followers(
            leaf,
            toward_down,
            desired,
            plan,
            down_nbs,
            up_nbs,
            down_deg,
            up_deg,
            class_claimed,
            fan_claimed,
            slots,
        );
        return;
    }

    let pitch = fan_pitch(&leaves, pass1, size_of, node_gap);
    let mults = slot_multipliers(leaves.len());
    for (i, &leaf) in leaves.iter().enumerate() {
        fan_claimed[leaf] = true;
        let desired = axis + mults[i] * pitch;
        slots.push(FanPackSlot {
            elem: leaf,
            desired,
        });
        // Exclusive 1:1 chain continuing away from the hub stays on the leaf column.
        append_leaf_followers(
            leaf,
            toward_down,
            desired,
            plan,
            down_nbs,
            up_nbs,
            down_deg,
            up_deg,
            class_claimed,
            fan_claimed,
            slots,
        );
    }
}

fn append_leaf_followers(
    start: usize,
    toward_down: bool,
    desired: f64,
    plan: &PlanGraph,
    down_nbs: &[Vec<usize>],
    up_nbs: &[Vec<usize>],
    down_deg: &[usize],
    up_deg: &[usize],
    class_claimed: &[bool],
    fan_claimed: &mut [bool],
    slots: &mut Vec<FanPackSlot>,
) {
    let mut cur = start;
    loop {
        let nbs = if toward_down {
            &down_nbs[cur]
        } else {
            &up_nbs[cur]
        };
        if nbs.len() != 1 {
            break;
        }
        let next = nbs[0];
        if plan.elems[next].key.is_virtual() {
            break;
        }
        if class_claimed[next] || fan_claimed[next] {
            break;
        }
        if is_fan(next, down_deg, up_deg) {
            break;
        }
        let deg = down_deg[next] + up_deg[next];
        if deg > 2 {
            break;
        }
        fan_claimed[next] = true;
        slots.push(FanPackSlot {
            elem: next,
            desired,
        });
        cur = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::hierarchical::model::{Elem, ElemKey, PlanGraph, RealEdge, Segment};

    fn real(id: &str, rank: u32) -> Elem {
        Elem {
            key: ElemKey::Real(id.into()),
            group_path: Vec::new(),
            rank,
        }
    }

    fn virt(edge_id: &str, ordinal: u32, rank: u32) -> Elem {
        Elem {
            key: ElemKey::Virtual {
                edge_id: edge_id.into(),
                ordinal,
            },
            group_path: Vec::new(),
            rank,
        }
    }

    fn seg(edge_id: &str, ordinal: u32, from: usize, to: usize) -> Segment {
        Segment {
            edge_id: edge_id.into(),
            ordinal,
            from,
            to,
        }
    }

    fn plan(elems: Vec<Elem>, layers: Vec<Vec<usize>>, segments: Vec<Segment>) -> PlanGraph {
        let index_of = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let decl_index: Vec<usize> = elems
            .iter()
            .enumerate()
            .filter_map(|(i, e)| match &e.key {
                ElemKey::Real(_) => Some(i),
                _ => None,
            })
            .collect();
        PlanGraph {
            elems,
            index_of,
            decl_index,
            segments,
            layers,
        }
    }

    fn graph(ids: &[&str], edges: &[(&str, usize, usize, bool)]) -> RealGraph {
        RealGraph {
            ids: ids.iter().map(|s| s.to_string()).collect(),
            index_of: ids
                .iter()
                .enumerate()
                .map(|(i, s)| (s.to_string(), i))
                .collect(),
            group_path: vec![Vec::new(); ids.len()],
            shapes: vec![plotgram_model::NodeShape::DEFAULT; ids.len()],
            edges: edges
                .iter()
                .map(|(id, s, t, rev)| RealEdge {
                    edge_id: (*id).into(),
                    original_source: *s,
                    original_target: *t,
                    working_source: if *rev { *t } else { *s },
                    working_target: if *rev { *s } else { *t },
                    reversed: *rev,
                    from_port: None,
                    to_port: None,
                    critical: false,
                })
                .collect(),
            self_loops: Vec::new(),
        }
    }

    fn unit_size(_: usize) -> Size {
        Size::new(20.0, 10.0)
    }

    fn sym(p: &PlanGraph, g: &RealGraph, pass1: &[f64], dummy: &[bool]) -> SymmetryPlan {
        compute_symmetry_plan(p, g, pass1, dummy, &unit_size, 10.0)
    }

    #[test]
    fn axis_formula_odd_and_even() {
        let pass1 = [0.0, 10.0, 20.0, 30.0, 40.0];
        assert!((axis_from_neighbors(&[0, 1, 2], &pass1) - 10.0).abs() < 1e-9);
        assert!((axis_from_neighbors(&[1, 2], &pass1) - 15.0).abs() < 1e-9);
        assert!((axis_from_neighbors(&[0, 1, 2, 3], &pass1) - 15.0).abs() < 1e-9);
    }

    #[test]
    fn slot_multipliers_odd_even() {
        assert_eq!(slot_multipliers(2), vec![-0.5, 0.5]);
        assert_eq!(slot_multipliers(3), vec![-1.0, 0.0, 1.0]);
        assert_eq!(slot_multipliers(4), vec![-1.5, -0.5, 0.5, 1.5]);
    }

    #[test]
    fn multi_hop_spine_joins_down_fan_class() {
        let elems = vec![
            real("gw", 0),
            real("api", 1),
            real("worker", 2),
            real("m", 3),
            real("db", 3),
        ];
        let layers = vec![vec![0], vec![1], vec![2], vec![3, 4]];
        let segments = vec![
            seg("e0", 0, 0, 1),
            seg("e1", 0, 1, 2),
            seg("e2", 0, 2, 3),
            seg("e3", 0, 2, 4),
        ];
        let p = plan(elems, layers, segments);
        let g = graph(
            &["gw", "api", "worker", "m", "db"],
            &[
                ("e0", 0, 1, false),
                ("e1", 1, 2, false),
                ("e2", 2, 3, false),
                ("e3", 2, 4, false),
            ],
        );
        let pass1 = [0.0, 0.0, 0.0, -10.0, 10.0];
        let dummy = vec![false; 5];
        let plan_sym = sym(&p, &g, &pass1, &dummy);
        assert_eq!(plan_sym.axes.len(), 1);
        assert!((plan_sym.axes[0].coord - 0.0).abs() < 1e-9);
        let mem = &plan_sym.classes[0].members;
        assert!(mem.contains(&0) && mem.contains(&1) && mem.contains(&2));
        assert!(!mem.contains(&3) && !mem.contains(&4));
    }

    #[test]
    fn dummy_aligned_truncates_chain() {
        let elems = vec![
            real("a", 0),
            real("b", 1),
            real("c", 2),
            real("d", 2),
        ];
        let layers = vec![vec![0], vec![1], vec![2, 3]];
        let segments = vec![
            seg("e0", 0, 0, 1),
            seg("e1", 0, 1, 2),
            seg("e2", 0, 1, 3),
        ];
        let p = plan(elems, layers, segments);
        let g = graph(
            &["a", "b", "c", "d"],
            &[
                ("e0", 0, 1, false),
                ("e1", 1, 2, false),
                ("e2", 1, 3, false),
            ],
        );
        let pass1 = [0.0, 0.0, -5.0, 5.0];
        let mut dummy = vec![false; 4];
        dummy[0] = true;
        let plan_sym = sym(&p, &g, &pass1, &dummy);
        let mem = &plan_sym.classes[0].members;
        assert!(mem.contains(&1));
        assert!(!mem.contains(&0));
    }

    #[test]
    fn long_edge_fan_and_reversed_backedge() {
        let elems = vec![
            real("submit", 0),
            real("review", 1),
            real("check", 2),
            real("finance", 3),
            real("approved", 4),
            real("rejected", 4),
            virt("e_ca", 0, 3),
        ];
        let layers = vec![
            vec![0],
            vec![1],
            vec![2],
            vec![6, 3],
            vec![4, 5],
        ];
        let segments = vec![
            seg("e_sr", 0, 0, 1),
            seg("e_rc", 0, 1, 2),
            seg("e_cf", 0, 2, 3),
            seg("e_ca", 0, 2, 6),
            seg("e_ca", 1, 6, 4),
            seg("e_fa", 0, 3, 4),
            seg("e_fr", 0, 3, 5),
        ];
        let p = plan(elems, layers, segments);
        let g = graph(
            &["submit", "review", "check", "finance", "approved", "rejected"],
            &[
                ("e_sr", 0, 1, false),
                ("e_rc", 1, 2, false),
                ("e_cf", 2, 3, false),
                ("e_ca", 2, 4, false),
                ("e_fa", 3, 4, false),
                ("e_fr", 3, 5, false),
                ("e_rs", 5, 0, true),
            ],
        );
        let pass1 = [0.0, 0.0, 0.0, 10.0, -10.0, 10.0, -10.0];
        let dummy = vec![false; 7];
        let plan_sym = sym(&p, &g, &pass1, &dummy);
        let check_axis = plan_sym.axes.iter().find(|a| a.hub == 2);
        assert!(check_axis.is_some(), "check must be a fan hub despite long edge");
        let check_class = plan_sym.classes.iter().find(|c| c.axis_hub == 2).unwrap();
        assert!(
            check_class.members.contains(&0)
                && check_class.members.contains(&1)
                && check_class.members.contains(&2),
            "spine submit→review→check must join class: {:?}",
            check_class.members
        );
        assert!(
            !plan_sym.axes.iter().any(|a| a.hub == 0),
            "reversed back-edge must not invent a submit fan"
        );
    }

    #[test]
    fn fan_pack_even_two_leaves_symmetric() {
        let elems = vec![real("hub", 0), real("left", 1), real("right", 1)];
        let layers = vec![vec![0], vec![1, 2]];
        let segments = vec![seg("e0", 0, 0, 1), seg("e1", 0, 0, 2)];
        let p = plan(elems, layers, segments);
        let g = graph(
            &["hub", "left", "right"],
            &[("e0", 0, 1, false), ("e1", 0, 2, false)],
        );
        let pass1 = [0.0, -20.0, 20.0];
        let dummy = vec![false; 3];
        let plan_sym = sym(&p, &g, &pass1, &dummy);
        assert_eq!(plan_sym.packs.len(), 1);
        let pack = &plan_sym.packs[0];
        assert_eq!(pack.hub, 0);
        let left = pack.slots.iter().find(|s| s.elem == 1).unwrap();
        let right = pack.slots.iter().find(|s| s.elem == 2).unwrap();
        let axis = plan_sym.axes[0].coord;
        assert!((axis - 0.0).abs() < 1e-9);
        assert!((left.desired + right.desired - 2.0 * axis).abs() < 1e-9);
        assert!(left.desired < axis && right.desired > axis);
        let pitch = right.desired - left.desired;
        assert!(pitch >= 30.0 - 1e-9, "pitch must cover sep: {pitch}");
    }

    /// Merge hub → even fan: fan hub inherits upstream axis (not leaf midpoint).
    #[test]
    fn even_fan_inherits_upstream_merge_axis() {
        // p0,p1,p2 → handle → gate → L,R. Leaf mid = -5; handle median parents = 0.
        let elems = vec![
            real("p0", 0),
            real("p1", 0),
            real("p2", 0),
            real("handle", 1),
            real("gate", 2),
            real("left", 3),
            real("right", 3),
        ];
        let layers = vec![
            vec![0, 1, 2],
            vec![3],
            vec![4],
            vec![5, 6],
        ];
        let segments = vec![
            seg("e0", 0, 0, 3),
            seg("e1", 0, 1, 3),
            seg("e2", 0, 2, 3),
            seg("e3", 0, 3, 4),
            seg("e4", 0, 4, 5),
            seg("e5", 0, 4, 6),
        ];
        let p = plan(elems, layers, segments);
        let g = graph(
            &["p0", "p1", "p2", "handle", "gate", "left", "right"],
            &[
                ("e0", 0, 3, false),
                ("e1", 1, 3, false),
                ("e2", 2, 3, false),
                ("e3", 3, 4, false),
                ("e4", 4, 5, false),
                ("e5", 4, 6, false),
            ],
        );
        let pass1 = [-30.0, 0.0, 30.0, 0.0, -5.0, -20.0, 10.0];
        let dummy = vec![false; 7];
        let plan_sym = sym(&p, &g, &pass1, &dummy);
        let handle = 3usize;
        let gate = 4usize;
        let handle_axis = plan_sym
            .axes
            .iter()
            .find(|a| a.hub == handle)
            .expect("handle axis")
            .coord;
        assert!(
            (handle_axis - 0.0).abs() < 1e-9,
            "handle axis from parent median, got {handle_axis}"
        );
        let gate_axis = plan_sym
            .axes
            .iter()
            .find(|a| a.hub == gate)
            .expect("gate axis")
            .coord;
        assert!(
            (gate_axis - handle_axis).abs() < 1e-9,
            "gate must inherit handle axis ({handle_axis}), not leaf mid; got {gate_axis}"
        );
        let handle_class = plan_sym
            .classes
            .iter()
            .find(|c| c.axis_hub == handle)
            .expect("handle class");
        assert!(
            handle_class.members.contains(&gate),
            "unique child fan hub absorbed into upstream class: {:?}",
            handle_class.members
        );
        assert!(
            plan_sym.classes.iter().all(|c| c.axis_hub != gate),
            "absorbed gate must not open a second rigid column"
        );
        let pack = plan_sym
            .packs
            .iter()
            .find(|pk| pk.hub == gate)
            .expect("gate FanPack");
        let left = pack.slots.iter().find(|s| s.elem == 5).unwrap();
        let right = pack.slots.iter().find(|s| s.elem == 6).unwrap();
        assert!((left.desired + right.desired - 2.0 * gate_axis).abs() < 1e-9);
    }

    /// Root even fan (no upstream) still uses leaf-midpoint axis.
    #[test]
    fn root_even_fan_keeps_leaf_midpoint_axis() {
        let elems = vec![real("hub", 0), real("left", 1), real("right", 1)];
        let layers = vec![vec![0], vec![1, 2]];
        let segments = vec![seg("e0", 0, 0, 1), seg("e1", 0, 0, 2)];
        let p = plan(elems, layers, segments);
        let g = graph(
            &["hub", "left", "right"],
            &[("e0", 0, 1, false), ("e1", 0, 2, false)],
        );
        // Skewed leaves: mid = 5, not hub pass1 0.
        let pass1 = [0.0, -10.0, 20.0];
        let dummy = vec![false; 3];
        let plan_sym = sym(&p, &g, &pass1, &dummy);
        assert_eq!(plan_sym.axes.len(), 1);
        assert!((plan_sym.axes[0].coord - 5.0).abs() < 1e-9);
    }

    /// Short-span primary joins the rigid column; long-span sibling FanPacks off-axis.
    #[test]
    fn unique_min_span_primary_joins_class_long_leaf_offsets() {
        // hub → near (span 1), hub → far (span 2 via implicit ranks).
        let elems = vec![
            real("hub", 0),
            real("near", 1),
            real("far", 2),
            virt("e_far", 0, 1),
        ];
        let layers = vec![vec![0], vec![1, 3], vec![2]];
        let segments = vec![
            seg("e_near", 0, 0, 1),
            seg("e_far", 0, 0, 3),
            seg("e_far", 1, 3, 2),
        ];
        let p = plan(elems, layers, segments);
        let g = graph(
            &["hub", "near", "far"],
            &[("e_near", 0, 1, false), ("e_far", 0, 2, false)],
        );
        // Far sits left of axis in pass-1 so FanPack sign is west.
        let pass1 = [0.0, 0.0, -40.0, -20.0];
        let dummy = vec![false; 4];
        let plan_sym = sym(&p, &g, &pass1, &dummy);
        let hub = 0usize;
        let near = 1usize;
        let far = 2usize;
        let class = plan_sym
            .classes
            .iter()
            .find(|c| c.axis_hub == hub)
            .expect("hub class");
        assert!(
            class.members.contains(&near),
            "min-span primary must join class: {:?}",
            class.members
        );
        assert!(
            !class.members.contains(&far),
            "long leaf must not join class: {:?}",
            class.members
        );
        let axis = plan_sym.axes.iter().find(|a| a.hub == hub).unwrap().coord;
        let far_d = plan_sym.fan_desired_for(far).expect("far FanPack");
        assert!(
            (far_d - axis).abs() > 1e-6,
            "long leaf off axis: far={far_d} axis={axis}"
        );
        assert!(plan_sym.fan_desired_for(near).is_none());
    }

    /// Equal spans → no primary promotion; even FanPack mirror remains.
    #[test]
    fn tied_min_span_keeps_even_fan_mirror() {
        let elems = vec![real("hub", 0), real("left", 1), real("right", 1)];
        let layers = vec![vec![0], vec![1, 2]];
        let segments = vec![seg("e0", 0, 0, 1), seg("e1", 0, 0, 2)];
        let p = plan(elems, layers, segments);
        let g = graph(
            &["hub", "left", "right"],
            &[("e0", 0, 1, false), ("e1", 0, 2, false)],
        );
        let pass1 = [0.0, -20.0, 20.0];
        let dummy = vec![false; 3];
        let plan_sym = sym(&p, &g, &pass1, &dummy);
        let class = &plan_sym.classes[0];
        assert!(
            !class.members.contains(&1) && !class.members.contains(&2),
            "tied spans must not promote a primary: {:?}",
            class.members
        );
        assert_eq!(plan_sym.packs[0].slots.len(), 2);
    }

    /// constrain-sink shape: hub↔twin 2-cycle + offset sink — twin on axis,
    /// free leaf off-axis (not even-fan mirrored).
    #[test]
    fn twin_peer_joins_rigid_column_free_leaf_offsets() {
        let elems = vec![
            real("start", 0),
            real("hub", 1),
            real("twin", 2),
            real("sink", 2),
        ];
        let layers = vec![vec![0], vec![1], vec![2, 3]];
        let segments = vec![
            seg("e0", 0, 0, 1),
            seg("e1", 0, 1, 2),
            seg("e2", 0, 1, 3),
        ];
        let p = plan(elems, layers, segments);
        let g = graph(
            &["start", "hub", "twin", "sink"],
            &[
                ("e0", 0, 1, false),
                ("e1", 1, 2, false),
                ("e_back", 2, 1, true),
                ("e2", 1, 3, false),
            ],
        );
        let pass1 = [0.0, 0.0, -20.0, 20.0];
        let dummy = vec![false; 4];
        let plan_sym = sym(&p, &g, &pass1, &dummy);
        let hub = 1usize;
        let twin = 2usize;
        let sink = 3usize;
        let class = plan_sym
            .classes
            .iter()
            .find(|c| c.axis_hub == hub)
            .expect("hub class");
        assert!(
            class.members.contains(&twin),
            "twin must join rigid column: {:?}",
            class.members
        );
        assert!(
            !class.members.contains(&sink),
            "offset sink must not join column: {:?}",
            class.members
        );
        let axis = plan_sym.axes.iter().find(|a| a.hub == hub).unwrap().coord;
        assert!(
            (axis - 0.0).abs() < 1e-9,
            "twin side keeps hub pass-1 axis, got {axis}"
        );
        assert!(
            plan_sym.fan_desired_for(twin).is_none(),
            "twin must not take a FanPack slot"
        );
        let sink_d = plan_sym
            .fan_desired_for(sink)
            .expect("sink FanPack slot");
        assert!(
            (sink_d - axis).abs() > 1e-6,
            "sink must sit off axis: sink={sink_d}, axis={axis}"
        );
        assert!(
            sink_d > axis,
            "sink inherits pass-1 east of axis: sink={sink_d}, axis={axis}"
        );
    }

    #[test]
    fn fan_pack_odd_median_on_axis() {
        let elems = vec![
            real("hub", 0),
            real("a", 1),
            real("b", 1),
            real("c", 1),
        ];
        let layers = vec![vec![0], vec![1, 2, 3]];
        let segments = vec![
            seg("e0", 0, 0, 1),
            seg("e1", 0, 0, 2),
            seg("e2", 0, 0, 3),
        ];
        let p = plan(elems, layers, segments);
        let g = graph(
            &["hub", "a", "b", "c"],
            &[
                ("e0", 0, 1, false),
                ("e1", 0, 2, false),
                ("e2", 0, 3, false),
            ],
        );
        let pass1 = [0.0, -30.0, 0.0, 30.0];
        let dummy = vec![false; 4];
        let plan_sym = sym(&p, &g, &pass1, &dummy);
        let pack = &plan_sym.packs[0];
        let mid = pack.slots.iter().find(|s| s.elem == 2).unwrap();
        assert!(
            (mid.desired - plan_sym.axes[0].coord).abs() < 1e-9,
            "odd median leaf sits on axis"
        );
    }

    #[test]
    fn fan_pack_smaller_hub_wins_contested_leaf() {
        let elems = vec![
            real("top", 0),
            real("mid", 1),
            real("bot", 2),
            real("side", 1),
        ];
        let layers = vec![vec![0], vec![3, 1], vec![2]];
        let segments = vec![
            seg("e0", 0, 0, 1),
            seg("e1", 0, 1, 2),
            seg("e3", 0, 0, 3),
            seg("e4", 0, 3, 2),
        ];
        let p = plan(elems, layers, segments);
        let g = graph(
            &["top", "mid", "bot", "side"],
            &[
                ("e0", 0, 1, false),
                ("e1", 1, 2, false),
                ("e3", 0, 3, false),
                ("e4", 3, 2, false),
            ],
        );
        let pass1 = [0.0, 10.0, 0.0, -10.0];
        let dummy = vec![false; 4];
        let plan_sym = sym(&p, &g, &pass1, &dummy);
        assert!(plan_sym.axes.iter().any(|a| a.hub == 0));
        assert!(plan_sym.axes.iter().any(|a| a.hub == 2));
        let top_pack = plan_sym.packs.iter().find(|pk| pk.hub == 0);
        let bot_pack = plan_sym.packs.iter().find(|pk| pk.hub == 2);
        assert!(top_pack.is_some(), "top should own FanPack");
        assert!(
            bot_pack.is_none() || bot_pack.unwrap().slots.is_empty(),
            "bot must not double-write contested leaves: {:?}",
            bot_pack
        );
        assert!(plan_sym.fan_desired_for(1).is_some());
        assert!(plan_sym.fan_desired_for(3).is_some());
    }
}
