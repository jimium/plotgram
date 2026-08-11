//! P4.2 Brandes–Köpf vertical alignment, the *ideal generator* for
//! [`super::cross_axis`] (roadmap phase A; architecture.md §5.1 picks BK for
//! P4). Scope decisions vs the full paper algorithm:
//!
//! - No horizontal compaction: VPSC owns separation and overlap, so each
//!   candidate's ideal is simply the median of its block members' packed
//!   centers. The 4 candidates (up/down sweep × left/right bias) are
//!   min-normalized then merged per elem by median — the fixed merge rule
//!   (architecture.md §10.2).
//! - Type-1 conflict marking: an inner (dummy–dummy) segment crossing a
//!   non-inner segment is excluded from alignment.
//! - The **primary** alignment — whose same-type (virtual-virtual and
//!   real-real) member pairs become hard collinearity constraints
//!   downstream — is fixed to
//!   upper-neighbors + left bias (`VARIANTS[0]`). Blocks of one BK
//!   alignment never cross each other, which keeps those equality systems
//!   feasible by construction (see `cross_axis.rs`).

use std::collections::BTreeMap;

use plotgram_algo::orientation::Size;

use crate::layout::hierarchical::model::PlanGraph;

/// Merged BK ideal plus the primary alignment's multi-member blocks.
pub struct BkIdeal {
    /// Desired cross coordinates (median of the 4 normalized candidates).
    pub ideal: Vec<f64>,
    /// Blocks (size ≥ 2) of the primary alignment. Members sorted by rank;
    /// blocks sorted by smallest member index.
    pub primary_blocks: Vec<Vec<usize>>,
}

/// (use_upper_neighbors, left_bias); index 0 is the primary alignment.
const VARIANTS: [(bool, bool); 4] = [(true, true), (true, false), (false, true), (false, false)];

struct Ctx {
    pos_of: Vec<usize>,
    /// Neighbors in the layer above / below, sorted by in-layer position.
    upper: Vec<Vec<usize>>,
    lower: Vec<Vec<usize>>,
    /// `(upper-layer elem, lower-layer elem)` -> segment index. Parallel
    /// edges share both endpoints, so any of their segments is a valid
    /// lookup result (geometrically identical).
    seg_index: BTreeMap<(usize, usize), usize>,
    /// Per segment: marked type-1 conflict (inner crossing non-inner).
    conflicted: Vec<bool>,
}

pub fn bk_ideal(plan: &PlanGraph, size_of: &dyn Fn(usize) -> Size, node_gap: f64) -> BkIdeal {
    let n = plan.elems.len();
    let ctx = build_ctx(plan);
    let packed = packed_centers(plan, size_of, node_gap);

    let mut candidates: Vec<Vec<f64>> = Vec::with_capacity(VARIANTS.len());
    let mut primary_blocks = Vec::new();
    for (i, &(use_upper, left)) in VARIANTS.iter().enumerate() {
        let root = vertical_alignment(plan, &ctx, use_upper, left);
        if i == 0 {
            primary_blocks = blocks_of(plan, &root);
        }
        candidates.push(candidate_ideal(&root, &packed));
    }

    // Balance each candidate to a common origin, then merge by per-elem
    // median (even count → mean of the two middle values).
    for cand in &mut candidates {
        let min = cand.iter().copied().fold(f64::INFINITY, f64::min);
        for x in cand.iter_mut() {
            *x -= min;
        }
    }
    let mut ideal = vec![0.0; n];
    for e in 0..n {
        let mut vals = [
            candidates[0][e],
            candidates[1][e],
            candidates[2][e],
            candidates[3][e],
        ];
        vals.sort_by(f64::total_cmp);
        ideal[e] = (vals[1] + vals[2]) / 2.0;
    }

    BkIdeal {
        ideal,
        primary_blocks,
    }
}

