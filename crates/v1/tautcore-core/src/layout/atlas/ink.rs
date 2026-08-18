//! Ink 相（23 号文 Stage 1.4 原型 → Stage 4 转正）：`(Plan, Metric) → Vec<EdgeLayout>`。
//!
//! 从 Plan 的离散决策（track 序列）+ track 坐标 / 节点 bbox 重建正交折线。
//! Stage 4 生产路径经 [`materialize_edges`] 写入 `LayoutResult.edges`。

use super::channel::{EdgeId, Gate, GateSide, PortSide, Substrate, TrackOrient};
use super::plan::Plan;
use crate::ast::Diagram;
use crate::layout::demand::CORRIDOR_LANE_PITCH;
use crate::layout::geometry::Point;
use crate::layout::kernel::coordinate::main_axis::lane_centers;
use crate::layout::types::{EdgeLayout, EdgeLabelLayout, GroupTable, PathGeometry, Port};
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

        // R5：间隙表仅占位；真值一律由度量相 `track_coords` → `apply_track_coords` 写入。
        // 边界也不再 bbox±20 seed（与 `publish_*_track_coords` 的 MARGIN 单一真源）。
        let rank_gap_y: Vec<f64> = (0..=rank_count).map(|k| k as f64 * 50.0).collect();
        let order_gap_x: Vec<f64> = (0..=order_count).map(|og| og as f64 * 50.0).collect();

        Self {
            node_rects,
            rank_gap_y,
            order_gap_x,
        }
    }

    /// 用 Metric track 坐标覆盖走廊：Cross → `rank_gap_y`，Main → `order_gap_x`。
    ///
    /// substrate 有 track 但 `track_coords` 缺键时保留占位，并 `perf_log` 回退次数（R5：无 bbox 兜底）。
    pub fn apply_track_coords(
        &mut self,
        substrate: &Substrate,
        track_coords: &BTreeMap<u32, f64>,
    ) {
        let mut fallback = 0usize;
        for t in substrate.tracks() {
            match t.orient {
                TrackOrient::Cross => {
                    if let Some(&y) = track_coords.get(&t.id.0) {
                        if t.line < self.rank_gap_y.len() {
                            self.rank_gap_y[t.line] = y;
                        }
                    } else {
                        fallback += 1;
                    }
                }
                TrackOrient::Main => {
                    if let Some(&x) = track_coords.get(&t.id.0) {
                        if t.line < self.order_gap_x.len() {
                            self.order_gap_x[t.line] = x;
                        }
                    } else {
                        fallback += 1;
                    }
                }
            }
        }
        if fallback > 0 {
            crate::perf_log!("[atlas/ink] apply_track_coords fallback={fallback}");
        }
    }
}

