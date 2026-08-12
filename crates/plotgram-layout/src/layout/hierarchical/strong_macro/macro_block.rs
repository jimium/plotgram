//! SM-C macro-block writer: the **single writer** of group frame geometry on
//! the StrongMacro path (strong-macro.md §5.2). Recursively (SM-2): packs
//! each block's content bbox + pads, stacks scope rows by super rank on the
//! main axis, and center-aligns every row against the widest one — the same
//! mechanics at every nesting level.
//!
//! SM-3: cross-entry edge counts publish gap lower bounds on a scope-local
//! [`DemandBoard`] (row seams + within-row adjacencies), and cross-row edge
//! weights drive a fixed-budget alignment sweep on row offsets —
//! `J = Σ w_ab × ((o_a + cx_a) − (o_b + cx_b))²` minimized by Gauss-Seidel,
//! starting from the centered placement (no cross-row edges ⇒ bit-identical).
//!
//! The engine finalize derives a group frame as `union(member frames ∪
//! nested child frames) + pad`; placing child frames and content at
//! `frame origin + pad` makes that union reproduce every block frame exactly
//! (one pad layer per level, never two — strong-macro.md §5.4).

use std::collections::BTreeMap;

use plotgram_model::geometry::Rect;

use crate::layout::hierarchical::demand::{publish_macro_pair_demand, DemandBoard, DemandKey};
use crate::layout::hierarchical::group_frame::{group_top_pad, GROUP_FRAME_GAP, GROUP_PAD};
use crate::layout::hierarchical::params::HierarchicalParams;

use super::Block;

/// Fixed Gauss-Seidel budget for the row-alignment sweep (SM-3).
const MACRO_ALIGN_SWEEPS: u32 = 8;

/// Cross-entry edge statistics of one scope (SM-3), keyed by the ordered
/// slot pair `(lo, hi)` in scope declaration order.
pub(super) struct ScopePairStats {
    /// `(slot_lo, slot_hi)` → (edge count, sum of edge weights).
    pub pairs: BTreeMap<(usize, usize), (usize, f64)>,
}

/// Place the whole block tree: recursive scope placement from the canvas
/// origin, then global content-origin propagation. `stats` maps a scope key
/// (`None` = top scope, `Some(container block)` = its entries) to the
/// cross-entry edge statistics feeding demand + alignment.
pub(super) fn place_blocks(
    blocks: &mut [Block],
    top_scope: &[usize],
    params: &HierarchicalParams,
    stats: &BTreeMap<Option<usize>, ScopePairStats>,
) {
    place_scope(blocks, top_scope, params, stats, None);
    propagate_origins(blocks, top_scope, (0.0, 0.0));
}

/// Place one scope's entries: children first (post-order — a container's
/// size derives from its placed children), pack each entry, then row-stack
/// all entries from origin (0, 0) in this scope's content space. Writes
/// `local_frame` relative to the parent scope's content origin.
fn place_scope(
    blocks: &mut [Block],
    scope: &[usize],
    params: &HierarchicalParams,
    stats: &BTreeMap<Option<usize>, ScopePairStats>,
    key: Option<usize>,
) {
    for &bi in scope {
        let children = blocks[bi].child_idx.clone();
        if !children.is_empty() {
            place_scope(blocks, &children, params, stats, Some(bi));
        }
        pack(blocks, bi);
    }
    place_rows(blocks, scope, params, stats.get(&key));
}

/// Pack one block: content envelope + one pad layer (label band on top when
/// labeled). Leaf content = intra bbox; container content = envelope of the
/// placed child frames (origin-based by construction of [`place_rows`]).
fn pack(blocks: &mut [Block], bi: usize) {
    let bbox = if blocks[bi].is_container() {
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for &c in &blocks[bi].child_idx {
            let f = blocks[c].local_frame;
            min_x = min_x.min(f.x);
            min_y = min_y.min(f.y);
            max_x = max_x.max(f.right());
            max_y = max_y.max(f.bottom());
        }
        Rect::new(min_x, min_y, max_x - min_x, max_y - min_y)
    } else {
        blocks[bi].intra.content_bbox
    };
    match &blocks[bi].group_id {
        Some(gid) => {
            let top = group_top_pad(blocks[bi].labeled.contains(gid));
            blocks[bi].width = bbox.width + GROUP_PAD * 2.0;
            blocks[bi].height = bbox.height + top + GROUP_PAD;
            blocks[bi].content_pad = (GROUP_PAD, top);
        }
        None => {
            blocks[bi].width = bbox.width;
            blocks[bi].height = bbox.height;
            blocks[bi].content_pad = (0.0, 0.0);
        }
    }
}

