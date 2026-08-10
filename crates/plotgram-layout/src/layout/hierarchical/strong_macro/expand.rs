//! SM-D expand (SM-2): normalize the recursive block-tree solutions into the
//! **same** global `PlanGraph` schema the Weak path produces
//! (strong-macro.md §5.1 SM-D). No second IR leaks past this module — the
//! shared Channel/Ink tail consumes `plan` + `canonical_frames` exactly as
//! it does for Weak.
//!
//! Global rank = top band offset (top super rank) + layer index inside the
//! block's flattened layer sequence (`block_layers` — leaves contribute their
//! intra layers; containers stack rows like bands: each row starts after the
//! **full** depth of the previous row, padding rows interleaved). Band
//! widths are the deepest top block per rank. Cross-block directed edges
//! always span layers by the same argument as the top bands: an edge into
//! row `r'` starts at offset(r') ≥ offset(r) + depth(row r) ≥ source layer
//! + 1, so span ≥ 1 regardless of the source's layer depth. Same-scope
//! same-rank undirected edges either span local layers or drop to
//! `intra_layer` side-links via `split_intra_layer`.

use std::collections::BTreeMap;

use plotgram_engine_api::LayoutError;
use plotgram_model::geometry::Rect;

use crate::layout::hierarchical::compose::properify;
use crate::layout::hierarchical::model::{ElemKey, PlanGraph, RealGraph};

use super::Block;

pub(super) struct ExpandResult {
    /// The global working graph (post `split_intra_layer`) — port finalize
    /// and the shared tail read it alongside the plan.
    pub real_graph: RealGraph,
    pub plan: PlanGraph,
    /// Final canonical frames for every plan elem (reals = translated local
    /// frames; virtuals = deterministic chain interpolation).
    pub canonical_frames: Vec<Rect>,
}

/// Flattened ordered real-node layers of one block (recursive): leaves keep
/// their intra order verbatim (write-authority — local Compose wrote it
/// once); containers stack super-rank rows like bands — row `r'` starts
/// after the full depth of row `r' - 1` (padding layers interleaved so every
/// cross-row edge spans ≥ 1), row `d` interleaving each entry's layer `d`
/// in declaration (cross) order.
fn block_layers(
    blocks: &[Block],
    bi: usize,
    cache: &mut BTreeMap<usize, Vec<Vec<String>>>,
) -> Vec<Vec<String>> {
    if let Some(v) = cache.get(&bi) {
        return v.clone();
    }
    let out = if blocks[bi].child_idx.is_empty() {
        blocks[bi].intra.layer_order.clone()
    } else {
        let max_rank = blocks[bi]
            .child_idx
            .iter()
            .map(|&c| blocks[c].super_rank)
            .max()
            .unwrap_or(0);
        // Row depths + band-style offsets (each row starts after the full
        // depth of the previous row — the same jump that keeps top-band
        // edges feasible, applied one level down).
        let mut depths = vec![0usize; max_rank as usize + 1];
        for r in 0..=max_rank {
            depths[r as usize] = blocks[bi]
                .child_idx
                .iter()
                .filter(|&&c| blocks[c].super_rank == r)
                .map(|&c| block_layers(blocks, c, cache).len())
                .max()
                .unwrap_or(0);
        }
        let mut offsets = vec![0usize; max_rank as usize + 1];
        let mut acc = 0usize;
        for r in 0..=max_rank as usize {
            offsets[r] = acc;
            acc += depths[r];
        }
        let mut rows: Vec<Vec<String>> = vec![Vec::new(); acc];
        for r in 0..=max_rank {
            let row: Vec<usize> = blocks[bi]
                .child_idx
                .iter()
                .copied()
                .filter(|&c| blocks[c].super_rank == r)
                .collect();
            let entry_layers: Vec<Vec<Vec<String>>> = row
                .iter()
                .map(|&c| block_layers(blocks, c, cache))
                .collect();
            for d in 0..depths[r as usize] {
                let target = &mut rows[offsets[r as usize] + d];
                for el in &entry_layers {
                    if d < el.len() {
                        target.extend(el[d].iter().cloned());
                    }
                }
            }
        }
        rows
    };
    cache.insert(bi, out.clone());
    out
}

