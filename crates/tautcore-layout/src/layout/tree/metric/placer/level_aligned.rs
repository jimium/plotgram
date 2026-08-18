//! `level-aligned`: same tree-depth shares a layer (Buchheim + layer y).
//! Same geometry as single-layer today; kept as its own type so M3 dendrogram
//! can specialize without touching the org-chart placer.

use tautcore_engine_api::LayoutError;

use super::super::shape::SubtreeShape;
use super::layered;
use super::{ISubtreePlacer, PlaceCtx};

#[derive(Debug, Default, Clone, Copy)]
pub struct LevelAlignedPlacer;

impl ISubtreePlacer for LevelAlignedPlacer {
    fn place_subtree(
        &self,
        ctx: &PlaceCtx<'_>,
        root: &str,
        children: Vec<(String, SubtreeShape)>,
    ) -> Result<SubtreeShape, LayoutError> {
        layered::pack(ctx, root, children)
    }
}

impl LevelAlignedPlacer {
    pub fn place(&self, ctx: &PlaceCtx<'_>, root: &str) -> Result<SubtreeShape, LayoutError> {
        if layered::subtree_uniform_layered(root, ctx.plan) {
            layered::place_uniform(root, None, ctx.plan.transform(root), ctx)
        } else {
            let kids = super::place_children(ctx, root)?;
            self.place_subtree(ctx, root, kids)
        }
    }
}
