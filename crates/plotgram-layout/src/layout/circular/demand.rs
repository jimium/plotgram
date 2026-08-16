//! Circular DemandBoard (architecture.md §3.3).
//!
//! Publish size / spacing / radius floors, freeze, then Metric reads.

use std::collections::BTreeMap;

use plotgram_model::geometry::Size;
use plotgram_model::sizes::NodeSizes;

use super::geom::cycle_geom;
use super::params::{CircularParams, RoutingPolicy};
use super::plan::{CircPlan, PartitionId};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum CircDemandKey {
    NodeWidth(String),
    NodeHeight(String),
    NodeGap,
    Radius(PartitionId),
    ExteriorSep,
}

#[derive(Debug, Clone, Default)]
pub struct CircDemandBoard {
    values: BTreeMap<CircDemandKey, f64>,
    frozen: bool,
}

impl CircDemandBoard {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn publish(&mut self, key: CircDemandKey, lower_bound: f64) {
        assert!(
            !self.frozen,
            "circular DemandBoard: publish after freeze (phase-order invariant)"
        );
        assert!(
            lower_bound.is_finite() && lower_bound >= 0.0,
            "circular DemandBoard: InternalInvariant — publish requires finite \
             non-negative lower_bound, got {lower_bound}"
        );
        self.values
            .entry(key)
            .and_modify(|v| *v = (*v).max(lower_bound))
            .or_insert(lower_bound);
    }

    pub fn freeze(&mut self) {
        self.frozen = true;
    }

    pub fn get(&self, key: CircDemandKey) -> f64 {
        self.values.get(&key).copied().unwrap_or(0.0)
    }

    pub fn node_size(&self, id: &str, raw: Size) -> Size {
        Size::new(
            self.get(CircDemandKey::NodeWidth(id.to_string()))
                .max(raw.width),
            self.get(CircDemandKey::NodeHeight(id.to_string()))
                .max(raw.height),
        )
    }

    pub fn node_gap(&self, base: f64) -> f64 {
        self.get(CircDemandKey::NodeGap).max(base)
    }

    pub fn radius(&self, pid: PartitionId) -> f64 {
        self.get(CircDemandKey::Radius(pid))
    }

    pub fn exterior_sep(&self) -> f64 {
        self.get(CircDemandKey::ExteriorSep)
    }
}

pub fn publish_floors(
    plan: &CircPlan,
    params: &CircularParams,
    sizes: &NodeSizes,
) -> CircDemandBoard {
    let mut board = CircDemandBoard::new();
    for id in &plan.nodes {
        let s = sizes.get(id).unwrap_or(Size::new(0.0, 0.0));
        board.publish(CircDemandKey::NodeWidth(id.clone()), s.width.max(0.0));
        board.publish(CircDemandKey::NodeHeight(id.clone()), s.height.max(0.0));
    }
    board.publish(CircDemandKey::NodeGap, params.node_gap);

    let gap = board.node_gap(params.node_gap);
    for (pid, members) in &plan.partitions {
        let part_sizes: Vec<Size> = members
            .iter()
            .map(|id| {
                let raw = sizes.get(id).unwrap_or(Size::new(0.0, 0.0));
                board.node_size(id, raw)
            })
            .collect();
        let geom = cycle_geom(&part_sizes, gap, params.min_radius, params.rotation);
        board.publish(CircDemandKey::Radius(*pid), geom.radius.max(0.0));
    }
    if params.routing_policy == RoutingPolicy::Exterior {
        board.publish(CircDemandKey::ExteriorSep, params.node_gap.max(16.0));
    }

    board.freeze();
    board
}