pub(super) fn expand(
    mut real_graph: RealGraph,
    blocks: &[Block],
    top_scope: &[usize],
    block_of_node: &[usize],
) -> Result<ExpandResult, LayoutError> {
    // Block ancestry (child → container) and each block's top-scope root
    // (declaration-order slot in `top_scope`).
    let mut parent_of: BTreeMap<usize, usize> = BTreeMap::new();
    for (bi, b) in blocks.iter().enumerate() {
        for &c in &b.child_idx {
            parent_of.insert(c, bi);
        }
    }
    let root_top_of = |mut bi: usize| -> usize {
        while let Some(&p) = parent_of.get(&bi) {
            bi = p;
        }
        bi
    };
    // Declaration-order slot of a top-scope block (cross order for virtual
    // sections).
    let top_slot: BTreeMap<usize, usize> =
        top_scope.iter().enumerate().map(|(k, &b)| (b, k)).collect();

    let mut cache: BTreeMap<usize, Vec<Vec<String>>> = BTreeMap::new();
    let layers_of = |bi: usize, cache: &mut BTreeMap<usize, Vec<Vec<String>>>| {
        block_layers(blocks, bi, cache)
    };

    // Top bands: band width = deepest top block of that super rank.
    let max_top_rank = top_scope
        .iter()
        .map(|&bi| blocks[bi].super_rank)
        .max()
        .unwrap_or(0);
    let mut band_width = vec![0u32; max_top_rank as usize + 1];
    for &bi in top_scope {
        let r = blocks[bi].super_rank as usize;
        band_width[r] = band_width[r].max(layers_of(bi, &mut cache).len() as u32);
    }
    let mut offsets = vec![0u32; max_top_rank as usize + 1];
    let mut acc = 0u32;
    for (r, w) in band_width.iter().enumerate() {
        offsets[r] = acc;
        acc += w;
    }

    // Global rank map + per-global-layer real order (top-scope declaration
    // order — the deterministic cross order SM-C places with).
    let mut global_ranks = vec![0u32; real_graph.ids.len()];
    let total_layers = offsets[max_top_rank as usize] + band_width[max_top_rank as usize];
    let mut layer_real_order: Vec<Vec<String>> = vec![Vec::new(); total_layers as usize];
    let mut node_layer_idx: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for &bi in top_scope {
        let r = blocks[bi].super_rank as usize;
        let layers = layers_of(bi, &mut cache);
        for (l, layer) in layers.iter().enumerate() {
            let g = offsets[r] + l as u32;
            layer_real_order[g as usize].extend(layer.iter().cloned());
            for id in layer {
                node_layer_idx.insert(id.clone(), (bi, l));
            }
        }
    }
    for (gi, id) in real_graph.ids.iter().enumerate() {
        let &(bi, l) = &node_layer_idx[id];
        let r = blocks[root_top_of(bi)].super_rank as usize;
        global_ranks[gi] = offsets[r] + l as u32;
    }

    // Zero-span undirected edges become side-links; long edges get dummies.
    properify::split_intra_layer(&mut real_graph, &global_ranks);
    let mut plan = properify::properify(&real_graph, &global_ranks);

    // Per-layer order: reals verbatim from the block tree (write-authority —
    // never re-sorted globally); virtuals follow their working source's
    // top-block section when that block covers the layer, orphans trail.
    // Section order keeps dummy chains near their source macro; within a
    // section (edge_id, ordinal) is deterministic.
    let top_block_band: Vec<(u32, u32)> = top_scope
        .iter()
        .map(|&bi| {
            let r = blocks[bi].super_rank as usize;
            (offsets[r], offsets[r] + layers_of(bi, &mut cache).len() as u32)
        })
        .collect();
    let edge_source_top: BTreeMap<String, usize> = real_graph
        .edges
        .iter()
        .map(|e| {
            (
                e.edge_id.clone(),
                top_slot[&root_top_of(block_of_node[e.working_source])],
            )
        })
        .collect();

    for g in 0..plan.layers.len() {
        let layer = std::mem::take(&mut plan.layers[g]);
        let mut reals: BTreeMap<&str, usize> = BTreeMap::new();
        let mut virtuals: Vec<usize> = Vec::new();
        for &e in &layer {
            match &plan.elems[e].key {
                ElemKey::Real(id) => {
                    reals.insert(id.as_str(), e);
                }
                ElemKey::Virtual { .. } => virtuals.push(e),
                ElemKey::GroupBoundary { .. } | ElemKey::OrderPad { .. } => {
                    // StrongMacro plans never insert boundaries / pads.
                    unreachable!("strong-macro plan carries no boundary elems");
                }
            }
        }

        let mut ordered: Vec<usize> = Vec::with_capacity(layer.len());
        for id in &layer_real_order[g] {
            if let Some(&e) = reals.get(id.as_str()) {
                ordered.push(e);
            }
        }
        virtuals.sort_by_key(|&e| {
            let (edge_id, ordinal) = virtual_sort_key(&plan, e);
            let section = edge_source_top[&edge_id];
            let (start, end) = top_block_band[section];
            let covered = (start..end).contains(&(g as u32));
            (
                if covered { section } else { usize::MAX },
                edge_id,
                ordinal,
            )
        });
        ordered.extend(virtuals);
        plan.layers[g] = ordered;
    }

    // Frames: reals translate from their leaf block's local solution
    // (content origin = global frame origin + pads, so finalize's
    // union+pad reproduces every block frame level by level).
    let mut canonical_frames = vec![Rect::new(0.0, 0.0, 0.0, 0.0); plan.elems.len()];
    for b in blocks {
        if b.is_container() {
            continue;
        }
        let bbox = b.intra.content_bbox;
        let (ox, oy) = b.content_origin;
        let dx = ox - bbox.x;
        let dy = oy - bbox.y;
        for (id, &li) in b.intra.local_real_elem.iter() {
            let lf = b.intra.local_real_frames[li];
            let f = Rect::new(lf.x + dx, lf.y + dy, lf.width, lf.height);
            let ge = plan.index_of[&ElemKey::Real(id.clone())];
            canonical_frames[ge] = f;
        }
    }

    // Virtuals: deterministic interpolation along each edge chain between the
    // real endpoints (source bottom → target top; centers on the cross axis).
    let mut segs_by_edge: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (si, s) in plan.segments.iter().enumerate() {
        segs_by_edge.entry(s.edge_id.as_str()).or_default().push(si);
    }
    for (_, mut seg_idxs) in segs_by_edge {
        seg_idxs.sort_by_key(|&si| plan.segments[si].ordinal);
        let src_elem = plan.segments[*seg_idxs.first().unwrap()].from;
        let dst_elem = plan.segments[*seg_idxs.last().unwrap()].to;
        let r0 = plan.elems[src_elem].rank;
        let r1 = plan.elems[dst_elem].rank;
        let span = r1 - r0;
        let sf = canonical_frames[src_elem];
        let tf = canonical_frames[dst_elem];
        let sx = sf.x + sf.width / 2.0;
        let tx = tf.x + tf.width / 2.0;
        // Properify should never emit mid-chain virtuals on a zero-span edge
        // (those become `intra_layer`); still guard against divide-by-zero.
        if span == 0 {
            let x = (sx + tx) / 2.0;
            let y = (sf.bottom() + tf.y) / 2.0;
            for &si in &seg_idxs {
                let s = &plan.segments[si];
                if plan.elems[s.to].key.is_virtual() {
                    canonical_frames[s.to] = Rect::new(x, y, 0.0, 0.0);
                }
            }
            continue;
        }
        for &si in &seg_idxs {
            let s = &plan.segments[si];
            if plan.elems[s.to].key.is_virtual() {
                let g = plan.elems[s.to].rank;
                let t = (g - r0) as f64 / span as f64;
                let x = sx + (tx - sx) * t;
                let y = sf.bottom() + (tf.y - sf.bottom()) * t;
                canonical_frames[s.to] = Rect::new(x, y, 0.0, 0.0);
            }
        }
    }

    Ok(ExpandResult {
        real_graph,
        plan,
        canonical_frames,
    })
}

/// Deterministic virtual identity inside a section: (edge_id, ordinal).
fn virtual_sort_key(plan: &PlanGraph, e: usize) -> (String, u32) {
    match &plan.elems[e].key {
        ElemKey::Virtual { edge_id, ordinal } => (edge_id.clone(), *ordinal),
        _ => unreachable!(),
    }
}
