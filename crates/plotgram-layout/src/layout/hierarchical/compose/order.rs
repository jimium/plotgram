//! P6 ordering: flat median + transpose + best-snapshot.
//!
//! Group containment is enforced upstream by
//! [`super::boundary::insert_group_boundaries`] (Left/Right clamps + high-weight
//! cross-rank segments). Crossing minimization is fully group-agnostic.

use std::collections::{BTreeMap, BTreeSet};

use crate::layout::hierarchical::model::{Elem, PlanGraph};

const EPS: f64 = 1e-9;
const MAX_SWEEPS: usize = 16;
const NO_IMPROVE_STOP: usize = 2;

/// real-real / real-virtual / virtual-virtual base weight (composition.md §6).
fn edge_weight(a: &Elem, b: &Elem) -> f64 {
    match (a.key.is_virtual(), b.key.is_virtual()) {
        (false, false) => 1.0,
        (true, true) => 8.0,
        _ => 2.0,
    }
}

/// Segment weight: both ends group-boundary → `group_boundary_weight`;
/// else base + optional critical doubling (vv corridor stays 8.0).
fn segment_weight(a: &Elem, b: &Elem, critical: bool, group_boundary_weight: f64) -> f64 {
    if a.key.is_group_boundary() && b.key.is_group_boundary() {
        return group_boundary_weight;
    }
    let base = edge_weight(a, b);
    if critical && !matches!((a.key.is_virtual(), b.key.is_virtual()), (true, true)) {
        base * 2.0
    } else {
        base
    }
}

struct Adjacency {
    /// elem -> (neighbor elem, segment weight) one rank above.
    up: Vec<Vec<(usize, f64)>>,
    /// elem -> (neighbor elem, segment weight) one rank below.
    down: Vec<Vec<(usize, f64)>>,
}

fn build_adjacency(
    plan: &PlanGraph,
    critical: &BTreeSet<String>,
    group_boundary_weight: f64,
) -> Adjacency {
    let n = plan.elems.len();
    let mut up = vec![Vec::new(); n];
    let mut down = vec![Vec::new(); n];
    for s in &plan.segments {
        let w = segment_weight(
            &plan.elems[s.from],
            &plan.elems[s.to],
            critical.contains(&s.edge_id),
            group_boundary_weight,
        );
        down[s.from].push((s.to, w));
        up[s.to].push((s.from, w));
    }
    for v in up.iter_mut().chain(down.iter_mut()) {
        v.sort_unstable_by_key(|&(e, _)| e);
    }
    Adjacency { up, down }
}

pub fn order_layers(
    plan: &mut PlanGraph,
    critical: &BTreeSet<String>,
    group_boundary_weight: f64,
) {
    if plan.layers.len() < 2 {
        return; // nothing to reorder
    }
    let adj = build_adjacency(plan, critical, group_boundary_weight);

    let mut best = plan.layers.clone();
    let mut best_crossings = total_crossings(plan);
    let mut no_improve = 0usize;

    for sweep in 0..MAX_SWEEPS {
        if sweep % 2 == 0 {
            for r in 1..plan.layers.len() {
                reorder_layer(plan, &adj, r, Direction::Up);
            }
        } else {
            for r in (0..plan.layers.len() - 1).rev() {
                reorder_layer(plan, &adj, r, Direction::Down);
            }
        }
        transpose_pass(plan);
        for r in 0..plan.layers.len() {
            restore_group_clamps(plan, r);
        }

        let c = total_crossings(plan);
        if c < best_crossings {
            best_crossings = c;
            best.clone_from(&plan.layers);
            no_improve = 0;
        } else {
            no_improve += 1;
        }
        if c == 0 || no_improve >= NO_IMPROVE_STOP {
            break;
        }
    }

    plan.layers = best;
    for r in 0..plan.layers.len() {
        restore_group_clamps(plan, r);
    }
    if !super::super::CHANNEL_FORCE_ROOT.get() {
        super::boundary::align_group_left_pads(plan);
    }
    tighten_one_to_one(plan, &adj);
}