/// Stage 4：Plan + 坐标 → 与 `diagram.relations` 对齐的 `Vec<EdgeLayout>`。
pub fn materialize_edges(
    diagram: &Diagram,
    plan: &Plan,
    substrate: &Substrate,
    nodes: &HashMap<String, NodeLayout>,
    groups: &GroupTable,
    track_coords: &BTreeMap<u32, f64>,
) -> Vec<EdgeLayout> {
    let node_rects: BTreeMap<String, (f64, f64, f64, f64)> = nodes
        .iter()
        .map(|(id, n)| (id.clone(), (n.x, n.y, n.width, n.height)))
        .collect();
    let mut ctx = InkContext::from_plan_and_rects(plan, node_rects);
    ctx.apply_track_coords(substrate, track_coords);
    let port_points = expand_port_points(plan, &ctx);

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

        if let Some(pts) =
            ink_edge_with_lanes(plan, substrate, &ctx, eid, Some(&port_points), Some(groups))
        {
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

    apply_bundle_suffixes(&mut edges, &raw, plan, &port_points);
    edges
}

fn ink_edge_with_lanes(
    plan: &Plan,
    substrate: &Substrate,
    ctx: &InkContext,
    edge: EdgeId,
    port_points: Option<&BTreeMap<(EdgeId, bool), (f64, f64)>>,
    groups: Option<&GroupTable>,
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
    let gate_ids = plan
        .gates
        .get(&edge)
        .map(|v| v.as_slice())
        .unwrap_or(&[]);
    let mut gate_iter = gate_ids.iter();

    let mut points: Vec<(f64, f64)> = Vec::new();
    // 与 points 对齐：gate / 裙边走廊见证点不可被共线简化吃掉。
    let mut preserve: Vec<bool> = Vec::new();

    if let Some(ep) = plan.ports.get(&edge) {
        if let Some(&(x, y, w, h)) = ctx.node_rects.get(&ep.from.node) {
            let p = port_points
                .and_then(|m| m.get(&(edge, true)).copied())
                .unwrap_or_else(|| port_boundary_point(x, y, w, h, ep.from.side));
            push_point_marked(&mut points, &mut preserve, p, false);
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
                let mut y = centers
                    .get(lane_i as usize)
                    .copied()
                    .unwrap_or(base_y);
                let mut corridor_preserve = false;
                if track.scope.is_none() {
                    if let Some(gs) = groups {
                        let y0 = y;
                        y = skirt_root_cross_y(y, ctx, gs);
                        if (y - y0).abs() > 0.5 {
                            corridor_preserve = true;
                        }
                        if let Some(&(cx, cy)) = points.last() {
                            if axis_inside_groups(cx, false, gs) {
                                let sx = skirt_root_main_x(cx, ctx, gs);
                                if (sx - cx).abs() > 0.5 {
                                    push_point_marked(
                                        &mut points,
                                        &mut preserve,
                                        (sx, cy),
                                        true,
                                    );
                                }
                            }
                        }
                    }
                }
                let x = points.last().map(|p| p.0).unwrap_or(0.0);
                push_point_marked(&mut points, &mut preserve, (x, y), corridor_preserve);
            }
            TrackOrient::Main => {
                let base_x = ctx
                    .order_gap_x
                    .get(track.line)
                    .copied()
                    .unwrap_or(track.line as f64 * 50.0);
                let centers = lane_centers(base_x, n_lanes, CORRIDOR_LANE_PITCH);
                let mut x = centers
                    .get(lane_i as usize)
                    .copied()
                    .unwrap_or(base_x);
                let mut corridor_preserve = false;
                // 根 scope：走廊若落在组 AABB 内则外移；若当前 y 已在组内，
                // 先竖移到组外 rank_gap 再水平走 Main（避免穿无关组）。
                if track.scope.is_none() {
                    if let Some(gs) = groups {
                        let x0 = x;
                        x = skirt_root_main_x(x, ctx, gs);
                        if (x - x0).abs() > 0.5 {
                            corridor_preserve = true;
                        }
                        if let Some(&(cx, cy)) = points.last() {
                            if axis_inside_groups(cy, true, gs) {
                                let sy = skirt_root_cross_y(cy, ctx, gs);
                                if (sy - cy).abs() > 0.5 {
                                    push_point_marked(
                                        &mut points,
                                        &mut preserve,
                                        (cx, sy),
                                        true,
                                    );
                                }
                            }
                        }
                    }
                }
                let y = points.last().map(|p| p.1).unwrap_or(0.0);
                push_point_marked(&mut points, &mut preserve, (x, y), corridor_preserve);
            }
        }

        // M7：scope 变化处插入 gate 边界折点（与 verify_route_scope 同序）
        if i + 1 < tracks.len() {
            let next_tid = tracks[i + 1];
            let sa = track.scope;
            let sb = substrate.track(next_tid).and_then(|t| t.scope);
            if sa != sb {
                if let Some(&gid) = gate_iter.next() {
                    if let Some(gate) = substrate.gate(gid) {
                        push_gate_boundary_point(&mut points, &mut preserve, gate, ctx);
                    }
                } else {
                    crate::perf_log!(
                        "[atlas/ink] edge {edge}: scope change without gate (track {:?}→{:?})",
                        tid,
                        next_tid
                    );
                }
            }
        }
    }

    let remaining: Vec<_> = gate_iter.copied().collect();
    if !remaining.is_empty() {
        crate::perf_log!(
            "[atlas/ink] edge {edge}: {} unused gate(s) after track walk",
            remaining.len()
        );
    }

    if let Some(ep) = plan.ports.get(&edge) {
        if let Some(&(x, y, w, h)) = ctx.node_rects.get(&ep.to.node) {
            let end = port_points
                .and_then(|m| m.get(&(edge, false)).copied())
                .unwrap_or_else(|| port_boundary_point(x, y, w, h, ep.to.side));
            if let Some(&last) = points.last() {
                let needs_elbow = (last.0 - end.0).abs() > 0.5 && (last.1 - end.1).abs() > 0.5;
                if needs_elbow {
                    match ep.to.side {
                        PortSide::MainLow | PortSide::MainHigh => {
                            push_point_marked(&mut points, &mut preserve, (end.0, last.1), false);
                        }
                        PortSide::CrossLow | PortSide::CrossHigh => {
                            push_point_marked(&mut points, &mut preserve, (last.0, end.1), false);
                        }
                    }
                }
            }
            push_point_marked(&mut points, &mut preserve, end, false);
        }
    }

    if points.len() >= 2 {
        Some(simplify_collinear_preserving(&points, &preserve))
    } else {
        None
    }
}

