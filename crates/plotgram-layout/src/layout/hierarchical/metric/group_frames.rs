//! D₂.0 Weak group-frame writer (group-frame-d2.md §6.2 / §7.2 option B).
//!
//! Write authority: group frame geometry is a Metric product. Each group
//! owns four frame variables (`L/R` cross axis, `T/B` main axis) solved by
//! VPSC with hard containment constraints (members ⊆ frame with pad,
//! child frames ⊆ parent frame inset 0 — children already carry pad, so a
//! parent stays one pad wider, the finalize double-pad contract) and a Fit
//! objective (frame desired = member ∪ child envelope ± pad, so frames hug
//! the outer envelope exactly — today's visual, solved instead of invented
//! in finalize).
//!
//! Option B ruling: `GroupBoundary` clamps keep sandwiching members in the
//! cross-axis solve; frame variables are solved separately afterwards and
//! boundaries are not rewritten here. Infeasibility fails hard
//! (`VpscError::Infeasible` → `LayoutError`); containment constraints are
//! never dropped silently.
//!
//! Forbidden (group-frame-d2.md §6.3): finalize re-deriving frames when the
//! layout provided them; Ink translating groups to "fix" containment.

use std::collections::BTreeSet;

use plotgram_algo::vpsc::{self, Constraint, Variable, VpscError};
use plotgram_model::geometry::Rect;
use plotgram_model::graph::Group;
use plotgram_model::result::GroupPlacement;

use crate::layout::hierarchical::group_frame::{group_top_pad, GROUP_PAD};
use crate::layout::hierarchical::model::{ElemKey, PlanGraph};

/// The anchor variable pins constant containment bounds (`L ≤ c`, `R ≥ c`).
/// With Fit desired every constraint is satisfied at the desired position,
/// so the anchor never joins an active block; the huge weight guards against
/// future callers whose desired violates containment.
const ANCHOR_WEIGHT: f64 = 1e12;

/// Solve canonical group frames for every group with placed content
/// (post-order: child frames precede their parent; empty groups produce no
/// frame — same policy as the facade fallback).
pub fn solve_group_frames(
    groups: &[Group],
    plan: &PlanGraph,
    canonical_frames: &[Rect],
    labeled: &BTreeSet<String>,
) -> Result<Vec<GroupPlacement>, VpscError> {
    // Post-order with content only: a group without member frames and without
    // framed children is skipped (its ancestors then union nothing from it).
    let mut order: Vec<GroupInfo> = Vec::new();
    for g in groups {
        collect_group_content(g, plan, None, &mut order);
    }
    if order.is_empty() {
        return Ok(Vec::new());
    }

    // Fit desired per axis: union(member frames ∪ child FRAMES) ± pad.
    // Child frames already carry their own pad, so a parent ends up one
    // pad wider (the finalize double-pad contract). Post-order guarantees
    // child desired are final when their parent reads them.
    let n = order.len();
    let mut left_d = vec![0.0; n];
    let mut right_d = vec![0.0; n];
    let mut top_d = vec![0.0; n];
    let mut bottom_d = vec![0.0; n];
    for i in 0..n {
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for &ei in &order[i].members {
            let r = canonical_frames[ei];
            min_x = min_x.min(r.x);
            min_y = min_y.min(r.y);
            max_x = max_x.max(r.right());
            max_y = max_y.max(r.bottom());
        }
        for &ci in &order[i].children {
            min_x = min_x.min(left_d[ci]);
            min_y = min_y.min(top_d[ci]);
            max_x = max_x.max(right_d[ci]);
            max_y = max_y.max(bottom_d[ci]);
        }
        left_d[i] = min_x - GROUP_PAD;
        right_d[i] = max_x + GROUP_PAD;
        top_d[i] = min_y - group_top_pad(labeled.contains(&order[i].id));
        bottom_d[i] = max_y + GROUP_PAD;
    }

    // Cross axis: L/R frame variables; main axis: T/B.
    let parents: Vec<Option<usize>> = order.iter().map(|g| g.parent).collect();
    let (lefts, rights) = solve_axis(n, &parents, &left_d, &right_d)?;
    let (tops, bottoms) = solve_axis(n, &parents, &top_d, &bottom_d)?;

    Ok(order
        .iter()
        .enumerate()
        .map(|(i, g)| GroupPlacement {
            id: g.id.clone(),
            frame: Rect::new(
                lefts[i],
                tops[i],
                rights[i] - lefts[i],
                bottoms[i] - tops[i],
            ),
        })
        .collect())
}

