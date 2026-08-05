//! Minimal DemandBoard for Channel MetricBudget (channel-d1.md).
//!
//! LayerGap lower bounds from Cross-track lane counts.

use crate::layout::hierarchical::compose::track_order::TrackOrderPlan;

/// Resolved main-axis gap between layer `r` and `r+1` (length = n_layers - 1).
pub fn resolved_layer_gaps(
    n_layers: usize,
    base_layer_gap: f64,
    edge_gap: f64,
    track_order: &TrackOrderPlan,
) -> Vec<f64> {
    if n_layers == 0 {
        return Vec::new();
    }
    let n_gaps = n_layers - 1;
    let mut gaps = vec![base_layer_gap; n_gaps];
    for (&corridor, &count) in &track_order.rank_gap_track_counts {
        let r = corridor as usize;
        if r >= n_gaps || count == 0 {
            continue;
        }
        let demand = base_layer_gap + (count.saturating_sub(1) as f64) * edge_gap;
        gaps[r] = gaps[r].max(demand);
    }
    gaps
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::hierarchical::channel::TrackId;
    use crate::layout::hierarchical::compose::track_order::{HopTrack, TrackOrderPlan};

    #[test]
    fn expands_gap_by_track_pitch() {
        let mut plan = TrackOrderPlan::default();
        plan.rank_gap_track_counts.insert(0, 3);
        let gaps = resolved_layer_gaps(2, 40.0, 16.0, &plan);
        assert_eq!(gaps, vec![40.0 + 2.0 * 16.0]);
    }

    #[test]
    fn single_track_keeps_base() {
        let mut plan = TrackOrderPlan::default();
        plan.rank_gap_track_counts.insert(0, 1);
        plan.assignments.insert(
            ("e".into(), TrackId(0)),
            HopTrack {
                track: TrackId(0),
                track_index: 0,
            },
        );
        let gaps = resolved_layer_gaps(2, 40.0, 16.0, &plan);
        assert_eq!(gaps, vec![40.0]);
    }
}
