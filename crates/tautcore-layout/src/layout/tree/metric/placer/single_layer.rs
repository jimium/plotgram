//! `single-layer`: children in one row, parent centered (Buchheim / RT).

use tautcore_engine_api::LayoutError;

use super::super::shape::SubtreeShape;
use super::layered;
use super::{ISubtreePlacer, PlaceCtx};

#[derive(Debug, Default, Clone, Copy)]
pub struct SingleLayerPlacer;

impl ISubtreePlacer for SingleLayerPlacer {
    fn place_subtree(
        &self,
        ctx: &PlaceCtx<'_>,
        root: &str,
        children: Vec<(String, SubtreeShape)>,
    ) -> Result<SubtreeShape, LayoutError> {
        layered::pack(ctx, root, children)
    }
}

impl SingleLayerPlacer {
    pub fn place(&self, ctx: &PlaceCtx<'_>, root: &str) -> Result<SubtreeShape, LayoutError> {
        if layered::subtree_uniform_layered(root, ctx.plan) {
            layered::place_uniform(root, None, ctx.plan.transform(root), ctx)
        } else {
            let kids = super::place_children(ctx, root)?;
            self.place_subtree(ctx, root, kids)
        }
    }
}
