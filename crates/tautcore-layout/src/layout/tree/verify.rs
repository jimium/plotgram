//! Plan / Metric / Ink verifiers. Fail hard, never patch.

use std::collections::BTreeSet;

use tautcore_engine_api::LayoutError;
use tautcore_model::result::EdgePlacement;

use super::demand::{TreeDemandBoard, TreeDemandKey};
use super::metric::TreeMetric;
use super::plan::TreePlan;
use super::tree_err;

const EPS: f64 = 1e-6;

pub fn verify_plan(plan: &TreePlan) -> Result<(), LayoutError> {
    let mut seen = BTreeSet::new();
    for id in &plan.nodes {
        if !seen.insert(id.as_str()) {
            return Err(tree_err(format!("tree: invariant: duplicate node `{id}`")));
        }
        if !plan.placer_of.contains_key(id) {
            return Err(tree_err(format!(
                "tree: invariant: missing placer_of `{id}`"
            )));
        }
        let is_root = plan.roots.iter().any(|r| r == id);
        if is_root {
            if plan.parent.contains_key(id) {
                return Err(tree_err(format!(
                    "tree: invariant: root `{id}` has a parent"
                )));
            }
        } else if !plan.parent.contains_key(id) {
            return Err(tree_err(format!(
                "tree: invariant: non-root `{id}` has no parent"
            )));
        }
        if let Some(p) = plan.parent.get(id) {
            let pd = plan.depth.get(p).copied().unwrap_or(0);
            let d = plan.depth.get(id).copied().unwrap_or(u32::MAX);
            if d != pd + 1 {
                return Err(tree_err(format!(
                    "tree: invariant: depth of `{id}` is {d}, parent `{p}` is {pd}"
                )));
            }
        } else if plan.depth.get(id).copied() != Some(0) {
            return Err(tree_err(format!(
                "tree: invariant: root `{id}` depth is not 0"
            )));
        }
        if plan.explicit_placer.contains_key(id) && !plan.placer_of.contains_key(id) {
            return Err(tree_err(format!(
                "tree: invariant: explicit_placer `{id}` missing from placer_of"
            )));
        }
    }
    let mut tree_set: BTreeSet<&str> = plan.tree_edge_ids.iter().map(|s| s.as_str()).collect();
    let extra: BTreeSet<&str> = plan.extra_edge_ids.iter().map(|s| s.as_str()).collect();
    if tree_set.intersection(&extra).next().is_some() {
        return Err(tree_err(
            "tree: invariant: tree and extra edge id sets overlap",
        ));
    }
    for (child, eid) in &plan.edge_of_child {
        if !tree_set.remove(eid.as_str()) {
            return Err(tree_err(format!(
                "tree: invariant: edge_of_child `{child}` → `{eid}` not in tree_edge_ids"
            )));
        }
    }
    if !tree_set.is_empty() {
        return Err(tree_err(format!(
            "tree: invariant: tree_edge_ids without edge_of_child: {tree_set:?}"
        )));
    }
    Ok(())
}

pub fn verify_metric(
    plan: &TreePlan,
    metric: &TreeMetric,
    demand: &TreeDemandBoard,
) -> Result<(), LayoutError> {
    for id in &plan.nodes {
        let Some(f) = metric.frames.get(id) else {
            return Err(tree_err(format!("tree: invariant: missing frame `{id}`")));
        };
        if !f.x.is_finite() || !f.y.is_finite() || !f.width.is_finite() || !f.height.is_finite() {
            return Err(tree_err(format!(
                "tree: invariant: non-finite frame `{id}`"
            )));
        }
        let need_w = demand.get(TreeDemandKey::NodeWidth(id.clone()));
        let need_h = demand.get(TreeDemandKey::NodeHeight(id.clone()));
        if f.width + EPS < need_w || f.height + EPS < need_h {
            return Err(tree_err(format!(
                "tree: invariant: frame `{id}` below demand size"
            )));
        }
    }
    for eid in &plan.tree_edge_ids {
        let Some(r) = metric.routes.get(eid) else {
            return Err(tree_err(format!("tree: invariant: missing route `{eid}`")));
        };
        let s = r.start();
        let e = r.end();
        if !s.x.is_finite() || !s.y.is_finite() || !e.x.is_finite() || !e.y.is_finite() {
            return Err(tree_err(format!(
                "tree: invariant: non-finite route `{eid}`"
            )));
        }
    }
    for id in &plan.nodes {
        let kids = plan.children_of(id);
        for (i, a) in kids.iter().enumerate() {
            let Some(fa) = metric.frames.get(a) else {
                continue;
            };
            for b in kids.iter().skip(i + 1) {
                let Some(fb) = metric.frames.get(b) else {
                    continue;
                };
                let overlap_x = fa.right() > fb.x + EPS && fb.right() > fa.x + EPS;
                let overlap_y = fa.bottom() > fb.y + EPS && fb.bottom() > fa.y + EPS;
                if overlap_x && overlap_y {
                    return Err(tree_err(format!(
                        "tree: invariant: sibling frames overlap `{a}` / `{b}`"
                    )));
                }
            }
        }
    }
    Ok(())
}

pub fn verify_ink(
    edges: &[EdgePlacement],
    metric: &TreeMetric,
    defer: bool,
) -> Result<(), LayoutError> {
    if defer {
        for e in edges {
            if !e.path.samples().is_empty() {
                return Err(tree_err(format!(
                    "tree: invariant: deferred edge `{}` has a path",
                    e.id
                )));
            }
        }
        return Ok(());
    }
    for e in edges {
        let pts = e.path.samples();
        if pts.len() < 2 {
            return Err(tree_err(format!(
                "tree: invariant: edge `{}` path too short",
                e.id
            )));
        }
        for p in &pts {
            if !p.x.is_finite() || !p.y.is_finite() {
                return Err(tree_err(format!(
                    "tree: invariant: non-finite path on `{}`",
                    e.id
                )));
            }
        }
        if let Some(route) = metric.routes.get(&e.id) {
            let s = route.start();
            let t = route.end();
            let a = pts[0];
            let b = pts[pts.len() - 1];
            if (a.x - s.x).abs() > EPS
                || (a.y - s.y).abs() > EPS
                || (b.x - t.x).abs() > EPS
                || (b.y - t.y).abs() > EPS
            {
                return Err(tree_err(format!(
                    "tree: invariant: edge `{}` path terminals != route",
                    e.id
                )));
            }
        }
    }
    Ok(())
}