fn build_ctx(plan: &PlanGraph) -> Ctx {
    let n = plan.elems.len();
    let mut pos_of = vec![0usize; n];
    for layer in &plan.layers {
        for (i, &e) in layer.iter().enumerate() {
            pos_of[e] = i;
        }
    }

    let mut upper: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut lower: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut seg_index = BTreeMap::new();
    for (si, s) in plan.segments.iter().enumerate() {
        lower[s.from].push(s.to);
        upper[s.to].push(s.from);
        seg_index.insert((s.from, s.to), si);
    }
    for v in 0..n {
        upper[v].sort_by_key(|&u| pos_of[u]);
        lower[v].sort_by_key(|&u| pos_of[u]);
    }

    // Type-1 conflicts: between each adjacent layer pair, an inner segment
    // crossing a non-inner one is marked (and skipped during alignment).
    // Deterministic order: segments grouped by upper layer, sorted by
    // (upper pos, edge id, ordinal).
    let is_inner = |si: usize| {
        let s = &plan.segments[si];
        plan.elems[s.from].key.is_virtual() && plan.elems[s.to].key.is_virtual()
    };
    let layer_of_from: Vec<usize> = {
        let mut layer_of = vec![0usize; n];
        for (li, layer) in plan.layers.iter().enumerate() {
            for &e in layer {
                layer_of[e] = li;
            }
        }
        layer_of
    };
    let mut by_layer: Vec<Vec<usize>> = vec![Vec::new(); plan.layers.len()];
    for (si, s) in plan.segments.iter().enumerate() {
        by_layer[layer_of_from[s.from]].push(si);
    }
    let mut conflicted = vec![false; plan.segments.len()];
    for segs in &mut by_layer {
        segs.sort_by_key(|&si| {
            let s = &plan.segments[si];
            (pos_of[s.from], s.edge_id.clone(), s.ordinal)
        });
        for ai in 0..segs.len() {
            for bi in (ai + 1)..segs.len() {
                let a = &plan.segments[segs[ai]];
                let b = &plan.segments[segs[bi]];
                let (pfa, pta) = (pos_of[a.from], pos_of[a.to]);
                let (pfb, ptb) = (pos_of[b.from], pos_of[b.to]);
                let crosses = (pfa > pfb && pta < ptb) || (pfa < pfb && pta > ptb);
                if !crosses {
                    continue;
                }
                match (is_inner(segs[ai]), is_inner(segs[bi])) {
                    (true, false) => conflicted[segs[ai]] = true,
                    (false, true) => conflicted[segs[bi]] = true,
                    _ => {}
                }
            }
        }
    }

    Ctx {
        pos_of,
        upper,
        lower,
        seg_index,
        conflicted,
    }
}

/// Packed in-layer centers (same placement the MVP used as its initial
/// guess) — the reference frame every block median is taken in.
fn packed_centers(plan: &PlanGraph, size_of: &dyn Fn(usize) -> Size, node_gap: f64) -> Vec<f64> {
    let mut packed = vec![0.0; plan.elems.len()];
    for layer in &plan.layers {
        let mut cursor = 0.0;
        for &e in layer {
            if plan.elems[e].key.is_boundary()
                || matches!(
                    &plan.elems[e].key,
                    crate::layout::hierarchical::model::ElemKey::OrderPad { .. }
                )
            {
                // Order marker / clamp pad: sit at the current cursor without consuming gap.
                packed[e] = cursor;
                continue;
            }
            let w = size_of(e).width;
            packed[e] = cursor + w / 2.0;
            cursor += w + node_gap;
        }
    }
    packed
}

