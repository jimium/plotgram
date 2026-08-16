//! `single-split-layered`: one split at the local root; each side is
//! level-aligned in TB then wrap-rotated (no second Buchheim implementation).

use plotgram_engine_api::LayoutError;

use super::super::shape::SubtreeShape;
use super::layered;
use super::{ISubtreePlacer, PlaceCtx};
use crate::layout::tree::compose::partition_children;
use crate::layout::tree::params::SubtreeTransform;

#[derive(Debug, Default, Clone, Copy)]
pub struct SingleSplitPlacer;

impl ISubtreePlacer for SingleSplitPlacer {
    fn place_subtree(
        &self,
        ctx: &PlaceCtx<'_>,
        root: &str,
        _children: Vec<(String, SubtreeShape)>,
    ) -> Result<SubtreeShape, LayoutError> {
        // Sides are uniform layered regions including this root; wrap origin
        // is the split root (architecture §6.5), not each child.
        self.place(ctx, root)
    }
}

impl SingleSplitPlacer {
    pub fn place(&self, ctx: &PlaceCtx<'_>, id: &str) -> Result<SubtreeShape, LayoutError> {
        let kids = ctx.plan.children_of(id);
        let (left, right) = partition_children(kids, ctx.params.split_policy, &ctx.plan.split_side);
        let left_ids: Vec<String> = left.iter().map(|s| (*s).to_string()).collect();
        let right_ids: Vec<String> = right.iter().map(|s| (*s).to_string()).collect();

        let mut left_p = if left_ids.is_empty() {
            layered::place_uniform(id, Some(&[]), SubtreeTransform::None, ctx)?
        } else {
            layered::place_uniform(id, Some(&left_ids), SubtreeTransform::RotateLeft, ctx)?
        };
        let right_p = if right_ids.is_empty() {
            SubtreeShape::forest()
        } else {
            layered::place_uniform(id, Some(&right_ids), SubtreeTransform::RotateRight, ctx)?
        };

        if let (Some(lf), Some(rf)) = (left_p.frame(), right_p.frame()) {
            let dx = lf.center().x - rf.center().x;
            let dy = lf.center().y - rf.center().y;
            let mut right_p = right_p;
            right_p.translate(dx, dy);
            right_p.frames.remove(id);
            left_p.merge(right_p);
        } else if !right_p.frames.is_empty() {
            left_p.merge(right_p);
        }
        left_p.root = id.to_string();
        Ok(left_p)
    }
}
