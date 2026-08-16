//! Metric: CYCLE coordinates, balloon backbone, component pack, CircRoute.

use std::collections::{BTreeMap, BTreeSet};

use plotgram_engine_api::LayoutError;
use plotgram_model::geometry::{Point, Rect, Size};
use plotgram_model::graph::Graph;
use plotgram_model::sizes::NodeSizes;

use super::circ_err;
use super::demand::CircDemandBoard;
use super::geom::{
    aabb, cycle_geom, fit_ring_radius, frame_at, loop_short_arc, node_extent, offset_segment,
    polar_point, rect_boundary_toward, rotate_point, BALLOON_RESERVED, THETA0,
};
use super::params::{CircularParams, RoutingPolicy};
use super::plan::{CircMetric, CircPlan, CircRoute, CircleGeom, EdgeRole, PartitionId};

pub fn assign(
    graph: &Graph,
    plan: &CircPlan,
    params: &CircularParams,
    sizes: &NodeSizes,
    demand: &CircDemandBoard,
) -> Result<CircMetric, LayoutError> {
    let mut size_of: BTreeMap<String, Size> = BTreeMap::new();
    for id in &plan.nodes {
        let s = sizes
            .get(id)
            .ok_or_else(|| plotgram_model::MissingNodeSize {
                node_id: id.clone(),
            })?;
        size_of.insert(id.clone(), demand.node_size(id, s));
    }

    let gap = demand.node_gap(params.node_gap);
    let mut metric = CircMetric {
        frames: BTreeMap::new(),
        circles: BTreeMap::new(),
        angles: BTreeMap::new(),
        routes: BTreeMap::new(),
    };

    for (pid, members) in &plan.partitions {
        place_cycle(
            *pid,
            members,
            &size_of,
            demand.radius(*pid),
            gap,
            params,
            &mut metric,
        )?;
    }

    for comp in &plan.components {
        place_backbone(comp.root_partition, None, plan, &size_of, gap, &mut metric)?;
    }

    metric.routes = write_routes(graph, plan, params, demand, &metric);
    pack_components(graph, plan, params, &mut metric);
    Ok(metric)
}

fn place_cycle(
    pid: PartitionId,
    members: &[String],
    size_of: &BTreeMap<String, Size>,
    demand_r: f64,
    gap: f64,
    params: &CircularParams,
    metric: &mut CircMetric,
) -> Result<(), LayoutError> {
    let part_sizes: Vec<Size> = members
        .iter()
        .map(|id| size_of.get(id).copied().unwrap_or(Size::new(0.0, 0.0)))
        .collect();
    let geom = cycle_geom(&part_sizes, gap, params.min_radius, params.rotation);
    let radius = geom.radius.max(demand_r);
    if !radius.is_finite() {
        return Err(circ_err(format!(
            "circular: invariant: non-finite radius for partition {pid}"
        )));
    }
    let origin = Point { x: 0.0, y: 0.0 };
    metric.circles.insert(
        pid,
        CircleGeom {
            center: origin,
            radius,
        },
    );
    for (id, theta) in members.iter().zip(geom.angles.iter()) {
        if !theta.is_finite() {
            return Err(circ_err(format!(
                "circular: invariant: non-finite angle for `{id}`"
            )));
        }
        let c = polar_point(origin, radius, *theta);
        let sz = size_of.get(id).copied().unwrap_or(Size::new(0.0, 0.0));
        metric.frames.insert(id.clone(), frame_at(sz, c.x, c.y));
        metric.angles.insert(id.clone(), *theta);
    }
    Ok(())
}

