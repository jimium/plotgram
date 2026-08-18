//! `compact`: bounded search over a fixed set of packing strategies.

use tautcore_engine_api::LayoutError;

use super::super::shape::SubtreeShape;
use super::bus;
use super::pack;
use super::{ISubtreePlacer, PlaceCtx};
use crate::layout::tree::params::PlacerId;

#[derive(Debug, Default, Clone, Copy)]
pub struct CompactPlacer;

impl ISubtreePlacer for CompactPlacer {
    fn place_subtree(
        &self,
        ctx: &PlaceCtx<'_>,
        root: &str,
        children: Vec<(String, SubtreeShape)>,
    ) -> Result<SubtreeShape, LayoutError> {
        arrange(ctx, root, children)
    }
}

impl CompactPlacer {
    pub fn place(&self, ctx: &PlaceCtx<'_>, root: &str) -> Result<SubtreeShape, LayoutError> {
        let kids = super::place_children(ctx, root)?;
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
        return search(ctx, root, regular);
    }
    let base = bus::arrange(ctx, root, asst)?;
    if regular.is_empty() {
        return Ok(base);
    }
    let ids = pack::regular_ids(&regular);
    let consider_ratio = match ctx.plan.parent.get(root) {
        Some(p) => ctx.plan.placer(p) != PlacerId::Compact,
        None => true,
    };
    let mut best: Option<(Score, SubtreeShape)> = None;
    for body in bodies(ctx, &regular) {
        let mut cand = base.clone();
        pack::attach_below(&mut cand, root, body, ctx.params.layer_gap);
        pack::write_local_routes(&mut cand, ctx, root, &ids);
        let score = score_shape(&cand, ctx.params.preferred_aspect_ratio, consider_ratio);
        match &best {
            Some((s, _)) if !score.better_than(s) => {}
            _ => best = Some((score, cand)),
        }
    }
    Ok(best.map(|(_, s)| s).unwrap_or(base))
}

fn search(
    ctx: &PlaceCtx<'_>,
    root: &str,
    children: Vec<(String, SubtreeShape)>,
) -> Result<SubtreeShape, LayoutError> {
    if children.is_empty() {
        return Ok(pack::empty_parent(ctx, root));
    }
    let ids = pack::regular_ids(&children);
    let consider_ratio = match ctx.plan.parent.get(root) {
        Some(p) => ctx.plan.placer(p) != PlacerId::Compact,
        None => true,
    };
    let mut best: Option<(Score, SubtreeShape)> = None;
    for body in bodies(ctx, &children) {
        let mut placed = pack::with_parent_above(ctx, root, body);
        pack::write_local_routes(&mut placed, ctx, root, &ids);
        let score = score_shape(&placed, ctx.params.preferred_aspect_ratio, consider_ratio);
        match &best {
            Some((s, _)) if !score.better_than(s) => {}
            _ => best = Some((score, placed)),
        }
    }
    Ok(best
        .map(|(_, s)| s)
        .unwrap_or_else(|| pack::empty_parent(ctx, root)))
}

fn bodies(ctx: &PlaceCtx<'_>, children: &[(String, SubtreeShape)]) -> Vec<SubtreeShape> {
    let n = children.len();
    let gap = ctx.params.node_gap;
    let mut out = Vec::new();
    out.push(pack::row(pack::clone_kids(children), gap));
    if n >= 2 {
        out.push(pack::column(pack::clone_kids(children), gap));
        out.push(pack::staggered_two_rows(pack::clone_kids(children), gap));
        let max_rows = n.min(6);
        for rows in 2..max_rows {
            out.push(pack::grid(pack::clone_kids(children), rows, gap));
        }
    }
    out
}

#[derive(Clone, Copy)]
struct Score {
    ratio_err: f64,
    area: f64,
    width: f64,
}

impl Score {
    fn better_than(self, other: &Score) -> bool {
        match self.ratio_err.total_cmp(&other.ratio_err) {
            std::cmp::Ordering::Less => true,
            std::cmp::Ordering::Greater => false,
            std::cmp::Ordering::Equal => match self.area.total_cmp(&other.area) {
                std::cmp::Ordering::Less => true,
                std::cmp::Ordering::Greater => false,
                std::cmp::Ordering::Equal => self.width < other.width,
            },
        }
    }
}

fn score_shape(shape: &SubtreeShape, preferred: f64, consider_ratio: bool) -> Score {
    let Some(b) = shape.bounds() else {
        return Score {
            ratio_err: f64::INFINITY,
            area: f64::INFINITY,
            width: f64::INFINITY,
        };
    };
    let w = b.width.max(1e-6);
    let h = b.height.max(1e-6);
    let ratio_err = if consider_ratio && preferred > 0.0 {
        (w / h - preferred).abs()
    } else {
        0.0
    };
    Score {
        ratio_err,
        area: w * h,
        width: w,
    }
}
