//! Ink ↔ Plan 自反证（doc 30 R2 / M7-2）：落笔几何是否忠于 Plan 决策。
//!
//! 与 [`super::channel::verify_route_scope`]（拓扑作用域）互补：本模块验端口侧、
//! 正交折线、走廊 lane 中心、gate 边界线坐标，并识别 M7 repair 造成的 Plan 失真边。

use super::channel::{
    verify_route_scope, EdgeId, GateId, GateSide, GroupId, PortSide, RouteScopeViolation,
    Substrate, TrackOrient,
};
use super::ink::{skirt_root_cross_y, skirt_root_main_x, InkContext};
use super::plan::Plan;
use crate::layout::demand::CORRIDOR_LANE_PITCH;
use crate::layout::kernel::coordinate::main_axis::lane_centers;
use crate::layout::types::{EdgeLayout, GroupTable, NodeLayout, PathGeometry};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// 容差：与 lane pitch 同量级，覆盖 side_order 散布与浮点噪声。
const TOL: f64 = CORRIDOR_LANE_PITCH;

/// Ink↔Plan 违规项（按 EdgeId 升序收集）。
#[derive(Debug, Clone, PartialEq)]
pub enum InkPlanViolation {
    PlanDistorted(EdgeId),
    MissingGeometry(EdgeId),
    NonOrthogonal(EdgeId),
    PortSideMismatch {
        edge: EdgeId,
        is_from: bool,
        side: PortSide,
    },
    CorridorMiss {
        edge: EdgeId,
        track_index: usize,
    },
    /// 折线未落在 Plan 声称的 gate 边界线上（与 Ink `push_gate_boundary_point` 同轴）。
    GateMiss {
        edge: EdgeId,
        gate: GateId,
    },
    Scope(EdgeId, RouteScopeViolation),
}

