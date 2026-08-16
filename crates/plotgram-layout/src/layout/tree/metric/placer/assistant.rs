//! `assistant`: marked children on a left-right bus; the rest below (single-layer).

use plotgram_engine_api::LayoutError;

use super::super::shape::SubtreeShape;
use super::bus;
use super::pack;
use super::single_layer::SingleLayerPlacer;
use super::{ISubtreePlacer, PlaceCtx};

#[derive(Debug, Default, Clone, Copy)]
pub struct AssistantPlacer;

impl ISubtreePlacer for AssistantPlacer {
    fn place_subtree(
        &self,
        ctx: &PlaceCtx<'_>,
        root: &str,
        children: Vec<(String, SubtreeShape)>,
    ) -> Result<SubtreeShape, LayoutError> {
        arrange(ctx, root, children)
    }
}

impl AssistantPlacer {
    pub fn place(&self, ctx: &PlaceCtx<'_>, root: &str) -> Result<SubtreeShape, LayoutError> {
        let kids = super::place_children(ctx, root)?;
        if kids.iter().all(|(k, _)| !ctx.plan.is_assistant(k)) {
            return SingleLayerPlacer.place_subtree(ctx, root, kids);
        }
        self.place_subtree(ctx, root, kids)
    }
}

fn arrange(
    ctx: &PlaceCtx<'_>,
    root: &str,
    children: Vec<(String, SubtreeShape)>,
) -> Result<SubtreeShape, LayoutError> {
    let (asst, regular) = pack::split_assistants(ctx, children);
    if asst.is_empty() {
        return SingleLayerPlacer.place_subtree(ctx, root, regular);
    }
    let mut placed = bus::arrange(ctx, root, asst)?;
    if regular.is_empty() {
        return Ok(placed);
    }
    let ids = pack::regular_ids(&regular);
    let body = pack::row(regular, ctx.params.node_gap);
    pack::attach_below(&mut placed, root, body, ctx.params.layer_gap);
    pack::write_local_routes(&mut placed, ctx, root, &ids);
    Ok(placed)
}
