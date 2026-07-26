//! Ink 相（23 号文 Stage 1.4 原型 → Stage 4 转正）：`(Plan, Metric) → Vec<EdgeLayout>`。
//!
//! 从 Plan 的离散决策（track 序列）+ track 坐标 / 节点 bbox 重建正交折线。
//! Stage 4 生产路径经 [`materialize_edges`] 写入 `LayoutResult.edges`。

use super::channel::{EdgeId, PortSide, Substrate, TrackOrient};
use super::plan::Plan;
use crate::ast::Diagram;
use crate::layout::demand::CORRIDOR_LANE_PITCH;
use crate::layout::geometry::Point;
use crate::layout::kernel::coordinate::main_axis::lane_centers;
use crate::layout::types::{EdgeLayout, EdgeLabelLayout, PathGeometry, Port};
use crate::layout::NodeLayout;
use std::collections::{BTreeMap, HashMap};

/// Ink 坐标上下文：从 LayoutResult + Plan.substrate 构造。
#[derive(Debug, Clone)]
pub struct InkContext {
    /// 节点 id → (x, y, w, h) 像素坐标（左上角 + 尺寸）。
    pub node_rects: BTreeMap<String, (f64, f64, f64, f64)>,
    /// rank-gap k → y 坐标（水平走廊的 y 位置）。
    pub rank_gap_y: Vec<f64>,
    /// order-gap og → x 坐标（垂直走廊的 x 位置）。
    pub order_gap_x: Vec<f64>,
}

impl InkContext {
    /// 从节点坐标 + Plan 的 rank/order 数构造间隙坐标。
    pub fn from_plan_and_rects(
        plan: &Plan,
        node_rects: BTreeMap<String, (f64, f64, f64, f64)>,
    ) -> Self {
        let rank_count = plan.substrate.rank_count;
        let order_count = plan.substrate.order_count;

        let mut rank_top: Vec<f64> = vec![f64::MAX; rank_count];
        let mut rank_bottom: Vec<f64> = vec![f64::MIN; rank_count];
        let mut order_left: Vec<f64> = vec![f64::MAX; order_count];
        let mut order_right: Vec<f64> = vec![f64::MIN; order_count];

        for (node, slot) in &plan.node_slots {
            if let Some(&(x, y, w, h)) = node_rects.get(node) {
                let r = slot.rank.min(rank_count.saturating_sub(1));
                let o = slot.order.min(order_count.saturating_sub(1));
                rank_top[r] = rank_top[r].min(y);
                rank_bottom[r] = rank_bottom[r].max(y + h);
                order_left[o] = order_left[o].min(x);
                order_right[o] = order_right[o].max(x + w);
            }
        }

        let margin = 20.0;
        let mut rank_gap_y = Vec::with_capacity(rank_count + 1);
        for k in 0..=rank_count {
            if k == 0 {
                let top = if rank_count > 0 && rank_top[0] < f64::MAX {
                    rank_top[0]
                } else {
                    0.0
                };
                rank_gap_y.push(top - margin);
            } else if k == rank_count {
                let bottom = if rank_count > 0 && rank_bottom[rank_count - 1] > f64::MIN {
                    rank_bottom[rank_count - 1]
                } else {
                    100.0
                };
                rank_gap_y.push(bottom + margin);
            } else {
                let prev_bottom = rank_bottom[k - 1];
                let next_top = rank_top[k];
                if prev_bottom > f64::MIN && next_top < f64::MAX {
                    rank_gap_y.push((prev_bottom + next_top) / 2.0);
                } else {
                    rank_gap_y.push(k as f64 * 50.0);
                }
            }
        }

        let mut order_gap_x = Vec::with_capacity(order_count + 1);
        for og in 0..=order_count {
            if og == 0 {
                let left = if order_count > 0 && order_left[0] < f64::MAX {
                    order_left[0]
                } else {
                    0.0
                };
                order_gap_x.push(left - margin);
            } else if og == order_count {
                let right = if order_count > 0 && order_right[order_count - 1] > f64::MIN {
                    order_right[order_count - 1]
                } else {
                    100.0
                };
                order_gap_x.push(right + margin);
            } else {
                let prev_right = order_right[og - 1];
                let next_left = order_left[og];
                if prev_right > f64::MIN && next_left < f64::MAX {
                    order_gap_x.push((prev_right + next_left) / 2.0);
                } else {
                    order_gap_x.push(og as f64 * 50.0);
                }
            }
        }

        Self {
            node_rects,
            rank_gap_y,
            order_gap_x,
        }
    }

