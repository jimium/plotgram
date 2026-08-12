//! SM-B super-graph (SM-2, scoped): one super-graph per scope — the top
//! scope (root blocks) and every container block's child entries.
//! Super-edges = post-FAS directed edges crossing scope entries. Ranking
//! reuses the shared longest-path stack on a mini `RealGraph`
//! (strong-macro.md §5.1 SM-B).
//!
//! The node-level working graph is acyclic after the global FAS, but
//! contracting scope entries can reintroduce cycles at block level (e.g. a
//! reversed back-edge plus the surrounding forward chain). Each scope
//! therefore runs its own Greedy-FAS over its deduplicated entry pairs and
//! flips the working direction of the real edges behind the cut (toggling
//! `reversed` so Ink still chains in original order) — the same feasibility
//! move the global FAS performs, one level up, applied per scope in
//! post-order. `original_*` are never touched, so arrows/labels keep their
//! semantics.

use std::collections::{BTreeMap, BTreeSet};

use plotgram_engine_api::LayoutError;

use crate::layout::hierarchical::compose::rank;
use crate::layout::hierarchical::model::{RealEdge, RealGraph};

/// Deterministic deduplicated directed pairs between distinct scope entries
/// (declaration-order edge scan; BTreeSet iteration). `slots[gi]` is the
/// scope slot of node `gi` (`None` outside the scope).
fn cross_scope_pairs(slots: &[Option<usize>], real_graph: &RealGraph) -> Vec<(usize, usize)> {
    let mut pairs: BTreeSet<(usize, usize)> = BTreeSet::new();
    for e in &real_graph.edges {
        if e.undirected {
            continue;
        }
        let (Some(bs), Some(bt)) = (slots[e.working_source], slots[e.working_target]) else {
            continue;
        };
        if bs != bt {
            pairs.insert((bs, bt));
        }
    }
    pairs.into_iter().collect()
}

/// Flip the working direction of every real edge whose scope-entry pair lies
/// in this scope's feedback arc set, making the scoped block graph acyclic.
/// Toggles `reversed` alongside (the flag tracks working-vs-original
/// divergence, so an edge an earlier (deeper scope / node-level) FAS already
/// flipped un-reverses here). Must run before SM-A (cross-entry edges never
/// enter the intra solves, so local rankings are unaffected) and before
/// [`assign_scope_ranks`]. Returns the flipped edge ids — SM-A isolates
/// their endpoints into dedicated local layers so the side-corridor approach
/// never crosses a same-layer sibling.
pub(super) fn break_scope_cycles(
    slots: &[Option<usize>],
    real_graph: &mut RealGraph,
) -> BTreeSet<String> {
    let pairs = cross_scope_pairs(slots, real_graph);
    if pairs.is_empty() {
        return BTreeSet::new();
    }
    let slot_count = slots
        .iter()
        .filter_map(|s| *s)
        .max()
        .map(|m| m + 1)
        .unwrap_or(0);
    let cut = plotgram_algo::fas::greedy_fas(slot_count, &pairs);
    if cut.is_empty() {
        return BTreeSet::new();
    }
    let flip: BTreeSet<(usize, usize)> = cut.iter().map(|&i| pairs[i]).collect();
    let mut flipped_ids = BTreeSet::new();
    for e in real_graph.edges.iter_mut() {
        if e.undirected {
            continue;
        }
        let (Some(bs), Some(bt)) = (slots[e.working_source], slots[e.working_target]) else {
            continue;
        };
        if flip.contains(&(bs, bt)) {
            std::mem::swap(&mut e.working_source, &mut e.working_target);
            e.reversed = !e.reversed;
            flipped_ids.insert(e.edge_id.clone());
        }
    }
    flipped_ids
}

/// Assign a super rank to each scope entry (slot order = declaration order
/// of the scope entries). Deterministic. Requires a scope-acyclic working
/// graph (see [`break_scope_cycles`]).
pub(super) fn assign_scope_ranks(
    slots: &[Option<usize>],
    entry_count: usize,
    real_graph: &RealGraph,
) -> Result<Vec<u32>, LayoutError> {
    let ids: Vec<String> = (0..entry_count).map(|b| format!("entry{b}")).collect();
    let index_of: BTreeMap<String, usize> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.clone(), i))
        .collect();

    // Undirected edges impose no rank; self-loops were extracted pre-FAS.
    let edges: Vec<RealEdge> = cross_scope_pairs(slots, real_graph)
        .into_iter()
        .enumerate()
        .map(|(i, (s, t))| RealEdge {
            edge_id: format!("super{i}"),
            original_source: s,
            original_target: t,
            working_source: s,
            working_target: t,
            reversed: false,
            weight: 1.0,
            ..Default::default()
        })
        .collect();

    let super_graph = RealGraph {
        ids,
        index_of,
        group_path: vec![Vec::new(); entry_count],
        shapes: vec![plotgram_model::NodeShape::DEFAULT; entry_count],
        edges,
        self_loops: Vec::new(),
        intra_layer: Vec::new(),
        // Derived block-granularity graph — no partition facts of its own.
        partition: None,
        partition_cell: Vec::new(),
    };
    rank::assign_ranks(&super_graph)
}
