//! Group-boundary dummies (architecture.md §8.3).
//!
//! After properify, insert a Left/Right zero-width clamp for every
//! `(group × rank)` that has real members, nest by `group_path` depth, and
//! wire high-weight segments between adjacent ranks so order intervals align
//! across layers. Crossing minimization stays group-agnostic (flat transpose).

use std::collections::{BTreeMap, BTreeSet};

use super::super::model::{BoundarySide, Elem, ElemKey, PlanGraph, Segment};

/// Insert group-boundary clamps and cross-rank boundary segments into `plan`.
///
/// Clamps cover every rank in a group's member span `[r0, r1]` (not only ranks
/// that already have members), so the rectangular group rect used by Channel
/// derive cannot be pierced by foreign nodes on intermediate ranks.
pub fn insert_group_boundaries(plan: &mut PlanGraph) {
    if plan.layers.is_empty() {
        return;
    }

    // group_id -> full root..leaf path (first observation wins; paths are unique).
    let mut group_path_of: BTreeMap<String, Vec<String>> = BTreeMap::new();
    // group_id -> (min_rank, max_rank) over real members.
    let mut group_rank_span: BTreeMap<String, (usize, usize)> = BTreeMap::new();

    for &ei in plan.layers.iter().flatten() {
        let elem = &plan.elems[ei];
        if !matches!(elem.key, ElemKey::Real(_)) {
            continue;
        }
        let rank = elem.rank as usize;
        for (depth, gname) in elem.group_path.iter().enumerate() {
            group_path_of
                .entry(gname.clone())
                .or_insert_with(|| elem.group_path[..=depth].to_vec());
            group_rank_span
                .entry(gname.clone())
                .and_modify(|(lo, hi)| {
                    *lo = (*lo).min(rank);
                    *hi = (*hi).max(rank);
                })
                .or_insert((rank, rank));
        }
    }

    if group_path_of.is_empty() {
        return;
    }

    // rank -> groups whose member span covers this rank (corridor reservation).
    let mut groups_at_rank: Vec<BTreeSet<String>> = vec![BTreeSet::new(); plan.layers.len()];
    for (gname, &(r0, r1)) in &group_rank_span {
        for rank in r0..=r1 {
            if rank < groups_at_rank.len() {
                groups_at_rank[rank].insert(gname.clone());
            }
        }
    }

    // Alloc boundary elems for every (group, rank, side) that needs them.
    let mut boundary_elem: BTreeMap<(String, u32, BoundarySide), usize> = BTreeMap::new();
    let mut next_decl = plan.decl_index.iter().copied().max().unwrap_or(0) + 1;

    for (rank, groups) in groups_at_rank.iter().enumerate() {
        let rank_u = rank as u32;
        // Stable order: depth then group name.
        let mut ranked: Vec<&String> = groups.iter().collect();
        ranked.sort_by_key(|g| {
            (
                group_path_of.get(*g).map(|p| p.len()).unwrap_or(0),
                (*g).clone(),
            )
        });
        for gname in ranked {
            let path = group_path_of[gname].clone();
            for side in [BoundarySide::Left, BoundarySide::Right] {
                let key = ElemKey::GroupBoundary {
                    group: gname.clone(),
                    rank: rank_u,
                    side,
                };
                if plan.index_of.contains_key(&key) {
                    continue;
                }
                let idx = plan.elems.len();
                plan.elems.push(Elem {
                    key: key.clone(),
                    group_path: path.clone(),
                    rank: rank_u,
                });
                plan.index_of.insert(key, idx);
                plan.decl_index.push(next_decl);
                next_decl += 1;
                boundary_elem.insert((gname.clone(), rank_u, side), idx);
            }
        }
    }

    // Rebuild each layer: nested Left … members … Right, ungrouped outside.
    for rank in 0..plan.layers.len() {
        let old = plan.layers[rank].clone();
        let mut by_exact_path: BTreeMap<Vec<String>, Vec<usize>> = BTreeMap::new();
        let mut ungrouped = Vec::new();
        for &ei in &old {
            match &plan.elems[ei].key {
                ElemKey::Real(_) => {
                    let path = plan.elems[ei].group_path.clone();
                    if path.is_empty() {
                        ungrouped.push(ei);
                    } else {
                        by_exact_path.entry(path).or_default().push(ei);
                    }
                }
                ElemKey::Virtual { .. } => ungrouped.push(ei),
                ElemKey::GroupBoundary { .. }
                | ElemKey::PartitionBoundary { .. }
                | ElemKey::OrderPad { .. } => {
                    // Should not exist yet in `old`; ignore if re-run.
                }
            }
        }

        // Child groups under a path prefix: unique next segment among known groups.
        let groups_here = &groups_at_rank[rank];
        let mut new_layer = Vec::with_capacity(old.len() + groups_here.len() * 2);
        new_layer.extend(ungrouped);

        fn emit_scope(
            prefix: &[String],
            rank: u32,
            groups_here: &BTreeSet<String>,
            group_path_of: &BTreeMap<String, Vec<String>>,
            by_exact_path: &BTreeMap<Vec<String>, Vec<usize>>,
            boundary_elem: &BTreeMap<(String, u32, BoundarySide), usize>,
            out: &mut Vec<usize>,
        ) {
            // Nested groups first, then exact members of this scope
            // → outer_L … inner_L · members · inner_R … outer_R.
            let depth = prefix.len();
            let mut children: Vec<&String> = groups_here
                .iter()
                .filter(|g| {
                    group_path_of
                        .get(*g)
                        .is_some_and(|p| p.len() == depth + 1 && p[..depth] == *prefix)
                })
                .collect();
            children.sort();
            for child in children {
                let left = boundary_elem[&(child.clone(), rank, BoundarySide::Left)];
                let right = boundary_elem[&(child.clone(), rank, BoundarySide::Right)];
                out.push(left);
                let child_path = group_path_of[child].clone();
                emit_scope(
                    &child_path,
                    rank,
                    groups_here,
                    group_path_of,
                    by_exact_path,
                    boundary_elem,
                    out,
                );
                out.push(right);
            }
            if let Some(members) = by_exact_path.get(prefix) {
                out.extend(members.iter().copied());
            }
        }

        emit_scope(
            &[],
            rank as u32,
            groups_here,
            &group_path_of,
            &by_exact_path,
            &boundary_elem,
            &mut new_layer,
        );
        plan.layers[rank] = new_layer;
    }

    // Cross-rank high-weight segments between matching (group, side) clamps.
    for rank in 0..plan.layers.len().saturating_sub(1) {
        let r0 = rank as u32;
        let r1 = (rank + 1) as u32;
        let shared: BTreeSet<&String> = groups_at_rank[rank]
            .intersection(&groups_at_rank[rank + 1])
            .collect();
        for gname in shared {
            for side in [BoundarySide::Left, BoundarySide::Right] {
                let from = boundary_elem[&(gname.clone(), r0, side)];
                let to = boundary_elem[&(gname.clone(), r1, side)];
                let side_tag = match side {
                    BoundarySide::Left => "L",
                    BoundarySide::Right => "R",
                };
                plan.segments.push(Segment {
                    edge_id: format!("gb:{gname}:{side_tag}"),
                    ordinal: r0,
                    from,
                    to,
                });
            }
        }
    }
}