/// One axis of frame variables: per group a low/high edge variable with the
/// envelope edge as Fit desired, hard containment bounds against a pinned
/// anchor, and child ⊆ parent separation constraints (gap 0).
fn solve_axis(
    n: usize,
    parent: &[Option<usize>],
    low_bound: &[f64],
    high_bound: &[f64],
) -> Result<(Vec<f64>, Vec<f64>), VpscError> {
    // Variable layout: [anchor, (low_0, high_0), (low_1, high_1), ...].
    let mut vars = Vec::with_capacity(1 + 2 * n);
    vars.push(Variable {
        desired: 0.0,
        weight: ANCHOR_WEIGHT,
    });
    let mut constraints = Vec::new();
    for i in 0..n {
        let lo = 1 + 2 * i;
        let hi = lo + 1;
        vars.push(Variable {
            desired: low_bound[i],
            weight: 1.0,
        });
        vars.push(Variable {
            desired: high_bound[i],
            weight: 1.0,
        });
        // low_i ≤ bound  ⇔  anchor − low_i ≥ −bound (gap = exact negation of
        // the desired → zero violation, the block never merges on equality).
        constraints.push(Constraint::new(lo, 0, -low_bound[i]));
        // high_i ≥ bound  ⇔  high_i − anchor ≥ bound.
        constraints.push(Constraint::new(0, hi, high_bound[i]));
        if let Some(p) = parent[i] {
            let plo = 1 + 2 * p;
            let phi = plo + 1;
            // parent.low ≤ child.low; child.high ≤ parent.high.
            constraints.push(Constraint::new(plo, lo, 0.0));
            constraints.push(Constraint::new(hi, phi, 0.0));
        }
    }
    let pos = vpsc::solve(&vars, &constraints)?;
    let lows = (0..n).map(|i| pos[1 + 2 * i]).collect();
    let highs = (0..n).map(|i| pos[2 + 2 * i]).collect();
    Ok((lows, highs))
}

/// Post-order group descriptor: placed member element indices, framed child
/// indices (local), parent index (local).
struct GroupInfo {
    id: String,
    members: Vec<usize>,
    children: Vec<usize>,
    parent: Option<usize>,
}

