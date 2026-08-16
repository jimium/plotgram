//! Shared Buchheim + layer-y for `single-layer` / `level-aligned`.
//!
//! Uniform region: one Buchheim over the whole subtree (contour merge).
//! Mixed children: AABB pack of already-placed child shapes (fallback).

use std::collections::{BTreeMap, BTreeSet};

use plotgram_algo::buchheim::{self, BuchheimTree};
use plotgram_engine_api::LayoutError;
use plotgram_model::geometry::Rect;
use plotgram_model::port::Side;

use super::super::geom::{opposite, parent_child_route, port_point};
use super::super::shape::SubtreeShape;
use super::transform::rotate_frames_around;
use super::PlaceCtx;
use crate::layout::tree::params::{RootAlignment, SubtreeTransform, TreeParams};
use crate::layout::tree::plan::{TreePlan, TreeRoute};
use crate::layout::tree::tree_err;

pub fn subtree_uniform_layered(id: &str, plan: &TreePlan) -> bool {
    let mut stack = vec![id.to_string()];
    while let Some(cur) = stack.pop() {
        if !plan.placer(&cur).is_layered() {
            return false;
        }
        for k in plan.children_of(&cur) {
            stack.push(k.clone());
        }
    }
    true
}

pub(super) fn collect_subtree(
    root: &str,
    child_override: Option<&[String]>,
    plan: &TreePlan,
) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_string()];
    let mut seen = BTreeSet::new();
    while let Some(cur) = stack.pop() {
        if !seen.insert(cur.clone()) {
            continue;
        }
        out.push(cur.clone());
        let kids: Vec<String> = if cur == root {
            if let Some(ov) = child_override {
                ov.to_vec()
            } else {
                plan.children_of(&cur).to_vec()
            }
        } else {
            plan.children_of(&cur).to_vec()
        };
        for k in kids.into_iter().rev() {
            stack.push(k);
        }
    }
    out
}