/// 验证 `edges[eid]` 是否忠于 `plan` 对该边的决策。
///
/// `distorted` 中的边只报 [`InkPlanViolation::PlanDistorted`]，跳过几何项。
/// `groups` 用于与 Ink 对齐的根走廊裙边（`scope=None` track）。
/// 调用方须保证节点与边已在规范空间（M5：在 orientation 之前调用）。
pub fn verify_ink_vs_plan(
    plan: &Plan,
    substrate: &Substrate,
    nodes: &HashMap<String, NodeLayout>,
    edges: &[EdgeLayout],
    track_coords: &BTreeMap<u32, f64>,
    distorted: &BTreeSet<EdgeId>,
    groups: Option<&GroupTable>,
) -> Vec<InkPlanViolation> {
    let node_rects: BTreeMap<String, (f64, f64, f64, f64)> = nodes
        .iter()
        .map(|(id, n)| (id.clone(), (n.x, n.y, n.width, n.height)))
        .collect();
    let mut ctx = InkContext::from_plan_and_rects(plan, node_rects);
    ctx.apply_track_coords(substrate, track_coords);

    let mut out = Vec::new();
    let edge_ids: Vec<EdgeId> = plan.channels.keys().copied().collect();
    for eid in edge_ids {
        if distorted.contains(&eid) {
            out.push(InkPlanViolation::PlanDistorted(eid));
            continue;
        }

        let tracks = plan.channels.get(&eid).map(|t| t.as_slice()).unwrap_or(&[]);
        let gates = plan.gates.get(&eid).map(|g| g.as_slice()).unwrap_or(&[]);
        let u = endpoint_scope(substrate, plan, eid, true);
        let v = endpoint_scope(substrate, plan, eid, false);
        for sv in verify_route_scope(substrate, tracks, gates, u, v) {
            out.push(InkPlanViolation::Scope(eid, sv));
        }

        let Some(edge) = edges.get(eid) else {
            out.push(InkPlanViolation::MissingGeometry(eid));
            continue;
        };
        let pts_norm = edge_points(edge);
        if pts_norm.len() < 2 {
            out.push(InkPlanViolation::MissingGeometry(eid));
            continue;
        }

        if has_non_orthogonal(&pts_norm) {
            out.push(InkPlanViolation::NonOrthogonal(eid));
        }

        if let Some(ep) = plan.ports.get(&eid) {
            if ep.from.node != ep.to.node {
                if !port_on_side(
                    &ctx,
                    &ep.from.node,
                    ep.from.side,
                    pts_norm[0],
                ) {
                    out.push(InkPlanViolation::PortSideMismatch {
                        edge: eid,
                        is_from: true,
                        side: ep.from.side,
                    });
                }
                let last = *pts_norm.last().unwrap();
                if !port_on_side(&ctx, &ep.to.node, ep.to.side, last) {
                    out.push(InkPlanViolation::PortSideMismatch {
                        edge: eid,
                        is_from: false,
                        side: ep.to.side,
                    });
                }
            }
        }

        let lanes = plan
            .lane_indices
            .get(&eid)
            .cloned()
            .unwrap_or_else(|| vec![0; tracks.len()]);
        for (i, &tid) in tracks.iter().enumerate() {
            let Some(track) = substrate.track(tid) else {
                continue;
            };
            let lane_i = lanes.get(i).copied().unwrap_or(0);
            let n_lanes = (lane_i + 1).max(1);
            let ok = match track.orient {
                TrackOrient::Cross => {
                    let base = ctx
                        .rank_gap_y
                        .get(track.line)
                        .copied()
                        .unwrap_or(track.line as f64 * 50.0);
                    let centers = lane_centers(base, n_lanes, CORRIDOR_LANE_PITCH);
                    let mut y = centers
                        .get(lane_i as usize)
                        .copied()
                        .unwrap_or(base);
                    if track.scope.is_none() {
                        if let Some(gs) = groups {
                            y = skirt_root_cross_y(y, &ctx, gs);
                        }
                    }
                    polyline_hits_h_line(&pts_norm, y)
                }
                TrackOrient::Main => {
                    let base = ctx
                        .order_gap_x
                        .get(track.line)
                        .copied()
                        .unwrap_or(track.line as f64 * 50.0);
                    let centers = lane_centers(base, n_lanes, CORRIDOR_LANE_PITCH);
                    let mut x = centers
                        .get(lane_i as usize)
                        .copied()
                        .unwrap_or(base);
                    if track.scope.is_none() {
                        if let Some(gs) = groups {
                            x = skirt_root_main_x(x, &ctx, gs);
                        }
                    }
                    polyline_hits_v_line(&pts_norm, x)
                }
            };
            if !ok {
                out.push(InkPlanViolation::CorridorMiss {
                    edge: eid,
                    track_index: i,
                });
            }
        }

        // M7-2：折线须含每个 Plan gate 的边界线坐标（与 Ink push_gate_boundary_point 同轴）
        for &gid in gates {
            let Some(gate) = substrate.gate(gid) else {
                continue;
            };
            if !polyline_hits_gate_line(&pts_norm, gate.side, gate.line, &ctx) {
                out.push(InkPlanViolation::GateMiss {
                    edge: eid,
                    gate: gid,
                });
            }
        }
    }
    out
}

/// 与 Ink `push_gate_boundary_point` 同轴：Main* 钉 y=rank_gap；Cross* 钉 x=order_gap。
fn polyline_hits_gate_line(
    pts: &[(f64, f64)],
    side: GateSide,
    line: usize,
    ctx: &InkContext,
) -> bool {
    match side {
        GateSide::MainLow | GateSide::MainHigh => {
            let gy = ctx
                .rank_gap_y
                .get(line)
                .copied()
                .unwrap_or(line as f64 * 50.0);
            polyline_hits_h_line(pts, gy)
        }
        GateSide::CrossLow | GateSide::CrossHigh => {
            let gx = ctx
                .order_gap_x
                .get(line)
                .copied()
                .unwrap_or(line as f64 * 50.0);
            polyline_hits_v_line(pts, gx)
        }
    }
}

/// 折线是否命中水平走廊/gate 线 `y`（折点或正交段穿越）。
fn polyline_hits_h_line(pts: &[(f64, f64)], y: f64) -> bool {
    if pts.iter().any(|p| (p.1 - y).abs() <= TOL) {
        return true;
    }
    pts.windows(2).any(|w| {
        let (x0, y0) = w[0];
        let (x1, y1) = w[1];
        if (x0 - x1).abs() <= 0.5 {
            let lo = y0.min(y1);
            let hi = y0.max(y1);
            y >= lo - TOL && y <= hi + TOL
        } else if (y0 - y1).abs() <= 0.5 {
            (y0 - y).abs() <= TOL
        } else {
            false
        }
    })
}

