//! Plan / Metric / Ink verifiers. Fail hard, never patch.

use std::collections::BTreeSet;

use plotgram_engine_api::LayoutError;
use plotgram_model::result::EdgePlacement;

use super::circ_err;
use super::demand::{CircDemandBoard, CircDemandKey};
use super::geom::node_extent;
use super::params::CircleOrder;
use super::plan::{CircMetric, CircPlan, CircRoute, EdgeRole};

const EPS: f64 = 1e-6;

pub fn verify_plan(plan: &CircPlan) -> Result<(), LayoutError> {
    let mut seen = BTreeSet::new();
    for id in &plan.nodes {
        if !seen.insert(id.as_str()) {
            return Err(circ_err(format!(
                "circular: invariant: duplicate node `{id}`"
            )));
        }
        let Some(pid) = plan.partition_of.get(id) else {
            return Err(circ_err(format!(
                "circular: invariant: missing partition_of `{id}`"
            )));
        };
        let members = plan.members(*pid);
        if !members.iter().any(|m| m == id) {
            return Err(circ_err(format!(
                "circular: invariant: `{id}` not in partition {pid} circle order"
            )));
        }
    }
    for (pid, members) in &plan.partitions {
        let mut cover = BTreeSet::new();
        for m in members {
            if !cover.insert(m.as_str()) {
                return Err(circ_err(format!(
                    "circular: invariant: duplicate member `{m}` in partition {pid}"
                )));
            }
            match plan.partition_of.get(m) {
                Some(p) if p == pid => {}
                _ => {
                    return Err(circ_err(format!(
                        "circular: invariant: `{m}` partition_of mismatch"
                    )));
                }
            }
        }
    }
    if plan.partition_of.len() != plan.nodes.len() {
        return Err(circ_err(
            "circular: invariant: partition_of size != entity count",
        ));
    }

    let mut backbone_nodes = BTreeSet::new();
    for (pid, node) in &plan.backbone {
        if !backbone_nodes.insert(*pid) {
            return Err(circ_err(format!(
                "circular: invariant: duplicate backbone node {pid}"
            )));
        }
        if let Some(p) = node.parent {
            if p == *pid {
                return Err(circ_err(format!(
                    "circular: invariant: backbone self-parent {pid}"
                )));
            }
        }
        for c in &node.children {
            if *c == *pid {
                return Err(circ_err(format!(
                    "circular: invariant: backbone self-child {pid}"
                )));
            }
        }
    }
    for pid in plan.partitions.keys() {
        if !plan.backbone.contains_key(pid) {
            return Err(circ_err(format!(
                "circular: invariant: partition {pid} missing backbone node"
            )));
        }
    }

    for (i, comp) in plan.components.iter().enumerate() {
        if comp.id as usize != i {
            return Err(circ_err(format!(
                "circular: invariant: component id {} != index {i}",
                comp.id
            )));
        }
        if !comp.partitions.contains(&comp.root_partition) {
            return Err(circ_err(format!(
                "circular: invariant: component {} root partition missing",
                comp.id
            )));
        }
    }
    match plan.order_method {
        CircleOrder::Bfs | CircleOrder::Declaration | CircleOrder::Spectral => {}
    }
    for ((_, _), cut) in &plan.cut_of {
        if !plan.nodes.iter().any(|n| n == cut) {
            return Err(circ_err(format!(
                "circular: invariant: cut_of references unknown node `{cut}`"
            )));
        }
    }

    Ok(())
}

pub fn verify_edge_roles(
    plan: &CircPlan,
    edge_ids: impl Iterator<Item = String>,
) -> Result<(), LayoutError> {
    for eid in edge_ids {
        if !plan.edge_role.contains_key(&eid) {
            return Err(circ_err(format!(
                "circular: invariant: missing edge_role `{eid}`"
            )));
        }
    }
    Ok(())
}