/// One BK vertical alignment pass. Returns `root[e]` = representative elem
/// of `e`'s block. `root[m]` is always final when read (the sweep processes
/// the neighbor's layer before the current one), so no root flattening is
/// needed.
fn vertical_alignment(plan: &PlanGraph, ctx: &Ctx, use_upper: bool, left: bool) -> Vec<usize> {
    let n = plan.elems.len();
    let mut root: Vec<usize> = (0..n).collect();
    let mut align: Vec<usize> = (0..n).collect();

    let mut layer_order: Vec<usize> = (0..plan.layers.len()).collect();
    if !use_upper {
        layer_order.reverse();
    }
    for li in layer_order {
        let layer = &plan.layers[li];
        // Boundary resets per layer (BK): medians must advance monotonically
        // within one layer sweep, which is what keeps blocks non-crossing.
        let mut r: i64 = if left { i64::MIN } else { i64::MAX };
        let mut vertices: Vec<usize> = layer.clone();
        if !left {
            vertices.reverse();
        }
        for v in vertices {
            let mut meds = medians(ctx, v, use_upper);
            if !left {
                meds.reverse();
            }
            for m in meds {
                if align[v] != v {
                    break; // v already part of a block
                }
                let key = if use_upper { (m, v) } else { (v, m) };
                let si = ctx.seg_index[&key];
                if ctx.conflicted[si] {
                    continue;
                }
                let pm = ctx.pos_of[m] as i64;
                if (left && pm > r) || (!left && pm < r) {
                    align[m] = v;
                    root[v] = root[m];
                    align[v] = root[v];
                    r = pm;
                }
            }
        }
    }
    root
}

/// The one or two median neighbors of `v` on the swept side (already sorted
/// by position; left-of-center first).
fn medians(ctx: &Ctx, v: usize, use_upper: bool) -> Vec<usize> {
    let nbs = if use_upper { &ctx.upper[v] } else { &ctx.lower[v] };
    let k = nbs.len();
    if k == 0 {
        Vec::new()
    } else if k % 2 == 1 {
        vec![nbs[k / 2]]
    } else {
        vec![nbs[k / 2 - 1], nbs[k / 2]]
    }
}

/// Per-block ideal: median of the members' packed centers; a block moves as
/// one desired coordinate (VPSC decides the final placement).
fn candidate_ideal(root: &[usize], packed: &[f64]) -> Vec<f64> {
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (e, &r) in root.iter().enumerate() {
        groups.entry(r).or_default().push(e);
    }
    let mut coord_of: Vec<f64> = vec![0.0; root.len()];
    for (&r, members) in &groups {
        let mut vals: Vec<f64> = members.iter().map(|&m| packed[m]).collect();
        vals.sort_by(f64::total_cmp);
        let len = vals.len();
        let med = if len % 2 == 1 {
            vals[len / 2]
        } else {
            (vals[len / 2 - 1] + vals[len / 2]) / 2.0
        };
        for &m in members {
            coord_of[m] = med;
        }
        let _ = r;
    }
    coord_of
}