    /// 用 Metric track 坐标覆盖 Cross 走廊 y（TrackId → y）。
    pub fn apply_track_coords(
        &mut self,
        substrate: &Substrate,
        track_coords: &BTreeMap<u32, f64>,
    ) {
        for t in substrate.tracks() {
            if t.orient != TrackOrient::Cross {
                continue;
            }
            if let Some(&y) = track_coords.get(&t.id.0) {
                if t.line < self.rank_gap_y.len() {
                    self.rank_gap_y[t.line] = y;
                }
            }
        }
    }
}

/// Stage 4：Plan + 坐标 → 与 `diagram.relations` 对齐的 `Vec<EdgeLayout>`。
pub fn materialize_edges(
    diagram: &Diagram,
    plan: &Plan,
    substrate: &Substrate,
    nodes: &HashMap<String, NodeLayout>,
    track_coords: &BTreeMap<u32, f64>,
) -> Vec<EdgeLayout> {
    let node_rects: BTreeMap<String, (f64, f64, f64, f64)> = nodes
        .iter()
        .map(|(id, n)| (id.clone(), (n.x, n.y, n.width, n.height)))
        .collect();
    let mut ctx = InkContext::from_plan_and_rects(plan, node_rects);
    ctx.apply_track_coords(substrate, track_coords);

    let n = diagram.relations.len();
    let mut edges: Vec<EdgeLayout> = Vec::with_capacity(n);
    let mut raw: BTreeMap<EdgeId, Vec<(f64, f64)>> = BTreeMap::new();

    for (eid, rel) in diagram.relations.iter().enumerate() {
        let from = rel.from.as_str();
        let to = rel.to.as_str();
        let label = rel.label.as_ref().map(|s| s.as_str());
        if from == to {
            let pts = stub_self_loop(&ctx, from);
            raw.insert(eid, pts.clone());
            edges.push(edge_from_points(&pts, Port::Right, Port::Right, label));
            continue;
        }

        if let Some(pts) = ink_edge_with_lanes(plan, substrate, &ctx, eid) {
            raw.insert(eid, pts.clone());
            let (fp, tp) = ports_for_edge(plan, eid);
            edges.push(edge_from_points(&pts, fp, tp, label));
        } else {
            let pts = stub_short_jog(&ctx, from, to);
            raw.insert(eid, pts.clone());
            edges.push(edge_from_points(&pts, Port::Bottom, Port::Top, label));
            crate::perf_log!("[atlas/ink] edge {eid} stub (no channel route)");
        }
    }

    apply_bundle_suffixes(&mut edges, &raw, plan);
    edges
}