/// 折线是否命中垂直走廊/gate 线 `x`（折点或正交段穿越）。
fn polyline_hits_v_line(pts: &[(f64, f64)], x: f64) -> bool {
    if pts.iter().any(|p| (p.0 - x).abs() <= TOL) {
        return true;
    }
    pts.windows(2).any(|w| {
        let (x0, y0) = w[0];
        let (x1, y1) = w[1];
        if (y0 - y1).abs() <= 0.5 {
            let lo = x0.min(x1);
            let hi = x0.max(x1);
            x >= lo - TOL && x <= hi + TOL
        } else if (x0 - x1).abs() <= 0.5 {
            (x0 - x).abs() <= TOL
        } else {
            false
        }
    })
}

fn endpoint_scope(
    substrate: &Substrate,
    plan: &Plan,
    edge: EdgeId,
    from: bool,
) -> Option<GroupId> {
    let ep = plan.ports.get(&edge)?;
    let slot_id = if from {
        ep.from.slot_id
    } else {
        ep.to.slot_id
    }?;
    let port = substrate.port(slot_id)?;
    substrate.track(port.track).and_then(|t| t.scope)
}

fn edge_points(edge: &EdgeLayout) -> Vec<(f64, f64)> {
    match &edge.geometry {
        PathGeometry::Polyline { points } => points.iter().map(|p| (p.x, p.y)).collect(),
        PathGeometry::Straight { start, end } => {
            vec![(start.x, start.y), (end.x, end.y)]
        }
        PathGeometry::Bezier { start, end, .. } => {
            vec![(start.x, start.y), (end.x, end.y)]
        }
    }
}

fn has_non_orthogonal(pts: &[(f64, f64)]) -> bool {
    pts.windows(2).any(|w| {
        let dx = (w[1].0 - w[0].0).abs();
        let dy = (w[1].1 - w[0].1).abs();
        dx > 0.5 && dy > 0.5
    })
}

/// 端点是否落在节点声称侧上（沿侧方向允许 `TOL` 散布，法向贴边）。
fn port_on_side(
    ctx: &InkContext,
    node: &str,
    side: PortSide,
    p: (f64, f64),
) -> bool {
    let Some(&(x, y, w, h)) = ctx.node_rects.get(node) else {
        return true; // 无 bbox 时不误报
    };
    match side {
        PortSide::MainLow => (p.1 - y).abs() <= TOL && p.0 >= x - TOL && p.0 <= x + w + TOL,
        PortSide::MainHigh => {
            (p.1 - (y + h)).abs() <= TOL && p.0 >= x - TOL && p.0 <= x + w + TOL
        }
        PortSide::CrossLow => (p.0 - x).abs() <= TOL && p.1 >= y - TOL && p.1 <= y + h + TOL,
        PortSide::CrossHigh => {
            (p.0 - (x + w)).abs() <= TOL && p.1 >= y - TOL && p.1 <= y + h + TOL
        }
    }
}

/// 统计报告辅助：几何类 vs 失真类。
pub fn partition_violations(v: &[InkPlanViolation]) -> (usize, usize) {
    let mut geom = 0usize;
    let mut distorted = 0usize;
    for item in v {
        match item {
            InkPlanViolation::PlanDistorted(_) => distorted += 1,
            _ => geom += 1,
        }
    }
    (geom, distorted)
}