pub fn verify_metric(
    plan: &CircPlan,
    metric: &CircMetric,
    demand: &CircDemandBoard,
) -> Result<(), LayoutError> {
    for id in &plan.nodes {
        let Some(f) = metric.frames.get(id) else {
            return Err(circ_err(format!(
                "circular: invariant: missing frame `{id}`"
            )));
        };
        if !f.x.is_finite() || !f.y.is_finite() || !f.width.is_finite() || !f.height.is_finite() {
            return Err(circ_err(format!(
                "circular: invariant: non-finite frame `{id}`"
            )));
        }
        let need_w = demand.get(CircDemandKey::NodeWidth(id.clone()));
        let need_h = demand.get(CircDemandKey::NodeHeight(id.clone()));
        if f.width + EPS < need_w || f.height + EPS < need_h {
            return Err(circ_err(format!(
                "circular: invariant: frame `{id}` below demand size"
            )));
        }
        if !metric
            .angles
            .get(id)
            .copied()
            .unwrap_or(f64::NAN)
            .is_finite()
        {
            return Err(circ_err(format!(
                "circular: invariant: non-finite angle `{id}`"
            )));
        }
    }
    for (pid, circle) in &metric.circles {
        if !circle.center.x.is_finite()
            || !circle.center.y.is_finite()
            || !circle.radius.is_finite()
            || circle.radius < -EPS
        {
            return Err(circ_err(format!(
                "circular: invariant: non-finite circle {pid}"
            )));
        }
        let need = demand.radius(*pid);
        if circle.radius + EPS < need {
            return Err(circ_err(format!(
                "circular: invariant: circle {pid} below demand radius"
            )));
        }
    }

    let gap = demand.node_gap(0.0);
    for members in plan.partitions.values() {
        if members.len() < 2 {
            continue;
        }
        for i in 0..members.len() {
            let a = &members[i];
            let b = &members[(i + 1) % members.len()];
            let Some(fa) = metric.frames.get(a) else {
                continue;
            };
            let Some(fb) = metric.frames.get(b) else {
                continue;
            };
            let dx = fa.center().x - fb.center().x;
            let dy = fa.center().y - fb.center().y;
            let dist = (dx * dx + dy * dy).sqrt();
            let need = node_extent(fa.size()) + node_extent(fb.size()) + gap;
            if dist + 1e-4 < need {
                return Err(circ_err(format!(
                    "circular: invariant: adjacent `{a}` / `{b}` closer than demand"
                )));
            }
        }
    }

    for pid in plan.partitions.keys() {
        let kids = plan.children_of(*pid);
        for (i, a) in kids.iter().enumerate() {
            let Some(ca) = metric.circles.get(a) else {
                continue;
            };
            let ra = ca.radius;
            for b in kids.iter().skip(i + 1) {
                let Some(cb) = metric.circles.get(b) else {
                    continue;
                };
                let dx = ca.center.x - cb.center.x;
                let dy = ca.center.y - cb.center.y;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist + 1e-3 < ra + cb.radius {
                    return Err(circ_err(format!(
                        "circular: invariant: sibling partition disks overlap {a} / {b}"
                    )));
                }
            }
        }
    }

    for (eid, role) in &plan.edge_role {
        if matches!(
            role,
            EdgeRole::Intra | EdgeRole::Inter | EdgeRole::Loop | EdgeRole::Parallel
        ) {
            let Some(r) = metric.routes.get(eid) else {
                continue;
            };
            if matches!(r, CircRoute::ExteriorArc { .. }) && *role == EdgeRole::Inter {
                return Err(circ_err(format!(
                    "circular: invariant: exterior arc used on inter edge `{eid}`"
                )));
            }
            let s = r.start();
            let e = r.end();
            if !s.x.is_finite() || !s.y.is_finite() || !e.x.is_finite() || !e.y.is_finite() {
                return Err(circ_err(format!(
                    "circular: invariant: non-finite route `{eid}`"
                )));
            }
        }
    }
    Ok(())
}

pub fn verify_ink(
    edges: &[EdgePlacement],
    metric: &CircMetric,
    defer: bool,
) -> Result<(), LayoutError> {
    if defer {
        for e in edges {
            if !e.path.samples().is_empty() {
                return Err(circ_err(format!(
                    "circular: invariant: deferred edge `{}` has a path",
                    e.id
                )));
            }
        }
        return Ok(());
    }
    for e in edges {
        let pts = e.path.samples();
        if pts.len() < 2 {
            return Err(circ_err(format!(
                "circular: invariant: edge `{}` path too short",
                e.id
            )));
        }
        for p in &pts {
            if !p.x.is_finite() || !p.y.is_finite() {
                return Err(circ_err(format!(
                    "circular: invariant: non-finite path on `{}`",
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
                return Err(circ_err(format!(
                    "circular: invariant: edge `{}` path terminals != route",
                    e.id
                )));
            }
        }
    }
    Ok(())
}