/// Stack scope entries on the main axis by super rank: gaps resolved from
/// the scope-local demand board (SM-3), every row centered against the
/// widest one, then the alignment sweep adjusts row offsets. Deterministic
/// declaration order within rows.
fn place_rows(
    blocks: &mut [Block],
    scope: &[usize],
    params: &HierarchicalParams,
    stats: Option<&ScopePairStats>,
) {
    let max_rank = scope
        .iter()
        .map(|&bi| blocks[bi].super_rank)
        .max()
        .unwrap_or(0);
    let row_gap_base = params.layer_gap.max(GROUP_FRAME_GAP);

    // SM-3 demand: cross-entry edge counts → row-seam / within-row gap
    // lower bounds, published on a scope-local board (freeze hand-off).
    let mut board = DemandBoard::new();
    if let Some(stats) = stats {
        let mut row_counts: BTreeMap<u32, usize> = BTreeMap::new();
        let mut col_counts: BTreeMap<u32, usize> = BTreeMap::new();
        for (&(sa, sb), &(count, _)) in &stats.pairs {
            let ra = blocks[scope[sa]].super_rank;
            let rb = blocks[scope[sb]].super_rank;
            let (lo, hi) = if ra <= rb { (ra, rb) } else { (rb, ra) };
            if lo != hi {
                for seam in lo..hi {
                    *row_counts.entry(seam).or_insert(0) += count;
                }
            } else {
                // Same row: only row-adjacent entries share a col gap. A
                // long-range same-rank pair must not inflate every intermediate
                // adjacency (or gaps belonging to other rows whose slot
                // indices happen to lie between `sa` and `sb`).
                let (lo_s, hi_s) = if sa < sb { (sa, sb) } else { (sb, sa) };
                let intervening = scope[lo_s + 1..hi_s]
                    .iter()
                    .any(|&bi| blocks[bi].super_rank == ra);
                if !intervening {
                    *col_counts.entry(lo_s as u32).or_insert(0) += count;
                }
            }
        }
        publish_macro_pair_demand(
            &mut board,
            row_gap_base,
            GROUP_FRAME_GAP,
            &row_counts,
            &col_counts,
            params.edge_gap,
        );
    }
    board.freeze();
    let seam_gap = |seam: u32| {
        board
            .get(DemandKey::MacroRowGap(seam))
            .unwrap_or(row_gap_base)
    };
    let col_gap = |slot: usize| {
        board
            .get(DemandKey::MacroColGap(slot as u32))
            .unwrap_or(GROUP_FRAME_GAP)
    };

    // Pass 1: row extents (height + packed width), declaration order within;
    // within-row gaps are the resolved per-adjacency demands.
    let mut row_height = vec![0.0_f64; max_rank as usize + 1];
    let mut row_width = vec![0.0_f64; max_rank as usize + 1];
    let mut row_count = vec![0usize; max_rank as usize + 1];
    let mut prev_slot_in_row: BTreeMap<usize, usize> = BTreeMap::new();
    for (slot, &bi) in scope.iter().enumerate() {
        let r = blocks[bi].super_rank as usize;
        if let Some(&p) = prev_slot_in_row.get(&r) {
            // The previous entry of this row sits at a lower slot
            // (declaration order), so its slot key addresses the shared
            // adjacency gap.
            row_width[r] += col_gap(p);
        }
        row_height[r] = row_height[r].max(blocks[bi].height);
        row_width[r] += blocks[bi].width;
        row_count[r] += 1;
        prev_slot_in_row.insert(r, slot);
    }
    let max_row_width = row_width.iter().copied().fold(0.0_f64, f64::max);

    // Pass 2: stack rows on the main axis; the gap between consecutive
    // non-empty rows is the max seam demand across the spanned seams.
    let mut y = 0.0_f64;
    let mut prev_rank: Option<u32> = None;
    let mut row_x: BTreeMap<u32, f64> = BTreeMap::new();
    let mut inner_x: BTreeMap<usize, f64> = BTreeMap::new();
    for r in 0..=max_rank {
        if row_count[r as usize] == 0 {
            continue;
        }
        if let Some(p) = prev_rank {
            let mut gap = row_gap_base;
            for s in p..r {
                gap = gap.max(seam_gap(s));
            }
            y += gap;
        }
        let mut x = (max_row_width - row_width[r as usize]) / 2.0;
        row_x.insert(r, x);
        for (slot, &bi) in scope.iter().enumerate() {
            if blocks[bi].super_rank != r {
                continue;
            }
            inner_x.insert(bi, x);
            blocks[bi].local_frame = Rect::new(x, y, blocks[bi].width, blocks[bi].height);
            x += blocks[bi].width + col_gap(slot);
        }
        y += row_height[r as usize];
        prev_rank = Some(r);
    }

    // SM-3 alignment: cross-row edge weights pull row offsets (starts from
    // the centered placement; no cross-row pairs ⇒ untouched).
    if params.macro_align_weight > 0.0 {
        if let Some(stats) = stats {
            align_rows(blocks, scope, stats, &mut row_x, &inner_x);
        }
    }
}

