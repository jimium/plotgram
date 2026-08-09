//! MetricBudget DemandBoard (channel-d1.md · D1.3.4 Corridor Demand).
//!
//! Channel / TrackOrder publish LayerGap lower bounds **before** Metric
//! consumes them. Freeze is the only legal hand-off (write-authority H3):
//! no publish after freeze, no Metric callback into the board.

use std::collections::{BTreeMap, BTreeSet};

use crate::layout::hierarchical::compose::track_order::TrackOrderPlan;
use crate::layout::hierarchical::group_frame::{group_shell_bands, GROUP_FRAME_GAP};
use crate::layout::hierarchical::model::PlanGraph;

/// Typed MetricBudget demand keys (coordinate-and-demand.md §1.1 subset).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DemandKey {
    /// Main-axis gap between layer `r` and `r+1` (px lower bound).
    LayerGap(u32),
}

/// Max-merge demand board with a single freeze epoch (MetricBudget).
#[derive(Debug, Clone, Default)]
pub struct DemandBoard {
    values: BTreeMap<DemandKey, f64>,
    frozen: bool,
}

impl DemandBoard {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_frozen(&self) -> bool {
        self.frozen
    }

    /// Publish a lower bound. Same key keeps the max. Panics after freeze
    /// or when `lower_bound` is non-finite / negative (InternalInvariant).
    pub fn publish(&mut self, key: DemandKey, lower_bound: f64) {
        assert!(
            !self.frozen,
            "DemandBoard: publish after freeze (phase-order invariant)"
        );
        assert!(
            lower_bound.is_finite() && lower_bound >= 0.0,
            "DemandBoard: InternalInvariant — publish requires finite non-negative \
             lower_bound, got {lower_bound}"
        );
        self.values
            .entry(key)
            .and_modify(|v| *v = (*v).max(lower_bound))
            .or_insert(lower_bound);
    }

    /// Freeze MetricBudget — further [`publish`] is a contract violation.
    pub fn freeze(&mut self) {
        self.frozen = true;
    }

    pub fn get(&self, key: DemandKey) -> Option<f64> {
        self.values.get(&key).copied()
    }
}

/// Channel producer: interior Cross lane counts → LayerGap px lower bounds.
///
/// Formula (params.md): `layer_gap + (track_count - 1) × edge_gap`.
/// Outer Cross lines (stack top/bottom) are **not** published — D1.3.1/2 keep
/// them rare, and inflating canvas padding from outer occupancy is rejected
/// by D1.3.4 (prefer raising interior seams only).
pub fn publish_channel_layer_gap_demand(
    board: &mut DemandBoard,
    track_order: &TrackOrderPlan,
    base_layer_gap: f64,
    edge_gap: f64,
) {
    for (&corridor, &count) in &track_order.rank_gap_track_counts {
        if count == 0 {
            continue;
        }
        let demand = base_layer_gap + (count.saturating_sub(1) as f64) * edge_gap;
        board.publish(DemandKey::LayerGap(corridor), demand);
    }
}

/// Group shell producer: rank gaps adjacent to a group frame edge must cover
/// the shell bands plus a corridor core, or frames derived by the engine's
/// finalize coincide / overlap and Cross lanes pierce the pad bands.
///
/// For every gap with a band: `demand = bands + max(GROUP_FRAME_GAP,
/// (track_count + 1) × edge_gap)` — the track term leaves one `edge_gap`
/// clearance on each side of the centered lanes. Sibling groups that *share*
/// a rank are separated on the cross axis instead (symmetry_objective clamp
/// separation), so they contribute no main-axis demand here.
pub fn publish_group_layer_gap_demand(
    board: &mut DemandBoard,
    plan: &PlanGraph,
    labeled: &BTreeSet<String>,
    track_order: &TrackOrderPlan,
    edge_gap: f64,
) {
    let bands = group_shell_bands(plan, labeled);
    for (r, &(below, above)) in bands.gap.iter().enumerate() {
        let pads = below + above;
        if pads <= 0.0 {
            continue;
        }
        let count = track_order
            .rank_gap_track_counts
            .get(&(r as u32))
            .copied()
            .unwrap_or(0);
        let core = ((count + 1) as f64 * edge_gap).max(GROUP_FRAME_GAP);
        board.publish(DemandKey::LayerGap(r as u32), pads + core);
    }
}

