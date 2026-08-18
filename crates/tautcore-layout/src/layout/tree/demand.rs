//! Tree DemandBoard (architecture.md T3).
//!
//! Publish size / spacing floors, freeze, then Metric reads. No callback
//! after freeze. Node labels already live in `NodeSizes` (do not dual-source).
//! Edge labels and `min_first_segment` raise `LayerGap` before placers merge.

use std::collections::BTreeMap;

use tautcore_model::geometry::Size;
use tautcore_model::graph::Graph;
use tautcore_model::sizes::NodeSizes;

use super::params::{TreeParams, TreeRoutingStyle};
use super::plan::TreePlan;

const LABEL_H: f64 = 14.0;
const LABEL_PAD: f64 = 4.0;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum TreeDemandKey {
    NodeWidth(String),
    NodeHeight(String),
    LayerGap,
    NodeGap,
}

#[derive(Debug, Clone, Default)]
pub struct TreeDemandBoard {
    values: BTreeMap<TreeDemandKey, f64>,
    frozen: bool,
}

impl TreeDemandBoard {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn publish(&mut self, key: TreeDemandKey, lower_bound: f64) {
        assert!(
            !self.frozen,
            "tree DemandBoard: publish after freeze (phase-order invariant)"
        );
        assert!(
            lower_bound.is_finite() && lower_bound >= 0.0,
            "tree DemandBoard: InternalInvariant — publish requires finite \
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

    pub fn get(&self, key: TreeDemandKey) -> f64 {
        self.values.get(&key).copied().unwrap_or(0.0)
    }

    pub fn node_size(&self, id: &str, raw: Size) -> Size {
        Size::new(
            self.get(TreeDemandKey::NodeWidth(id.to_string()))
                .max(raw.width),
            self.get(TreeDemandKey::NodeHeight(id.to_string()))
                .max(raw.height),
        )
    }

    pub fn layer_gap(&self, base: f64) -> f64 {
        self.get(TreeDemandKey::LayerGap).max(base)
    }

    pub fn node_gap(&self, base: f64) -> f64 {
        self.get(TreeDemandKey::NodeGap).max(base)
    }
}

/// Publish floors then freeze. Metric must not publish.
pub fn publish_floors(
    graph: &Graph,
    plan: &TreePlan,
    params: &TreeParams,
    sizes: &NodeSizes,
) -> TreeDemandBoard {
    let mut board = TreeDemandBoard::new();
    for id in &plan.nodes {
        let s = sizes.get(id).unwrap_or(Size::new(0.0, 0.0));
        board.publish(TreeDemandKey::NodeWidth(id.clone()), s.width.max(0.0));
        board.publish(TreeDemandKey::NodeHeight(id.clone()), s.height.max(0.0));
    }

    board.publish(TreeDemandKey::NodeGap, params.node_gap);
    let mut layer = params.layer_gap;
    if matches!(
        params.routing_style,
        TreeRoutingStyle::Orthogonal
            | TreeRoutingStyle::Polyline
            | TreeRoutingStyle::OrthogonalAtRoot
    ) {
        layer = layer.max(params.min_first_segment);
    }
    board.publish(TreeDemandKey::LayerGap, layer);

    for eid in &plan.tree_edge_ids {
        let Some(edge) = graph.find_edge(eid) else {
            continue;
        };
        let Some(text) = edge.label.as_deref() else {
            continue;
        };
        if text.is_empty() {
            continue;
        };
        // Gap and label share the parent–child seam: sum at the producer.
        board.publish(
            TreeDemandKey::LayerGap,
            params.layer_gap + LABEL_H + LABEL_PAD,
        );
    }

    board.freeze();
    board
}
