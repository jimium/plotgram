//! SymmetryAxisWriter — explicit Metric sub-step for spine∩fan co-linearity.
//!
//! Contract: [`docs/design/layout/hierarchical/phases/symmetry-axis.md`]
//!
//! After pass-1 (BK ideal + soft VPSC), this writer emits:
//! - [`SymmetryAxis`] per fan hub (odd/even axis formula from pass-1 neighbors)
//! - [`RigidColumnClass`] membership (hub + non-fan-side 1:1 chain)
//! - [`FanPack`] leaf slots (`axis ± k·pitch`) covering BK ideal on pass-2
//!
//! Fan adjacency is **forward real endpoints** on [`RealGraph`] (non-reversed
//! edges): long edges still count as one hop, so a decision that spans ranks
//! via dummies remains a fan. Reversed back-edges are excluded so a return
//! path does not invent a spurious fan at the loop head.
//!
//! Downstream constraints / desired must **only consume** these tables.

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

    let mut axes = Vec::new();
    let mut classes = Vec::new();
    let mut packs = Vec::new();

    // Hubs in ascending elem index — smaller hub wins contested members / leaves.
    let mut hubs: Vec<usize> = (0..n)
        .filter(|&e| {
            !plan.elems[e].key.is_virtual() && (down_deg[e] >= 2 || up_deg[e] >= 2)
        })
        .collect();
    hubs.sort_unstable();

    let mut claimed = vec![false; n];
    let mut fan_claimed = vec![false; n];

    for &hub in &hubs {
        let mut centers = Vec::new();
        if down_deg[hub] >= 2 {
            centers.push(axis_from_neighbors(&down_nbs[hub], pass1));
        }
        if up_deg[hub] >= 2 {
            centers.push(axis_from_neighbors(&up_nbs[hub], pass1));
        }
        if centers.is_empty() {
            continue;
        }
        let coord = centers.iter().sum::<f64>() / centers.len() as f64;
        axes.push(SymmetryAxis { hub, coord });

        let mut members = vec![hub];
        // Non-fan side walks: fan below → walk up; fan above → walk down.
        if down_deg[hub] >= 2 {
            walk_chain(
                hub,
                /*toward_up=*/ true,
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
                plan,
                &down_nbs,
                &up_nbs,
                &down_deg,
                &up_deg,
                dummy_aligned,
                &mut members,
            );
        }

        // Drop members already claimed by an earlier (smaller-index) hub.
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

        // FanPack: leaf slots about this hub's axis, then exclusive 1:1
        // followers under each leaf share the leaf's desired (straight chain).
        let mut slots = Vec::new();
        if down_deg[hub] >= 2 {
            append_fan_slots(
                plan,
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
fn walk_chain(
    start: usize,
    toward_up: bool,
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
    if leaves.len() < 2 {
        // Contested down to <2: no pack for this side (not a usable fan packing).
        return;
    }
    leaves.sort_by(|&a, &b| compose_order_key(plan, a).cmp(&compose_order_key(plan, b)));
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