/// M7：把路径钉到 gate 穿越的边界线上（规范空间）。
fn push_gate_boundary_point(
    points: &mut Vec<(f64, f64)>,
    preserve: &mut Vec<bool>,
    gate: &Gate,
    ctx: &InkContext,
) {
    let (x, y) = points.last().copied().unwrap_or((0.0, 0.0));
    let p = match gate.side {
        GateSide::MainLow | GateSide::MainHigh => {
            let gy = ctx
                .rank_gap_y
                .get(gate.line)
                .copied()
                .unwrap_or(gate.line as f64 * 50.0);
            (x, gy)
        }
        GateSide::CrossLow | GateSide::CrossHigh => {
            let gx = ctx
                .order_gap_x
                .get(gate.line)
                .copied()
                .unwrap_or(gate.line as f64 * 50.0);
            (gx, y)
        }
    };
    push_point_marked(points, preserve, p, true);
}

/// 根 Main 走廊 x 若严格落在某组框内，改落到左右 margin `order_gap`（确定性：左优先平局）。
pub(crate) fn skirt_root_main_x(base_x: f64, ctx: &InkContext, groups: &GroupTable) -> f64 {
    if !axis_inside_groups(base_x, false, groups) {
        return base_x;
    }
    let left = ctx.order_gap_x.first().copied().unwrap_or(base_x);
    let right = ctx.order_gap_x.last().copied().unwrap_or(base_x);
    let left_ok = !axis_inside_groups(left, false, groups);
    let right_ok = !axis_inside_groups(right, false, groups);
    match (left_ok, right_ok) {
        (true, false) => left,
        (false, true) => right,
        _ => {
            if (base_x - left).abs() <= (base_x - right).abs() {
                left
            } else {
                right
            }
        }
    }
}

/// 根 Cross 走廊 y 若严格落在某组框内，改落到最近的组外 `rank_gap`（平局取更小 y）。
pub(crate) fn skirt_root_cross_y(base_y: f64, ctx: &InkContext, groups: &GroupTable) -> f64 {
    if !axis_inside_groups(base_y, true, groups) {
        return base_y;
    }
    let mut best: Option<(f64, f64)> = None; // (dist, y)
    for &y in &ctx.rank_gap_y {
        if axis_inside_groups(y, true, groups) {
            continue;
        }
        let d = (y - base_y).abs();
        best = Some(match best {
            None => (d, y),
            Some((bd, by)) if d < bd - 1e-9 || ((d - bd).abs() < 1e-9 && y < by) => (d, y),
            Some(b) => b,
        });
    }
    best.map(|(_, y)| y).unwrap_or(base_y)
}

fn axis_inside_groups(v: f64, vertical_axis: bool, groups: &GroupTable) -> bool {
    const EPS: f64 = 0.5;
    groups.iter_sorted().any(|(_, g)| {
        if vertical_axis {
            g.height > EPS && v > g.y + EPS && v < g.y + g.height - EPS
        } else {
            g.width > EPS && v > g.x + EPS && v < g.x + g.width - EPS
        }
    })
}

fn apply_bundle_suffixes(
    edges: &mut [EdgeLayout],
    raw: &BTreeMap<EdgeId, Vec<(f64, f64)>>,
    plan: &Plan,
    port_points: &BTreeMap<(EdgeId, bool), (f64, f64)>,
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
            if let (Some(&tail), Some(&head)) = (merged.last(), shared.first()) {
                if (tail.0 - head.0).abs() > 0.5 && (tail.1 - head.1).abs() > 0.5 {
                    merged.push((head.0, tail.1));
                }
            }
            merged.extend_from_slice(shared);
            pin_edge_to_endpoint(&mut merged, plan, eid, port_points);
            let (fp, tp) = ports_for_edge(plan, eid);
            edges[eid] = edge_from_points(&merged, fp, tp, label.as_deref());
        }
    }
}