/// Post-order traversal keeping only groups with placed content; assigns
/// local indices so child/parent references stay consistent. The group's
/// own descriptor is appended after its kept children (post-order, matching
/// the facade fallback's emission order).
fn collect_group_content(
    group: &Group,
    plan: &PlanGraph,
    parent: Option<usize>,
    out: &mut Vec<GroupInfo>,
) -> Option<usize> {
    let members: Vec<usize> = group
        .nodes
        .iter()
        .filter_map(|n| plan.index_of.get(&ElemKey::Real(n.id.clone())).copied())
        .collect();

    // Phase 1: recurse with no parent fix-up yet; remember each kept
    // child's own index relative to the current tail.
    let base = out.len();
    let mut children_rel: Vec<usize> = Vec::new();
    let mut kept = !members.is_empty();
    for child in &group.groups {
        if let Some(ci) = collect_group_content(child, plan, None, out) {
            children_rel.push(ci - base);
            kept = true;
        }
    }
    if !kept {
        return None; // no placed content anywhere in this subtree
    }

    // Phase 2: append this group, then remap the local child indices.
    let my_index = out.len();
    let children: Vec<usize> = children_rel.iter().map(|&r| base + r).collect();
    out.push(GroupInfo {
        id: group.id.clone(),
        members,
        children: children.clone(),
        parent,
    });
    for &ci in &children {
        out[ci].parent = Some(my_index);
    }
    Some(my_index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::hierarchical::group_frame::GROUP_LABEL_TOP_PAD;
    use crate::layout::hierarchical::model::Elem;
    use plotgram_model::attr::AttrMap;
    use plotgram_model::graph::Node;

    /// Plan whose real elements are exactly `ids` (dense index = position).
    fn plan_of(ids: &[&str]) -> PlanGraph {
        let elems: Vec<Elem> = ids
            .iter()
            .map(|id| Elem {
                key: ElemKey::Real((*id).to_string()),
                group_path: Vec::new(),
                rank: 0,
            })
            .collect();
        let index_of = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let decl_index = (0..elems.len()).collect();
        PlanGraph {
            elems,
            index_of,
            decl_index,
            segments: Vec::new(),
            layers: vec![(0..ids.len()).collect()],
        }
    }

    fn group(id: &str, label: Option<&str>, nodes: &[&str], children: Vec<Group>) -> Group {
        Group {
            id: id.to_string(),
            label: label.map(str::to_string),
            attrs: AttrMap::new(),
            nodes: nodes
                .iter()
                .map(|n| Node {
                    id: (*n).to_string(),
                    label: None,
                    shape: None,
                    role: plotgram_model::graph::NodeRole::Entity,
                    host_group: None,
                    anchor: None,
                    partition_cell: None,
                    attrs: AttrMap::new(),
                })
                .collect(),
            edges: Vec::new(),
            groups: children,
        }
    }

    fn by_id<'a>(placements: &'a [GroupPlacement], id: &str) -> &'a Rect {
        placements
            .iter()
            .find(|g| g.id == id)
            .map(|g| &g.frame)
            .unwrap_or_else(|| panic!("group `{id}` missing"))
    }

    /// Table-driven Fit + containment check: (frames, groups, labeled,
    /// expected `(id, x, y, w, h)` frames). Containment follows from the
    /// expected values (pad contract asserted explicitly).
    #[test]
    fn fit_hugs_member_envelope_with_pad_contract() {
        let eps = 1e-9;
        let plan = plan_of(&["a", "b"]);
        let frames = [
            Rect::new(100.0, 10.0, 60.0, 30.0), // a
            Rect::new(200.0, 20.0, 40.0, 30.0), // b
        ];
        let labeled: BTreeSet<String> = ["g".to_string()].into_iter().collect();
        let groups = vec![group("g", Some("G"), &["a", "b"], Vec::new())];
        let out = solve_group_frames(&groups, &plan, &frames, &labeled).unwrap();
        assert_eq!(out.len(), 1);
        let f = by_id(&out, "g");
        // left/right/bottom: GROUP_PAD; top: label band.
        assert!((f.x - (100.0 - GROUP_PAD)).abs() < eps);
        assert!((f.y - (10.0 - GROUP_LABEL_TOP_PAD)).abs() < eps);
        assert!((f.right() - (240.0 + GROUP_PAD)).abs() < eps);
        assert!((f.bottom() - (50.0 + GROUP_PAD)).abs() < eps);
    }

    #[test]
    fn sibling_groups_get_independent_frames_in_postorder() {
        let eps = 1e-9;
        let plan = plan_of(&["a", "b"]);
        let frames = [
            Rect::new(0.0, 0.0, 50.0, 20.0),   // a (in `l`)
            Rect::new(300.0, 0.0, 50.0, 20.0), // b (in `r`)
        ];
        let labeled = BTreeSet::new();
        let groups = vec![
            group("l", None, &["a"], Vec::new()),
            group("r", None, &["b"], Vec::new()),
        ];
        let out = solve_group_frames(&groups, &plan, &frames, &labeled).unwrap();
        assert_eq!(out.len(), 2);
        let l = by_id(&out, "l");
        let r = by_id(&out, "r");
        assert!((l.x - (-GROUP_PAD)).abs() < eps);
        assert!((l.right() - (50.0 + GROUP_PAD)).abs() < eps);
        assert!((r.x - (300.0 - GROUP_PAD)).abs() < eps);
        assert!((r.right() - (350.0 + GROUP_PAD)).abs() < eps);
        // Unlabeled top pad.
        assert!((l.y - (-GROUP_PAD)).abs() < eps);
    }

    #[test]
    fn nested_parent_is_one_pad_wider_than_child_frame() {
        let eps = 1e-9;
        let plan = plan_of(&["a"]);
        let frames = [Rect::new(10.0, 10.0, 40.0, 20.0)];
        let labeled = BTreeSet::new();
        let groups = vec![group(
            "outer",
            None,
            &[],
            vec![group("inner", None, &["a"], Vec::new())],
        )];
        let out = solve_group_frames(&groups, &plan, &frames, &labeled).unwrap();
        // Post-order: child first.
        assert_eq!(out[0].id, "inner");
        assert_eq!(out[1].id, "outer");
        let inner = by_id(&out, "inner");
        let outer = by_id(&out, "outer");
        // Double-pad contract: outer = inner frame ± GROUP_PAD.
        assert!((outer.x - (inner.x - GROUP_PAD)).abs() < eps);
        assert!((outer.right() - (inner.right() + GROUP_PAD)).abs() < eps);
        assert!((outer.y - (inner.y - GROUP_PAD)).abs() < eps);
        assert!((outer.bottom() - (inner.bottom() + GROUP_PAD)).abs() < eps);
        // Containment: member ⊆ inner ⊆ outer.
        let m = frames[0];
        assert!(m.x >= inner.x + GROUP_PAD - eps);
        assert!(m.right() <= inner.right() - GROUP_PAD + eps);
        assert!(inner.x >= outer.x - eps);
        assert!(inner.right() <= outer.right() + eps);
    }

    #[test]
    fn empty_group_emits_no_frame_but_parent_of_framed_child_does() {
        let plan = plan_of(&["a"]);
        let frames = [Rect::new(0.0, 0.0, 10.0, 10.0)];
        let labeled = BTreeSet::new();
        // `empty` has no placed members → no frame; `holder` only wraps the
        // empty group → also no frame.
        let groups = vec![
            group("empty", None, &["ghost"], Vec::new()),
            group("holder", None, &[], vec![group("empty2", None, &[], Vec::new())]),
            group("real", None, &["a"], Vec::new()),
        ];
        let out = solve_group_frames(&groups, &plan, &frames, &labeled).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, "real");
    }
}