fn ink_edge_with_lanes(
    plan: &Plan,
    substrate: &Substrate,
    ctx: &InkContext,
    edge: EdgeId,
) -> Option<Vec<(f64, f64)>> {
    let tracks = plan.channels.get(&edge)?;
    if tracks.is_empty() {
        return None;
    }
    let lanes = plan
        .lane_indices
        .get(&edge)
        .cloned()
        .unwrap_or_else(|| vec![0; tracks.len()]);

    let mut points: Vec<(f64, f64)> = Vec::new();

    if let Some(ep) = plan.ports.get(&edge) {
        if let Some(&(x, y, w, h)) = ctx.node_rects.get(&ep.from.node) {
            points.push(port_boundary_point(x, y, w, h, ep.from.side));
        }
    }

    for (i, &tid) in tracks.iter().enumerate() {
        let Some(track) = substrate.track(tid) else {
            continue;
        };
        let lane_i = lanes.get(i).copied().unwrap_or(0);
        let n_lanes = (lane_i + 1).max(1);
        match track.orient {
            TrackOrient::Cross => {
                let base_y = ctx
                    .rank_gap_y
                    .get(track.line)
                    .copied()
                    .unwrap_or(track.line as f64 * 50.0);
                let centers = lane_centers(base_y, n_lanes, CORRIDOR_LANE_PITCH);
                let y = centers
                    .get(lane_i as usize)
                    .copied()
                    .unwrap_or(base_y);
                let x = points.last().map(|p| p.0).unwrap_or(0.0);
                push_point(&mut points, (x, y));
            }
            TrackOrient::Main => {
                let base_x = ctx
                    .order_gap_x
                    .get(track.line)
                    .copied()
                    .unwrap_or(track.line as f64 * 50.0);
                let centers = lane_centers(base_x, n_lanes, CORRIDOR_LANE_PITCH);
                let x = centers
                    .get(lane_i as usize)
                    .copied()
                    .unwrap_or(base_x);
                let y = points.last().map(|p| p.1).unwrap_or(0.0);
                push_point(&mut points, (x, y));
            }
        }
    }

    if let Some(ep) = plan.ports.get(&edge) {
        if let Some(&(x, y, w, h)) = ctx.node_rects.get(&ep.to.node) {
            points.push(port_boundary_point(x, y, w, h, ep.to.side));
        }
    }

    if points.len() >= 2 {
        Some(simplify_collinear(&points))
    } else {
        None
    }
}

fn apply_bundle_suffixes(
    edges: &mut [EdgeLayout],
    raw: &BTreeMap<EdgeId, Vec<(f64, f64)>>,
    plan: &Plan,
) {
    for bundle in &plan.bundles {
        if bundle.edges.len() < 2 || bundle.suffix.is_empty() {
            continue;
        }
        let canon = bundle.edges[0];
        let Some(canon_pts) = raw.get(&canon) else {
            continue;
        };
        let suffix_pts = bundle.suffix.len().saturating_add(1);
        if canon_pts.len() < suffix_pts {
            continue;
        }
        let shared = &canon_pts[canon_pts.len() - suffix_pts..];
        for &eid in &bundle.edges[1..] {
            if eid >= edges.len() {
                continue;
            }
            let Some(pts) = raw.get(&eid) else {
                continue;
            };
            if pts.len() < suffix_pts {
                continue;
            }
            let label = edges[eid]
                .labels
                .first()
                .map(|l| l.text.clone());
            let mut merged = pts[..pts.len() - suffix_pts].to_vec();
            merged.extend_from_slice(shared);
            let (fp, tp) = ports_for_edge(plan, eid);
            edges[eid] = edge_from_points(&merged, fp, tp, label.as_deref());
        }
    }
}

fn ports_for_edge(plan: &Plan, eid: EdgeId) -> (Port, Port) {
    plan.ports
        .get(&eid)
        .map(|ep| (side_to_port(ep.from.side), side_to_port(ep.to.side)))
        .unwrap_or((Port::Bottom, Port::Top))
}

fn side_to_port(side: PortSide) -> Port {
    match side {
        PortSide::MainLow => Port::Top,
        PortSide::MainHigh => Port::Bottom,
        PortSide::CrossLow => Port::Left,
        PortSide::CrossHigh => Port::Right,
    }
}

fn edge_from_points(
    pts: &[(f64, f64)],
    from_port: Port,
    to_port: Port,
    label: Option<&str>,
) -> EdgeLayout {
    let points: Vec<Point> = pts.iter().map(|&(x, y)| Point { x, y }).collect();
    let mut labels = Vec::new();
    if let Some(text) = label {
        if !text.is_empty() && points.len() >= 2 {
            let mid = points.len() / 2;
            labels.push(EdgeLabelLayout::new(text, points[mid]));
        }
    }
    EdgeLayout {
        geometry: PathGeometry::Polyline { points },
        labels,
        from_port,
        to_port,
    }
}