/// M7-2 硬门禁计入的违规（gate / 走廊 / 正交 / 端口侧 / 缺失 / scope）。
///
/// `PlanDistorted` 仍不计入（与 partition 软轨一致）。
pub fn hard_geom_count(v: &[InkPlanViolation]) -> usize {
    v.iter()
        .filter(|item| {
            matches!(
                item,
                InkPlanViolation::MissingGeometry(_)
                    | InkPlanViolation::NonOrthogonal(_)
                    | InkPlanViolation::CorridorMiss { .. }
                    | InkPlanViolation::GateMiss { .. }
                    | InkPlanViolation::PortSideMismatch { .. }
                    | InkPlanViolation::Scope(_, _)
            )
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::atlas::channel::{
        GateCapacity, GateId, GateSide, GroupId, PortSlotId, TrackId, TrackOrient,
    };
    use crate::layout::atlas::plan::{EdgePorts, PortRef, Slot, SubstrateSketch};
    use crate::layout::geometry::Point;
    use crate::layout::types::{EdgeLayout, PathGeometry, Port};

    fn rect_node(x: f64, y: f64, w: f64, h: f64) -> NodeLayout {
        NodeLayout {
            x,
            y,
            width: w,
            height: h,
            ..Default::default()
        }
    }

    fn poly(pts: &[(f64, f64)]) -> EdgeLayout {
        let mut e = EdgeLayout::empty();
        e.geometry = PathGeometry::Polyline {
            points: pts.iter().map(|&(x, y)| Point { x, y }).collect(),
        };
        e.from_port = Port::Bottom;
        e.to_port = Port::Top;
        e
    }

    fn diamond_plan_substrate() -> (Plan, Substrate, HashMap<String, NodeLayout>) {
        let mut substrate = Substrate::default();
        substrate
            .add_track(TrackId(0), TrackOrient::Cross, None, 1.0, 0, (0, 4))
            .unwrap();
        substrate
            .add_track(TrackId(10), TrackOrient::Main, None, 1.0, 0, (0, 2))
            .unwrap();
        substrate
            .add_track(TrackId(1), TrackOrient::Cross, None, 1.0, 1, (0, 4))
            .unwrap();
        substrate.link(TrackId(0), TrackId(10)).unwrap();
        substrate.link(TrackId(1), TrackId(10)).unwrap();
        substrate
            .attach_port(PortSlotId(0), "a", PortSide::MainHigh, 0, TrackId(0), 0)
            .unwrap();
        substrate
            .attach_port(PortSlotId(1), "b", PortSide::MainLow, 0, TrackId(1), 0)
            .unwrap();

        let mut plan = Plan {
            substrate: SubstrateSketch {
                rank_count: 2,
                order_count: 1,
            },
            ..Plan::default()
        };
        plan.node_slots.insert("a".into(), Slot { rank: 0, order: 0 });
        plan.node_slots.insert("b".into(), Slot { rank: 1, order: 0 });
        plan.channels.insert(0, vec![TrackId(0), TrackId(10), TrackId(1)]);
        plan.gates.insert(0, vec![]);
        plan.lane_indices.insert(0, vec![0, 0, 0]);
        plan.ports.insert(
            0,
            EdgePorts {
                from: PortRef {
                    node: "a".into(),
                    side: PortSide::MainHigh,
                    slot_index: 0,
                    slot_id: Some(PortSlotId(0)),
                    side_order: 0,
                    along_offset: 0.0,
                },
                to: PortRef {
                    node: "b".into(),
                    side: PortSide::MainLow,
                    slot_index: 0,
                    slot_id: Some(PortSlotId(1)),
                    side_order: 0,
                    along_offset: 0.0,
                },
            },
        );

        let mut nodes = HashMap::new();
        nodes.insert("a".into(), rect_node(0.0, 0.0, 40.0, 40.0));
        nodes.insert("b".into(), rect_node(0.0, 100.0, 40.0, 40.0));
        (plan, substrate, nodes)
    }

    /// 忠实折线：经 `apply_track_coords` 取走廊中心（不再依赖 Ink interior bbox seed）。
    fn faithful_polyline(
        plan: &Plan,
        substrate: &Substrate,
        nodes: &HashMap<String, NodeLayout>,
    ) -> Vec<(f64, f64)> {
        let rects: BTreeMap<_, _> = nodes
            .iter()
            .map(|(id, n)| (id.clone(), (n.x, n.y, n.width, n.height)))
            .collect();
        // 与 publish_* 同构：外框 margin + 行/列 bbox 中点
        let mut coords = BTreeMap::new();
        coords.insert(0u32, -20.0); // Cross line 0
        coords.insert(1u32, 70.0); // Cross line 1：a.bottom(40) 与 b.top(100) 中点
        coords.insert(10u32, -20.0); // Main line 0
        let mut ctx = InkContext::from_plan_and_rects(plan, rects);
        ctx.apply_track_coords(substrate, &coords);
        let y0 = ctx.rank_gap_y[0];
        let y1 = ctx.rank_gap_y[1];
        let x_m = ctx.order_gap_x[0];
        // a MainHigh=(20,40), b MainLow=(20,100)
        vec![
            (20.0, 40.0),
            (20.0, y0),
            (x_m, y0),
            (x_m, y1),
            (20.0, y1),
            (20.0, 100.0),
        ]
    }

    #[test]
    fn verify_table_driven_cases() {
        let (plan, substrate, nodes) = diamond_plan_substrate();
        let mut track_coords = BTreeMap::new();
        track_coords.insert(0u32, -20.0);
        track_coords.insert(1u32, 70.0);
        track_coords.insert(10u32, -20.0);

        // faithful
        {
            let edges = vec![poly(&faithful_polyline(&plan, &substrate, &nodes))];
            let v = verify_ink_vs_plan(
                &plan,
                &substrate,
                &nodes,
                &edges,
                &track_coords,
                &BTreeSet::new(),
                None,
            );
            let (geom, _) = partition_violations(&v);
            assert_eq!(geom, 0, "faithful: {v:?}");
        }
        // diagonal
        {
            let edges = vec![poly(&[(20.0, 40.0), (50.0, 80.0), (20.0, 100.0)])];
            let v = verify_ink_vs_plan(
                &plan,
                &substrate,
                &nodes,
                &edges,
                &track_coords,
                &BTreeSet::new(),
                None,
            );
            assert!(
                partition_violations(&v).0 > 0,
                "diagonal should fail: {v:?}"
            );
        }
        // port mismatch
        {
            let mut pts = faithful_polyline(&plan, &substrate, &nodes);
            pts[0] = (0.0, 20.0);
            let edges = vec![poly(&pts)];
            let v = verify_ink_vs_plan(
                &plan,
                &substrate,
                &nodes,
                &edges,
                &track_coords,
                &BTreeSet::new(),
                None,
            );
            assert!(
                v.iter().any(|x| matches!(x, InkPlanViolation::PortSideMismatch { .. })),
                "port mismatch: {v:?}"
            );
        }
        // distorted skips geom
        {
            let mut d = BTreeSet::new();
            d.insert(0);
            let edges = vec![poly(&[(0.0, 0.0), (30.0, 40.0)])];
            let v = verify_ink_vs_plan(
                &plan,
                &substrate,
                &nodes,
                &edges,
                &track_coords,
                &d,
                None,
            );
            let (geom, dist_n) = partition_violations(&v);
            assert_eq!(dist_n, 1, "{v:?}");
            assert_eq!(geom, 0, "{v:?}");
        }
    }

    /// M7-2：有 Plan gate 时折线必须命中边界线；缺折点 → GateMiss。
    #[test]
    fn verify_gate_line_miss_and_hit() {
        let g = GroupId(1);
        let mut substrate = Substrate::default();
        substrate.add_group(g, None, (0, 0), (0, 0)).unwrap();
        substrate
            .add_track(TrackId(10), TrackOrient::Main, Some(g), 1.0, 1, (1, 1))
            .unwrap();
        substrate
            .add_track(TrackId(11), TrackOrient::Main, None, 1.0, 1, (2, 2))
            .unwrap();
        substrate
            .add_gate(
                GateId(1),
                g,
                GateSide::MainHigh,
                1,
                vec![(TrackId(10), TrackId(11))],
                GateCapacity::Unbounded,
            )
            .unwrap();
        let mut plan = Plan {
            substrate: SubstrateSketch {
                rank_count: 2,
                order_count: 1,
            },
            ..Plan::default()
        };
        plan.node_slots.insert("a".into(), Slot { rank: 0, order: 0 });
        plan.channels.insert(0, vec![TrackId(10), TrackId(11)]);
        plan.gates.insert(0, vec![GateId(1)]);
        plan.lane_indices.insert(0, vec![0, 0]);

        let mut nodes = HashMap::new();
        nodes.insert("a".into(), rect_node(10.0, 10.0, 40.0, 40.0));
        // 无 Cross track → rank_gap_y[1] 保持 seed 占位 50.0
        let track_coords = BTreeMap::new();

        // 整段远离 y=50（|Δ|>TOL）→ GateMiss
        {
            let edges = vec![poly(&[(30.0, 10.0), (30.0, 20.0)])];
            let v = verify_ink_vs_plan(
                &plan,
                &substrate,
                &nodes,
                &edges,
                &track_coords,
                &BTreeSet::new(),
                None,
            );
            assert!(
                v.iter()
                    .any(|x| matches!(x, InkPlanViolation::GateMiss { gate: GateId(1), .. })),
                "missing gate line: {v:?}"
            );
        }

        // 竖直段穿越 y=50 → 无 GateMiss（不必有折点）
        {
            let edges = vec![poly(&[(30.0, 10.0), (30.0, 90.0)])];
            let v = verify_ink_vs_plan(
                &plan,
                &substrate,
                &nodes,
                &edges,
                &track_coords,
                &BTreeSet::new(),
                None,
            );
            assert!(
                !v.iter()
                    .any(|x| matches!(x, InkPlanViolation::GateMiss { .. })),
                "segment crosses gate line: {v:?}"
            );
        }
    }
}