/// Resolved main-axis gap between layer `r` and `r+1` (length = n_layers - 1).
///
/// Requires a frozen board. Each gap is at least `base_layer_gap`, raised by
/// any published [`DemandKey::LayerGap`] lower bound.
pub fn resolved_layer_gaps(
    n_layers: usize,
    base_layer_gap: f64,
    board: &DemandBoard,
) -> Vec<f64> {
    assert!(
        board.is_frozen(),
        "DemandBoard: resolved_layer_gaps requires freeze"
    );
    if n_layers == 0 {
        return Vec::new();
    }
    let n_gaps = n_layers - 1;
    let mut gaps = vec![base_layer_gap; n_gaps];
    for r in 0..n_gaps {
        if let Some(d) = board.get(DemandKey::LayerGap(r as u32)) {
            gaps[r] = gaps[r].max(d);
        }
    }
    gaps
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::hierarchical::compose::track_order::TrackOrderPlan;

    #[test]
    fn expands_gap_by_track_pitch() {
        let mut plan = TrackOrderPlan::default();
        plan.rank_gap_track_counts.insert(0, 3);
        let mut board = DemandBoard::new();
        publish_channel_layer_gap_demand(&mut board, &plan, 40.0, 16.0);
        board.freeze();
        let gaps = resolved_layer_gaps(2, 40.0, &board);
        assert_eq!(gaps, vec![40.0 + 2.0 * 16.0]);
    }

    #[test]
    fn single_track_keeps_base() {
        let mut plan = TrackOrderPlan::default();
        plan.rank_gap_track_counts.insert(0, 1);
        let mut board = DemandBoard::new();
        publish_channel_layer_gap_demand(&mut board, &plan, 40.0, 16.0);
        board.freeze();
        let gaps = resolved_layer_gaps(2, 40.0, &board);
        assert_eq!(gaps, vec![40.0]);
    }

    #[test]
    fn layer_gap_demand_monotone_in_track_count() {
        let mut prev = 0.0;
        for count in 1..=6 {
            let mut plan = TrackOrderPlan::default();
            plan.rank_gap_track_counts.insert(0, count);
            let mut board = DemandBoard::new();
            publish_channel_layer_gap_demand(&mut board, &plan, 40.0, 16.0);
            board.freeze();
            let g = resolved_layer_gaps(2, 40.0, &board)[0];
            assert!(
                g + 1e-12 >= prev,
                "count {count}: gap {g} < previous {prev}"
            );
            prev = g;
        }
    }

    #[test]
    fn group_band_demand_reserves_frame_pads() {
        use crate::layout::hierarchical::model::{Elem, ElemKey};

        fn real(id: &str, rank: u32, group: &str) -> Elem {
            Elem {
                key: ElemKey::Real(id.into()),
                group_path: vec![group.into()],
                rank,
            }
        }
        // g1 ends at rank 0, g2 (labeled) starts at rank 1, g3 shares rank 1
        // with g2 → seam 0|1 carries both bands; g3's top band adds nothing
        // (unlabeled 16 < labeled 24).
        let elems = vec![real("a", 0, "g1"), real("b", 1, "g2"), real("c", 1, "g3")];
        let layers = vec![vec![0], vec![1, 2]];
        let plan = crate::layout::hierarchical::model::PlanGraph {
            elems,
            index_of: Default::default(),
            decl_index: vec![0, 1, 2],
            segments: Vec::new(),
            layers,
        };
        let labeled: BTreeSet<String> = ["g2".to_string()].into_iter().collect();
        let mut board = DemandBoard::new();
        publish_group_layer_gap_demand(&mut board, &plan, &labeled, &TrackOrderPlan::default(), 16.0);
        board.freeze();
        // bottom band 16 + top band 24 + frame-gap core 16 = 56.
        assert_eq!(board.get(DemandKey::LayerGap(0)), Some(56.0));
        assert_eq!(resolved_layer_gaps(2, 40.0, &board), vec![56.0]);
    }

    #[test]
    fn group_band_demand_grows_with_tracks() {
        use crate::layout::hierarchical::model::{Elem, ElemKey};

        let elems = vec![
            Elem {
                key: ElemKey::Real("a".into()),
                group_path: vec!["g1".into()],
                rank: 0,
            },
            Elem {
                key: ElemKey::Real("b".into()),
                group_path: vec!["g2".into()],
                rank: 1,
            },
        ];
        let plan = crate::layout::hierarchical::model::PlanGraph {
            elems,
            index_of: Default::default(),
            decl_index: vec![0, 1],
            segments: Vec::new(),
            layers: vec![vec![0], vec![1]],
        };
        let labeled = BTreeSet::new();
        let mut tracks = TrackOrderPlan::default();
        tracks.rank_gap_track_counts.insert(0, 2);
        let mut board = DemandBoard::new();
        publish_group_layer_gap_demand(&mut board, &plan, &labeled, &tracks, 16.0);
        board.freeze();
        // bands 32 + track core (2+1)*16 = 80.
        assert_eq!(board.get(DemandKey::LayerGap(0)), Some(80.0));
    }

    #[test]
    fn max_merge_keeps_higher_bound() {
        let mut board = DemandBoard::new();
        board.publish(DemandKey::LayerGap(0), 40.0);
        board.publish(DemandKey::LayerGap(0), 72.0);
        board.publish(DemandKey::LayerGap(0), 50.0);
        board.freeze();
        assert_eq!(board.get(DemandKey::LayerGap(0)), Some(72.0));
    }

    #[test]
    #[should_panic(expected = "publish after freeze")]
    fn publish_after_freeze_panics() {
        let mut board = DemandBoard::new();
        board.freeze();
        board.publish(DemandKey::LayerGap(0), 40.0);
    }

    #[test]
    #[should_panic(expected = "finite non-negative")]
    fn publish_rejects_non_finite_or_negative() {
        let mut board = DemandBoard::new();
        board.publish(DemandKey::LayerGap(0), f64::NAN);
    }

    #[test]
    #[should_panic(expected = "requires freeze")]
    fn resolve_before_freeze_panics() {
        let board = DemandBoard::new();
        let _ = resolved_layer_gaps(2, 40.0, &board);
    }
}