fn stub_self_loop(ctx: &InkContext, node: &str) -> Vec<(f64, f64)> {
    let (x, y, w, h) = ctx
        .node_rects
        .get(node)
        .copied()
        .unwrap_or((0.0, 0.0, 40.0, 40.0));
    let pad = 16.0;
    vec![
        (x + w, y + h * 0.35),
        (x + w + pad, y + h * 0.35),
        (x + w + pad, y - pad),
        (x + w * 0.5, y - pad),
        (x + w * 0.5, y),
    ]
}

fn stub_short_jog(ctx: &InkContext, from: &str, to: &str) -> Vec<(f64, f64)> {
    // 无 channel 时的正交出组 stub：底→水平→顶，端口中心；记 degraded 由调用方日志。
    let stub = 16.0;
    let (fx, fy, fw, fh) = ctx
        .node_rects
        .get(from)
        .copied()
        .unwrap_or((0.0, 0.0, 40.0, 40.0));
    let (tx, ty, tw, _th) = ctx
        .node_rects
        .get(to)
        .copied()
        .unwrap_or((0.0, 100.0, 40.0, 40.0));
    let start = (fx + fw * 0.5, fy + fh);
    let end = (tx + tw * 0.5, ty);
    let out_y = start.1 + stub;
    let in_y = (end.1 - stub).max(out_y + 1.0);
    let mid_y = ((out_y + in_y) * 0.5).max(out_y);
    // 保证正交折线：底边出口 → 下 stub → 水平 → 上 stub → 顶边入口
    vec![
        start,
        (start.0, out_y),
        (start.0, mid_y),
        (end.0, mid_y),
        (end.0, in_y),
        end,
    ]
}

/// 落笔后只读正交审计：统计非轴对齐段（不改几何）。
pub fn audit_orthogonal_segments(edges: &[EdgeLayout]) -> usize {
    let mut bad = 0usize;
    for e in edges {
        let pts = e.path_points();
        for w in pts.windows(2) {
            let dx = (w[1].x - w[0].x).abs();
            let dy = (w[1].y - w[0].y).abs();
            if dx > 0.5 && dy > 0.5 {
                bad += 1;
            }
        }
    }
    bad
}

fn push_point(points: &mut Vec<(f64, f64)>, p: (f64, f64)) {
    if points
        .last()
        .is_none_or(|q| (q.0 - p.0).abs() > 0.01 || (q.1 - p.1).abs() > 0.01)
    {
        points.push(p);
    }
}

fn simplify_collinear(pts: &[(f64, f64)]) -> Vec<(f64, f64)> {
    if pts.len() <= 2 {
        return pts.to_vec();
    }
    let mut out = vec![pts[0]];
    for i in 1..pts.len() - 1 {
        let a = out[out.len() - 1];
        let b = pts[i];
        let c = pts[i + 1];
        let colinear = ((b.0 - a.0).abs() < 0.01 && (c.0 - b.0).abs() < 0.01)
            || ((b.1 - a.1).abs() < 0.01 && (c.1 - b.1).abs() < 0.01);
        if !colinear {
            out.push(b);
        }
    }
    out.push(*pts.last().unwrap());
    out
}

/// 从 Plan + 坐标上下文重建一条边的正交折线（旁路对拍用）。
pub fn ink_edge(
    plan: &Plan,
    substrate: &Substrate,
    ctx: &InkContext,
    edge: EdgeId,
) -> Option<Vec<(f64, f64)>> {
    ink_edge_with_lanes(plan, substrate, ctx, edge)
}

/// 全图 Ink：返回每条边的重建折线。
pub fn ink_all(
    plan: &Plan,
    substrate: &Substrate,
    ctx: &InkContext,
) -> BTreeMap<EdgeId, Vec<(f64, f64)>> {
    let mut result = BTreeMap::new();
    for &edge in plan.channels.keys() {
        if let Some(polyline) = ink_edge(plan, substrate, ctx, edge) {
            result.insert(edge, polyline);
        }
    }
    result
}

