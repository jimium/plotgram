//! `balloon`: child disks around the parent; binary-search the ring radius.

use tautcore_engine_api::LayoutError;
use tautcore_model::geometry::{Point, Rect};

use super::super::geom::{node_extent, polar_point, spoke_straight};
use super::super::shape::SubtreeShape;
use super::polar::{radius_from_root, rotate_shape_upright, TAU, THETA0};
use super::{ISubtreePlacer, PlaceCtx};

#[derive(Debug, Default, Clone, Copy)]
pub struct BalloonPlacer;

impl ISubtreePlacer for BalloonPlacer {
    fn place_subtree(
        &self,
        ctx: &PlaceCtx<'_>,
        root: &str,
        children: Vec<(String, SubtreeShape)>,
    ) -> Result<SubtreeShape, LayoutError> {
        let reserved = if ctx.plan.parent.contains_key(root) {
            0.4
        } else {
            0.0
        };
        arrange_ring(ctx, root, children, reserved)
    }
}

impl BalloonPlacer {
    pub fn place(&self, ctx: &PlaceCtx<'_>, root: &str) -> Result<SubtreeShape, LayoutError> {
        let kids = super::place_children(ctx, root)?;
        self.place_subtree(ctx, root, kids)
    }
}

pub(super) fn arrange_ring(
    ctx: &PlaceCtx<'_>,
    root: &str,
    children: Vec<(String, SubtreeShape)>,
    reserved: f64,
) -> Result<SubtreeShape, LayoutError> {
    let sz = ctx.size_of[root];
    let parent = Rect::new(-sz.width / 2.0, -sz.height / 2.0, sz.width, sz.height);
    let mut placed = SubtreeShape::from_parts(root, Default::default(), Default::default());
    placed.frames.insert(root.to_string(), parent);
    if children.is_empty() {
        return Ok(placed);
    }

    let r_parent = node_extent(&parent);
    let gap = ctx.params.node_gap / 2.0;
    let radii: Vec<f64> = children
        .iter()
        .map(|(k, cp)| radius_from_root(cp, k) + gap)
        .collect();
    let r_max = radii.iter().copied().fold(0.0, f64::max);
    let mut lo = r_parent + ctx.params.layer_gap + r_max;
    let mut hi = lo * (children.len() as f64).max(2.0);
    for _ in 0..24 {
        if ring_fits(hi, &radii, reserved) {
            break;
        }
        hi *= 2.0;
    }
    for _ in 0..40 {
        let mid = (lo + hi) / 2.0;
        if ring_fits(mid, &radii, reserved) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    let ring = hi;

    let angles: Vec<f64> = radii
        .iter()
        .map(|&r| 2.0 * (r / ring).clamp(0.0, 0.999).asin())
        .collect();
    let used: f64 = angles.iter().sum();
    let span = (TAU - reserved).max(used);
    let mut cursor = THETA0 + reserved / 2.0 + (span - used) / 2.0;
    let origin = Point { x: 0.0, y: 0.0 };

    for ((k, mut cp), ang) in children.into_iter().zip(angles) {
        let th = cursor + ang / 2.0;
        cursor += ang;
        let target = polar_point(origin, ring, th);
        if let Some(cf) = cp.frames.get(&k).copied() {
            let dc = cf.center();
            cp.translate(target.x - dc.x, target.y - dc.y);
        }
        if reserved > 1e-6 {
            if let Some(cf) = cp.frames.get(&k).copied() {
                let toward_parent = th + std::f64::consts::PI;
                rotate_shape_upright(&mut cp, cf.center(), toward_parent - THETA0);
            }
        }
        if let Some(cf) = cp.frames.get(&k).copied() {
            if let Some(eid) = ctx.plan.edge_of_child.get(&k) {
                placed
                    .routes
                    .insert(eid.clone(), spoke_straight(&parent, &cf));
            }
        }
        placed.merge(cp);
    }
    Ok(placed)
}

fn ring_fits(r: f64, radii: &[f64], reserved: f64) -> bool {
    let mut sum = 0.0;
    for &ri in radii {
        if ri >= r - 1e-9 {
            return false;
        }
        sum += 2.0 * (ri / r).asin();
    }
    sum <= TAU - reserved + 1e-9
}
