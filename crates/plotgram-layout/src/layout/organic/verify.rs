//! Plan / Metric / Ink verifiers. Fail hard, never patch.

use plotgram_engine_api::LayoutError;
use plotgram_model::result::EdgePlacement;

use super::demand::OrgDemandBoard;
use super::geom::rects_overlap;
use super::org_err;
use super::params::OrganicParams;
use super::plan::{EdgeRole, OrganicMetric, OrganicPlan};

/// Slack for overlap verification (VPSC alternation may leave sub-pixel
/// interpenetration on the cross axis).
const OVERLAP_SLACK: f64 = 0.5;

pub fn verify_plan(plan: &OrganicPlan) -> Result<(), LayoutError> {
    let mut covered = std::collections::BTreeSet::new();
    for (ci, comp) in plan.components.iter().enumerate() {
        if comp.id as usize != ci {
            return Err(org_err(format!(
                "organic: invariant: component id {} != index {ci}",
                comp.id
            )));
        }
        if comp.len() != comp.adjacency.len() {
            return Err(org_err(format!(
                "organic: invariant: component {} adjacency size mismatch",
                comp.id
            )));
        }
        for (i, id) in comp.nodes.iter().enumerate() {
            if !covered.insert(id.as_str()) {
                return Err(org_err(format!(
                    "organic: invariant: node `{id}` in multiple components"
                )));
            }
            match plan.node_of.get(id) {
                Some(&(cid, idx)) if cid == comp.id && idx == i => {}
                _ => {
                    return Err(org_err(format!(
                        "organic: invariant: node_of mismatch for `{id}`"
                    )));
                }
            }
        }
        for (i, nbrs) in comp.adjacency.iter().enumerate() {
            if nbrs.contains(&i) {
                return Err(org_err(format!(
                    "organic: invariant: self adjacency at {i} in component {}",
                    comp.id
                )));
            }
            for &j in nbrs {
                if j >= comp.len() {
                    return Err(org_err(format!(
                        "organic: invariant: adjacency index {j} out of range in component {}",
                        comp.id
                    )));
                }
                if !comp.adjacency[j].contains(&i) {
                    return Err(org_err(format!(
                        "organic: invariant: adjacency not symmetric {i}/{j} in component {}",
                        comp.id
                    )));
                }
            }
        }
    }
    if covered.len() != plan.nodes.len() {
        return Err(org_err(
            "organic: invariant: components do not cover all nodes",
        ));
    }
    for (eid, role) in &plan.edge_role {
        match role {
            EdgeRole::Plain | EdgeRole::Loop | EdgeRole::Parallel => {}
        }
        if eid.is_empty() {
            return Err(org_err("organic: invariant: empty edge id in edge_role"));
        }
    }
    Ok(())
}

pub fn verify_edge_roles(
    plan: &OrganicPlan,
    edge_ids: impl Iterator<Item = String>,
) -> Result<(), LayoutError> {
    for eid in edge_ids {
        if !plan.edge_role.contains_key(&eid) {
            return Err(org_err(format!(
                "organic: invariant: missing edge_role `{eid}`"
            )));
        }
    }
    Ok(())
}

pub fn verify_metric(
    plan: &OrganicPlan,
    metric: &OrganicMetric,
    demand: &OrgDemandBoard,
    params: &OrganicParams,
) -> Result<(), LayoutError> {
    let frames: Vec<_> = plan
        .nodes
        .iter()
        .map(|id| {
            metric.frames.get(id).ok_or_else(|| {
                org_err(format!("organic: invariant: missing frame `{id}`"))
            })
        })
        .collect::<Result<_, _>>()?;

    for (id, f) in plan.nodes.iter().zip(frames.iter()) {
        if [f.x, f.y, f.width, f.height].iter().any(|v| !v.is_finite()) {
            return Err(org_err(format!(
                "organic: invariant: non-finite frame `{id}`"
            )));
        }
        let need_w = demand.get(&super::demand::OrgDemandKey::NodeWidth(id.clone()));
        let need_h = demand.get(&super::demand::OrgDemandKey::NodeHeight(id.clone()));
        if f.width + 1e-6 < need_w || f.height + 1e-6 < need_h {
            return Err(org_err(format!(
                "organic: invariant: frame `{id}` below demand size"
            )));
        }
    }

    if !params.allow_node_overlaps {
        for i in 0..frames.len() {
            for j in (i + 1)..frames.len() {
                if rects_overlap(&frames[i], &frames[j], -OVERLAP_SLACK) {
                    return Err(org_err(format!(
                        "organic: invariant: nodes `{}` / `{}` overlap",
                        plan.nodes[i], plan.nodes[j]
                    )));
                }
            }
        }
    }

    // Component bounding boxes must not interpenetrate after packing.
    if plan.components.len() > 1 {
        let boxes: Vec<_> = plan
            .components
            .iter()
            .map(|comp| {
                let mut min_x = f64::INFINITY;
                let mut min_y = f64::INFINITY;
                let mut max_x = f64::NEG_INFINITY;
                let mut max_y = f64::NEG_INFINITY;
                for id in &comp.nodes {
                    let f = &metric.frames[id];
                    min_x = min_x.min(f.x);
                    min_y = min_y.min(f.y);
                    max_x = max_x.max(f.right());
                    max_y = max_y.max(f.bottom());
                }
                (min_x, min_y, max_x, max_y)
            })
            .collect();
        for i in 0..boxes.len() {
            for j in (i + 1)..boxes.len() {
                let (ax0, ay0, ax1, ay1) = boxes[i];
                let (bx0, by0, bx1, by1) = boxes[j];
                let sep = ax1 <= bx0 - OVERLAP_SLACK
                    || bx1 <= ax0 - OVERLAP_SLACK
                    || ay1 <= by0 - OVERLAP_SLACK
                    || by1 <= ay0 - OVERLAP_SLACK;
                if !sep {
                    return Err(org_err(format!(
                        "organic: invariant: components {i} / {j} bounding boxes overlap"
                    )));
                }
            }
        }
    }
    Ok(())
}

pub fn verify_ink(
    edges: &[EdgePlacement],
    metric: &OrganicMetric,
    defer: bool,
) -> Result<(), LayoutError> {
    if defer {
        for e in edges {
            if e.source == e.target {
                continue; // self-loops always keep their flick
            }
            if !e.path.samples().is_empty() {
                return Err(org_err(format!(
                    "organic: invariant: deferred edge `{}` has a path",
                    e.id
                )));
            }
        }
        return Ok(());
    }
    for e in edges {
        let pts = e.path.samples();
        if pts.len() < 2 {
            return Err(org_err(format!(
                "organic: invariant: edge `{}` path too short",
                e.id
            )));
        }
        for p in &pts {
            if !p.x.is_finite() || !p.y.is_finite() {
                return Err(org_err(format!(
                    "organic: invariant: non-finite path on `{}`",
                    e.id
                )));
            }
        }
        if metric.frames.contains_key(&e.source) && e.from_port.is_none() {
            return Err(org_err(format!(
                "organic: invariant: edge `{}` missing from_port",
                e.id
            )));
        }
        if metric.frames.contains_key(&e.target) && e.to_port.is_none() {
            return Err(org_err(format!(
                "organic: invariant: edge `{}` missing to_port",
                e.id
            )));
        }
    }
    Ok(())
}