/// Ink 对拍报告。
#[derive(Debug, Clone, Default)]
pub struct InkDiffReport {
    pub total_edges: usize,
    pub matched: usize,
    pub topology_mismatch: Vec<EdgeId>,
    pub max_pointwise_deviation: f64,
}

/// 比较 Ink 重建折线与旧管线几何。
pub fn compare_ink_vs_legacy(
    ink: &BTreeMap<EdgeId, Vec<(f64, f64)>>,
    legacy_edges: &[EdgeLayout],
) -> InkDiffReport {
    let mut report = InkDiffReport {
        total_edges: ink.len(),
        ..Default::default()
    };

    for (&edge_id, ink_pts) in ink {
        if edge_id >= legacy_edges.len() {
            report.topology_mismatch.push(edge_id);
            continue;
        }
        let legacy_pts = geometry_to_points(&legacy_edges[edge_id].geometry);
        if topology_match(ink_pts, &legacy_pts) {
            report.matched += 1;
            if ink_pts.len() == legacy_pts.len() {
                for (ip, lp) in ink_pts.iter().zip(legacy_pts.iter()) {
                    let dx = ip.0 - lp.0;
                    let dy = ip.1 - lp.1;
                    let dev = (dx * dx + dy * dy).sqrt();
                    report.max_pointwise_deviation = report.max_pointwise_deviation.max(dev);
                }
            }
        } else {
            report.topology_mismatch.push(edge_id);
        }
    }

    report
}

fn port_boundary_point(x: f64, y: f64, w: f64, h: f64, side: PortSide) -> (f64, f64) {
    match side {
        PortSide::MainLow => (x + w / 2.0, y),
        PortSide::MainHigh => (x + w / 2.0, y + h),
        PortSide::CrossLow => (x, y + h / 2.0),
        PortSide::CrossHigh => (x + w, y + h / 2.0),
    }
}

fn geometry_to_points(geom: &PathGeometry) -> Vec<(f64, f64)> {
    match geom {
        PathGeometry::Straight { start, end } => {
            vec![(start.x, start.y), (end.x, end.y)]
        }
        PathGeometry::Bezier { start, end, .. } => {
            vec![(start.x, start.y), (end.x, end.y)]
        }
        PathGeometry::Polyline { points } => points.iter().map(|p| (p.x, p.y)).collect(),
    }
}

fn topology_match(ink: &[(f64, f64)], legacy: &[(f64, f64)]) -> bool {
    let a = simplify_collinear(ink);
    let b = simplify_collinear(legacy);
    if a.len() != b.len() || a.len() < 2 {
        return a.len() == b.len();
    }
    let dirs = |pts: &[(f64, f64)]| -> Vec<u8> {
        pts.windows(2)
            .map(|w| {
                let dx = w[1].0 - w[0].0;
                let dy = w[1].1 - w[0].1;
                if dx.abs() >= dy.abs() {
                    if dx >= 0.0 {
                        0
                    } else {
                        1
                    }
                } else if dy >= 0.0 {
                    2
                } else {
                    3
                }
            })
            .collect()
    };
    dirs(&a) == dirs(&b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_boundary_points() {
        let (x, y) = port_boundary_point(10.0, 20.0, 100.0, 60.0, PortSide::MainLow);
        assert_eq!((x, y), (60.0, 20.0));
        let (x, y) = port_boundary_point(10.0, 20.0, 100.0, 60.0, PortSide::MainHigh);
        assert_eq!((x, y), (60.0, 80.0));
        let (x, y) = port_boundary_point(10.0, 20.0, 100.0, 60.0, PortSide::CrossLow);
        assert_eq!((x, y), (10.0, 50.0));
        let (x, y) = port_boundary_point(10.0, 20.0, 100.0, 60.0, PortSide::CrossHigh);
        assert_eq!((x, y), (110.0, 50.0));
    }
}