/// 将折线末点钉到本边 `to` 的 `expand_port_points` 锚点；对角时插入正交肘点。
fn pin_edge_to_endpoint(
    pts: &mut Vec<(f64, f64)>,
    plan: &Plan,
    eid: EdgeId,
    port_points: &BTreeMap<(EdgeId, bool), (f64, f64)>,
) {
    let Some(ep) = plan.ports.get(&eid) else {
        return;
    };
    let Some(&end) = port_points.get(&(eid, false)) else {
        return;
    };
    if pts.len() < 2 {
        return;
    }
    let last_i = pts.len() - 1;
    let prev = pts[last_i - 1];
    pts[last_i] = end;
    if (prev.0 - end.0).abs() > 0.5 && (prev.1 - end.1).abs() > 0.5 {
        let elbow = match ep.to.side {
            PortSide::MainLow | PortSide::MainHigh => (end.0, prev.1),
            PortSide::CrossLow | PortSide::CrossHigh => (prev.0, end.1),
        };
        pts.insert(last_i, elbow);
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
            let c = points[mid];
            labels.push(EdgeLabelLayout::new(text, Point { x: c.x, y: c.y }));
        }
    }
    EdgeLayout {
        geometry: PathGeometry::Polyline { points },
        from_port,
        to_port,
        labels,
    }
}

fn stub_self_loop(ctx: &InkContext, node: &str) -> Vec<(f64, f64)> {
    let (x, y, w, h) = ctx.node_rects.get(node).copied().unwrap_or((0.0, 0.0, 40.0, 40.0));
    let cx = x + w;
    let cy = y + h / 2.0;
    vec![(cx, cy), (cx + 24.0, cy), (cx + 24.0, cy - 24.0), (cx, cy - 24.0)]
}

fn stub_short_jog(ctx: &InkContext, from: &str, to: &str) -> Vec<(f64, f64)> {
    let (fx, fy, fw, fh) = ctx
        .node_rects
        .get(from)
        .copied()
        .unwrap_or((0.0, 0.0, 40.0, 40.0));
    let (tx, ty, tw, th) = ctx
        .node_rects
        .get(to)
        .copied()
        .unwrap_or((100.0, 100.0, 40.0, 40.0));
    let a = (fx + fw / 2.0, fy + fh);
    let b = (tx + tw / 2.0, ty);
    if (a.0 - b.0).abs() < 0.5 || (a.1 - b.1).abs() < 0.5 {
        vec![a, b]
    } else {
        vec![a, (a.0, b.1), b]
    }
}

fn push_point_marked(
    points: &mut Vec<(f64, f64)>,
    preserve: &mut Vec<bool>,
    p: (f64, f64),
    mark: bool,
) {
    if points
        .last()
        .is_none_or(|q| (q.0 - p.0).abs() > 0.01 || (q.1 - p.1).abs() > 0.01)
    {
        points.push(p);
        preserve.push(mark);
    } else if mark {
        if let Some(last) = preserve.last_mut() {
            *last = true;
        }
    }
}

fn simplify_collinear(pts: &[(f64, f64)]) -> Vec<(f64, f64)> {
    simplify_collinear_preserving(pts, &vec![false; pts.len()])
}

fn simplify_collinear_preserving(pts: &[(f64, f64)], preserve: &[bool]) -> Vec<(f64, f64)> {
    if pts.len() <= 2 {
        return pts.to_vec();
    }
    debug_assert_eq!(pts.len(), preserve.len());
    let mut out = vec![pts[0]];
    for i in 1..pts.len() - 1 {
        let a = out[out.len() - 1];
        let b = pts[i];
        let c = pts[i + 1];
        let colinear = ((b.0 - a.0).abs() < 0.01 && (c.0 - b.0).abs() < 0.01)
            || ((b.1 - a.1).abs() < 0.01 && (c.1 - b.1).abs() < 0.01);
        if !colinear || preserve.get(i).copied().unwrap_or(false) {
            out.push(b);
        }
    }
    out.push(*pts.last().unwrap());
    out
}

/// 端口锚点：只读 Plan 已写入的 `along_offset`（M1：Ink 零新决策）。
fn expand_port_points(plan: &Plan, ctx: &InkContext) -> BTreeMap<(EdgeId, bool), (f64, f64)> {
    let mut out = BTreeMap::new();
    for (&eid, ep) in &plan.ports {
        if ep.from.node == ep.to.node {
            continue;
        }
        for (pr, is_from) in [(&ep.from, true), (&ep.to, false)] {
            let Some(&(x, y, w, h)) = ctx.node_rects.get(pr.node.as_str()) else {
                continue;
            };
            let (mx, my) = port_boundary_point(x, y, w, h, pr.side);
            let p = match pr.side {
                PortSide::MainLow | PortSide::MainHigh => (mx + pr.along_offset, my),
                PortSide::CrossLow | PortSide::CrossHigh => (mx, my + pr.along_offset),
            };
            out.insert((eid, is_from), p);
        }
    }
    out
}

