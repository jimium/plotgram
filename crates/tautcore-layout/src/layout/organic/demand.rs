//! Organic DemandBoard (architecture.md §3.3).
//!
//! Publishes size / spacing floors, freezes, then Metric reads.

use std::collections::BTreeMap;

use tautcore_model::geometry::Size;
use tautcore_model::sizes::NodeSizes;

use super::params::OrganicParams;
use super::plan::OrganicPlan;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum OrgDemandKey {
    NodeWidth(String),
    NodeHeight(String),
    NodeGap,
    ComponentGap,
}

#[derive(Debug, Clone, Default)]
pub struct OrgDemandBoard {
    values: BTreeMap<OrgDemandKey, f64>,
    frozen: bool,
}

impl OrgDemandBoard {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn publish(&mut self, key: OrgDemandKey, lower_bound: f64) {
        assert!(
            !self.frozen,
            "organic DemandBoard: publish after freeze (phase-order invariant)"
        );
        assert!(
            lower_bound.is_finite() && lower_bound >= 0.0,
            "organic DemandBoard: InternalInvariant — publish requires finite \
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

    pub fn get(&self, key: &OrgDemandKey) -> f64 {
        self.values.get(key).copied().unwrap_or(0.0)
    }

    pub fn node_size(&self, id: &str, raw: Size) -> Size {
        Size::new(
            self.get(&OrgDemandKey::NodeWidth(id.to_string()))
                .max(raw.width),
            self.get(&OrgDemandKey::NodeHeight(id.to_string()))
                .max(raw.height),
        )
    }

    pub fn node_gap(&self, base: f64) -> f64 {
        self.get(&OrgDemandKey::NodeGap).max(base)
    }

    pub fn component_gap(&self, base: f64) -> f64 {
        self.get(&OrgDemandKey::ComponentGap).max(base)
    }
}

pub fn publish_floors(
    plan: &OrganicPlan,
    params: &OrganicParams,
    sizes: &NodeSizes,
) -> OrgDemandBoard {
    let mut board = OrgDemandBoard::new();
    for id in &plan.nodes {
        let s = sizes.get(id).unwrap_or(Size::new(0.0, 0.0));
        board.publish(OrgDemandKey::NodeWidth(id.clone()), s.width.max(0.0));
        board.publish(OrgDemandKey::NodeHeight(id.clone()), s.height.max(0.0));
    }
    board.publish(OrgDemandKey::NodeGap, params.minimum_node_distance);
    board.publish(OrgDemandKey::ComponentGap, params.component_gap);
    board.freeze();
    board
}
