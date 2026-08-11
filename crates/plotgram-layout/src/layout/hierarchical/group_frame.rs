//! Group frame geometry contract (single source for frame pads).
//!
//! Write authority (group-frame-d2.md §6.2, D₂.0): the group-frame true
//! source is the Metric — [`super::metric::group_frames`] solves per-group
//! frame variables with containment hard constraints + Fit and writes the
//! `LayoutOutput.groups` that the engine facade passes through unchanged.
//! Option B ruling (§7.2): `GroupBoundary` clamps keep sandwiching members
//! in the cross-axis solve; frame variables are solved separately afterwards.
//! The hierarchical core reserves the *same* pads upstream so sibling frames
//! can never overlap (the inter-group gap degree of freedom is written by
//! the cross-axis separation chain below; D₂.1 moves sibling separation
//! fully into the solve):
//!
//! - cross axis — [`super::metric::symmetry_objective`] includes group-boundary
//!   clamps in the hard separation chain and ties same-side clamps across
//!   ranks with hard equalities (rectangular frame column). Unrelated sibling
//!   clamp pairs reserve [`GROUP_FRAME_GAP`] directly in the solve — D₂.1
//!   moved push-apart fully into the hard set; the retired compact pass's
//!   only survivor is a pull-left-only slack closure (separation is never
//!   overridden post-solve; compactness in J is the D₂.2 backlog item).
//!   Drawn extent must match the Metric frame writer: `union(members ∪ child
//!   frames) + pads` — child frames already carry pad, so a parent is one
//!   pad wider than `union(members)+pad`;
//! - main axis — [`super::demand::publish_group_layer_gap_demand`]
//!   publishes LayerGap lower bounds from the shell bands below.
//!
//! Sibling groups that *share* a rank overlap vertically by construction; they
//! are kept disjoint by the cross-axis clamp separation instead.

use std::collections::{BTreeMap, BTreeSet};

use crate::layout::hierarchical::model::{ElemKey, PlanGraph};

/// Frame pad on left / right / bottom (and on top for unlabeled groups).
pub const GROUP_PAD: f64 = 16.0;

/// Top pad for labeled groups: must stay >= the label band height (18.0) the
/// engine draws inside the frame, plus a little breathing room.
pub const GROUP_LABEL_TOP_PAD: f64 = 24.0;

/// Minimum visual gap reserved between two sibling group frames.
/// Kept above [`GROUP_PAD`] so nested envelopes (finalize unions child frames
/// that already include pad, then pads again) still show clear breathing room.
pub const GROUP_FRAME_GAP: f64 = 24.0;

/// Top pad of one group frame: labeled groups reserve the label band.
pub fn group_top_pad(has_label: bool) -> f64 {
    if has_label {
        GROUP_LABEL_TOP_PAD
    } else {
        GROUP_PAD
    }
}

/// Real-member rank span `(min, max)` of every group named in member
/// `group_path`s (ancestors included).
pub fn group_rank_spans(plan: &PlanGraph) -> BTreeMap<&str, (u32, u32)> {
    let mut span: BTreeMap<&str, (u32, u32)> = BTreeMap::new();
    for e in &plan.elems {
        if !matches!(e.key, ElemKey::Real(_)) {
            continue;
        }
        for g in &e.group_path {
            span.entry(g.as_str())
                .and_modify(|s| {
                    s.0 = s.0.min(e.rank);
                    s.1 = s.1.max(e.rank);
                })
                .or_insert((e.rank, e.rank));
        }
    }
    span
}

/// Main-axis shell bands no Cross-track lane may pierce.
///
/// A group frame extends `GROUP_PAD` below its bottom-most members and
/// `top_pad` above its top-most members, so every rank gap adjacent to a
/// frame edge carries a forbidden band (a superset: shorter members of the
/// same rank pull the real edge further into the gap).
#[derive(Debug, Default, Clone)]
pub struct GroupShellBands {
    /// Per interior rank gap `r`: (band below rank `r`, band above rank `r+1`).
    pub gap: Vec<(f64, f64)>,
    /// Band extending above rank 0 / below the last rank.
    pub outer_top: f64,
    pub outer_bottom: f64,
}

/// Derive the shell bands from member spans. `labeled` = group ids with a
/// label (their top pad reserves the label band).
pub fn group_shell_bands(plan: &PlanGraph, labeled: &BTreeSet<String>) -> GroupShellBands {
    let span = group_rank_spans(plan);
    let n = plan.layers.len();
    let mut bands = GroupShellBands::default();
    if span.is_empty() || n == 0 {
        bands.gap = vec![(0.0, 0.0); n.saturating_sub(1)];
        return bands;
    }
    let last = (n - 1) as u32;
    for r in 0..n.saturating_sub(1) {
        let ru = r as u32;
        let below = span.values().any(|&(_, hi)| hi == ru);
        let above = span
            .iter()
            .filter(|(_, &(lo, _))| lo == ru + 1)
            .map(|(g, _)| group_top_pad(labeled.contains(*g)))
            .fold(0.0_f64, f64::max);
        bands
            .gap
            .push((if below { GROUP_PAD } else { 0.0 }, above));
    }
    bands.outer_top = span
        .iter()
        .filter(|(_, &(lo, _))| lo == 0)
        .map(|(g, _)| group_top_pad(labeled.contains(*g)))
        .fold(0.0_f64, f64::max);
    bands.outer_bottom = if span.values().any(|&(_, hi)| hi == last) {
        GROUP_PAD
    } else {
        0.0
    };
    bands
}
