//! Metric: ISubtreePlacer recursion in canonical TB space.

use std::collections::BTreeMap;

use tautcore_engine_api::LayoutError;
use tautcore_model::geometry::{Rect, Size};
use tautcore_model::sizes::NodeSizes;

use super::demand::TreeDemandBoard;
use super::params::TreeParams;
use super::plan::{TreePlan, TreeRoute};

mod geom;
mod placer;
mod shape;

pub use shape::SubtreeShape;

pub struct TreeMetric {
    pub frames: BTreeMap<String, Rect>,
    pub routes: BTreeMap<String, TreeRoute>,
}

pub fn assign(
    plan: &TreePlan,
    params: &TreeParams,
    sizes: &NodeSizes,
    demand: &TreeDemandBoard,
) -> Result<TreeMetric, LayoutError> {
    let mut size_of: BTreeMap<String, Size> = BTreeMap::new();
    for id in &plan.nodes {
        let s = sizes
            .get(id)
            .ok_or_else(|| tautcore_model::MissingNodeSize {
                node_id: id.clone(),
            })?;
        size_of.insert(id.clone(), demand.node_size(id, s));
    }

    let mut spaced = params.clone();
    spaced.layer_gap = demand.layer_gap(params.layer_gap);
    spaced.node_gap = demand.node_gap(params.node_gap);

    let ctx = placer::PlaceCtx {
        plan,
        params: &spaced,
        size_of: &size_of,
    };
    let mut placed = SubtreeShape::forest();
    let mut cursor = 0.0;
    for root in &plan.roots {
        let mut one = placer::place(root, &ctx)?;
        if let Some(b) = one.bounds() {
            one.translate(cursor - b.x, -b.y);
            cursor = one.bounds().map(|bb| bb.right()).unwrap_or(cursor) + spaced.node_gap;
        }
        placed.merge(one);
    }

    Ok(TreeMetric {
        frames: placed.frames,
        routes: placed.routes,
    })
}
