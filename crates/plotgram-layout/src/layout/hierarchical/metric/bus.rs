//! Bus main-axis levels for automatic edge grouping (yFiles bus-style).
//!
//! Compose writes [`BusPrefix`] topology; Metric expands the shared trunk
//! length into a main-axis coordinate. Ink only joins
//! `SharedPort → Trunk → Bus → Stub` — it does not invent bus height.

use std::collections::BTreeMap;

use plotgram_algo::orientation::Side;
use plotgram_model::geometry::Rect;

use crate::layout::hierarchical::compose::ports::{BusPrefix, EdgePorts};
use crate::layout::hierarchical::metric::anchor::port_anchor;
use crate::layout::hierarchical::model::{ElemKey, PlanGraph, RealGraph};

/// Per-edge bus main-axis coordinate at clustered ends.
#[derive(Debug, Default, Clone)]
pub struct BusLevels {
    /// edge_id → bus y (canonical TB) at the source end.
    pub source: BTreeMap<String, f64>,
    /// edge_id → bus y at the target end.
    pub target: BTreeMap<String, f64>,
}

/// Trunk length for a bus prefix: fraction of `layer_gap`, clamped.
/// Applied once to the shared trunk — not per-member parallel stubs.
fn trunk_length(layer_gap: f64, member_count: usize) -> f64 {
    let raw = (layer_gap * 0.35).max(12.0) + (member_count.saturating_sub(2) as f64) * 2.0;
    raw.clamp(12.0, 32.0)
}

fn side_normal_y(side: Side) -> f64 {
    match side {
        Side::North => -1.0,
        Side::South => 1.0,
        Side::West | Side::East => 0.0,
    }
}

/// Expand each [`BusPrefix`] into a shared bus main-axis coordinate for all
/// members. Frames must already be known (post cross-axis).
pub fn assign_bus_levels(
    prefixes: &[BusPrefix],
    ports: &BTreeMap<String, EdgePorts>,
    graph: &RealGraph,
    plan: &PlanGraph,
    frames: &[Rect],
    layer_gap: f64,
) -> BusLevels {
    let mut out = BusLevels::default();
    for prefix in prefixes {
        if prefix.members.len() < 2 {
            continue;
        }
        let lead = &prefix.members[0];
        let rp = &ports[lead];
        let (port, at_source) = if prefix.at_source {
            (rp.source, true)
        } else {
            (rp.target, false)
        };
        // Only main-axis sides get a bus (Compose eligibility); skip others.
        if !matches!(port.side, Side::North | Side::South) {
            continue;
        }
        let edge = graph
            .edges
            .iter()
            .find(|e| e.edge_id == *lead)
            .expect("bus member edge must exist");
        let node_idx = if at_source {
            edge.original_source
        } else {
            edge.original_target
        };
        let elem = plan.index_of[&ElemKey::Real(graph.ids[node_idx].clone())];
        let anchor = port_anchor(frames[elem], port);
        let trunk = trunk_length(layer_gap, prefix.members.len());
        let bus_y = anchor.y + side_normal_y(port.side) * trunk;
        let map = if at_source {
            &mut out.source
        } else {
            &mut out.target
        };
        for id in &prefix.members {
            map.insert(id.clone(), bus_y);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trunk_length_is_clamped_and_deterministic() {
        assert_eq!(trunk_length(40.0, 2), 14.0); // 0.35*40=14
        assert_eq!(trunk_length(10.0, 2), 12.0); // floor
        assert_eq!(trunk_length(200.0, 2), 32.0); // ceil
        assert_eq!(trunk_length(40.0, 4), 18.0); // +2 per extra beyond 2
    }
}