/// Gauss-Seidel sweep on row offsets minimizing
/// `J = Σ w_ab × ((o_a + cx_a) − (o_b + cx_b))²` over cross-row pairs
/// (`cx` = block center relative to its row origin — independent of the row
/// offset; within-row order and gaps are fixed). Free variables up to a
/// global translation: after each sweep the first non-empty row (lowest
/// super_rank) is pinned back to offset 0, then frames are rewritten and
/// shifted so the leftmost rests at `x = 0` (centered pair-free scopes stay
/// bit-stable).
fn align_rows(
    blocks: &mut [Block],
    scope: &[usize],
    stats: &ScopePairStats,
    row_x: &mut BTreeMap<u32, f64>,
    inner_x: &BTreeMap<usize, f64>,
) {
    // Block center relative to its row origin (row offsets cancel in the
    // absolute-center difference, so this is offset-free).
    let center_of = |bi: usize| inner_x[&bi] + blocks[bi].width / 2.0;

    // Cross-row terms: (entry_a, entry_b, weight).
    let mut terms: Vec<(usize, usize, f64)> = Vec::new();
    for (&(sa, sb), &(_, w)) in &stats.pairs {
        let (ba, bb) = (scope[sa], scope[sb]);
        if blocks[ba].super_rank != blocks[bb].super_rank {
            terms.push((ba, bb, w));
        }
    }
    if terms.is_empty() {
        return;
    }

    let ranks: Vec<u32> = row_x.keys().copied().collect();
    let pin = ranks[0];
    for _ in 0..MACRO_ALIGN_SWEEPS {
        for &r in &ranks {
            let mut num = 0.0_f64;
            let mut den = 0.0_f64;
            for &(ba, bb, w) in &terms {
                let (ra, rb) = (blocks[ba].super_rank, blocks[bb].super_rank);
                if ra == r {
                    num += w * (row_x[&rb] + center_of(bb) - center_of(ba));
                    den += w;
                } else if rb == r {
                    num += w * (row_x[&ra] + center_of(ba) - center_of(bb));
                    den += w;
                }
            }
            if den > 0.0 {
                row_x.insert(r, num / den);
            }
        }
        // Kill the translation nullspace each sweep (strong-macro.md SM-3).
        let shift = row_x[&pin];
        if shift != 0.0 {
            for &r in &ranks {
                *row_x.get_mut(&r).expect("row offset present") -= shift;
            }
        }
    }

    // Reapply offsets, then normalize the global translation of frames
    // (inner_x centering can leave the pinned row's blocks off x = 0).
    let mut min_x = f64::INFINITY;
    for &bi in scope {
        let x = row_x[&blocks[bi].super_rank] + inner_x[&bi];
        min_x = min_x.min(x);
    }
    for &bi in scope {
        let f = blocks[bi].local_frame;
        let x = row_x[&blocks[bi].super_rank] + inner_x[&bi] - min_x;
        blocks[bi].local_frame = Rect::new(x, f.y, f.width, f.height);
    }
}

/// Top-down global origin accumulation: global content origin = parent
/// content origin + local frame origin + pads. Top-scope blocks hang off the
/// canvas origin.
fn propagate_origins(blocks: &mut [Block], scope: &[usize], origin: (f64, f64)) {
    for &bi in scope {
        let f = blocks[bi].local_frame;
        let (px, py) = blocks[bi].content_pad;
        let content = (origin.0 + f.x + px, origin.1 + f.y + py);
        blocks[bi].content_origin = content;
        let children = blocks[bi].child_idx.clone();
        if !children.is_empty() {
            propagate_origins(blocks, &children, content);
        }
    }
}
