//! ISubtreePlacer dispatch. Connectors are already frozen on TreePlan.

use std::collections::BTreeMap;

use tautcore_engine_api::LayoutError;
use tautcore_model::geometry::Size;

use super::shape::SubtreeShape;
use crate::layout::tree::params::{PlacerId, TreeParams};
use crate::layout::tree::plan::TreePlan;

mod aspect_ratio;
mod assistant;
mod balloon;
mod bus;
mod compact;
mod dendrogram;
mod double_layer;
mod layered;
mod left_right;
mod level_aligned;
mod pack;
mod polar;
mod radial;
mod single_layer;
mod single_split;
mod transform;

use aspect_ratio::AspectRatioPlacer;
use assistant::AssistantPlacer;
use balloon::BalloonPlacer;
use bus::BusPlacer;
use compact::CompactPlacer;
use dendrogram::DendrogramPlacer;
use double_layer::DoubleLayerPlacer;
use left_right::LeftRightPlacer;
use level_aligned::LevelAlignedPlacer;
use radial::RadialPlacer;
use single_layer::SingleLayerPlacer;
use single_split::SingleSplitPlacer;

pub struct PlaceCtx<'a> {
    pub plan: &'a TreePlan,
    pub params: &'a TreeParams,
    pub size_of: &'a BTreeMap<String, Size>,
}

/// yFiles-style subtree placer. `place_subtree` sees already-placed children.
/// Uniform layered regions take a Buchheim fast path in `place` instead of
/// packing child AABBs (contour merge needs the whole region).
pub trait ISubtreePlacer {
    fn place_subtree(
        &self,
        ctx: &PlaceCtx<'_>,
        root: &str,
        children: Vec<(String, SubtreeShape)>,
    ) -> Result<SubtreeShape, LayoutError>;
}

pub fn place(id: &str, ctx: &PlaceCtx<'_>) -> Result<SubtreeShape, LayoutError> {
    match ctx.plan.placer(id) {
        PlacerId::SingleSplitLayered => SingleSplitPlacer.place(ctx, id),
        PlacerId::LeftRight => LeftRightPlacer.place(ctx, id),
        PlacerId::Bus => BusPlacer.place(ctx, id),
        PlacerId::SingleLayer => SingleLayerPlacer.place(ctx, id),
        PlacerId::LevelAligned => LevelAlignedPlacer.place(ctx, id),
        PlacerId::DoubleLayer => DoubleLayerPlacer.place(ctx, id),
        PlacerId::Dendrogram => DendrogramPlacer.place(ctx, id),
        PlacerId::Assistant => AssistantPlacer.place(ctx, id),
        PlacerId::Compact => CompactPlacer.place(ctx, id),
        PlacerId::AspectRatio => AspectRatioPlacer.place(ctx, id),
        PlacerId::Radial => RadialPlacer.place(ctx, id),
        PlacerId::Balloon => BalloonPlacer.place(ctx, id),
    }
}

fn place_children(
    ctx: &PlaceCtx<'_>,
    root: &str,
) -> Result<Vec<(String, SubtreeShape)>, LayoutError> {
    let mut out = Vec::new();
    for k in ctx.plan.children_of(root) {
        out.push((k.clone(), place(k, ctx)?));
    }
    Ok(out)
}