fn place_backbone(
    pid: PartitionId,
    parent: Option<PartitionId>,
    plan: &CircPlan,
    size_of: &BTreeMap<String, Size>,
    gap: f64,
    metric: &mut CircMetric,
) -> Result<(), LayoutError> {
    let children: Vec<PartitionId> = plan.children_of(pid).to_vec();
    for &c in &children {
        place_backbone(c, Some(pid), plan, size_of, gap, metric)?;
    }
    if children.is_empty() {
        return Ok(());
    }

    let origin = metric
        .circles
        .get(&pid)
        .map(|c| c.center)
        .unwrap_or(Point { x: 0.0, y: 0.0 });
    let r_parent = partition_enclosing(pid, plan, size_of, metric);
    let reserved = if parent.is_some() {
        BALLOON_RESERVED
    } else {
        0.0
    };
    let radii: Vec<f64> = children
        .iter()
        .map(|&c| subtree_enclosing(c, plan, size_of, metric) + gap)
        .collect();
    let r_max = radii.iter().copied().fold(0.0, f64::max);
    let lo = r_parent + gap + r_max;
    let ring = fit_ring_radius(&radii, reserved, lo);
    if !ring.is_finite() {
        return Err(circ_err(format!(
            "circular: invariant: non-finite balloon radius for partition {pid}"
        )));
    }

    let angles: Vec<f64> = radii
        .iter()
        .map(|&r| 2.0 * (r / ring).clamp(0.0, 0.999).asin())
        .collect();
    let used: f64 = angles.iter().sum();
    let span = (super::geom::TAU - reserved).max(used);
    let mut cursor = THETA0 + reserved / 2.0 + (span - used) / 2.0;

    for (&child, ang) in children.iter().zip(angles) {
        let th = cursor + ang / 2.0;
        cursor += ang;
        let target = polar_point(origin, ring, th);
        let child_center = metric
            .circles
            .get(&child)
            .map(|c| c.center)
            .unwrap_or(Point { x: 0.0, y: 0.0 });
        let dx = target.x - child_center.x;
        let dy = target.y - child_center.y;
        translate_subtree(child, plan, metric, dx, dy);

        let toward_parent = th + std::f64::consts::PI;
        let pivot = metric
            .circles
            .get(&child)
            .map(|c| c.center)
            .unwrap_or(target);
        let mut rot = toward_parent - THETA0;
        if let Some(cut) = plan.cut_of.get(&(pid, child)) {
            if plan.members(child).iter().any(|m| m == cut) {
                if let Some(&cut_th) = metric.angles.get(cut) {
                    rot = toward_parent - cut_th;
                }
            }
        }
        rotate_subtree(child, plan, metric, pivot, rot);
    }
    Ok(())
}

fn translate_subtree(pid: PartitionId, plan: &CircPlan, metric: &mut CircMetric, dx: f64, dy: f64) {
    let mut stack = vec![pid];
    while let Some(cur) = stack.pop() {
        metric.translate_partition(cur, plan.members(cur), dx, dy);
        stack.extend(plan.children_of(cur).iter().copied());
    }
}

fn rotate_subtree(
    pid: PartitionId,
    plan: &CircPlan,
    metric: &mut CircMetric,
    pivot: Point,
    angle: f64,
) {
    if angle.abs() < 1e-12 {
        return;
    }
    let mut stack = vec![pid];
    while let Some(cur) = stack.pop() {
        if let Some(c) = metric.circles.get_mut(&cur) {
            c.center = rotate_point(c.center, pivot, angle);
        }
        for id in plan.members(cur) {
            if let Some(f) = metric.frames.get_mut(id) {
                let c = rotate_point(f.center(), pivot, angle);
                f.x = c.x - f.width / 2.0;
                f.y = c.y - f.height / 2.0;
            }
            if let Some(th) = metric.angles.get_mut(id) {
                *th += angle;
            }
        }
        stack.extend(plan.children_of(cur).iter().copied());
    }
}

fn partition_enclosing(
    pid: PartitionId,
    plan: &CircPlan,
    size_of: &BTreeMap<String, Size>,
    metric: &CircMetric,
) -> f64 {
    let origin = metric
        .circles
        .get(&pid)
        .map(|c| c.center)
        .unwrap_or(Point { x: 0.0, y: 0.0 });
    let mut r = metric.circles.get(&pid).map(|c| c.radius).unwrap_or(0.0);
    for id in plan.members(pid) {
        let Some(f) = metric.frames.get(id) else {
            continue;
        };
        let sz = size_of.get(id).copied().unwrap_or(f.size());
        let c = f.center();
        let d = ((c.x - origin.x).powi(2) + (c.y - origin.y).powi(2)).sqrt() + node_extent(sz);
        r = r.max(d);
    }
    r
}