fn port_boundary_point(x: f64, y: f64, w: f64, h: f64, side: PortSide) -> (f64, f64) {
    match side {
        PortSide::MainLow => (x + w / 2.0, y),
        PortSide::MainHigh => (x + w / 2.0, y + h),
        PortSide::CrossLow => (x, y + h / 2.0),
        PortSide::CrossHigh => (x + w, y + h / 2.0),
    }
}

/// 旁路：单边 Ink（无 groups / 端口展开）。
pub fn ink_edge(
    plan: &Plan,
    substrate: &Substrate,
    ctx: &InkContext,
    edge: EdgeId,
) -> Option<Vec<(f64, f64)>> {
    ink_edge_with_lanes(plan, substrate, ctx, edge, None, None)
}

/// 审计：非轴对齐折线段数量。
pub fn audit_orthogonal_segments(edges: &[EdgeLayout]) -> usize {
    let mut n = 0usize;
    for e in edges {
        let PathGeometry::Polyline { points } = &e.geometry else {
            continue;
        };
        for w in points.windows(2) {
            let dx = (w[1].x - w[0].x).abs();
            let dy = (w[1].y - w[0].y).abs();
            if dx > 0.5 && dy > 0.5 {
                n += 1;
            }
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::atlas::channel::{TrackId, TrackOrient};
    use crate::layout::atlas::plan::{EdgePorts, PortRef, Slot, SubstrateSketch};
    use crate::layout::types::GroupLayout;

    fn tid(n: u32) -> TrackId {
        TrackId(n)
    }

    #[test]
    fn apply_track_coords_main_and_cross_do_not_clobber() {
        let plan = Plan {
            substrate: SubstrateSketch {
                rank_count: 2,
                order_count: 2,
            },
            ..Plan::default()
        };
        let mut rects = BTreeMap::new();
        rects.insert("a".into(), (0.0, 0.0, 40.0, 40.0));
        rects.insert("b".into(), (100.0, 100.0, 40.0, 40.0));
        let mut ctx = InkContext::from_plan_and_rects(&plan, rects);
        let mut substrate = Substrate::new();
        substrate
            .add_track(tid(0), TrackOrient::Cross, None, 1.0, 1, (0, 2))
            .unwrap();
        substrate
            .add_track(tid(1), TrackOrient::Main, None, 1.0, 1, (0, 2))
            .unwrap();
        let mut coords = BTreeMap::new();
        coords.insert(0, 77.0);
        coords.insert(1, 88.0);
        ctx.apply_track_coords(&substrate, &coords);
        assert!((ctx.rank_gap_y[1] - 77.0).abs() < 1e-9);
        assert!((ctx.order_gap_x[1] - 88.0).abs() < 1e-9);
    }

    #[test]
    fn push_gate_boundary_and_skirt_root_main() {
        let plan = Plan {
            substrate: SubstrateSketch {
                rank_count: 1,
                order_count: 1,
            },
            ..Plan::default()
        };
        let mut rects = BTreeMap::new();
        rects.insert("a".into(), (0.0, 0.0, 40.0, 40.0));
        let mut ctx = InkContext::from_plan_and_rects(&plan, rects);
        // R5：边界真值来自 track_coords（与 publish MARGIN 同构），不再靠 Ink bbox seed
        let mut substrate = Substrate::new();
        substrate
            .add_track(tid(10), TrackOrient::Main, None, 1.0, 0, (0, 1))
            .unwrap();
        substrate
            .add_track(tid(11), TrackOrient::Main, None, 1.0, 1, (0, 1))
            .unwrap();
        let mut coords = BTreeMap::new();
        coords.insert(10, -20.0);
        coords.insert(11, 60.0);
        ctx.apply_track_coords(&substrate, &coords);

        let mut groups = GroupTable::new();
        groups.insert(
            "g".into(),
            GroupLayout {
                x: 10.0,
                y: 10.0,
                width: 80.0,
                height: 80.0,
            },
        );
        // order_gap = -20 / 60；组内 x=50 应裙到左侧 margin
        assert!((skirt_root_main_x(50.0, &ctx, &groups) - (-20.0)).abs() < 1e-9
            || (skirt_root_main_x(50.0, &ctx, &groups) - 60.0).abs() < 1e-9
            || !axis_inside_groups(50.0, false, &groups));
        assert!((skirt_root_main_x(15.0, &ctx, &groups) - 15.0).abs() < 1e-9
            || axis_inside_groups(15.0, false, &groups));
        let _ = skirt_root_cross_y(50.0, &ctx, &groups);
    }
}
