//! `dendrogram`: child subtree bottoms aligned (leaves share a baseline).

use std::collections::BTreeMap;

use plotgram_engine_api::LayoutError;
use plotgram_model::geometry::Rect;

use super::super::shape::SubtreeShape;
use super::layered::{
    apply_root_alignment, apply_root_alignment_all, collect_subtree, place_uniform, subtree_bounds,
    write_routes,
};
use super::{ISubtreePlacer, PlaceCtx};
use crate::layout::tree::params::PlacerId;
use crate::layout::tree::plan::TreePlan;

#[derive(Debug, Default, Clone, Copy)]
pub struct DendrogramPlacer;

impl ISubtreePlacer for DendrogramPlacer {
    fn place_subtree(
        &self,
        ctx: &PlaceCtx<'_>,
        root: &str,
        children: Vec<(String, SubtreeShape)>,
    ) -> Result<SubtreeShape, LayoutError> {
        align_child_shapes(ctx, root, children)
    }
}

impl DendrogramPlacer {
    pub fn place(&self, ctx: &PlaceCtx<'_>, root: &str) -> Result<SubtreeShape, LayoutError> {
        if subtree_all_dendrogram(root, ctx.plan) {
            let mut shape = place_uniform(root, None, ctx.plan.transform(root), ctx)?;
            align_bottoms(
                &mut shape.frames,
                root,
                None,
                ctx.plan,
                ctx.params.layer_gap,
            );
            apply_root_alignment_all(
                &mut shape.frames,
                root,
                None,
                ctx.plan,
                ctx.params.root_alignment,
            );
            shape.routes = write_routes(&shape.frames, root, None, ctx.plan, ctx.params);
            Ok(shape)
        } else {
            let kids = super::place_children(ctx, root)?;
            self.place_subtree(ctx, root, kids)
        }
    }
}

fn subtree_all_dendrogram(id: &str, plan: &TreePlan) -> bool {
    let mut stack = vec![id.to_string()];
    while let Some(cur) = stack.pop() {
        if plan.placer(&cur) != PlacerId::Dendrogram {
            return false;
        }
        for k in plan.children_of(&cur) {
            stack.push(k.clone());
        }
    }
    true
}

fn align_bottoms(
    frames: &mut BTreeMap<String, Rect>,
    local_root: &str,
    child_override: Option<&[String]>,
    plan: &TreePlan,
    layer_gap: f64,
) {
    let ids = collect_subtree(local_root, child_override, plan);
    for id in ids.iter().rev() {
        let kids: &[String] = if id == local_root {
            child_override.unwrap_or_else(|| plan.children_of(id))
        } else {
            plan.children_of(id)
        };
        if kids.is_empty() {
            continue;
        }
        let target = kids
            .iter()
            .map(|k| subtree_bounds(k, frames, plan).bottom())
            .fold(f64::NEG_INFINITY, f64::max);
        if !target.is_finite() {
            continue;
        }
        for k in kids {
            let dy = target - subtree_bounds(k, frames, plan).bottom();
            if dy.abs() < 1e-12 {
                continue;
            }
            let mut stack = vec![k.clone()];
            while let Some(cur) = stack.pop() {
                if let Some(f) = frames.get_mut(&cur) {
                    f.y += dy;
                }
                for d in plan.children_of(&cur) {
                    stack.push(d.clone());
                }
            }
        }
        let child_top = kids
            .iter()
            .filter_map(|k| frames.get(k).map(|f| f.y))
            .fold(f64::INFINITY, f64::min);
        if let Some(pf) = frames.get_mut(id) {
            if child_top.is_finite() {
                pf.y = child_top - layer_gap - pf.height;
            }
        }
    }
}

fn align_child_shapes(
    ctx: &PlaceCtx<'_>,
    root: &str,
    children: Vec<(String, SubtreeShape)>,
) -> Result<SubtreeShape, LayoutError> {
    let sz = ctx.size_of[root];
    let mut placed = SubtreeShape::from_parts(root, BTreeMap::new(), BTreeMap::new());
    let mut packed = Vec::new();
    let mut cursor = 0.0;
    for (k, mut cp) in children {
        if let Some(b) = cp.bounds() {
            cp.translate(cursor - b.x, -b.y);
            cursor = cp.bounds().map(|bb| bb.right()).unwrap_or(cursor) + ctx.params.node_gap;
        }
        packed.push((k, cp));
    }
    let target = packed
        .iter()
        .filter_map(|(_, cp)| cp.bounds().map(|b| b.bottom()))
        .fold(f64::NEG_INFINITY, f64::max);
    let kids: Vec<String> = packed.iter().map(|(k, _)| k.clone()).collect();
    for (_k, mut cp) in packed {
        if let Some(b) = cp.bounds() {
            cp.translate(0.0, target - b.bottom());
        }
        placed.merge(cp);
    }
    let child_top = kids
        .iter()
        .filter_map(|k| placed.frames.get(k).map(|f| f.y))
        .fold(f64::INFINITY, f64::min);
    let parent_y = if child_top.is_finite() {
        child_top - ctx.params.layer_gap - sz.height
    } else {
        0.0
    };
    let parent_x = if let Some(b) = placed.bounds() {
        (b.x + b.right()) / 2.0 - sz.width / 2.0
    } else {
        0.0
    };
    placed.frames.insert(
        root.to_string(),
        Rect::new(parent_x, parent_y, sz.width, sz.height),
    );
    apply_root_alignment(
        &mut placed.frames,
        root,
        &kids,
        ctx.plan,
        ctx.params.root_alignment,
    );
    placed.routes = write_routes(&placed.frames, root, Some(&kids), ctx.plan, ctx.params);
    Ok(placed)
}