fn subtree_enclosing(
    pid: PartitionId,
    plan: &CircPlan,
    size_of: &BTreeMap<String, Size>,
    metric: &CircMetric,
) -> f64 {
    let origin = metric
        .circles
        .get(&pid)
        .map(|c| c.center)
        .unwrap_or(Point { x: 0.0, y: 0.0 });
    let mut r = 0.0_f64;
    let mut stack = vec![pid];
    while let Some(cur) = stack.pop() {
        for id in plan.members(cur) {
            let Some(f) = metric.frames.get(id) else {
                continue;
            };
            let sz = size_of.get(id).copied().unwrap_or(f.size());
            let c = f.center();
            let d = ((c.x - origin.x).powi(2) + (c.y - origin.y).powi(2)).sqrt() + node_extent(sz);
            r = r.max(d);
        }
        stack.extend(plan.children_of(cur).iter().copied());
    }
    r.max(partition_enclosing(pid, plan, size_of, metric))
}

fn pack_components(
    graph: &Graph,
    plan: &CircPlan,
    params: &CircularParams,
    metric: &mut CircMetric,
) {
    let mut cursor = 0.0;
    for comp in &plan.components {
        let mut frames: Vec<_> = comp
            .nodes
            .iter()
            .filter_map(|id| metric.frames.get(id).copied())
            .collect();
        let in_comp: BTreeSet<&str> = comp.nodes.iter().map(|id| id.as_str()).collect();
        for edge in graph.edges_in_declaration_order() {
            if !in_comp.contains(edge.source.as_str()) && !in_comp.contains(edge.target.as_str()) {
                continue;
            }
            if let Some(route) = metric.routes.get(&edge.id) {
                for p in route.points() {
                    frames.push(Rect::new(p.x, p.y, 0.0, 0.0));
                }
            }
        }
        let Some(box_) = aabb(frames.into_iter()) else {
            continue;
        };
        let dx = cursor - box_.x;
        let dy = -box_.y;
        if dx.abs() > 1e-12 || dy.abs() > 1e-12 {
            for pid in &comp.partitions {
                metric.translate_partition(*pid, plan.members(*pid), dx, dy);
            }
            for edge in graph.edges_in_declaration_order() {
                if !in_comp.contains(edge.source.as_str())
                    && !in_comp.contains(edge.target.as_str())
                {
                    continue;
                }
                if let Some(route) = metric.routes.get_mut(&edge.id) {
                    route.translate(dx, dy);
                }
            }
        }
        cursor = box_.right() + dx + params.component_gap;
    }
}