/// G4: pull 1:1 real leaves under their only neighbor by adjacent swaps that
/// do not increase crossings (and prefer reducing |Δorder|).
fn tighten_one_to_one(plan: &mut PlanGraph, adj: &Adjacency) {
    let _ = adj;
    let n = plan.elems.len();
    let mut down_real = vec![Vec::new(); n];
    let mut up_real = vec![Vec::new(); n];
    for s in &plan.segments {
        if !plan.elems[s.from].key.is_virtual()
            && !plan.elems[s.from].key.is_group_boundary()
            && !plan.elems[s.to].key.is_virtual()
            && !plan.elems[s.to].key.is_group_boundary()
        {
            down_real[s.from].push(s.to);
            up_real[s.to].push(s.from);
        }
    }
    let base = total_crossings(plan);
    for r in 0..plan.layers.len() {
        let layer_len = plan.layers[r].len();
        for i in 0..layer_len.saturating_sub(1) {
            let a = plan.layers[r][i];
            let b = plan.layers[r][i + 1];
            if plan.elems[a].key.is_zero_width() || plan.elems[b].key.is_zero_width() {
                continue;
            }
            let a_up = up_real[a].len() == 1;
            let b_up = up_real[b].len() == 1;
            let a_dn = down_real[a].len() == 1;
            let b_dn = down_real[b].len() == 1;
            if !(a_up && b_up) && !(a_dn && b_dn) {
                continue;
            }
            let order_of = |p: &PlanGraph, e: usize| -> isize {
                let rank = p.elems[e].rank as usize;
                p.layers[rank].iter().position(|&x| x == e).unwrap_or(0) as isize
            };
            let score = |p: &PlanGraph| -> isize {
                let mut s = 0isize;
                for &e in &p.layers[r] {
                    if up_real[e].len() == 1 {
                        s += (order_of(p, e) - order_of(p, up_real[e][0])).abs();
                    }
                    if down_real[e].len() == 1 {
                        s += (order_of(p, e) - order_of(p, down_real[e][0])).abs();
                    }
                }
                s
            };
            let before = score(plan);
            plan.layers[r].swap(i, i + 1);
            if total_crossings(plan) > base || score(plan) > before {
                plan.layers[r].swap(i, i + 1);
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Direction {
    /// Reference layer is `r - 1` (used during the down-sweep).
    Up,
    /// Reference layer is `r + 1` (used during the up-sweep).
    Down,
}

fn reference_positions(layer: &[usize]) -> BTreeMap<usize, usize> {
    layer.iter().enumerate().map(|(i, &e)| (e, i)).collect()
}

struct Key {
    median: Option<f64>,
    barycenter: Option<f64>,
    /// Position within the layer *before* this sort (stable fallback).
    prev_pos: usize,
    /// Declaration index (final tie-break).
    repr_decl: usize,
}

/// Quantize neighbor positions for a total-order sort key (avoids ε-threshold
/// non-transitivity that panics driftsort at wide layers).
const QUANT: f64 = 1024.0;

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct SortKey {
    /// `(false, q)` = has value; `(true, 0)` = no neighbors, sorts after all valued keys.
    median: (bool, i64),
    barycenter: (bool, i64),
    prev_pos: usize,
    repr_decl: usize,
}

fn quant(v: Option<f64>) -> (bool, i64) {
    match v {
        None => (true, 0),
        Some(x) => (false, (x * QUANT).round() as i64),
    }
}

fn sort_key(k: &Key) -> SortKey {
    SortKey {
        median: quant(k.median),
        barycenter: quant(k.barycenter),
        prev_pos: k.prev_pos,
        repr_decl: k.repr_decl,
    }
}

fn median_of(mut positions: Vec<f64>) -> Option<f64> {
    if positions.is_empty() {
        return None;
    }
    positions.sort_by(f64::total_cmp);
    let m = positions.len();
    if m % 2 == 1 {
        return Some(positions[m / 2]);
    }
    if m == 2 {
        return Some((positions[0] + positions[1]) / 2.0);
    }
    let left = positions[m / 2 - 1];
    let right = positions[m / 2];
    let left_spread = right - positions[0];
    let right_spread = positions[m - 1] - left;
    if left_spread + right_spread <= EPS {
        Some((left + right) / 2.0)
    } else {
        Some((left * right_spread + right * left_spread) / (left_spread + right_spread))
    }
}

fn weighted_barycenter(pairs: &[(f64, f64)]) -> Option<f64> {
    let wsum: f64 = pairs.iter().map(|(_, w)| w).sum();
    if wsum <= EPS {
        return None;
    }
    Some(pairs.iter().map(|(p, w)| p * w).sum::<f64>() / wsum)
}

fn reorder_layer(plan: &mut PlanGraph, adj: &Adjacency, r: usize, dir: Direction) {
    let ref_layer = match dir {
        Direction::Up => &plan.layers[r - 1],
        Direction::Down => &plan.layers[r + 1],
    };
    let ref_pos = reference_positions(ref_layer);
    let neighbors_of: &[Vec<(usize, f64)>] = match dir {
        Direction::Up => &adj.up,
        Direction::Down => &adj.down,
    };

    let layer = plan.layers[r].clone();
    let mut keyed: Vec<(Key, usize)> = layer
        .iter()
        .enumerate()
        .map(|(prev_pos, &e)| {
            let mut pooled = Vec::new();
            for &(n, w) in &neighbors_of[e] {
                if let Some(&p) = ref_pos.get(&n) {
                    pooled.push((p as f64, w));
                }
            }
            let positions: Vec<f64> = pooled.iter().map(|(p, _)| *p).collect();
            let key = Key {
                median: median_of(positions),
                barycenter: weighted_barycenter(&pooled),
                prev_pos,
                repr_decl: plan.decl_index[e],
            };
            (key, e)
        })
        .collect();

    keyed.sort_by_key(|(k, _)| sort_key(k));
    plan.layers[r] = keyed.into_iter().map(|(_, e)| e).collect();
    restore_group_clamps(plan, r);
}

/// Re-pack after a group-agnostic median/transpose: keep Left/Right where
/// the soft weights placed them (cross-rank alignment), eject foreign elems
/// from the open interval, and pull any escaped members back inside.
fn restore_group_clamps(plan: &mut PlanGraph, r: usize) {
    use crate::layout::hierarchical::model::{BoundarySide, ElemKey};

    let mut clamps: Vec<(usize, String, usize, usize)> = Vec::new();
    for &ei in &plan.layers[r] {
        if let ElemKey::GroupBoundary {
            group,
            side: BoundarySide::Left,
            ..
        } = &plan.elems[ei].key
        {
            let right_key = ElemKey::GroupBoundary {
                group: group.clone(),
                rank: r as u32,
                side: BoundarySide::Right,
            };
            let Some(&right_ei) = plan.index_of.get(&right_key) else {
                continue;
            };
            let depth = plan.elems[ei].group_path.len();
            clamps.push((depth, group.clone(), ei, right_ei));
        }
    }
    clamps.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));

    for (_, group, left_ei, right_ei) in clamps {
        let layer = plan.layers[r].clone();
        let Some(mut lpos) = layer.iter().position(|&e| e == left_ei) else {
            continue;
        };
        let Some(mut rpos) = layer.iter().position(|&e| e == right_ei) else {
            continue;
        };
        if lpos > rpos {
            plan.layers[r].swap(lpos, rpos);
            std::mem::swap(&mut lpos, &mut rpos);
        }
        if lpos == rpos {
            continue;
        }

        let is_member = |e: usize| {
            e != left_ei
                && e != right_ei
                && plan.elems[e].group_path.iter().any(|g| g == &group)
        };

        let mut before = Vec::new();
        let mut interior = Vec::new();
        let mut foreign = Vec::new();
        let mut after = Vec::new();
        for (i, &e) in layer.iter().enumerate() {
            if e == left_ei || e == right_ei {
                continue;
            }
            if is_member(e) {
                interior.push(e);
            } else if i < lpos {
                before.push(e);
            } else if i > rpos {
                after.push(e);
            } else {
                foreign.push(e);
            }
        }

        // Keep Left at `before.len()` (same index as before the eject) so
        // cross-rank high-weight alignment survives the pack.
        let mut new_layer = Vec::with_capacity(layer.len());
        new_layer.extend(before);
        new_layer.push(left_ei);
        new_layer.extend(interior);
        new_layer.push(right_ei);
        new_layer.extend(foreign);
        new_layer.extend(after);
        plan.layers[r] = new_layer;
    }
}

fn total_crossings(plan: &PlanGraph) -> u64 {
    let mut total = 0u64;
    for r in 0..plan.layers.len().saturating_sub(1) {
        let segs: Vec<(usize, usize)> = plan
            .segments
            .iter()
            .filter(|s| {
                plan.elems[s.from].rank as usize == r && !s.edge_id.starts_with("gb:")
            })
            .map(|s| (s.from, s.to))
            .collect();
        if segs.is_empty() {
            continue;
        }
        total += plotgram_algo::crossing::count_bipartite_crossings(
            &plan.layers[r],
            &plan.layers[r + 1],
            &segs,
        );
    }
    total
}

/// Adjacent swaps accepted only when they strictly reduce total crossings
/// against both neighboring layers. No group-path guard — clamps are
/// re-packed after the pass so intervals stay contiguous.
fn transpose_pass(plan: &mut PlanGraph) {
    let budget = plan.elems.len() + plan.layers.len() * 4 + 32;
    for _ in 0..budget {
        let mut improved = false;
        for r in 0..plan.layers.len() {
            let mut i = 0;
            while i + 1 < plan.layers[r].len() {
                let before = local_crossings(plan, r);
                plan.layers[r].swap(i, i + 1);
                let after = local_crossings(plan, r);
                if after < before {
                    improved = true;
                } else {
                    plan.layers[r].swap(i, i + 1); // revert
                }
                i += 1;
            }
        }
        if !improved {
            break;
        }
    }
}

fn local_crossings(plan: &PlanGraph, r: usize) -> u64 {
    let mut total = 0u64;
    if r > 0 {
        let segs: Vec<(usize, usize)> = plan
            .segments
            .iter()
            .filter(|s| {
                plan.elems[s.to].rank as usize == r && !s.edge_id.starts_with("gb:")
            })
            .map(|s| (s.from, s.to))
            .collect();
        if !segs.is_empty() {
            total += plotgram_algo::crossing::count_bipartite_crossings(
                &plan.layers[r - 1],
                &plan.layers[r],
                &segs,
            );
        }
    }
    if r + 1 < plan.layers.len() {
        let segs: Vec<(usize, usize)> = plan
            .segments
            .iter()
            .filter(|s| {
                plan.elems[s.from].rank as usize == r && !s.edge_id.starts_with("gb:")
            })
            .map(|s| (s.from, s.to))
            .collect();
        if !segs.is_empty() {
            total += plotgram_algo::crossing::count_bipartite_crossings(
                &plan.layers[r],
                &plan.layers[r + 1],
                &segs,
            );
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::hierarchical::compose::boundary::insert_group_boundaries;
    use crate::layout::hierarchical::model::{BoundarySide, ElemKey, Segment};

    fn plain_elem(id: &str, rank: u32, group: &[&str]) -> Elem {
        Elem {
            key: ElemKey::Real(id.to_string()),
            group_path: group.iter().map(|s| s.to_string()).collect(),
            rank,
        }
    }

    /// K3,3-ish crossing graph: layer0 = [a0,a1], layer1 = [b0,b1],
    /// edges wired so the identity order has crossings and a rearrangement
    /// removes them.
    fn crossing_plan() -> PlanGraph {
        let elems = vec![
            plain_elem("a0", 0, &[]),
            plain_elem("a1", 0, &[]),
            plain_elem("b0", 1, &[]),
            plain_elem("b1", 1, &[]),
        ];
        let segments = vec![
            Segment {
                edge_id: "e0".into(),
                ordinal: 0,
                from: 0,
                to: 3,
            }, // a0-b1
            Segment {
                edge_id: "e1".into(),
                ordinal: 0,
                from: 1,
                to: 2,
            }, // a1-b0
        ];
        let layers = vec![vec![0, 1], vec![2, 3]];
        let index_of = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let decl_index = vec![0, 1, 2, 3];
        PlanGraph {
            elems,
            index_of,
            decl_index,
            segments,
            layers,
        }
    }

    #[test]
    fn ordering_removes_a_crossing() {
        let mut plan = crossing_plan();
        assert_eq!(total_crossings(&plan), 1);
        order_layers(&mut plan, &BTreeSet::new(), 16.0);
        assert_eq!(total_crossings(&plan), 0);
    }

    #[test]
    fn group_members_stay_contiguous_after_ordering() {
        // Layer 1 has a 2-member group {g0,g1} interleaved (by declaration)
        // with an ungrouped node u. After boundary insert + ordering, members
        // must sit between the group's Left/Right clamps.
        let elems = vec![
            plain_elem("s0", 0, &[]),
            plain_elem("s1", 0, &[]),
            plain_elem("s2", 0, &[]),
            plain_elem("g0", 1, &["g"]),
            plain_elem("u", 1, &[]),
            plain_elem("g1", 1, &["g"]),
        ];
        let segments = vec![
            Segment {
                edge_id: "e0".into(),
                ordinal: 0,
                from: 0,
                to: 3,
            },
            Segment {
                edge_id: "e1".into(),
                ordinal: 0,
                from: 2,
                to: 5,
            },
            Segment {
                edge_id: "e2".into(),
                ordinal: 0,
                from: 1,
                to: 4,
            },
        ];
        let layers = vec![vec![0, 1, 2], vec![3, 4, 5]];
        let index_of = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let decl_index = vec![0, 1, 2, 3, 4, 5];
        let mut plan = PlanGraph {
            elems,
            index_of,
            decl_index,
            segments,
            layers,
        };

        insert_group_boundaries(&mut plan);
        order_layers(&mut plan, &BTreeSet::new(), 16.0);

        let layer1 = &plan.layers[1];
        let left = layer1.iter().position(|&e| {
            matches!(
                &plan.elems[e].key,
                ElemKey::GroupBoundary {
                    group,
                    side: BoundarySide::Left,
                    ..
                } if group == "g"
            )
        });
        let right = layer1.iter().position(|&e| {
            matches!(
                &plan.elems[e].key,
                ElemKey::GroupBoundary {
                    group,
                    side: BoundarySide::Right,
                    ..
                } if group == "g"
            )
        });
        let (left, right) = (
            left.expect("g Left boundary"),
            right.expect("g Right boundary"),
        );
        assert!(left < right, "Left must precede Right: {layer1:?}");
        let members: Vec<usize> = layer1
            .iter()
            .enumerate()
            .filter(|(_, &e)| {
                matches!(&plan.elems[e].key, ElemKey::Real(id) if id == "g0" || id == "g1")
            })
            .map(|(i, _)| i)
            .collect();
        assert_eq!(members.len(), 2);
        assert!(
            members.iter().all(|&i| i > left && i < right),
            "members must sit between clamps: left={left} right={right} members={members:?}"
        );
    }

    #[test]
    fn deterministic_rerun() {
        let mut p1 = crossing_plan();
        let mut p2 = crossing_plan();
        order_layers(&mut p1, &BTreeSet::new(), 16.0);
        order_layers(&mut p2, &BTreeSet::new(), 16.0);
        assert_eq!(p1.layers, p2.layers);
    }

    /// Critical marks double the ordering pull of real-real / real-virtual
    /// segments (edge-parameters §2.5).
    #[test]
    fn critical_edge_doubles_ordering_weight() {
        let elems = vec![
            plain_elem("a0", 0, &[]),
            plain_elem("a1", 0, &[]),
            plain_elem("m0", 1, &[]),
            plain_elem("m1", 1, &[]),
        ];
        let segments = vec![
            Segment {
                edge_id: "e0".into(),
                ordinal: 0,
                from: 0,
                to: 2,
            },
            Segment {
                edge_id: "e1".into(),
                ordinal: 0,
                from: 1,
                to: 2,
            },
        ];
        let layers = vec![vec![0, 1], vec![2, 3]];
        let index_of: BTreeMap<ElemKey, usize> = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let plan_of = || PlanGraph {
            elems: elems.clone(),
            index_of: index_of.clone(),
            decl_index: vec![0, 1, 2, 3],
            segments: segments.clone(),
            layers: layers.clone(),
        };

        let mut p = plan_of();
        order_layers(&mut p, &BTreeSet::new(), 16.0);
        assert_eq!(p.layers[1], vec![2, 3]);

        let crit: BTreeSet<String> = ["e0".to_string()].into_iter().collect();
        let adj = build_adjacency(&plan_of(), &crit, 16.0);
        let ups: Vec<f64> = adj.up[2].iter().map(|&(_, w)| w).collect();
        assert_eq!(ups, vec![2.0, 1.0], "critical real-real must weigh 2x");

        let virt_a = Elem {
            key: ElemKey::Virtual {
                edge_id: "va".into(),
                ordinal: 0,
            },
            group_path: Vec::new(),
            rank: 1,
        };
        let virt_b = Elem {
            key: ElemKey::Virtual {
                edge_id: "vb".into(),
                ordinal: 1,
            },
            group_path: Vec::new(),
            rank: 2,
        };
        assert_eq!(
            segment_weight(&virt_a, &virt_b, true, 16.0),
            8.0,
            "virtual-virtual corridor weight must not scale"
        );
    }

    /// Wide layers (≥18 elems) used to panic driftsort when `cmp_key` was not a
    /// total order; regression for review §2.1.
    fn wide_layer_plan(width: usize) -> PlanGraph {
        let n = width;
        let mut elems: Vec<Elem> = Vec::with_capacity(2 * n + 4);
        for i in 0..n {
            let id = format!("n{i}");
            elems.push(plain_elem(&id, 0, &[]));
        }
        for i in 0..n {
            let id = format!("m{i}");
            elems.push(plain_elem(&id, 1, &[]));
        }
        elems.push(plain_elem("iso_l0_a", 0, &[]));
        elems.push(plain_elem("iso_l0_b", 0, &[]));
        elems.push(plain_elem("iso_l1_a", 1, &[]));
        elems.push(plain_elem("iso_l1_b", 1, &[]));

        let iso_l0_a = 2 * n;
        let iso_l0_b = 2 * n + 1;
        let iso_l1_a = 2 * n + 2;
        let iso_l1_b = 2 * n + 3;

        let segments: Vec<Segment> = (0..n)
            .map(|i| Segment {
                edge_id: format!("e{i}"),
                ordinal: 0,
                from: i,
                to: n + ((i * 7 + 3) % n),
            })
            .collect();

        let layer0: Vec<usize> = (0..n).chain([iso_l0_a, iso_l0_b]).collect();
        let layer1: Vec<usize> = (n..2 * n).chain([iso_l1_a, iso_l1_b]).collect();
        let layers = vec![layer0, layer1];

        let index_of = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let decl_index: Vec<usize> = (0..elems.len()).collect();

        PlanGraph {
            elems,
            index_of,
            decl_index,
            segments,
            layers,
        }
    }

    #[test]
    fn ordering_survives_wide_layers() {
        for width in [18, 32, 64] {
            let mut plan = wide_layer_plan(width);
            let layer_len = width + 2;
            order_layers(&mut plan, &BTreeSet::new(), 16.0);
            assert_eq!(plan.layers[0].len(), layer_len, "width={width}");
            assert_eq!(plan.layers[1].len(), layer_len, "width={width}");
        }
    }
}
