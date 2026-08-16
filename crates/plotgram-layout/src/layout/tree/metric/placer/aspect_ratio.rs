//! `aspect-ratio`: pack children into rows/columns so W/H approaches the target.

use plotgram_engine_api::LayoutError;

use super::super::shape::SubtreeShape;
use super::pack;
use super::{ISubtreePlacer, PlaceCtx};

#[derive(Debug, Default, Clone, Copy)]
pub struct AspectRatioPlacer;

impl ISubtreePlacer for AspectRatioPlacer {
    fn place_subtree(
        &self,
        ctx: &PlaceCtx<'_>,
        root: &str,
        children: Vec<(String, SubtreeShape)>,
    ) -> Result<SubtreeShape, LayoutError> {
        arrange(ctx, root, children)
    }
}

impl AspectRatioPlacer {
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
    if children.is_empty() {
        return Ok(pack::empty_parent(ctx, root));
    }
    let ids = pack::regular_ids(&children);
    let ratio = if ctx.params.preferred_aspect_ratio > 0.0 {
        ctx.params.preferred_aspect_ratio
    } else {
        1.0
    };
    let mut best: Option<(Score, SubtreeShape)> = None;
    for body in bodies(ctx, &children) {
        let mut placed = pack::with_parent_corner(ctx, root, body);
        pack::write_local_routes(&mut placed, ctx, root, &ids);
        let score = score_shape(&placed, ratio);
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
    for rows in 1..=n {
        out.push(pack::grid(pack::clone_kids(children), rows, gap));
    }
    if n >= 2 {
        for cols in 1..=n {
            out.push(pack::grid_columns(pack::clone_kids(children), cols, gap));
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

fn score_shape(shape: &SubtreeShape, preferred: f64) -> Score {
    let Some(b) = shape.bounds() else {
        return Score {
            ratio_err: f64::INFINITY,
            area: f64::INFINITY,
            width: f64::INFINITY,
        };
    };
    let w = b.width.max(1e-6);
    let h = b.height.max(1e-6);
    Score {
        ratio_err: (w / h - preferred).abs(),
        area: w * h,
        width: w,
    }
}