fn write_routes(
    graph: &Graph,
    plan: &CircPlan,
    params: &CircularParams,
    demand: &CircDemandBoard,
    metric: &CircMetric,
) -> BTreeMap<String, CircRoute> {
    let mut pair_ids: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for edge in graph.edges_in_declaration_order() {
        if edge.source == edge.target {
            continue;
        }
        let key = if edge.source <= edge.target {
            (edge.source.clone(), edge.target.clone())
        } else {
            (edge.target.clone(), edge.source.clone())
        };
        pair_ids.entry(key).or_default().push(edge.id.clone());
    }
    let mut pair_index: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for ids in pair_ids.values() {
        let n = ids.len();
        for (i, id) in ids.iter().enumerate() {
            pair_index.insert(id.clone(), (i, n));
        }
    }

    let mut routes = BTreeMap::new();
    let mut loop_count: BTreeMap<String, u32> = BTreeMap::new();
    let sep = demand.exterior_sep();
    for edge in graph.edges_in_declaration_order() {
        let Some(role) = plan.edge_role.get(&edge.id) else {
            continue;
        };
        let Some(src) = metric.frames.get(&edge.source) else {
            continue;
        };
        let Some(tgt) = metric.frames.get(&edge.target) else {
            continue;
        };
        let mut route = match role {
            EdgeRole::Loop => {
                let idx = loop_count.entry(edge.source.clone()).or_insert(0);
                let extra = *idx as f64 * 10.0;
                *idx += 1;
                let origin = plan
                    .partition_of
                    .get(&edge.source)
                    .and_then(|p| metric.circles.get(p))
                    .map(|c| c.center)
                    .unwrap_or(src.center());
                CircRoute::Loop {
                    points: loop_short_arc(src, origin, extra),
                }
            }
            EdgeRole::Intra | EdgeRole::Parallel => {
                let same_part = plan.partition_of.get(&edge.source)
                    == plan.partition_of.get(&edge.target);
                let exterior = params.routing_policy == RoutingPolicy::Exterior
                    && same_part
                    && *role == EdgeRole::Intra
                    && !is_circle_adjacent(plan, &edge.source, &edge.target);
                if exterior {
                    exterior_arc(
                        src,
                        tgt,
                        &edge.source,
                        &edge.target,
                        plan,
                        metric,
                        sep,
                    )
                } else {
                    CircRoute::Chord {
                        start: rect_boundary_toward(src, tgt.center()),
                        end: rect_boundary_toward(tgt, src.center()),
                    }
                }
            }
            EdgeRole::Inter => {
                let src_c = plan
                    .partition_of
                    .get(&edge.source)
                    .and_then(|p| metric.circles.get(p))
                    .map(|c| c.center)
                    .unwrap_or_else(|| tgt.center());
                let tgt_c = plan
                    .partition_of
                    .get(&edge.target)
                    .and_then(|p| metric.circles.get(p))
                    .map(|c| c.center)
                    .unwrap_or_else(|| src.center());
                CircRoute::Spoke {
                    start: rect_boundary_toward(src, tgt_c),
                    end: rect_boundary_toward(tgt, src_c),
                }
            }
        };
        if matches!(role, EdgeRole::Parallel | EdgeRole::Intra | EdgeRole::Inter) {
            if let Some(&(i, n)) = pair_index.get(&edge.id) {
                if n > 1 {
                    let amount = (i as f64 - (n as f64 - 1.0) / 2.0) * 6.0;
                    match &mut route {
                        CircRoute::Chord { start, end }
                        | CircRoute::Spoke { start, end } => {
                            let (s, e) = offset_segment(*start, *end, amount);
                            *start = s;
                            *end = e;
                        }
                        CircRoute::ExteriorArc { radius, .. } => {
                            *radius += amount.abs();
                        }
                        CircRoute::Loop { .. } => {}
                    }
                }
            }
        }
        routes.insert(edge.id.clone(), route);
    }
    routes
}

fn is_circle_adjacent(plan: &CircPlan, a: &str, b: &str) -> bool {
    let Some(pid) = plan.partition_of.get(a) else {
        return false;
    };
    let members = plan.members(*pid);
    let n = members.len();
    if n <= 2 {
        return true;
    }
    let Some(i) = members.iter().position(|m| m == a) else {
        return false;
    };
    let Some(j) = members.iter().position(|m| m == b) else {
        return false;
    };
    (i + 1) % n == j || (j + 1) % n == i
}

fn exterior_arc(
    src: &plotgram_model::geometry::Rect,
    tgt: &plotgram_model::geometry::Rect,
    src_id: &str,
    tgt_id: &str,
    plan: &CircPlan,
    metric: &CircMetric,
    sep: f64,
) -> CircRoute {
    let pid = plan.partition_of.get(src_id).copied();
    let circle = pid.and_then(|p| metric.circles.get(&p));
    let origin = circle
        .map(|c| c.center)
        .unwrap_or_else(|| src.center());
    let r0 = circle.map(|c| c.radius).unwrap_or(0.0);
    let t0 = metric.angles.get(src_id).copied().unwrap_or(0.0);
    let t_tgt = metric.angles.get(tgt_id).copied().unwrap_or(0.0);
    let span = super::geom::clockwise_span(t0, t_tgt);
    let t1 = t0 + span;
    let radius = r0 + sep.max(16.0);
    let start = rect_boundary_toward(src, polar_point(origin, radius, t0));
    let end = rect_boundary_toward(tgt, polar_point(origin, radius, t_tgt));
    CircRoute::ExteriorArc {
        origin,
        radius,
        t0,
        t1,
        start,
        end,
    }
}