/// Buchheim x + layer y, optional 90° wrap. `child_override` limits the local
/// root's children (split sides); descendants follow the plan.
pub fn place_uniform(
    root: &str,
    child_override: Option<&[String]>,
    transform: SubtreeTransform,
    ctx: &PlaceCtx<'_>,
) -> Result<SubtreeShape, LayoutError> {
    let plan = ctx.plan;
    let params = ctx.params;
    let size_of = ctx.size_of;
    let ids = collect_subtree(root, child_override, plan);
    let n = ids.len();
    if n == 0 {
        return Ok(SubtreeShape::forest());
    }
    let index: BTreeMap<&str, usize> = ids.iter().map(|s| s.as_str()).zip(0..).collect();
    let mut children = vec![Vec::new(); n];
    for id in &ids {
        let kids: &[String] = if id == root {
            child_override.unwrap_or_else(|| plan.children_of(id))
        } else {
            plan.children_of(id)
        };
        children[index[id.as_str()]] = kids
            .iter()
            .filter_map(|k| index.get(k.as_str()).copied())
            .collect();
    }
    // 90° wrap: Buchheim/layers see swapped axes so AABB gap survives rotation.
    let swap = transform != SubtreeTransform::None;
    let widths: Vec<f64> = ids
        .iter()
        .map(|id| {
            let sz = size_of[id];
            if swap {
                sz.height
            } else {
                sz.width
            }
        })
        .collect();
    let xs = buchheim::place(&BuchheimTree {
        root: 0,
        children,
        widths,
        sibling_gap: params.node_gap,
    })
    .map_err(|e| tree_err(format!("tree: invariant: {e}")))?;

    let mut rel_depth: BTreeMap<&str, u32> = BTreeMap::new();
    rel_depth.insert(root, 0);
    let mut q = vec![root.to_string()];
    while let Some(u) = q.pop() {
        let d = *rel_depth.get(u.as_str()).unwrap_or(&0);
        let kids: &[String] = if u == root {
            child_override.unwrap_or_else(|| plan.children_of(&u))
        } else {
            plan.children_of(&u)
        };
        for k in kids {
            rel_depth.insert(k.as_str(), d + 1);
            q.push(k.clone());
        }
    }
    let mut layer_h: BTreeMap<u32, f64> = BTreeMap::new();
    for id in &ids {
        let d = *rel_depth.get(id.as_str()).unwrap_or(&0);
        let sz = size_of[id];
        let h = if swap { sz.width } else { sz.height };
        layer_h
            .entry(d)
            .and_modify(|v| *v = (*v).max(h))
            .or_insert(h);
    }
    let max_d = layer_h.keys().copied().max().unwrap_or(0);
    let mut layer_y = BTreeMap::new();
    let mut y = 0.0;
    for d in 0..=max_d {
        layer_y.insert(d, y);
        y += layer_h.get(&d).copied().unwrap_or(0.0) + params.layer_gap;
    }

    let mut frames = BTreeMap::new();
    for (i, id) in ids.iter().enumerate() {
        let sz = size_of[id];
        let (pw, ph) = if swap {
            (sz.height, sz.width)
        } else {
            (sz.width, sz.height)
        };
        let d = *rel_depth.get(id.as_str()).unwrap_or(&0);
        let top = *layer_y.get(&d).unwrap_or(&0.0);
        frames.insert(id.clone(), Rect::new(xs[i] - pw / 2.0, top, pw, ph));
    }

    let origin = frames[root].center();
    rotate_frames_around(&mut frames, origin, transform);
    for (id, f) in frames.iter_mut() {
        let sz = size_of[id];
        let c = f.center();
        *f = Rect::new(
            c.x - sz.width / 2.0,
            c.y - sz.height / 2.0,
            sz.width,
            sz.height,
        );
    }

    apply_root_alignment_all(
        &mut frames,
        root,
        child_override,
        plan,
        params.root_alignment,
    );
    let routes = write_routes(&frames, root, child_override, plan, params);
    Ok(SubtreeShape::from_parts(root, frames, routes))
}

pub fn apply_root_alignment_all(
    frames: &mut BTreeMap<String, Rect>,
    local_root: &str,
    child_override: Option<&[String]>,
    plan: &TreePlan,
    alignment: RootAlignment,
) {
    let ids = collect_subtree(local_root, child_override, plan);
    for id in &ids {
        let kids: &[String] = if id == local_root {
            child_override.unwrap_or_else(|| plan.children_of(id))
        } else {
            plan.children_of(id)
        };
        align_one_root(frames, id, kids, plan, alignment);
    }
}

pub fn apply_root_alignment(
    frames: &mut BTreeMap<String, Rect>,
    parent_id: &str,
    kids: &[String],
    plan: &TreePlan,
    alignment: RootAlignment,
) {
    align_one_root(frames, parent_id, kids, plan, alignment);
}

fn align_one_root(
    frames: &mut BTreeMap<String, Rect>,
    id: &str,
    kids: &[String],
    plan: &TreePlan,
    alignment: RootAlignment,
) {
    if kids.is_empty() {
        return;
    }
    let Some(parent) = frames.get(id).copied() else {
        return;
    };
    let mut child_frames = Vec::new();
    let mut child_bounds = Vec::new();
    for k in kids {
        let Some(cf) = frames.get(k) else { continue };
        child_frames.push(*cf);
        child_bounds.push(subtree_bounds(k, frames, plan));
    }
    if child_frames.is_empty() {
        return;
    }
    let new_x = match alignment {
        RootAlignment::Center | RootAlignment::CenterOfPorts => parent.x,
        RootAlignment::Median => {
            let mut xs: Vec<f64> = child_frames.iter().map(|f| f.center().x).collect();
            xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let mid = xs[xs.len() / 2];
            mid - parent.width / 2.0
        }
        RootAlignment::Leading => {
            let left = child_bounds
                .iter()
                .map(|b| b.x)
                .fold(f64::INFINITY, f64::min);
            left
        }
        RootAlignment::Trailing => {
            let right = child_bounds
                .iter()
                .map(|b| b.right())
                .fold(f64::NEG_INFINITY, f64::max);
            right - parent.width
        }
        RootAlignment::LeadingOnBus => child_frames[0].center().x - parent.width / 2.0,
        RootAlignment::TrailingOnBus => {
            child_frames[child_frames.len() - 1].center().x - parent.width / 2.0
        }
    };
    if let Some(f) = frames.get_mut(id) {
        f.x = new_x;
    }
}

