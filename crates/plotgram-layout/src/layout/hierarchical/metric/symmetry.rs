//! SymmetryAxisWriter — explicit Metric sub-step for spine∩fan co-linearity.
//!
//! Contract: [`docs/design/layout/hierarchical/phases/symmetry-axis.md`]
//!
//! After pass-1 (BK ideal + soft VPSC), this writer emits:
//! - [`SymmetryAxis`] per fan hub (odd/even axis formula from pass-1 neighbors)
//! - [`RigidColumnClass`] membership (hub + non-fan-side 1:1 chain)
//!
//! Fan adjacency is **forward real endpoints** on [`RealGraph`] (non-reversed
//! edges): long edges still count as one hop, so a decision that spans ranks
//! via dummies remains a fan. Reversed back-edges are excluded so a return
//! path does not invent a spurious fan at the loop head.
//!
//! Downstream constraints / desired must **only consume** these tables.

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

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SymmetryPlan {
    pub axes: Vec<SymmetryAxis>,
    pub classes: Vec<RigidColumnClass>,
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

/// Build symmetry axes and rigid column classes from pass-1 centers.
pub fn compute_symmetry_plan(
    plan: &PlanGraph,
    graph: &RealGraph,
    pass1: &[f64],
    dummy_aligned: &[bool],
) -> SymmetryPlan {
    let n = plan.elems.len();
    let (down_nbs, up_nbs) = forward_real_adjacency(plan, graph);
    let down_deg = degrees_of(&down_nbs);
    let up_deg = degrees_of(&up_nbs);

    let mut axes = Vec::new();
    let mut classes = Vec::new();

    // Hubs in ascending elem index — smaller hub wins contested members.
    let mut hubs: Vec<usize> = (0..n)
        .filter(|&e| {
            !plan.elems[e].key.is_virtual() && (down_deg[e] >= 2 || up_deg[e] >= 2)
        })
        .collect();
    hubs.sort_unstable();

    let mut claimed = vec![false; n];

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
    }

    SymmetryPlan { axes, classes }
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

    #[test]
    fn axis_formula_odd_and_even() {
        let pass1 = [0.0, 10.0, 20.0, 30.0, 40.0];
        // odd 3: median neighbor center
        assert!((axis_from_neighbors(&[0, 1, 2], &pass1) - 10.0).abs() < 1e-9);
        // even 2: midpoint
        assert!((axis_from_neighbors(&[1, 2], &pass1) - 15.0).abs() < 1e-9);
        // even 4: midpoint of two middle
        assert!((axis_from_neighbors(&[0, 1, 2, 3], &pass1) - 15.0).abs() < 1e-9);
    }

    #[test]
    fn multi_hop_spine_joins_down_fan_class() {
        // gw=0 → api=1 → worker=2 → {3,4}
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
        let sym = compute_symmetry_plan(&p, &g, &pass1, &dummy);
        assert_eq!(sym.axes.len(), 1);
        assert!((sym.axes[0].coord - 0.0).abs() < 1e-9);
        let mem = &sym.classes[0].members;
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
        let sym = compute_symmetry_plan(&p, &g, &pass1, &dummy);
        let mem = &sym.classes[0].members;
        assert!(mem.contains(&1));
        assert!(!mem.contains(&0));
    }

    /// Decision fan with a long edge (`check → approved` via dummy) must still
    /// treat `check` as hub and pull the upstream spine into the class.
    /// Reversed back-edges must not invent a fan at the loop head.
    #[test]
    fn long_edge_fan_and_reversed_backedge() {
        // submit=0, review=1, check=2, finance=3, approved=4, rejected=5,
        // d_approved=6 (dummy on check→approved)
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
            // reversed rejected→submit as working submit→… omitted; graph marks reversed
        ];
        let p = plan(elems, layers, segments);
        let g = graph(
            &["submit", "review", "check", "finance", "approved", "rejected"],
            &[
                ("e_sr", 0, 1, false),
                ("e_rc", 1, 2, false),
                ("e_cf", 2, 3, false),
                ("e_ca", 2, 4, false), // long, still a fan leg
                ("e_fa", 3, 4, false),
                ("e_fr", 3, 5, false),
                ("e_rs", 5, 0, true), // reverse — must not fan submit
            ],
        );
        let pass1 = [0.0, 0.0, 0.0, 10.0, -10.0, 10.0, -10.0];
        let dummy = vec![false; 7];
        let sym = compute_symmetry_plan(&p, &g, &pass1, &dummy);
        // check is a hub (forward down to finance+approved)
        let check_axis = sym.axes.iter().find(|a| a.hub == 2);
        assert!(check_axis.is_some(), "check must be a fan hub despite long edge");
        let check_class = sym.classes.iter().find(|c| c.axis_hub == 2).unwrap();
        assert!(
            check_class.members.contains(&0)
                && check_class.members.contains(&1)
                && check_class.members.contains(&2),
            "spine submit→review→check must join class: {:?}",
            check_class.members
        );
        // submit must not become a hub solely from the reversed back-edge
        assert!(
            !sym.axes.iter().any(|a| a.hub == 0),
            "reversed back-edge must not invent a submit fan"
        );
    }
}
