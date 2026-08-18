//! `radial`: concentric circles by depth; angular demand bottom-up (Eades).

use std::collections::BTreeMap;
use std::f64::consts::PI;

use tautcore_engine_api::LayoutError;
use tautcore_model::geometry::{Point, Rect};

use super::super::geom::{node_extent, polar_point, spoke_straight};
use super::super::shape::SubtreeShape;
use super::polar::{polar_spoke, TAU, THETA0};
use super::{ISubtreePlacer, PlaceCtx};
use crate::layout::tree::params::PlacerId;
use crate::layout::tree::plan::TreePlan;

#[derive(Debug, Default, Clone, Copy)]
pub struct RadialPlacer;

impl ISubtreePlacer for RadialPlacer {
    fn place_subtree(
        &self,
        ctx: &PlaceCtx<'_>,
        root: &str,
        children: Vec<(String, SubtreeShape)>,
    ) -> Result<SubtreeShape, LayoutError> {
        ring_fallback(ctx, root, children)
    }
}

impl RadialPlacer {
    pub fn place(&self, ctx: &PlaceCtx<'_>, root: &str) -> Result<SubtreeShape, LayoutError> {
        if subtree_all(root, ctx.plan, PlacerId::Radial) {
            eades_region(ctx, root)
        } else {
            let kids = super::place_children(ctx, root)?;
            self.place_subtree(ctx, root, kids)
        }
    }
}

fn subtree_all(id: &str, plan: &TreePlan, want: PlacerId) -> bool {
    let mut stack = vec![id.to_string()];
    while let Some(cur) = stack.pop() {
        if plan.placer(&cur) != want {
            return false;
        }
        for k in plan.children_of(&cur) {
            stack.push(k.clone());
        }
    }
    true
}

fn eades_region(ctx: &PlaceCtx<'_>, root: &str) -> Result<SubtreeShape, LayoutError> {
    let ids = collect_preorder(root, ctx.plan);
    let mut depth: BTreeMap<String, u32> = BTreeMap::new();
    depth.insert(root.to_string(), 0);
    let mut max_d = 0u32;
    for id in &ids {
        let d = depth.get(id).copied().unwrap_or(0);
        for k in ctx.plan.children_of(id) {
            depth.insert(k.clone(), d + 1);
            max_d = max_d.max(d + 1);
        }
    }

    let mut demand: BTreeMap<String, f64> = BTreeMap::new();
    for id in ids.iter().rev() {
        let own = size_weight(ctx, id);
        let sum: f64 = ctx
            .plan
            .children_of(id)
            .iter()
            .map(|k| demand.get(k).copied().unwrap_or(own))
            .sum();
        demand.insert(id.clone(), own.max(sum).max(1.0));
    }

    let mut wedge: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    wedge.insert(root.to_string(), (THETA0, TAU));
    let mut theta: BTreeMap<String, f64> = BTreeMap::new();
    theta.insert(root.to_string(), THETA0);
    for id in &ids {
        let (start, width) = wedge.get(id).copied().unwrap_or((THETA0, TAU));
        let kids = ctx.plan.children_of(id);
        if kids.is_empty() {
            continue;
        }
        let total: f64 = kids.iter().map(|k| demand[k]).sum();
        let mut cursor = start;
        for k in kids {
            let w = if total > 1e-12 {
                width * (demand[k] / total)
            } else {
                width / (kids.len() as f64)
            };
            wedge.insert(k.clone(), (cursor, w));
            theta.insert(k.clone(), cursor + w / 2.0);
            cursor += w;
        }
    }

    let mut by_depth: Vec<Vec<String>> = vec![Vec::new(); (max_d as usize) + 1];
    for id in &ids {
        let d = depth[id] as usize;
        by_depth[d].push(id.clone());
    }

    let mut radius = vec![0.0; by_depth.len()];
    for d in 1..by_depth.len() {
        let prev_ext = by_depth[d - 1]
            .iter()
            .map(|id| node_extent(&frame_at(ctx, id, 0.0, 0.0)))
            .fold(0.0, f64::max);
        let cur_ext = by_depth[d]
            .iter()
            .map(|id| node_extent(&frame_at(ctx, id, 0.0, 0.0)))
            .fold(0.0, f64::max);
        let mut r = radius[d - 1] + ctx.params.layer_gap + prev_ext + cur_ext;
        for id in &by_depth[d] {
            let width = wedge.get(id).map(|w| w.1).unwrap_or(TAU);
            if width >= PI {
                continue;
            }
            let half = (width / 2.0).max(1e-4);
            let need = (node_extent(&frame_at(ctx, id, 0.0, 0.0)) + ctx.params.node_gap / 2.0)
                / half.sin();
            r = r.max(need);
        }
        radius[d] = r;
    }

    let origin = Point { x: 0.0, y: 0.0 };
    let mut frames = BTreeMap::new();
    for id in &ids {
        let d = depth[id] as usize;
        let th = theta[id];
        let c = polar_point(origin, radius[d], th);
        frames.insert(id.clone(), frame_at(ctx, id, c.x, c.y));
    }

    let mut routes = BTreeMap::new();
    for id in &ids {
        let pd = depth[id] as usize;
        let pth = theta[id];
        let Some(pf) = frames.get(id).copied() else {
            continue;
        };
        for k in ctx.plan.children_of(id) {
            let Some(eid) = ctx.plan.edge_of_child.get(k) else {
                continue;
            };
            let Some(cf) = frames.get(k).copied() else {
                continue;
            };
            let cd = depth[k] as usize;
            let cth = theta[k];
            let route = if pd == 0 {
                spoke_straight(&pf, &cf)
            } else {
                polar_spoke(origin, &pf, radius[pd], pth, &cf, radius[cd], cth)
            };
            routes.insert(eid.clone(), route);
        }
    }

    Ok(SubtreeShape::from_parts(root, frames, routes))
}

fn size_weight(ctx: &PlaceCtx<'_>, id: &str) -> f64 {
    let sz = ctx.size_of[id];
    sz.width.max(sz.height).max(1.0)
}

fn frame_at(ctx: &PlaceCtx<'_>, id: &str, cx: f64, cy: f64) -> Rect {
    let sz = ctx.size_of[id];
    Rect::new(
        cx - sz.width / 2.0,
        cy - sz.height / 2.0,
        sz.width,
        sz.height,
    )
}

fn collect_preorder(root: &str, plan: &TreePlan) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_string()];
    while let Some(cur) = stack.pop() {
        out.push(cur.clone());
        for k in plan.children_of(&cur).iter().rev() {
            stack.push(k.clone());
        }
    }
    out
}

fn ring_fallback(
    ctx: &PlaceCtx<'_>,
    root: &str,
    children: Vec<(String, SubtreeShape)>,
) -> Result<SubtreeShape, LayoutError> {
    super::balloon::arrange_ring(ctx, root, children, 0.0)
}
