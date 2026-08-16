//! `left-right`: children on a vertical bus (no trailing bottom slot).

use plotgram_engine_api::LayoutError;

use super::super::shape::SubtreeShape;
use super::bus;
use super::{ISubtreePlacer, PlaceCtx};

#[derive(Debug, Default, Clone, Copy)]
pub struct LeftRightPlacer;

impl ISubtreePlacer for LeftRightPlacer {
    fn place_subtree(
        &self,
        ctx: &PlaceCtx<'_>,
        root: &str,
        children: Vec<(String, SubtreeShape)>,
    ) -> Result<SubtreeShape, LayoutError> {
        bus::arrange(ctx, root, children)
    }
}

impl LeftRightPlacer {
    pub fn place(&self, ctx: &PlaceCtx<'_>, root: &str) -> Result<SubtreeShape, LayoutError> {
        let kids = super::place_children(ctx, root)?;
        self.place_subtree(ctx, root, kids)
    }
}