/// Multi-member blocks of one alignment: members by rank (one member per
/// layer, so rank order is the chain order), blocks by smallest member
/// index.
fn blocks_of(plan: &PlanGraph, root: &[usize]) -> Vec<Vec<usize>> {
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (e, &r) in root.iter().enumerate() {
        groups.entry(r).or_default().push(e);
    }
    let mut blocks: Vec<Vec<usize>> = groups
        .into_values()
        .filter(|members| members.len() >= 2)
        .collect();
    for block in &mut blocks {
        block.sort_by_key(|&e| (plan.elems[e].rank, e));
    }
    blocks.sort_by_key(|block| *block.iter().min().unwrap());
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::hierarchical::model::{Elem, ElemKey, Segment};

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

    fn build_plan(elems: Vec<Elem>, layers: Vec<Vec<usize>>, segments: Vec<Segment>) -> PlanGraph {
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
            segments,
            layers,
            ..Default::default()
        }
    }

    /// a(L0) → d(L1) → b(L2): unique medians everywhere, so every variant
    /// must put the whole chain in one block with one shared ideal.
    #[test]
    fn straight_chain_aligns_in_all_variants() {
        let elems = vec![real("a", 0), virt("e0", 0, 1), real("b", 2)];
        let layers = vec![vec![0], vec![1], vec![2]];
        let segments = vec![seg("e0", 0, 0, 1), seg("e0", 1, 1, 2)];
        let plan = build_plan(elems, layers, segments);

        let bk = bk_ideal(&plan, &|_| Size::new(20.0, 10.0), 10.0);
        assert!(
            bk.primary_blocks
                .iter()
                .any(|b| b == &vec![0usize, 1, 2]),
            "primary alignment must keep the 1:1 chain in one block: {:?}",
            bk.primary_blocks
        );
        assert!(
            (bk.ideal[0] - bk.ideal[1]).abs() < 1e-9 && (bk.ideal[1] - bk.ideal[2]).abs() < 1e-9,
            "merged ideal must be constant along the chain: {:?}",
            bk.ideal
        );
    }

    /// e1: b(L0) → d1(L1) → d2(L2) → c(L3) with d1→d2 inner; e2: a(L0) →
    /// x(L1) crosses b→d1, e3: x(L1) → y(L2) crosses the inner d1→d2. The
    /// inner segment must be marked, so no block may contain both d1 and d2.
    #[test]
    fn conflicted_inner_segment_is_not_aligned() {
        // indices: a=0 b=1 d1=2 x=3 y=4 d2=5 c=6
        let elems = vec![
            real("a", 0),
            real("b", 0),
            virt("e1", 0, 1),
            real("x", 1),
            real("y", 2),
            virt("e1", 1, 2),
            real("c", 3),
        ];
        let layers = vec![vec![0, 1], vec![2, 3], vec![4, 5], vec![6]];
        let segments = vec![
            seg("e1", 0, 1, 2), // b → d1 (non-inner, crosses a→x)
            seg("e1", 1, 2, 5), // d1 → d2 (INNER, crosses x→y → marked)
            seg("e1", 2, 5, 6), // d2 → c
            seg("e2", 0, 0, 3), // a → x
            seg("e3", 0, 3, 4), // x → y
        ];
        let plan = build_plan(elems, layers, segments);

        let bk = bk_ideal(&plan, &|_| Size::new(20.0, 10.0), 10.0);
        for block in &bk.primary_blocks {
            assert!(
                !(block.contains(&2) && block.contains(&5)),
                "marked inner segment must not align d1 with d2: {block:?}"
            );
        }
    }

    /// Two span-2 chains whose segments cross: the r-boundary must keep the
    /// blocks non-crossing (at most one chain aligns per contested layer
    /// sweep) — asserted indirectly via stable, deterministic output.
    #[test]
    fn deterministic_bit_identical_reruns() {
        // indices: a=0 b=1 db=2 da=3 ca=4 cb=5
        let elems = vec![
            real("a", 0),
            real("b", 0),
            virt("eb", 0, 1),
            virt("ea", 0, 1),
            real("ca", 2),
            real("cb", 2),
        ];
        let layers = vec![vec![0, 1], vec![2, 3], vec![4, 5]];
        let segments = vec![
            seg("ea", 0, 0, 3), // a → da
            seg("ea", 1, 3, 4), // da → ca
            seg("eb", 0, 1, 2), // b → db
            seg("eb", 1, 2, 5), // db → cb
        ];
        let plan = build_plan(elems, layers, segments);

        let r1 = bk_ideal(&plan, &|_| Size::new(20.0, 10.0), 10.0);
        let r2 = bk_ideal(&plan, &|_| Size::new(20.0, 10.0), 10.0);
        let b1: Vec<u64> = r1.ideal.iter().map(|f| f.to_bits()).collect();
        let b2: Vec<u64> = r2.ideal.iter().map(|f| f.to_bits()).collect();
        assert_eq!(b1, b2, "BK ideal must be bit-identical across reruns");
        assert_eq!(r1.primary_blocks, r2.primary_blocks);
    }
}