/// Insert leading [`ElemKey::OrderPad`]s so each group's Left clamp shares a
/// common raw layer index across ranks (physical expand of boundary write).
pub fn align_group_left_pads(plan: &mut PlanGraph) {
    use crate::layout::hierarchical::model::BoundarySide;

    let mut target_left: BTreeMap<String, usize> = BTreeMap::new();
    for layer in &plan.layers {
        for (ord, &ei) in layer.iter().enumerate() {
            if let ElemKey::GroupBoundary {
                group,
                side: BoundarySide::Left,
                ..
            } = &plan.elems[ei].key
            {
                target_left
                    .entry(group.clone())
                    .and_modify(|t| *t = (*t).max(ord))
                    .or_insert(ord);
            }
        }
    }
    if target_left.is_empty() {
        return;
    }

    let mut next_decl = plan.decl_index.iter().copied().max().unwrap_or(0) + 1;
    for rank in 0..plan.layers.len() {
        let mut need = 0usize;
        for (ord, &ei) in plan.layers[rank].iter().enumerate() {
            if let ElemKey::GroupBoundary {
                group,
                side: BoundarySide::Left,
                ..
            } = &plan.elems[ei].key
            {
                if let Some(&tgt) = target_left.get(group) {
                    need = need.max(tgt.saturating_sub(ord));
                }
            }
        }
        if need == 0 {
            continue;
        }
        let mut pads = Vec::with_capacity(need);
        for ordinal in 0..need as u32 {
            let key = ElemKey::OrderPad {
                rank: rank as u32,
                ordinal,
            };
            if plan.index_of.contains_key(&key) {
                pads.push(plan.index_of[&key]);
                continue;
            }
            let idx = plan.elems.len();
            plan.elems.push(Elem {
                key: key.clone(),
                group_path: Vec::new(),
                rank: rank as u32,
            });
            plan.index_of.insert(key, idx);
            plan.decl_index.push(next_decl);
            next_decl += 1;
            pads.push(idx);
        }
        let mut new_layer = pads;
        new_layer.extend(plan.layers[rank].iter().copied());
        plan.layers[rank] = new_layer;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::hierarchical::model::ElemKey;

    fn plain_real(id: &str, rank: u32, path: &[&str]) -> Elem {
        Elem {
            key: ElemKey::Real(id.into()),
            group_path: path.iter().map(|s| (*s).to_string()).collect(),
            rank,
        }
    }

    #[test]
    fn inserts_nested_boundaries_and_cross_rank_segments() {
        // rank0: a in g1; rank1: b in g1/g2, c in g1
        let elems = vec![
            plain_real("a", 0, &["g1"]),
            plain_real("b", 1, &["g1", "g2"]),
            plain_real("c", 1, &["g1"]),
        ];
        let index_of = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let mut plan = PlanGraph {
            elems,
            index_of,
            decl_index: vec![0, 1, 2],
            segments: vec![],
            layers: vec![vec![0], vec![1, 2]],
            ..Default::default()
        };
        insert_group_boundaries(&mut plan);

        let l0 = &plan.layers[0];
        assert!(
            plan.elems[l0[0]].key.is_group_boundary(),
            "layer0 should start with g1 Left"
        );
        let l1 = &plan.layers[1];
        // g1_L, g2_L, b, g2_R, c, g1_R
        assert_eq!(l1.len(), 6, "nested clamps around members: {l1:?}");
        assert!(matches!(
            plan.elems[l1[0]].key,
            ElemKey::GroupBoundary {
                side: BoundarySide::Left,
                ..
            }
        ));
        // g1 spans both ranks → cross-rank boundary segments; g2 only on rank1.
        assert!(plan
            .segments
            .iter()
            .any(|s| s.edge_id.starts_with("gb:g1:")));
        assert!(!plan
            .segments
            .iter()
            .any(|s| s.edge_id.starts_with("gb:g2:")));
    }
}
