//! P6 ordering: median + transpose + best-snapshot, group-contiguous.
//!
//! Group containment is kept via **recursive block ordering** rather than
//! literal boundary-dummy nodes (see
//! `docs/design/layout/hierarchical/notes/2026-08-02-mvp-scope.md` §2.2):
//! each layer's elements are grouped into a tree of blocks by
//! [`Elem::group_path`]; sorting and transpose only ever reorder siblings
//! within the same block, so same-group elements can never be split apart by
//! an unrelated element.

use std::cmp::Ordering;
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

/// Segment weight with the author critical-path mark: chain segments of a
/// critical edge pull twice as hard (real-real / real-virtual only — the
/// virtual-virtual corridor weight stays 8.0 so corridor shaping is not
/// out-weighted; edge-parameters §2.5).
fn segment_weight(a: &Elem, b: &Elem, critical: bool) -> f64 {
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

fn build_adjacency(plan: &PlanGraph, critical: &BTreeSet<String>) -> Adjacency {
    let n = plan.elems.len();
    let mut up = vec![Vec::new(); n];
    let mut down = vec![Vec::new(); n];
    for s in &plan.segments {
        let w = segment_weight(
            &plan.elems[s.from],
            &plan.elems[s.to],
            critical.contains(&s.edge_id),
        );
        down[s.from].push((s.to, w));
        up[s.to].push((s.from, w));
    }
    for v in up.iter_mut().chain(down.iter_mut()) {
        v.sort_unstable_by_key(|&(e, _)| e);
    }
    Adjacency { up, down }
}

pub fn order_layers(plan: &mut PlanGraph, critical: &BTreeSet<String>) {
    if plan.layers.len() < 2 {
        return; // nothing to reorder
    }
    let adj = build_adjacency(plan, critical);

    // Contiguity is a hard invariant, not a crossing-driven preference: make
    // every layer block-contiguous *before* the crossing-minimizing sweeps
    // start, so `best` is never overwritten back to a (possibly interleaved)
    // pre-ordering snapshot when no sweep happens to improve crossings.
    for r in 0..plan.layers.len() {
        let blocks = build_blocks(&plan.layers[r], 0, &plan.elems);
        let mut flat = Vec::with_capacity(plan.layers[r].len());
        flatten(&blocks, &mut flat);
        plan.layers[r] = flat;
    }

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
        transpose_pass(plan, &adj);

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
        if !plan.elems[s.from].key.is_virtual() && !plan.elems[s.to].key.is_virtual() {
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
            if plan.elems[a].key.is_virtual() || plan.elems[b].key.is_virtual() {
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
            let blocks = build_blocks(&plan.layers[r], 0, &plan.elems);
            let mut flat = Vec::new();
            flatten(&blocks, &mut flat);
            let layer_now = plan.layers[r].clone();
            if flat != layer_now || total_crossings(plan) > base || score(plan) > before {
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

enum BlockNode {
    Leaf(usize),
    Group { children: Vec<BlockNode> },
}

/// Stable-partition `members` by `group_path[depth]` into a block tree —
/// **all** members sharing a key end up in the same block regardless of
/// whether they were already adjacent in `members` (a prior sweep's
/// transpose/median step could have interleaved them), ordered by each key's
/// first appearance for determinism. This is what actually *establishes* and
/// then *preserves* group contiguity — see module doc.
fn build_blocks(members: &[usize], depth: usize, elems: &[Elem]) -> Vec<BlockNode> {
    let mut key_order: Vec<Option<String>> = Vec::new();
    let mut buckets: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for &m in members {
        let seg = elems[m].group_path.get(depth).cloned();
        let key_idx = match key_order.iter().position(|k| k == &seg) {
            Some(i) => i,
            None => {
                key_order.push(seg);
                key_order.len() - 1
            }
        };
        buckets.entry(key_idx).or_default().push(m);
    }

    let mut out = Vec::new();
    for (key_idx, seg) in key_order.into_iter().enumerate() {
        let run = &buckets[&key_idx];
        match seg {
            None => out.extend(run.iter().map(|&m| BlockNode::Leaf(m))),
            Some(_) => out.push(BlockNode::Group {
                children: build_blocks(run, depth + 1, elems),
            }),
        }
    }
    out
}

fn flatten(blocks: &[BlockNode], out: &mut Vec<usize>) {
    for b in blocks {
        match b {
            BlockNode::Leaf(e) => out.push(*e),
            BlockNode::Group { children } => flatten(children, out),
        }
    }
}

/// Median + weighted-barycenter over neighbor positions in the reference
/// layer, pooled recursively for `Group` blocks.
struct Key {
    median: Option<f64>,
    barycenter: Option<f64>,
    /// Position within the sibling list *before* this sort (stable fallback).
    prev_pos: usize,
    /// Smallest declaration index among this block's leaves (final tie-break).
    repr_decl: usize,
}

fn pooled_neighbor_positions(
    block: &BlockNode,
    neighbors_of: &[Vec<(usize, f64)>],
    ref_pos: &BTreeMap<usize, usize>,
    decl_index: &[usize],
    out: &mut Vec<(f64, f64)>, // (position, weight)
    repr_decl: &mut usize,
) {
    match block {
        BlockNode::Leaf(e) => {
            *repr_decl = (*repr_decl).min(decl_index[*e]);
            for &(n, w) in &neighbors_of[*e] {
                if let Some(&p) = ref_pos.get(&n) {
                    out.push((p as f64, w));
                }
            }
        }
        BlockNode::Group { children } => {
            for c in children {
                pooled_neighbor_positions(
                    c,
                    neighbors_of,
                    ref_pos,
                    decl_index,
                    out,
                    repr_decl,
                );
            }
        }
    }
}

fn median_of(mut positions: Vec<f64>) -> Option<f64> {
    if positions.is_empty() {
        return None;
    }
    positions.sort_by(|a, b| a.partial_cmp(b).unwrap());
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

fn sort_blocks(
    blocks: &mut Vec<BlockNode>,
    neighbors_of: &[Vec<(usize, f64)>],
    ref_pos: &BTreeMap<usize, usize>,
    decl_index: &[usize],
) {
    for b in blocks.iter_mut() {
        if let BlockNode::Group { children } = b {
            sort_blocks(children, neighbors_of, ref_pos, decl_index);
        }
    }

    let mut keyed: Vec<(Key, BlockNode)> = std::mem::take(blocks)
        .into_iter()
        .enumerate()
        .map(|(prev_pos, b)| {
            let mut pooled = Vec::new();
            let mut repr_decl = usize::MAX;
            pooled_neighbor_positions(
                &b,
                neighbors_of,
                ref_pos,
                decl_index,
                &mut pooled,
                &mut repr_decl,
            );
            let positions: Vec<f64> = pooled.iter().map(|(p, _)| *p).collect();
            let key = Key {
                median: median_of(positions),
                barycenter: weighted_barycenter(&pooled),
                prev_pos,
                repr_decl,
            };
            (key, b)
        })
        .collect();

    keyed.sort_by(|(a, _), (b, _)| cmp_key(a, b));
    *blocks = keyed.into_iter().map(|(_, b)| b).collect();
}

fn cmp_key(a: &Key, b: &Key) -> Ordering {
    if let (Some(x), Some(y)) = (a.median, b.median) {
        if (x - y).abs() > EPS {
            return x.partial_cmp(&y).unwrap();
        }
    }
    if let (Some(x), Some(y)) = (a.barycenter, b.barycenter) {
        if (x - y).abs() > EPS {
            return x.partial_cmp(&y).unwrap();
        }
    }
    a.prev_pos
        .cmp(&b.prev_pos)
        .then(a.repr_decl.cmp(&b.repr_decl))
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

    let mut blocks = build_blocks(&plan.layers[r], 0, &plan.elems);
    sort_blocks(
        &mut blocks,
        neighbors_of,
        &ref_pos,
        &plan.decl_index,
    );
    let mut flat = Vec::with_capacity(plan.layers[r].len());
    flatten(&blocks, &mut flat);
    plan.layers[r] = flat;
}

fn total_crossings(plan: &PlanGraph) -> u64 {
    let mut total = 0u64;
    for r in 0..plan.layers.len().saturating_sub(1) {
        let segs: Vec<(usize, usize)> = plan
            .segments
            .iter()
            .filter(|s| plan.elems[s.from].rank as usize == r)
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

/// Adjacent-leaf swaps within the same immediate block (same `group_path`),
/// accepted only when they strictly reduce total crossings against both
/// neighboring layers.
fn transpose_pass(plan: &mut PlanGraph, adj: &Adjacency) {
    let budget = plan.elems.len() + plan.layers.len() * 4 + 32;
    for _ in 0..budget {
        let mut improved = false;
        for r in 0..plan.layers.len() {
            let mut i = 0;
            while i + 1 < plan.layers[r].len() {
                let (u, v) = (plan.layers[r][i], plan.layers[r][i + 1]);
                if plan.elems[u].group_path != plan.elems[v].group_path {
                    i += 1;
                    continue;
                }
                let before = local_crossings(plan, adj, r);
                plan.layers[r].swap(i, i + 1);
                let after = local_crossings(plan, adj, r);
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

fn local_crossings(plan: &PlanGraph, _adj: &Adjacency, r: usize) -> u64 {
    let mut total = 0u64;
    if r > 0 {
        let segs: Vec<(usize, usize)> = plan
            .segments
            .iter()
            .filter(|s| plan.elems[s.to].rank as usize == r)
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
            .filter(|s| plan.elems[s.from].rank as usize == r)
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
    use crate::layout::hierarchical::model::{ElemKey, Segment};

    fn plain_elem(id: &str, rank: u32, group: &[&str]) -> Elem {
        Elem {
            key: ElemKey::Real(id.to_string()),
            group_path: group.iter().map(|s| s.to_string()).collect(),
            rank,
        }
    }

    /// K3,3-ish crossing graph: layer0 = [a0,a1,a2], layer1 = [b0,b1,b2],
    /// edges wired so the identity order has crossings and a rearrangement
    /// removes them (a "crossing" pattern: a0-b1, a1-b0).
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
        order_layers(&mut plan, &BTreeSet::new());
        assert_eq!(total_crossings(&plan), 0);
    }

    #[test]
    fn group_members_stay_contiguous_after_ordering() {
        // Layer 1 has a 2-member group {g0,g1} interleaved (by declaration)
        // with an ungrouped node u; wiring pulls u between them by median,
        // but group contiguity must still hold after ordering.
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

        order_layers(&mut plan, &BTreeSet::new());

        let layer1 = &plan.layers[1];
        let g_positions: Vec<usize> = layer1
            .iter()
            .enumerate()
            .filter(|(_, &e)| plan.elems[e].group_path == vec!["g".to_string()])
            .map(|(i, _)| i)
            .collect();
        assert_eq!(g_positions.len(), 2);
        assert_eq!(
            g_positions[1] - g_positions[0],
            1,
            "group members must stay adjacent: {:?}",
            g_positions
        );
    }

    #[test]
    fn deterministic_rerun() {
        let mut p1 = crossing_plan();
        let mut p2 = crossing_plan();
        order_layers(&mut p1, &BTreeSet::new());
        order_layers(&mut p2, &BTreeSet::new());
        assert_eq!(p1.layers, p2.layers);
    }

    /// Critical marks double the ordering pull of real-real / real-virtual
    /// segments (edge-parameters §2.5): a tie-broken sibling order flips
    /// toward the critical source once its weight doubles.
    #[test]
    fn critical_edge_doubles_ordering_weight() {
        // layer0 = [a0, a1] (positions 0, 1); layer1 = [m0, m1] where m0 is
        // wired to both sources. m0's neighbor median/barycenter is 0.5 for
        // equal weights, exactly between m0 (prev_pos 0) and m1 (prev_pos 1):
        // both keys tie → stable prev_pos keeps [m0, m1]. Critical `e0`
        // shifts the barycenter to 1/3 and (as the sole median) sorts m0 to
        // the left; symmetric inputs with the mark on `e1` instead sort m1's
        // pull harder — observed via m0 moving right.
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

        // Baseline: no critical marks → tie keeps declaration order.
        let mut p = plan_of();
        order_layers(&mut p, &BTreeSet::new());
        assert_eq!(p.layers[1], vec![2, 3]);

        // Weight sanity: the critical segment's weight doubles.
        let crit: BTreeSet<String> = ["e0".to_string()].into_iter().collect();
        let adj = build_adjacency(&plan_of(), &crit);
        let ups: Vec<f64> = adj.up[2].iter().map(|&(_, w)| w).collect();
        assert_eq!(ups, vec![2.0, 1.0], "critical real-real must weigh 2x");

        // Virtual-virtual segments never scale (corridor weight stays 8.0).
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
            segment_weight(&virt_a, &virt_b, true),
            8.0,
            "virtual-virtual corridor weight must not scale"
        );
    }
}