pub fn write_routes(
    frames: &BTreeMap<String, Rect>,
    local_root: &str,
    child_override: Option<&[String]>,
    plan: &TreePlan,
    params: &TreeParams,
) -> BTreeMap<String, TreeRoute> {
    let ids = collect_subtree(local_root, child_override, plan);
    let mut routes = BTreeMap::new();
    for id in &ids {
        let kids: &[String] = if id == local_root {
            child_override.unwrap_or_else(|| plan.children_of(id))
        } else {
            plan.children_of(id)
        };
        for k in kids {
            let Some(eid) = plan.edge_of_child.get(k) else {
                continue;
            };
            let Some(pf) = frames.get(id) else { continue };
            let Some(cf) = frames.get(k) else { continue };
            let to_side = plan.child_connectors.get(k).copied().unwrap_or(Side::North);
            let from_side = opposite(to_side);
            let start = port_point(pf, from_side);
            let end = port_point(cf, to_side);
            routes.insert(
                eid.clone(),
                parent_child_route(
                    start,
                    from_side,
                    end,
                    to_side,
                    params.routing_style,
                    params.min_first_segment,
                ),
            );
        }
    }
    routes
}

pub fn subtree_bounds(id: &str, frames: &BTreeMap<String, Rect>, plan: &TreePlan) -> Rect {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut stack = vec![id.to_string()];
    while let Some(cur) = stack.pop() {
        if let Some(f) = frames.get(&cur) {
            min_x = min_x.min(f.x);
            min_y = min_y.min(f.y);
            max_x = max_x.max(f.right());
            max_y = max_y.max(f.bottom());
        }
        for k in plan.children_of(&cur) {
            stack.push(k.clone());
        }
    }
    if !min_x.is_finite() {
        frames
            .get(id)
            .copied()
            .unwrap_or_else(|| Rect::new(0.0, 0.0, 0.0, 0.0))
    } else {
        Rect::new(min_x, min_y, max_x - min_x, max_y - min_y)
    }
}

/// Mixed-placer fallback: children already placed; pack AABB under the parent.
pub fn pack(
    ctx: &PlaceCtx<'_>,
    root: &str,
    children: Vec<(String, SubtreeShape)>,
) -> Result<SubtreeShape, LayoutError> {
    let sz = ctx.size_of[root];
    let mut placed = SubtreeShape::from_parts(root, BTreeMap::new(), BTreeMap::new());
    let parent = Rect::new(0.0, 0.0, sz.width, sz.height);
    let mut cursor = 0.0;
    let child_y = parent.bottom() + ctx.params.layer_gap;
    let kids: Vec<String> = children.iter().map(|(k, _)| k.clone()).collect();
    for (_k, mut cp) in children {
        if let Some(b) = cp.bounds() {
            cp.translate(cursor - b.x, child_y - b.y);
            cursor = cp.bounds().map(|bb| bb.right()).unwrap_or(cursor) + ctx.params.node_gap;
        }
        placed.merge(cp);
    }
    if let Some(b) = placed.bounds() {
        let pc = (b.x + b.right()) / 2.0;
        let mut parent = parent;
        parent.x = pc - parent.width / 2.0;
        placed.frames.insert(root.to_string(), parent);
    } else {
        placed.frames.insert(root.to_string(), parent);
    }
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
