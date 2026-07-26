//! 通道图不变量测试（27 号文 L1–L8 合法化后）。
//!
//! 测试按不变量组织，不按函数组织：
//! - 构建期防线：穿组 / 斜线 / 不相交 link / 非法 gate（G-inv-1/2/3）不可表达
//! - 切割规则：B1–B8 逐条边界用例（A9）
//! - 跨组路径：gate 序列由构造产生，顺序正确
//! - 作用域掩码（L8）：借道穿组被拦截，显式 Infeasible
//! - 容量语义：gate 边界段容量池（诊断 Fixed）、track lane 需求（软，B1）
//! - 词典序：折点严格优先于长度
//! - 确定性：平局可复现、derive 双跑逐字段一致（A8）
//! - 不可行：显式 Infeasible，无静默兜底
//! - 合流：共享 track 后缀 = bundle
//! - 推导 adapter：从 rank/order/group 蓝图生成可路由的 Substrate
//! - L6 检查器：合法基底全通过、注入越界段报违规

use super::bundle::detect_bundles;
use super::derive::{
    ChannelBlueprint, DeriveError, DerivePortsOptions, GroupSpec, NodeSpec, derive_node_ports,
    derive_substrate,
};
use super::graph::{ChannelGraph, EndpointError, Occupancy};
use super::search::{ScopeMask, route, route_candidates, route_node_sides};
use super::substrate::{
    GateCapacity, GateId, GateSide, GroupId, PortSide, PortSlotId, Substrate, SubstrateError,
    TrackId, TrackOrient,
};
use super::verify::{RouteScopeViolation, verify_route_scope};
use crate::layout::kernel::cost::SolverStatus;

const MAIN: TrackOrient = TrackOrient::Main;
const CROSS: TrackOrient = TrackOrient::Cross;

fn tid(n: u32) -> TrackId {
    TrackId(n)
}
fn gid(n: u32) -> GroupId {
    GroupId(n)
}
fn gate(n: u32) -> GateId {
    GateId(n)
}
fn port(n: u32) -> PortSlotId {
    PortSlotId(n)
}

/// 测试辅助：按宿主轨道走向自动选相容侧（P-inv-4），按端口 id 取唯一节点名（P-inv-2），
/// slot_index=0、capacity=0（不限）。供手工搭建的基底测试使用。
fn attach(s: &mut Substrate, p: u32, t: u32) {
    let side = match s.track(tid(t)).unwrap().orient {
        TrackOrient::Cross => PortSide::MainLow,
        TrackOrient::Main => PortSide::CrossLow,
    };
    s.attach_port(port(p), &format!("n{p}"), side, 0, tid(t), 0)
        .unwrap();
}

/// 测试辅助（derive 场景）：按轨道走向选相容侧，用真实端点节点名挂 src/dst 两端，
/// 返回 (pa, pb)。capacity=0（不限）。
fn attach_edge(s: &mut Substrate, src: TrackId, dst: TrackId, from: &str, to: &str) -> (PortSlotId, PortSlotId) {
    attach_edge_slot(s, src, dst, from, to, 0)
}

/// 同 [`attach_edge`]，但显式指定 `slot_index`（同节点多条边时避免槽位冲突）。
fn attach_edge_slot(
    s: &mut Substrate,
    src: TrackId,
    dst: TrackId,
    from: &str,
    to: &str,
    slot: u32,
) -> (PortSlotId, PortSlotId) {
    let src_side = match s.track(src).unwrap().orient {
        TrackOrient::Cross => PortSide::MainLow,
        TrackOrient::Main => PortSide::CrossLow,
    };
    let dst_side = match s.track(dst).unwrap().orient {
        TrackOrient::Cross => PortSide::MainLow,
        TrackOrient::Main => PortSide::CrossLow,
    };
    let pa = s.alloc_port_id();
    s.attach_port(pa, from, src_side, slot, src, 0).unwrap();
    let pb = s.alloc_port_id();
    s.attach_port(pb, to, dst_side, slot, dst, 0).unwrap();
    (pa, pb)
}

/// 顶层 3 缝 × 2 走廊网格（无组，全部整线段）：
///
/// ```text
/// C0 ──┬────┬──   (rank 缝 0)
///      M0   M1    (纵向走廊，order-gap 0/1)
/// C1 ──┼────┼──   (rank 缝 1)
///      M0   M1
/// C2 ──┴────┴──   (rank 缝 2)
/// ```
///
/// 奇偶坐标：2 列网格 → 沿线延展 (0,4)。端口：p0 挂 C0，p1 挂 C2。
fn plain_grid() -> Substrate {
    let mut s = Substrate::new();
    for c in 0..3 {
        s.add_track(tid(c), CROSS, None, 1.0, c as usize, (0, 4)).unwrap();
    }
    for m in 10..12 {
        s.add_track(tid(m), MAIN, None, 1.0, (m - 10) as usize, (0, 4)).unwrap();
    }
    for c in 0..3 {
        for m in 10..12 {
            s.link(tid(c), tid(m)).unwrap();
        }
    }
    attach(&mut s, 0, 0);
    attach(&mut s, 1, 2);
    s
}

/// 两层嵌套组：top ⊃ G1 ⊃ G2。同一条 Main 线（order-gap 0）被两层边界切成三段，
/// 靠边界段 gate 逐层穿出：
///
/// ```text
/// M0 根 (0,2) ══g1(rank-gap 1)══ M1 G1 (3,4) ══g2(rank-gap 2)══ M2 G2 (5,9)
/// ```
///
/// 端口：p0 挂 M2（最内层），p1 挂 M0（顶层）。gate 容量由参数给定。
fn nested_groups(capacity: GateCapacity) -> Substrate {
    let mut s = Substrate::new();
    s.add_group(gid(1), None, (1, 4), (0, 0)).unwrap();
    s.add_group(gid(2), Some(gid(1)), (2, 4), (0, 0)).unwrap();
    s.add_track(tid(0), MAIN, None, 1.0, 0, (0, 2)).unwrap();
    s.add_track(tid(1), MAIN, Some(gid(1)), 1.0, 0, (3, 4)).unwrap();
    s.add_track(tid(2), MAIN, Some(gid(2)), 1.0, 0, (5, 9)).unwrap();
    s.add_gate(gate(1), gid(1), GateSide::MainLow, 1, vec![(tid(1), tid(0))], capacity)
        .unwrap();
    s.add_gate(gate(2), gid(2), GateSide::MainLow, 2, vec![(tid(2), tid(1))], capacity)
        .unwrap();
    attach(&mut s, 0, 2);
    attach(&mut s, 1, 0);
    s
}

// ---------------------------------------------------------------------------
// 构建期防线：非法结构不可表达（H2 / H3 由构造保证，不是检测项）
// ---------------------------------------------------------------------------

#[test]
fn link_across_scopes_is_rejected() {
    let mut s = Substrate::new();
    s.add_group(gid(1), None, (0, 0), (0, 0)).unwrap();
    s.add_track(tid(0), CROSS, None, 1.0, 0, (0, 2)).unwrap();
    s.add_track(tid(1), MAIN, Some(gid(1)), 1.0, 0, (0, 2)).unwrap();
    // 组内轨道与顶层轨道直连 = 穿组，必须在构建期拒绝
    assert_eq!(
        s.link(tid(0), tid(1)),
        Err(SubstrateError::CrossScopeConnection {
            a: tid(0),
            b: tid(1)
        })
    );
}

#[test]
fn parallel_tracks_cannot_intersect() {
    let mut s = Substrate::new();
    s.add_track(tid(0), MAIN, None, 1.0, 0, (0, 2)).unwrap();
    s.add_track(tid(1), MAIN, None, 1.0, 1, (0, 2)).unwrap();
    // 同向轨道无交点：link 只能发生在 Main × Cross 交口 → 折点即转移（H3）
    assert_eq!(
        s.link(tid(0), tid(1)),
        Err(SubstrateError::ParallelLink {
            a: tid(0),
            b: tid(1)
        })
    );
}

#[test]
fn non_intersecting_segments_cannot_link() {
    // L1「相交才 link」：段延展必须覆盖对方所在线的 gap 坐标
    let mut s = Substrate::new();
    s.add_track(tid(0), CROSS, None, 1.0, 0, (0, 2)).unwrap(); // 只覆盖 order-gap 0..1
    s.add_track(tid(1), MAIN, None, 1.0, 2, (0, 2)).unwrap(); // 走廊在 order-gap 2
    assert_eq!(
        s.link(tid(0), tid(1)),
        Err(SubstrateError::NonIntersectingLink {
            a: tid(0),
            b: tid(1)
        })
    );
}

#[test]
fn inverted_extent_is_rejected() {
    // B5：零长/倒置延展在构建期拒绝
    let mut s = Substrate::new();
    assert_eq!(
        s.add_track(tid(0), CROSS, None, 1.0, 0, (2, 1)),
        Err(SubstrateError::InvalidExtent { track: tid(0) })
    );
}

#[test]
fn gate_scope_mismatch_is_rejected() {
    let mut s = Substrate::new();
    s.add_group(gid(1), None, (1, 1), (0, 0)).unwrap();
    s.add_track(tid(0), MAIN, None, 1.0, 0, (0, 2)).unwrap();
    s.add_track(tid(1), MAIN, Some(gid(1)), 1.0, 0, (3, 4)).unwrap();
    // inner/outer 颠倒：inner 必须在组内、outer 必须在祖先 scope
    assert_eq!(
        s.add_gate(
            gate(1),
            gid(1),
            GateSide::MainLow,
            1,
            vec![(tid(0), tid(1))],
            GateCapacity::Fixed(4)
        ),
        Err(SubstrateError::InvalidGateScope {
            gate: gate(1),
            group: gid(1)
        })
    );
    // 空配对列表被拒绝
    assert_eq!(
        s.add_gate(gate(2), gid(1), GateSide::MainLow, 1, vec![], GateCapacity::Unbounded),
        Err(SubstrateError::EmptyGate { gate: gate(2) })
    );
}

#[test]
fn gate_pair_orientation_must_match_side() {
    // G-inv-1：CrossLow/High 闸口必须配 Cross 段；给 Main 段对 → 拒绝
    let mut s = Substrate::new();
    s.add_group(gid(1), None, (1, 4), (0, 0)).unwrap();
    s.add_track(tid(0), MAIN, None, 1.0, 0, (0, 2)).unwrap();
    s.add_track(tid(1), MAIN, Some(gid(1)), 1.0, 0, (3, 4)).unwrap();
    assert_eq!(
        s.add_gate(
            gate(1),
            gid(1),
            GateSide::CrossLow,
            1,
            vec![(tid(1), tid(0))],
            GateCapacity::Unbounded
        ),
        Err(SubstrateError::GatePairMismatch {
            gate: gate(1),
            inner: tid(1),
            outer: tid(0)
        })
    );
}

#[test]
fn gate_pair_must_be_boundary_adjacent() {
    // G-inv-2：配对段必须同线且隔边界相邻
    let mut s = Substrate::new();
    s.add_group(gid(1), None, (1, 4), (0, 1)).unwrap();
    s.add_track(tid(0), MAIN, None, 1.0, 0, (0, 2)).unwrap();
    s.add_track(tid(2), MAIN, Some(gid(1)), 1.0, 0, (5, 6)).unwrap(); // 不贴边界（ext.0 ≠ 3）
    s.add_track(tid(3), MAIN, Some(gid(1)), 1.0, 1, (3, 4)).unwrap(); // 异线（line 1 ≠ 0）
    assert_eq!(
        s.add_gate(
            gate(1),
            gid(1),
            GateSide::MainLow,
            1,
            vec![(tid(2), tid(0))],
            GateCapacity::Unbounded
        ),
        Err(SubstrateError::GatePairMismatch {
            gate: gate(1),
            inner: tid(2),
            outer: tid(0)
        })
    );
    assert_eq!(
        s.add_gate(
            gate(2),
            gid(1),
            GateSide::MainLow,
            1,
            vec![(tid(3), tid(0))],
            GateCapacity::Unbounded
        ),
        Err(SubstrateError::GatePairMismatch {
            gate: gate(2),
            inner: tid(3),
            outer: tid(0)
        })
    );
}

#[test]
fn duplicate_gate_side_is_rejected() {
    // G-inv-3：同一 (group, side) 只允许一个 gate
    let mut s = Substrate::new();
    s.add_group(gid(1), None, (1, 4), (0, 0)).unwrap();
    s.add_track(tid(0), MAIN, None, 1.0, 0, (0, 2)).unwrap();
    s.add_track(tid(1), MAIN, Some(gid(1)), 1.0, 0, (3, 4)).unwrap();
    s.add_gate(gate(1), gid(1), GateSide::MainLow, 1, vec![(tid(1), tid(0))], GateCapacity::Unbounded)
        .unwrap();
    assert_eq!(
        s.add_gate(
            gate(2),
            gid(1),
            GateSide::MainLow,
            1,
            vec![(tid(1), tid(0))],
            GateCapacity::Unbounded
        ),
        Err(SubstrateError::DuplicateGateSide {
            group: gid(1),
            side: GateSide::MainLow
        })
    );
}

#[test]
fn b8_boundary_seam_belongs_to_outer_segment() {
    // B8 钉死：边界缝归外段——组外段与边界缝正常 link；组内段与边界缝被构建期
    // 拒绝（scope 防线先触发；几何上组内段 ext 也不含缝 gap 坐标，双重防线）。
    let mut s = Substrate::new();
    s.add_group(gid(1), None, (1, 2), (0, 1)).unwrap();
    s.add_track(tid(0), MAIN, None, 1.0, 0, (0, 2)).unwrap(); // 根段（含边界缝 gap 坐标 2）
    s.add_track(tid(1), MAIN, Some(gid(1)), 1.0, 0, (3, 5)).unwrap(); // 组内段
    s.add_track(tid(2), CROSS, None, 1.0, 1, (0, 4)).unwrap(); // 边界横缝（根）
    s.link(tid(0), tid(2)).unwrap();
    assert_eq!(
        s.link(tid(1), tid(2)),
        Err(SubstrateError::CrossScopeConnection {
            a: tid(1),
            b: tid(2)
        })
    );

    // 图上无该转移：组内段到边界缝必须经 gate（而非直接 link 旁路）
    s.add_gate(gate(1), gid(1), GateSide::MainLow, 1, vec![(tid(1), tid(0))], GateCapacity::Unbounded)
        .unwrap();
    attach(&mut s, 0, 1); // 组内
    attach(&mut s, 1, 2); // 边界缝
    let g = ChannelGraph::from_substrate(&s);
    let mask = ScopeMask::for_ports(&s, port(0), port(1)).unwrap();
    let out = route(&g, port(0), port(1), &Occupancy::new(), &mask).unwrap();
    assert_eq!(out.status, SolverStatus::Converged);
    assert_eq!(out.gates, vec![gate(1)], "组内段出组必须经 gate，无 link 旁路");
    assert_eq!(out.tracks, vec![tid(1), tid(0), tid(2)]);
}

// ---------------------------------------------------------------------------
// 跨组路径：gate 序列由构造产生
// ---------------------------------------------------------------------------

#[test]
fn cross_group_route_passes_gates_innermost_first() {
    let s = nested_groups(GateCapacity::Fixed(4));
    let g = ChannelGraph::from_substrate(&s);
    let mask = ScopeMask::for_ports(&s, port(0), port(1)).unwrap();
    let out = route(&g, port(0), port(1), &Occupancy::new(), &mask).unwrap();

    assert_eq!(out.status, SolverStatus::Converged);
    // 从 G2 内穿到顶层：轨道逐层向外，gate 按穿越序 = [g2, g1]
    assert_eq!(out.tracks, vec![tid(2), tid(1), tid(0)]);
    assert_eq!(out.gates, vec![gate(2), gate(1)]);
    // 三条轨道同向（Main），gate 直穿无折点——折点账目与构造一致
    assert_eq!(out.cost.q3_bends, 0);
}

// ---------------------------------------------------------------------------
// 作用域掩码（L8）：借道穿组被拦截
// ---------------------------------------------------------------------------

#[test]
fn l8_scope_mask_blocks_borrowed_passage_through_group() {
    // §3.1 场景：两端都在组外，同一条 Main 线上组内段是唯一「直穿」捷径。
    // 掩码 = {None}（两端 scope 链）→ 直穿被拦、显式 Infeasible；
    // 显式放行组后可走 → 证明拦截来自掩码而非图结构。
    let mut s = Substrate::new();
    s.add_group(gid(1), None, (1, 1), (0, 0)).unwrap();
    s.add_track(tid(0), MAIN, None, 1.0, 0, (0, 2)).unwrap();
    s.add_track(tid(1), MAIN, Some(gid(1)), 1.0, 0, (3, 3)).unwrap();
    s.add_track(tid(2), MAIN, None, 1.0, 0, (4, 6)).unwrap();
    s.add_gate(gate(1), gid(1), GateSide::MainLow, 1, vec![(tid(1), tid(0))], GateCapacity::Unbounded)
        .unwrap();
    s.add_gate(gate(2), gid(1), GateSide::MainHigh, 2, vec![(tid(1), tid(2))], GateCapacity::Unbounded)
        .unwrap();
    attach(&mut s, 0, 0);
    attach(&mut s, 1, 2);
    let g = ChannelGraph::from_substrate(&s);
    let occ = Occupancy::new();

    let mask = ScopeMask::for_ports(&s, port(0), port(1)).unwrap();
    let blocked = route(&g, port(0), port(1), &occ, &mask).unwrap();
    assert_eq!(blocked.status, SolverStatus::Infeasible, "借道穿组必须被 L8 掩码拦截");
    assert!(blocked.tracks.is_empty(), "不可行解不得携带假路径");

    let permissive = ScopeMask::for_scopes(&s, Some(gid(1)), None);
    let through = route(&g, port(0), port(1), &occ, &permissive).unwrap();
    assert_eq!(through.status, SolverStatus::Converged);
    assert_eq!(through.gates, vec![gate(1), gate(2)], "显式放行后经两侧 gate 直穿");
    assert_eq!(through.tracks, vec![tid(0), tid(1), tid(2)]);
}

// ---------------------------------------------------------------------------
// 容量语义：gate 边界段容量池（诊断 Fixed）、track lane 需求（软，B1）
// ---------------------------------------------------------------------------

#[test]
fn track_lane_demand_grows_without_limit() {
    // B1 核心机制：5 条边挤同一条轨道全部成功，lane 数作为 Demand 输出
    let s = plain_grid();
    let g = ChannelGraph::from_substrate(&s);
    let mask = ScopeMask::for_ports(&s, port(0), port(1)).unwrap();
    let mut occ = Occupancy::new();

    for _ in 0..5 {
        let out = route(&g, port(0), port(1), &occ, &mask).unwrap();
        assert_eq!(out.status, SolverStatus::Converged);
        occ.commit(&out.tracks, &out.gates);
    }
    // 全部 5 条边共用同一最优路径（无拥挤惩罚），走廊 lane 需求 = 5
    let corridor_demand: u32 = (10..12).map(|m| occ.lane_demand(tid(m))).max().unwrap();
    assert_eq!(corridor_demand, 5);
}

#[test]
fn boundary_gate_is_shared_capacity_pool() {
    // 一个边界线 gate 按线逐对配对（L2），全部配对共享容量池：
    // 组内两条走廊（不同 order-gap 线）各配 (inner, outer) 对，容量 2 →
    // 两条边都过，第三条 Infeasible
    let mut s = Substrate::new();
    s.add_group(gid(1), None, (1, 2), (0, 1)).unwrap();
    s.add_track(tid(0), MAIN, None, 1.0, 0, (0, 2)).unwrap(); // 顶层走廊 og0（目标）
    s.add_track(tid(1), MAIN, Some(gid(1)), 1.0, 0, (3, 5)).unwrap(); // 组内走廊 1
    s.add_track(tid(10), MAIN, None, 1.0, 1, (0, 2)).unwrap(); // 顶层走廊 og1
    s.add_track(tid(11), MAIN, Some(gid(1)), 1.0, 1, (3, 5)).unwrap(); // 组内走廊 2
    s.add_track(tid(20), CROSS, None, 1.0, 0, (0, 2)).unwrap(); // 顶部横缝
    s.link(tid(20), tid(0)).unwrap();
    s.link(tid(20), tid(10)).unwrap();
    s.add_gate(
        gate(1),
        gid(1),
        GateSide::MainLow,
        1,
        vec![(tid(1), tid(0)), (tid(11), tid(10))],
        GateCapacity::Fixed(2),
    )
    .unwrap();
    attach(&mut s, 0, 1);
    attach(&mut s, 1, 11);
    attach(&mut s, 2, 0);

    let g = ChannelGraph::from_substrate(&s);
    let mut occ = Occupancy::new();

    // 两条边从不同内侧走廊穿出，共享 gate 容量池
    let m1 = ScopeMask::for_ports(&s, port(0), port(2)).unwrap();
    let e1 = route(&g, port(0), port(2), &occ, &m1).unwrap();
    assert_eq!(e1.status, SolverStatus::Converged);
    occ.commit(&e1.tracks, &e1.gates);
    let m2 = ScopeMask::for_ports(&s, port(1), port(2)).unwrap();
    let e2 = route(&g, port(1), port(2), &occ, &m2).unwrap();
    assert_eq!(e2.status, SolverStatus::Converged);
    occ.commit(&e2.tracks, &e2.gates);

    assert_eq!(occ.gate_load(gate(1)), 2, "两条边各占容量池 1 单位");

    // 第三条边：容量池满 → 显式 Infeasible
    let e3 = route(&g, port(0), port(2), &occ, &m1).unwrap();
    assert_eq!(e3.status, SolverStatus::Infeasible);
}

#[test]
fn exhausted_gate_yields_explicit_infeasible() {
    // 唯一 gate 链容量 1：第二条边必须显式 Infeasible，而非静默兜底几何
    let s = nested_groups(GateCapacity::Fixed(1));
    let g = ChannelGraph::from_substrate(&s);
    let mask = ScopeMask::for_ports(&s, port(0), port(1)).unwrap();
    let mut occ = Occupancy::new();

    let first = route(&g, port(0), port(1), &occ, &mask).unwrap();
    assert_eq!(first.status, SolverStatus::Converged);
    occ.commit(&first.tracks, &first.gates);

    let second = route(&g, port(0), port(1), &occ, &mask).unwrap();
    assert_eq!(second.status, SolverStatus::Infeasible);
    assert!(second.tracks.is_empty(), "不可行解不得携带假路径");
}

#[test]
fn full_gate_forces_detour_through_alternative() {
    // 组开两侧 gate：g1 直达（MainLow，容量 1），g2 绕行（MainHigh，绕外框多折点）。
    // 第一条边占满 g1 后，第二条边必须改走 g2 而不是失败。
    let mut s = Substrate::new();
    s.add_group(gid(1), None, (1, 1), (0, 0)).unwrap();
    s.add_track(tid(0), MAIN, Some(gid(1)), 1.0, 0, (3, 3)).unwrap(); // 组内走廊
    s.add_track(tid(1), MAIN, None, 1.0, 0, (0, 2)).unwrap(); // 上方根走廊（目标）
    s.add_track(tid(2), MAIN, None, 1.0, 0, (4, 6)).unwrap(); // 下方根走廊（绕行出口）
    s.add_track(tid(3), CROSS, None, 1.0, 0, (0, 2)).unwrap(); // 顶部横缝
    s.add_track(tid(4), CROSS, None, 1.0, 3, (0, 2)).unwrap(); // 底部横缝
    s.add_track(tid(5), MAIN, None, 1.0, 1, (0, 6)).unwrap(); // 旁路走廊（全高）
    s.link(tid(1), tid(3)).unwrap();
    s.link(tid(5), tid(3)).unwrap();
    s.link(tid(2), tid(4)).unwrap();
    s.link(tid(5), tid(4)).unwrap();
    s.add_gate(gate(1), gid(1), GateSide::MainLow, 1, vec![(tid(0), tid(1))], GateCapacity::Fixed(1))
        .unwrap(); // 直达 Main→Main（同向直穿）
    s.add_gate(gate(2), gid(1), GateSide::MainHigh, 2, vec![(tid(0), tid(2))], GateCapacity::Fixed(1))
        .unwrap(); // 绕行：出下侧再绕外框回来
    attach(&mut s, 0, 0);
    attach(&mut s, 1, 1);

    let g = ChannelGraph::from_substrate(&s);
    let mask = ScopeMask::for_ports(&s, port(0), port(1)).unwrap();
    let mut occ = Occupancy::new();

    let first = route(&g, port(0), port(1), &occ, &mask).unwrap();
    assert_eq!(first.gates, vec![gate(1)], "空载时应选无折点直达 gate");
    assert_eq!(first.cost.q3_bends, 0);
    occ.commit(&first.tracks, &first.gates);

    let second = route(&g, port(0), port(1), &occ, &mask).unwrap();
    assert_eq!(second.status, SolverStatus::Converged);
    assert_eq!(second.gates, vec![gate(2)], "g1 满容后必须绕行 g2");
    assert_eq!(second.tracks, vec![tid(0), tid(2), tid(4), tid(5), tid(3), tid(1)]);
    assert!(second.cost > first.cost, "绕行代价应严格更高");
}

#[test]
fn occupancy_release_restores_feasibility() {
    // 词典序有界回溯的账本前提：commit/release 严格可逆
    let s = nested_groups(GateCapacity::Fixed(1));
    let g = ChannelGraph::from_substrate(&s);
    let mask = ScopeMask::for_ports(&s, port(0), port(1)).unwrap();
    let mut occ = Occupancy::new();

    let first = route(&g, port(0), port(1), &occ, &mask).unwrap();
    occ.commit(&first.tracks, &first.gates);
    assert_eq!(
        route(&g, port(0), port(1), &occ, &mask).unwrap().status,
        SolverStatus::Infeasible
    );

    occ.release(&first.tracks, &first.gates);
    assert_eq!(occ.lane_demand(tid(2)), 0);
    let retry = route(&g, port(0), port(1), &occ, &mask).unwrap();
    assert_eq!(retry.status, SolverStatus::Converged);
    assert_eq!(retry, first, "释放后重路由应与首次完全一致");
}

// ---------------------------------------------------------------------------
// 词典序：高位严格压制低位
// ---------------------------------------------------------------------------

#[test]
fn fewer_bends_beat_shorter_length() {
    // 路径 A：C0 → M_long(span 10) → C2，2 折点、总长 12
    // 路径 B：C0 → M10 → C1 → M11 → C2，4 折点、总长 5
    // 词典序 Q3 > Q4：必须选 A，哪怕长一倍以上
    let mut s = Substrate::new();
    for c in 0..3 {
        s.add_track(tid(c), CROSS, None, 1.0, c as usize, (0, 6)).unwrap();
    }
    s.add_track(tid(10), MAIN, None, 1.0, 0, (0, 2)).unwrap(); // 上半段走廊（C0–C1）
    s.add_track(tid(11), MAIN, None, 1.0, 1, (2, 4)).unwrap(); // 下半段走廊（C1–C2）
    s.add_track(tid(20), MAIN, None, 10.0, 2, (0, 4)).unwrap(); // M_long：直达但贵
    s.link(tid(0), tid(10)).unwrap();
    s.link(tid(1), tid(10)).unwrap();
    s.link(tid(1), tid(11)).unwrap();
    s.link(tid(2), tid(11)).unwrap();
    s.link(tid(0), tid(20)).unwrap();
    s.link(tid(2), tid(20)).unwrap();
    attach(&mut s, 0, 0);
    attach(&mut s, 1, 2);

    let g = ChannelGraph::from_substrate(&s);
    let mask = ScopeMask::for_ports(&s, port(0), port(1)).unwrap();
    let out = route(&g, port(0), port(1), &Occupancy::new(), &mask).unwrap();
    assert_eq!(out.tracks, vec![tid(0), tid(20), tid(2)]);
    assert_eq!(out.cost.q3_bends, 2);
    assert_eq!(out.cost.q4_length.0, 12.0);
}

#[test]
fn equal_bends_prefer_shorter_length() {
    // 折点相同（各 2）时，Q4 生效：选 span 小的走廊
    let mut s = Substrate::new();
    s.add_track(tid(0), CROSS, None, 1.0, 0, (0, 4)).unwrap();
    s.add_track(tid(1), CROSS, None, 1.0, 1, (0, 4)).unwrap();
    s.add_track(tid(10), MAIN, None, 5.0, 0, (0, 2)).unwrap(); // 贵走廊
    s.add_track(tid(11), MAIN, None, 1.0, 1, (0, 2)).unwrap(); // 便宜走廊
    for m in 10..12 {
        s.link(tid(0), tid(m)).unwrap();
        s.link(tid(1), tid(m)).unwrap();
    }
    attach(&mut s, 0, 0);
    attach(&mut s, 1, 1);

    let g = ChannelGraph::from_substrate(&s);
    let mask = ScopeMask::for_ports(&s, port(0), port(1)).unwrap();
    let out = route(&g, port(0), port(1), &Occupancy::new(), &mask).unwrap();
    assert_eq!(out.tracks, vec![tid(0), tid(11), tid(1)]);
}

// ---------------------------------------------------------------------------
// 确定性（AGENTS.md §2）
// ---------------------------------------------------------------------------

#[test]
fn route_is_deterministic_under_exact_ties() {
    // 菱形：M10 / M11 完全等价（同向、同 span、同折点）——严格平局。
    // 要求：重复求解、重建基底后结果逐字段一致，且平局倾向可预期（小 TrackId 先扩展）。
    let build = || {
        let mut s = Substrate::new();
        s.add_track(tid(0), CROSS, None, 1.0, 0, (0, 4)).unwrap();
        s.add_track(tid(1), CROSS, None, 1.0, 1, (0, 4)).unwrap();
        s.add_track(tid(10), MAIN, None, 1.0, 0, (0, 2)).unwrap();
        s.add_track(tid(11), MAIN, None, 1.0, 1, (0, 2)).unwrap();
        for m in 10..12 {
            s.link(tid(0), tid(m)).unwrap();
            s.link(tid(1), tid(m)).unwrap();
        }
        attach(&mut s, 0, 0);
        attach(&mut s, 1, 1);
        s
    };

    let s1 = build();
    let s2 = build();
    let g1 = ChannelGraph::from_substrate(&s1);
    let g2 = ChannelGraph::from_substrate(&s2);
    let mask = ScopeMask::for_ports(&s1, port(0), port(1)).unwrap();
    let occ = Occupancy::new();

    let a = route(&g1, port(0), port(1), &occ, &mask).unwrap();
    let b = route(&g1, port(0), port(1), &occ, &mask).unwrap();
    let c = route(&g2, port(0), port(1), &occ, &mask).unwrap();
    assert_eq!(a, b, "同图重复求解必须逐字段一致");
    assert_eq!(a, c, "重建基底后结果必须一致（不依赖内存布局/哈希序）");
    assert_eq!(a.tracks, vec![tid(0), tid(10), tid(1)], "平局取小 TrackId");
}

#[test]
fn a8_derive_route_deterministic_across_rebuilds() {
    // A8：derive → 全边顺序布线，双跑（含重建基底）逐字段一致
    let run = || {
        let mut bp = ChannelBlueprint::default();
        bp.nodes.insert("I".into(), NodeSpec { rank: 1, order: 1 });
        bp.nodes.insert("J".into(), NodeSpec { rank: 2, order: 1 });
        bp.nodes.insert("M1".into(), NodeSpec { rank: 3, order: 0 });
        bp.nodes.insert("M2".into(), NodeSpec { rank: 3, order: 2 });
        bp.nodes.insert("X".into(), NodeSpec { rank: 5, order: 1 });
        bp.groups.insert(
            "outer".into(),
            GroupSpec {
                members: vec!["M1".into(), "M2".into()],
                parent: None,
            },
        );
        bp.groups.insert(
            "inner".into(),
            GroupSpec {
                members: vec!["I".into(), "J".into()],
                parent: Some("outer".into()),
            },
        );
        bp.edges.push(("I".into(), "X".into()));
        bp.edges.push(("J".into(), "M1".into()));
        bp.edges.push(("M2".into(), "X".into()));

        let (mut s, idx) = derive_substrate(&bp).unwrap();
        let mut ports = Vec::new();
        for (i, (f, t)) in bp.edges.iter().enumerate() {
            let (src, dst) = idx.edge_port_tracks(&bp, f, t).unwrap();
            ports.push((
                attach_edge_slot(&mut s, src, dst, f, t, i as u32),
                f.clone(),
                t.clone(),
            ));
        }
        let g = ChannelGraph::from_substrate(&s);
        let mut occ = Occupancy::new();
        let mut outs = Vec::new();
        for ((pa, pb), f, t) in &ports {
            let mask = idx.scope_mask_for_edge(&s, f, t);
            let out = route(&g, *pa, *pb, &occ, &mask).unwrap();
            occ.commit(&out.tracks, &out.gates);
            outs.push(out);
        }
        outs
    };

    let a = run();
    let b = run();
    assert_eq!(a, b, "重建基底 + 全边重布必须逐字段一致");
    assert!(a.iter().all(|o| o.status == SolverStatus::Converged));
}

// ---------------------------------------------------------------------------
// 边界情形
// ---------------------------------------------------------------------------

#[test]
fn same_host_track_is_trivially_converged() {
    // 两端口同轨：路径 = 单轨道，无 gate、无折点
    let mut s = Substrate::new();
    s.add_track(tid(0), CROSS, None, 1.0, 0, (0, 0)).unwrap();
    attach(&mut s, 0, 0);
    attach(&mut s, 1, 0);

    let g = ChannelGraph::from_substrate(&s);
    let mask = ScopeMask::for_ports(&s, port(0), port(1)).unwrap();
    let out = route(&g, port(0), port(1), &Occupancy::new(), &mask).unwrap();
    assert_eq!(out.status, SolverStatus::Converged);
    assert_eq!(out.tracks, vec![tid(0)]);
    assert!(out.gates.is_empty());
    assert_eq!(out.cost.q3_bends, 0);
}

#[test]
fn self_loop_same_node_is_infeasible() {
    // L7-T5 定案：两端口属同一节点（自环）→ 显式 Infeasible，
    // 不返回单轨道平凡解伪装成功（同宿主轨道也不例外）。
    let mut s = Substrate::new();
    s.add_track(tid(0), CROSS, None, 1.0, 0, (0, 0)).unwrap();
    s.attach_port(port(0), "n0", PortSide::MainLow, 0, tid(0), 0).unwrap();
    s.attach_port(port(1), "n0", PortSide::MainLow, 1, tid(0), 0).unwrap();

    let g = ChannelGraph::from_substrate(&s);
    let mask = ScopeMask::for_ports(&s, port(0), port(1)).unwrap();
    let out = route(&g, port(0), port(1), &Occupancy::new(), &mask).unwrap();
    assert_eq!(out.status, SolverStatus::Infeasible);
    assert!(out.tracks.is_empty() && out.gates.is_empty());

    // 候选版同样不伪造：全部候选对都是自环 → Infeasible。
    let out2 = route_candidates(&g, &[port(0)], &[port(1)], &Occupancy::new(), &mask).unwrap();
    assert_eq!(out2.status, SolverStatus::Infeasible);
}

// ---------------------------------------------------------------------------
// 合流：共享 track 后缀 = bundle（拓扑事实，非几何巧合）
// ---------------------------------------------------------------------------

#[test]
fn shared_suffix_forms_bundle() {
    // e1=[1,5,9]、e2=[2,5,9] 共享后缀 [5,9]；e3=[3,6,9] 只共享 [9]
    let t = |v: &[u32]| -> Vec<TrackId> { v.iter().map(|&x| tid(x)).collect() };
    let s1 = t(&[1, 5, 9]);
    let s2 = t(&[2, 5, 9]);
    let s3 = t(&[3, 6, 9]);
    let paths: Vec<(usize, &[TrackId])> =
        vec![(0, &s1), (1, &s2), (2, &s3)];

    let bundles = detect_bundles(&paths, 2);
    // 唯一长度 ≥2 的共享后缀是 [5,9]，覆盖 e1/e2
    assert_eq!(bundles.len(), 1);
    assert_eq!(bundles[0].suffix, t(&[5, 9]));
    assert_eq!(bundles[0].edges, vec![0, 1]);
}

#[test]
fn hierarchical_suffixes_all_reported() {
    // e1=[1,5,9]、e2=[2,5,9]、e3=[7,9]：层级后缀 [5,9](e1,e2) 与 [9](e1,e2,e3)
    let t = |v: &[u32]| -> Vec<TrackId> { v.iter().map(|&x| tid(x)).collect() };
    let s1 = t(&[1, 5, 9]);
    let s2 = t(&[2, 5, 9]);
    let s3 = t(&[7, 9]);
    let paths: Vec<(usize, &[TrackId])> = vec![(0, &s1), (1, &s2), (2, &s3)];

    let bundles = detect_bundles(&paths, 1);
    // 最长后缀优先：[5,9] 在前，[9] 在后
    assert_eq!(bundles[0].suffix, t(&[5, 9]));
    assert_eq!(bundles[0].edges, vec![0, 1]);
    assert_eq!(bundles[1].suffix, t(&[9]));
    assert_eq!(bundles[1].edges, vec![0, 1, 2]);
}

#[test]
fn disjoint_paths_form_no_bundle() {
    let t = |v: &[u32]| -> Vec<TrackId> { v.iter().map(|&x| tid(x)).collect() };
    let s1 = t(&[1, 2]);
    let s2 = t(&[3, 4]);
    let paths: Vec<(usize, &[TrackId])> = vec![(0, &s1), (1, &s2)];
    assert!(detect_bundles(&paths, 1).is_empty());
}

// ---------------------------------------------------------------------------
// 端口侧容量（I.5 PORT_SIDE_CAPACITY 硬约束）
// ---------------------------------------------------------------------------

#[test]
fn port_side_capacity_limits_concurrent_edges() {
    // 同一挂接点 p0 容量 1：第一条边占用后，第二条共用 p0 的边 → Infeasible
    let mut s = Substrate::new();
    s.add_track(tid(0), CROSS, None, 1.0, 0, (0, 2)).unwrap();
    s.add_track(tid(1), CROSS, None, 1.0, 1, (0, 2)).unwrap();
    s.add_track(tid(10), MAIN, None, 1.0, 0, (0, 2)).unwrap();
    s.link(tid(0), tid(10)).unwrap();
    s.link(tid(1), tid(10)).unwrap();
    s.attach_port(port(0), "src", PortSide::MainLow, 0, tid(0), 1)
        .unwrap(); // 源端口，容量 1
    s.attach_port(port(1), "d1", PortSide::MainLow, 0, tid(1), 0)
        .unwrap(); // 目标 1，不限
    s.attach_port(port(2), "d2", PortSide::MainLow, 0, tid(1), 0)
        .unwrap(); // 目标 2，不限

    let g = ChannelGraph::from_substrate(&s);
    let mask = ScopeMask::for_scopes(&s, None, None);
    let mut occ = Occupancy::new();

    let e1 = route(&g, port(0), port(1), &occ, &mask).unwrap();
    assert_eq!(e1.status, SolverStatus::Converged);
    occ.commit(&e1.tracks, &e1.gates);
    occ.commit_ports(&[port(0), port(1)]);

    // 第二条边共用 p0：端口侧满容 → 显式 Infeasible
    let e2 = route(&g, port(0), port(2), &occ, &mask).unwrap();
    assert_eq!(e2.status, SolverStatus::Infeasible);

    // 释放端口后恢复可布
    occ.release_ports(&[port(0), port(1)]);
    let e3 = route(&g, port(0), port(2), &occ, &mask).unwrap();
    assert_eq!(e3.status, SolverStatus::Converged);
}

// ---------------------------------------------------------------------------
// 推导 adapter：从 rank/order/group 蓝图生成可路由的 Substrate（P0）
// ---------------------------------------------------------------------------

fn spec(rank: usize, order: usize) -> NodeSpec {
    NodeSpec { rank, order }
}

#[test]
fn derive_flat_two_rank_flow_routes() {
    // 无组：A(rank0) → B(rank1)，推导后应能路由（经层间缝 + 走廊）
    let mut bp = ChannelBlueprint::default();
    bp.nodes.insert("A".into(), spec(0, 0));
    bp.nodes.insert("B".into(), spec(1, 0));
    bp.edges.push(("A".into(), "B".into()));

    let (mut s, idx) = derive_substrate(&bp).unwrap();
    let (src, dst) = idx.edge_port_tracks(&bp, "A", "B").unwrap();
    let (pa, pb) = attach_edge(&mut s, src, dst, "A", "B");
    let mask = idx.scope_mask_for_edge(&s, "A", "B");
    let g = ChannelGraph::from_substrate(&s);

    let out = route(&g, pa, pb, &Occupancy::new(), &mask).unwrap();
    assert_eq!(out.status, SolverStatus::Converged);
    // A、B 分列 rank0/rank1 同一 order，都挂到同一条层间缝 → 路径退化为单轨道（合法）。
    // 本测试只验证推导出的无组 Substrate 可路由。
    assert!(!out.tracks.is_empty());
}

#[test]
fn derive_lateral_edge_uses_corridor() {
    // 同 rank 不同 order：A(0,0) → B(0,2)，须经纵向走廊换轨，路径 ≥3 轨道
    let mut bp = ChannelBlueprint::default();
    bp.nodes.insert("A".into(), spec(0, 0));
    bp.nodes.insert("B".into(), spec(0, 2));
    bp.edges.push(("A".into(), "B".into()));

    let (mut s, idx) = derive_substrate(&bp).unwrap();
    let (src, dst) = idx.edge_port_tracks(&bp, "A", "B").unwrap();
    let (pa, pb) = attach_edge(&mut s, src, dst, "A", "B");
    let mask = idx.scope_mask_for_edge(&s, "A", "B");
    let g = ChannelGraph::from_substrate(&s);

    let out = route(&g, pa, pb, &Occupancy::new(), &mask).unwrap();
    assert_eq!(out.status, SolverStatus::Converged);
    assert!(out.tracks.len() >= 3, "横向边须经走廊：{:?}", out.tracks);
    assert_eq!(out.cost.q3_bends, 2, "缝→走廊→缝 两折点");
}

#[test]
fn derive_cross_group_edge_uses_gate() {
    // 组 G={A, A2}（跨两 rank，边界线被切出组内段），边 A(组内) → B(组外)：
    // 必须经 gate 穿出
    let mut bp = ChannelBlueprint::default();
    bp.nodes.insert("A".into(), spec(0, 0));
    bp.nodes.insert("A2".into(), spec(1, 0));
    bp.nodes.insert("B".into(), spec(3, 0));
    bp.groups.insert(
        "G".into(),
        GroupSpec {
            members: vec!["A".into(), "A2".into()],
            parent: None,
        },
    );
    bp.edges.push(("A".into(), "B".into()));

    let (mut s, idx) = derive_substrate(&bp).unwrap();
    let (src, dst) = idx.edge_port_tracks(&bp, "A", "B").unwrap();
    let (pa, pb) = attach_edge(&mut s, src, dst, "A", "B");
    let mask = idx.scope_mask_for_edge(&s, "A", "B");
    let g = ChannelGraph::from_substrate(&s);

    let out = route(&g, pa, pb, &Occupancy::new(), &mask).unwrap();
    assert_eq!(out.status, SolverStatus::Converged, "跨组边必须可路由");
    assert!(!out.gates.is_empty(), "跨组边必须经 gate（H2 由构造保证）");
}

#[test]
fn derive_nested_groups_route_through_gate_chain() {
    // 嵌套：inner ⊂ outer（outer 左右各多一列，环带非零），节点 I 在最内层，
    // 边 I → X(组外) 须穿越两层 gate
    let mut bp = ChannelBlueprint::default();
    bp.nodes.insert("I".into(), spec(1, 1));
    bp.nodes.insert("J".into(), spec(2, 1));
    bp.nodes.insert("M1".into(), spec(3, 0));
    bp.nodes.insert("M2".into(), spec(3, 2));
    bp.nodes.insert("X".into(), spec(5, 1));
    bp.groups.insert(
        "outer".into(),
        GroupSpec {
            members: vec!["M1".into(), "M2".into()],
            parent: None,
        },
    );
    bp.groups.insert(
        "inner".into(),
        GroupSpec {
            members: vec!["I".into(), "J".into()],
            parent: Some("outer".into()),
        },
    );
    bp.edges.push(("I".into(), "X".into()));

    let (mut s, idx) = derive_substrate(&bp).unwrap();
    // L6：合法推导产物无穿组违规
    assert!(s.verify_no_group_penetration().is_empty());

    let (src, dst) = idx.edge_port_tracks(&bp, "I", "X").unwrap();
    let (pa, pb) = attach_edge(&mut s, src, dst, "I", "X");
    let mask = idx.scope_mask_for_edge(&s, "I", "X");
    let g = ChannelGraph::from_substrate(&s);

    let out = route(&g, pa, pb, &Occupancy::new(), &mask).unwrap();
    assert_eq!(out.status, SolverStatus::Converged);
    assert!(
        out.gates.len() >= 2,
        "嵌套组穿出须穿越 ≥2 个 gate，实际 {:?}",
        out.gates
    );
}

#[test]
fn b7_degenerate_group_hosts_on_root_seam() {
    // B7 钉死：单节点组不切任何线、不产生任何 gate；端口宿主 = 边界缝根段。
    // （原 cloud-native 0% 回归场景：跨组边远端 rank 在组区间外，旧实现靠
    // clamp 兜底；段模型下全局线恒有根段，宿主恒有解。）
    let mut bp = ChannelBlueprint::default();
    bp.nodes.insert("A".into(), spec(2, 0));
    bp.nodes.insert("B".into(), spec(0, 0));
    bp.groups.insert(
        "G".into(),
        GroupSpec {
            members: vec!["A".into()],
            parent: None,
        },
    );
    bp.edges.push(("A".into(), "B".into()));

    let (mut s, idx) = derive_substrate(&bp).unwrap();
    assert_eq!(s.gates().count(), 0, "退化组不产生 gate（B7）");
    for segs in idx.cross_lines.values().chain(idx.main_lines.values()) {
        assert_eq!(segs.len(), 1, "退化组不切任何线");
        assert_eq!(segs[0].scope, None, "全部段归根");
    }

    let (src, dst) = idx
        .edge_port_tracks(&bp, "A", "B")
        .expect("退化组端口宿主 = 边界缝根段，恒有解");
    let (pa, pb) = attach_edge(&mut s, src, dst, "A", "B");
    let mask = idx.scope_mask_for_edge(&s, "A", "B");
    let g = ChannelGraph::from_substrate(&s);

    let out = route(&g, pa, pb, &Occupancy::new(), &mask).unwrap();
    assert_eq!(out.status, SolverStatus::Converged);
    assert!(out.gates.is_empty(), "全根路径无 gate 穿越");
}

#[test]
fn derive_container_group_without_direct_members() {
    // 回归 cloud-native：纯容器组 outer 自身无直属节点，只含子组 inner。
    // 组矩形覆盖后代节点，outer 矩形 = inner 包围盒（边界重合 → 穿越折入
    // inner 的 gate），跨组边可路由。
    let mut bp = ChannelBlueprint::default();
    bp.nodes.insert("A".into(), spec(1, 0));
    bp.nodes.insert("A2".into(), spec(2, 0));
    bp.nodes.insert("B".into(), spec(4, 0));
    bp.groups.insert(
        "outer".into(),
        GroupSpec {
            members: vec![], // 纯容器：无直属节点
            parent: None,
        },
    );
    bp.groups.insert(
        "inner".into(),
        GroupSpec {
            members: vec!["A".into(), "A2".into()],
            parent: Some("outer".into()),
        },
    );
    bp.edges.push(("A".into(), "B".into()));

    let (mut s, idx) = derive_substrate(&bp).expect("纯容器组不应报 EmptyGroup");
    let (src, dst) = idx
        .edge_port_tracks(&bp, "A", "B")
        .expect("跨组边端点应可表达");
    let (pa, pb) = attach_edge(&mut s, src, dst, "A", "B");
    let mask = idx.scope_mask_for_edge(&s, "A", "B");
    let g = ChannelGraph::from_substrate(&s);

    let out = route(&g, pa, pb, &Occupancy::new(), &mask).unwrap();
    assert_eq!(out.status, SolverStatus::Converged);
    assert!(!out.gates.is_empty(), "出组须经 gate（边界重合折入 inner 的 gate）");
}

/// L4 场景蓝图：组 G={A, A2}，6 条跨组边 A→B0..B5 全部经 G 的侧 gate 穿出。
fn fan_out_bp() -> ChannelBlueprint {
    let mut bp = ChannelBlueprint::default();
    bp.nodes.insert("A".into(), spec(0, 0));
    bp.nodes.insert("A2".into(), spec(1, 0));
    for i in 0..6 {
        bp.nodes.insert(format!("B{i}"), spec(2, i));
    }
    bp.groups.insert(
        "G".into(),
        GroupSpec {
            members: vec!["A".into(), "A2".into()],
            parent: None,
        },
    );
    for i in 0..6 {
        bp.edges.push(("A".into(), format!("B{i}")));
    }
    bp
}

/// 共享占用贪心布 fan_out_bp 的 6 条边，返回成功数与总 gate 穿越数。
fn route_fan_out(bp: &ChannelBlueprint) -> (u32, u32) {
    let (mut s, idx) = derive_substrate(bp).unwrap();
    let mut pairs = Vec::new();
    for i in 0..6u32 {
        let b = format!("B{i}");
        let (src, dst) = idx.edge_port_tracks(bp, "A", &b).unwrap();
        // 6 条平行边同从 A 的 MainLow 侧出：用 slot_index=i 区分（P-inv-2 要求
        // (node,side,slot) 唯一，slot_index 正是平行边建模机制）。
        let pa = s.alloc_port_id();
        s.attach_port(pa, "A", PortSide::MainLow, i, src, 0).unwrap();
        let pb = s.alloc_port_id();
        s.attach_port(pb, &b, PortSide::MainLow, 0, dst, 0).unwrap();
        pairs.push((pa, pb, b));
    }
    let g = ChannelGraph::from_substrate(&s);

    let mut occ = Occupancy::new();
    let mut routed = 0;
    for (pa, pb, b) in &pairs {
        let mask = idx.scope_mask_for_edge(&s, "A", b);
        if let Ok(out) = route(&g, *pa, *pb, &occ, &mask) {
            if out.status == SolverStatus::Converged {
                occ.commit(&out.tracks, &out.gates);
                occ.commit_ports(&[*pa, *pb]);
                routed += 1;
            }
        }
    }
    let crossings: u32 = s.gates().map(|gt| occ.gate_load(gt.id)).sum();
    (routed, crossings)
}

#[test]
fn derive_unbounded_gates_admit_all_crossings() {
    // L4：生产路径 gate 恒 Unbounded——容量是输出（crossing_demand）不是约束，
    // 6 条跨组边全部成功，穿越数如实入账
    let bp = fan_out_bp();
    let (routed, crossings) = route_fan_out(&bp);
    assert_eq!(routed, 6, "Unbounded 下全部跨组边必须成功");
    assert_eq!(crossings, 6, "gate_load 即 crossing_demand");
}

#[test]
fn derive_fixed_override_is_diagnostic_constraint() {
    // L4 诊断口径：gate_capacity_override=Some(1) → 每 gate 容量 1。
    // G 只有 CrossLow/CrossHigh 两个 gate（单列组不切 Main 线）→ 恰 2 条边成功
    let mut bp = fan_out_bp();
    bp.gate_capacity_override = Some(1);
    let (routed, _) = route_fan_out(&bp);
    assert_eq!(routed, 2, "Fixed(1) × 2 个侧 gate → 恰 2 条边可穿出");
}

#[test]
fn derive_rejects_unknown_node() {
    let mut bp = ChannelBlueprint::default();
    bp.nodes.insert("A".into(), spec(0, 0));
    bp.edges.push(("A".into(), "Ghost".into()));
    assert!(matches!(
        derive_substrate(&bp),
        Err(DeriveError::UnknownNodeInEdge { .. })
    ));
}

#[test]
fn derive_rejects_overlapping_group_rects() {
    // 兄弟组包围盒相交非嵌套：G1 ranks(0,5)×orders(0,2) ∩ G2 ranks(3,8)×orders(1,3)
    // → 破坏切割模型嵌套树前提，构建期拒绝（而非留给 L6 事后报违规）
    let mut bp = ChannelBlueprint::default();
    bp.nodes.insert("A".into(), spec(0, 0));
    bp.nodes.insert("B".into(), spec(5, 2));
    bp.nodes.insert("C".into(), spec(3, 1));
    bp.nodes.insert("D".into(), spec(8, 3));
    bp.groups.insert(
        "G1".into(),
        GroupSpec {
            members: vec!["A".into(), "B".into()],
            parent: None,
        },
    );
    bp.groups.insert(
        "G2".into(),
        GroupSpec {
            members: vec!["C".into(), "D".into()],
            parent: None,
        },
    );
    assert!(matches!(
        derive_substrate(&bp),
        Err(DeriveError::OverlappingGroups { .. })
    ));

    // 矩形包含但无祖先关系同样拒绝；改成真子组则合法
    let mut bp2 = ChannelBlueprint::default();
    bp2.nodes.insert("O".into(), spec(0, 0));
    bp2.nodes.insert("P".into(), spec(9, 9));
    bp2.nodes.insert("X".into(), spec(2, 2));
    bp2.nodes.insert("Y".into(), spec(3, 3));
    bp2.groups.insert(
        "Big".into(),
        GroupSpec {
            members: vec!["O".into(), "P".into()],
            parent: None,
        },
    );
    bp2.groups.insert(
        "Small".into(),
        GroupSpec {
            members: vec!["X".into(), "Y".into()],
            parent: None,
        },
    );
    assert!(matches!(
        derive_substrate(&bp2),
        Err(DeriveError::OverlappingGroups { .. })
    ));
    bp2.groups.get_mut("Small").unwrap().parent = Some("Big".into());
    let (s, _idx) = derive_substrate(&bp2).expect("真嵌套（祖先关系）合法");
    assert!(s.verify_no_group_penetration().is_empty());
}

// ---------------------------------------------------------------------------
// A9 切割规则：B1–B8 逐条边界用例
// ---------------------------------------------------------------------------

#[test]
fn a9_cut_rules_segment_structure_b1_b2_b4_b8() {
    // O(0,0)/P(3,3) 撑开 4×4 网格；G={A(1,1), B(2,2)} → 矩形 ranks(1,2)×orders(1,2)
    let mut bp = ChannelBlueprint::default();
    bp.nodes.insert("O".into(), spec(0, 0));
    bp.nodes.insert("A".into(), spec(1, 1));
    bp.nodes.insert("B".into(), spec(2, 2));
    bp.nodes.insert("P".into(), spec(3, 3));
    bp.groups.insert(
        "G".into(),
        GroupSpec {
            members: vec!["A".into(), "B".into()],
            parent: None,
        },
    );
    let (s, idx) = derive_substrate(&bp).unwrap();
    let g = idx.group_ids["G"];

    // B1：只有内部 Cross 线（rank-gap 2，r0=1 < 2 ≤ r1=2）被切成三段；
    // 边界线 1/3 不切、保持整段（一次切完，B4）
    let cut: Vec<_> = idx.cross_lines[&2].iter().map(|sg| (sg.ext, sg.scope)).collect();
    assert_eq!(cut, vec![((0, 2), None), ((3, 5), Some(g)), ((6, 8), None)]);
    assert_eq!(idx.cross_lines[&1].len(), 1);
    assert_eq!(idx.cross_lines[&1][0].ext, (0, 8));
    assert_eq!(idx.cross_lines[&3].len(), 1);
    // B2：Main 线对称（order-gap 2 被切）
    let mcut: Vec<_> = idx.main_lines[&2].iter().map(|sg| (sg.ext, sg.scope)).collect();
    assert_eq!(mcut, vec![((0, 2), None), ((3, 5), Some(g)), ((6, 8), None)]);
    // B8：边界缝 gap 坐标（2 / 6）落在组外段 ext 内，组内段 (3,5) 只含 gap 坐标 4

    // L3：span_weight = 段覆盖 gap 数（ext 内偶坐标个数）
    let w = |t: TrackId| s.track(t).unwrap().span_weight;
    assert_eq!(w(idx.cross_lines[&2][0].id), 2.0);
    assert_eq!(w(idx.cross_lines[&2][1].id), 1.0);
    assert_eq!(w(idx.cross_lines[&2][2].id), 2.0);
    assert_eq!(w(idx.cross_lines[&1][0].id), 5.0);

    // L6：合法推导产物全通过
    assert!(s.verify_no_group_penetration().is_empty());
}

#[test]
fn b6_shared_seam_between_adjacent_groups_is_root_segment() {
    // G1 列 1、G2 列 2 相邻：共享列缝（order-gap 2，坐标 4）独立成根段，
    // scope 取共同父（此处为根）
    let mut bp = ChannelBlueprint::default();
    bp.nodes.insert("O".into(), spec(0, 0));
    bp.nodes.insert("A".into(), spec(1, 1));
    bp.nodes.insert("A2".into(), spec(2, 1));
    bp.nodes.insert("B".into(), spec(1, 2));
    bp.nodes.insert("B2".into(), spec(2, 2));
    bp.nodes.insert("P".into(), spec(3, 3));
    bp.groups.insert(
        "G1".into(),
        GroupSpec {
            members: vec!["A".into(), "A2".into()],
            parent: None,
        },
    );
    bp.groups.insert(
        "G2".into(),
        GroupSpec {
            members: vec!["B".into(), "B2".into()],
            parent: None,
        },
    );
    let (s, idx) = derive_substrate(&bp).unwrap();
    let g1 = idx.group_ids["G1"];
    let g2 = idx.group_ids["G2"];

    // Cross 线 rank-gap 2 同时被两组切开：五段，共享缝 (4,4) 独立、归根（B6）
    let cut: Vec<_> = idx.cross_lines[&2].iter().map(|sg| (sg.ext, sg.scope)).collect();
    assert_eq!(
        cut,
        vec![
            ((0, 2), None),
            ((3, 3), Some(g1)),
            ((4, 4), None), // 共享缝：独立一段，scope 取共同父
            ((5, 5), Some(g2)),
            ((6, 8), None),
        ]
    );
    // 单列组的组内段 cover 为空（无偶坐标）：span_weight 取下限 1（L3）
    assert_eq!(s.track(idx.cross_lines[&2][1].id).unwrap().span_weight, 1.0);
    assert!(s.verify_no_group_penetration().is_empty());
}

#[test]
fn a10_full_width_group_outer_edge_stays_outside() {
    // A10：组横跨全部列（orders 0..2 = 全列）。两端在组外、分居组上下的边
    // 仍 Converged（走外框根走廊），且路径不含该组任何段、无 gate 穿越。
    let mut bp = ChannelBlueprint::default();
    bp.nodes.insert("T".into(), spec(0, 1));
    bp.nodes.insert("M1".into(), spec(1, 0));
    bp.nodes.insert("M2".into(), spec(1, 2));
    bp.nodes.insert("Bo".into(), spec(2, 1));
    bp.groups.insert(
        "G".into(),
        GroupSpec {
            members: vec!["M1".into(), "M2".into()],
            parent: None,
        },
    );
    bp.edges.push(("T".into(), "Bo".into()));

    let (mut s, idx) = derive_substrate(&bp).unwrap();
    let (src, dst) = idx.edge_port_tracks(&bp, "T", "Bo").unwrap();
    let (pa, pb) = attach_edge(&mut s, src, dst, "T", "Bo");
    let mask = idx.scope_mask_for_edge(&s, "T", "Bo");
    let g = ChannelGraph::from_substrate(&s);

    let out = route(&g, pa, pb, &Occupancy::new(), &mask).unwrap();
    assert_eq!(out.status, SolverStatus::Converged, "组横跨全列时外部边走外框");
    assert!(out.gates.is_empty(), "不得借道穿组");
    for t in &out.tracks {
        assert_eq!(s.track(*t).unwrap().scope, None, "路径不含该组任何段");
    }
}

// ---------------------------------------------------------------------------
// L6 检查器：合法基底全通过、注入越界段报违规
// ---------------------------------------------------------------------------

#[test]
fn l6_detects_injected_penetrating_segment() {
    let mut s = Substrate::new();
    s.add_group(gid(1), None, (0, 1), (0, 1)).unwrap();
    // 越界段：Cross 线 rank-gap 1 在组内部（0 < 1 ≤ 1），延展 (0,4) 与组内部
    // 列区间 (1,3) 相交 → 违规
    s.add_track(tid(0), CROSS, None, 1.0, 1, (0, 4)).unwrap();
    // 合法段：同线但延展 (0,0) 避开组内部列区间
    s.add_track(tid(1), CROSS, None, 1.0, 1, (0, 0)).unwrap();

    let v = s.verify_no_group_penetration();
    assert_eq!(v.len(), 1, "恰报一条违规：{v:?}");
    assert_eq!(v[0].track, tid(0));
    assert_eq!(v[0].group, gid(1));
}

// ---------------------------------------------------------------------------
// 24 号文 R1–R5：端口语义身份 / 候选端点选路（T1–T11）
// ---------------------------------------------------------------------------

#[test]
fn t1_attach_node_port_writes_semantic_identity() {
    // T1：attach_node_port 写入 node/side/slot_index，port(id) 字段完整
    let mut s = Substrate::new();
    s.add_track(tid(0), CROSS, None, 1.0, 0, (0, 2)).unwrap();
    s.attach_node_port(port(7), "nodeA", PortSide::MainHigh, 2, tid(0), 4)
        .unwrap();
    let p = s.port(port(7)).expect("端口应已注册");
    assert_eq!(p.id, port(7));
    assert_eq!(p.node, "nodeA");
    assert_eq!(p.side, PortSide::MainHigh);
    assert_eq!(p.slot_index, 2);
    assert_eq!(p.track, tid(0));
    assert_eq!(p.capacity, 4);
}

#[test]
fn t2_duplicate_node_side_slot_rejected() {
    // T2：重复 (node, side, slot_index) → Err，且不覆盖原端口（P-inv-2）
    let mut s = Substrate::new();
    s.add_track(tid(0), CROSS, None, 1.0, 0, (0, 2)).unwrap();
    s.add_track(tid(1), CROSS, None, 1.0, 1, (0, 2)).unwrap();
    s.attach_port(port(0), "A", PortSide::MainLow, 0, tid(0), 0)
        .unwrap();
    let err = s
        .attach_port(port(1), "A", PortSide::MainLow, 0, tid(1), 0)
        .unwrap_err();
    assert!(matches!(err, SubstrateError::DuplicatePortSlot { .. }));
    // 原端口未被覆盖
    assert_eq!(s.port(port(0)).unwrap().track, tid(0));
    assert!(s.port(port(1)).is_none(), "重复注册不得写入新端口");
}

#[test]
fn t2b_port_side_mismatch_rejected() {
    // P-inv-4：MainLow/High 须挂 Cross 轨道，挂到 Main 轨道 → PortSideMismatch
    let mut s = Substrate::new();
    s.add_track(tid(0), MAIN, None, 1.0, 0, (0, 2)).unwrap();
    let err = s
        .attach_port(port(0), "A", PortSide::MainLow, 0, tid(0), 0)
        .unwrap_err();
    assert!(matches!(err, SubstrateError::PortSideMismatch { .. }));
    // CrossLow 挂 Main 轨道则相容
    s.attach_port(port(1), "A", PortSide::CrossLow, 0, tid(0), 0)
        .unwrap();
}

#[test]
fn t3_ports_of_node_deterministic_order() {
    // T3：ports_of_node / ports_of_node_side 顺序对同一输入多次一致
    let mut s = Substrate::new();
    s.add_track(tid(0), CROSS, None, 1.0, 0, (0, 2)).unwrap();
    s.add_track(tid(1), MAIN, None, 1.0, 0, (0, 2)).unwrap();
    // 乱序挂接：侧与 slot 交错
    s.attach_port(port(0), "A", PortSide::CrossLow, 0, tid(1), 0)
        .unwrap();
    s.attach_port(port(1), "A", PortSide::MainLow, 1, tid(0), 0)
        .unwrap();
    s.attach_port(port(2), "A", PortSide::MainLow, 0, tid(0), 0)
        .unwrap();

    let first: Vec<PortSlotId> = s.ports_of_node("A").iter().map(|p| p.id).collect();
    for _ in 0..5 {
        let again: Vec<PortSlotId> = s.ports_of_node("A").iter().map(|p| p.id).collect();
        assert_eq!(first, again, "ports_of_node 顺序必须可复现");
    }
    // ports_of_node_side 按 slot_index 升序：slot0(port2) 先于 slot1(port1)
    let side: Vec<PortSlotId> = s
        .ports_of_node_side("A", PortSide::MainLow)
        .iter()
        .map(|p| p.id)
        .collect();
    assert_eq!(side, vec![port(2), port(1)]);
    // find_port 精确命中
    assert_eq!(s.find_port("A", PortSide::MainLow, 0).map(|p| p.id), Some(port(2)));
    assert!(s.find_port("A", PortSide::MainHigh, 0).is_none());
}

#[test]
fn t4_resolve_host_track_four_sides_and_boundary() {
    // T4：resolve_host_track 四侧与文档表一致；未知节点 / 越界线 → None。
    // 段按节点体位置取（cross_at(rg, order) / main_at(og, rank)），全局线恒有根段。
    let mut bp = ChannelBlueprint::default();
    bp.nodes.insert("A".into(), spec(0, 0));
    bp.nodes.insert("B".into(), spec(1, 1));
    let (_s, idx) = derive_substrate(&bp).unwrap();

    // A(0,0)：四侧与文档表一致（MainLow=cross[rank]、MainHigh=cross[rank+1]、
    // CrossLow=main[order]、CrossHigh=main[order+1]）
    assert_eq!(idx.resolve_host_track(&bp, "A", PortSide::MainLow), idx.cross_at(0, 0));
    assert_eq!(idx.resolve_host_track(&bp, "A", PortSide::MainHigh), idx.cross_at(1, 0));
    assert_eq!(idx.resolve_host_track(&bp, "A", PortSide::CrossLow), idx.main_at(0, 0));
    assert_eq!(idx.resolve_host_track(&bp, "A", PortSide::CrossHigh), idx.main_at(1, 0));
    assert!(idx.resolve_host_track(&bp, "A", PortSide::MainLow).is_some());

    // B(1,1)：四侧同样可解析（全局线覆盖到 rank+1 / order+1）
    assert_eq!(idx.resolve_host_track(&bp, "B", PortSide::MainLow), idx.cross_at(1, 1));
    assert_eq!(idx.resolve_host_track(&bp, "B", PortSide::MainHigh), idx.cross_at(2, 1));
    assert_eq!(idx.resolve_host_track(&bp, "B", PortSide::CrossLow), idx.main_at(1, 1));
    assert_eq!(idx.resolve_host_track(&bp, "B", PortSide::CrossHigh), idx.main_at(2, 1));
    assert!(idx.resolve_host_track(&bp, "B", PortSide::CrossHigh).is_some());

    // 未知节点 → None
    assert_eq!(idx.resolve_host_track(&bp, "Ghost", PortSide::MainLow), None);
    // 越界线 → None（resolve_host_track 的底层机制：委托 cross_at/main_at）
    assert_eq!(idx.cross_at(9999, 0), None);
    assert_eq!(idx.main_at(9999, 0), None);
}

#[test]
fn t5_derive_auto_attaches_node_ports() {
    // T5：derive + 自动挂端口；每节点挂上存在的侧，node_ports 可查。
    // 全局线恒有根段使已声明节点四侧均可解析，故默认选项下每节点挂 4 侧；
    // 「跳过某侧」由 options.sides 过滤驱动（真实可达路径），resolve 返回 None 亦跳过（防御）。
    let mut bp = ChannelBlueprint::default();
    bp.nodes.insert("A".into(), spec(0, 0));
    bp.nodes.insert("B".into(), spec(1, 0));
    bp.edges.push(("A".into(), "B".into()));

    let (mut s, mut idx) = derive_substrate(&bp).unwrap();
    derive_node_ports(&mut s, &bp, &mut idx, &DerivePortsOptions::default()).unwrap();

    // 默认四侧：A、B 各挂 4 个端口
    assert_eq!(idx.node_ports.get("A").map(Vec::len), Some(4));
    assert_eq!(idx.node_ports.get("B").map(Vec::len), Some(4));
    // node_ports 中的 id 都能在 substrate 查到，且 node 字段回填正确
    for (node, ids) in &idx.node_ports {
        for &id in ids {
            assert_eq!(s.port(id).unwrap().node, *node);
        }
    }
    // find_port 能定位自动挂的端口
    assert!(s.find_port("A", PortSide::MainLow, 0).is_some());
    assert!(s.find_port("B", PortSide::CrossHigh, 0).is_some());

    // 只挂部分侧：sides 过滤生效，每节点只挂指定侧
    let (mut s2, mut idx2) = derive_substrate(&bp).unwrap();
    let partial = DerivePortsOptions {
        enabled: true,
        default_capacity: 4,
        sides: vec![PortSide::MainLow, PortSide::CrossHigh],
    };
    derive_node_ports(&mut s2, &bp, &mut idx2, &partial).unwrap();
    assert_eq!(idx2.node_ports.get("A").map(Vec::len), Some(2));
    assert!(s2.find_port("A", PortSide::MainLow, 0).is_some());
    assert!(s2.find_port("A", PortSide::CrossHigh, 0).is_some());
    assert!(s2.find_port("A", PortSide::MainHigh, 0).is_none(), "未指定侧不应挂接");

    // enabled=false 时不挂任何端口
    let (mut s3, mut idx3) = derive_substrate(&bp).unwrap();
    let off = DerivePortsOptions {
        enabled: false,
        ..DerivePortsOptions::default()
    };
    derive_node_ports(&mut s3, &bp, &mut idx3, &off).unwrap();
    assert!(idx3.node_ports.is_empty());
}

#[test]
fn t7_route_candidates_prefers_lower_lexcost() {
    // T7：两对均可行时选 LexCost 更优；平局确定性（升序遍历 + 严格 <）
    let mut s = Substrate::new();
    s.add_track(tid(0), CROSS, None, 1.0, 0, (0, 2)).unwrap(); // C0
    s.add_track(tid(1), CROSS, None, 1.0, 1, (0, 2)).unwrap(); // C1
    s.add_track(tid(10), MAIN, None, 1.0, 0, (0, 2)).unwrap(); // 走廊
    s.link(tid(0), tid(10)).unwrap();
    s.link(tid(1), tid(10)).unwrap();
    // 源候选：p0 挂 C0（直达目标），p1 挂 C1（须经走廊，多折点）
    s.attach_port(port(0), "src", PortSide::MainLow, 0, tid(0), 0).unwrap();
    s.attach_port(port(1), "src", PortSide::MainLow, 1, tid(1), 0).unwrap();
    // 目标：p2 挂 C0
    s.attach_port(port(2), "dst", PortSide::MainLow, 0, tid(0), 0).unwrap();

    let g = ChannelGraph::from_substrate(&s);
    let mask = ScopeMask::for_scopes(&s, None, None);
    let out = route_candidates(&g, &[port(0), port(1)], &[port(2)], &Occupancy::new(), &mask).unwrap();
    assert_eq!(out.status, SolverStatus::Converged);
    // p0→p2 同 track C0：0 折点、长度 1，严格优于 p1→p2（2 折点）
    assert_eq!(out.tracks, vec![tid(0)]);
    assert_eq!(out.cost.q3_bends, 0);

    // 平局确定性：候选乱序传入，结果不变（内部升序去重）
    let out2 = route_candidates(&g, &[port(1), port(0)], &[port(2)], &Occupancy::new(), &mask).unwrap();
    assert_eq!(out2.tracks, out.tracks);
    assert_eq!(out2.cost, out.cost);
}

#[test]
fn t8_route_candidates_skips_full_port() {
    // T8：候选中一端满容 → 该对跳过，另一对仍可成功
    let mut s = Substrate::new();
    s.add_track(tid(0), CROSS, None, 1.0, 0, (0, 2)).unwrap();
    s.add_track(tid(1), CROSS, None, 1.0, 1, (0, 2)).unwrap();
    s.add_track(tid(10), MAIN, None, 1.0, 0, (0, 2)).unwrap();
    s.link(tid(0), tid(10)).unwrap();
    s.link(tid(1), tid(10)).unwrap();
    s.attach_port(port(0), "src", PortSide::MainLow, 0, tid(0), 1).unwrap(); // 容量 1
    s.attach_port(port(1), "src", PortSide::MainLow, 1, tid(0), 0).unwrap(); // 不限
    s.attach_port(port(2), "dst", PortSide::MainLow, 0, tid(1), 0).unwrap();

    let g = ChannelGraph::from_substrate(&s);
    let mask = ScopeMask::for_scopes(&s, None, None);
    let mut occ = Occupancy::new();
    occ.commit_ports(&[port(0)]); // p0 满容

    let out = route_candidates(&g, &[port(0), port(1)], &[port(2)], &occ, &mask).unwrap();
    assert_eq!(out.status, SolverStatus::Converged, "p0 满容应跳过、p1 仍可布");
    // p1→p2 经走廊：C0 → M10 → C1
    assert_eq!(out.tracks, vec![tid(0), tid(10), tid(1)]);
}

#[test]
fn t9_route_candidates_all_infeasible() {
    // T9：全部候选满容/无路 → Infeasible，空 tracks，不伪造路径
    let mut s = Substrate::new();
    s.add_track(tid(0), CROSS, None, 1.0, 0, (0, 2)).unwrap();
    s.add_track(tid(1), CROSS, None, 1.0, 1, (0, 2)).unwrap();
    s.attach_port(port(0), "src", PortSide::MainLow, 0, tid(0), 1).unwrap();
    s.attach_port(port(1), "src", PortSide::MainLow, 1, tid(0), 1).unwrap();
    s.attach_port(port(2), "dst", PortSide::MainLow, 0, tid(1), 0).unwrap();

    let g = ChannelGraph::from_substrate(&s);
    let mask = ScopeMask::for_scopes(&s, None, None);
    let mut occ = Occupancy::new();
    occ.commit_ports(&[port(0), port(1)]); // 两个源候选都满容

    let out = route_candidates(&g, &[port(0), port(1)], &[port(2)], &occ, &mask).unwrap();
    assert_eq!(out.status, SolverStatus::Infeasible);
    assert!(out.tracks.is_empty(), "不可行解不得携带假路径");
    assert!(out.gates.is_empty());
}

#[test]
fn t10_commit_release_ports_reversible() {
    // T10：commit_ports / release_ports 与现有可逆语义一致（含新挂的 port）
    let mut s = Substrate::new();
    s.add_track(tid(0), CROSS, None, 1.0, 0, (0, 2)).unwrap();
    s.add_track(tid(1), CROSS, None, 1.0, 1, (0, 2)).unwrap();
    s.add_track(tid(10), MAIN, None, 1.0, 0, (0, 2)).unwrap();
    s.link(tid(0), tid(10)).unwrap();
    s.link(tid(1), tid(10)).unwrap();
    s.attach_port(port(0), "src", PortSide::MainLow, 0, tid(0), 1).unwrap();
    s.attach_port(port(1), "dst", PortSide::MainLow, 0, tid(1), 0).unwrap();

    let g = ChannelGraph::from_substrate(&s);
    let mask = ScopeMask::for_scopes(&s, None, None);
    let mut occ = Occupancy::new();

    let e1 = route(&g, port(0), port(1), &occ, &mask).unwrap();
    assert_eq!(e1.status, SolverStatus::Converged);
    occ.commit(&e1.tracks, &e1.gates);
    occ.commit_ports(&[port(0), port(1)]);

    // p0 满容 → 第二条边不可布
    assert_eq!(route(&g, port(0), port(1), &occ, &mask).unwrap().status, SolverStatus::Infeasible);

    // 释放端口后恢复可布
    occ.release_ports(&[port(0), port(1)]);
    assert_eq!(occ.port_load(port(0)), 0);
    let retry = route(&g, port(0), port(1), &occ, &mask).unwrap();
    assert_eq!(retry.status, SolverStatus::Converged);
    assert_eq!(retry, e1, "释放后重路由应与首次完全一致");
}

#[test]
fn t11_empty_candidates_rejected() {
    // T11：空候选切片 → 明确 Err(EmptyCandidates)，不 panic
    let s = plain_grid();
    let g = ChannelGraph::from_substrate(&s);
    let mask = ScopeMask::for_scopes(&s, None, None);
    let occ = Occupancy::new();
    assert_eq!(
        route_candidates(&g, &[], &[port(1)], &occ, &mask).unwrap_err(),
        EndpointError::EmptyCandidates
    );
    assert_eq!(
        route_candidates(&g, &[port(0)], &[], &occ, &mask).unwrap_err(),
        EndpointError::EmptyCandidates
    );
    assert_eq!(
        route_candidates(&g, &[], &[], &occ, &mask).unwrap_err(),
        EndpointError::EmptyCandidates
    );
}

#[test]
fn t7b_route_node_sides_convenience() {
    // R3 便利 API：按节点×侧候选集选路；derive 自动挂端口后 A→B 横向边可布
    let mut bp = ChannelBlueprint::default();
    bp.nodes.insert("A".into(), spec(0, 0));
    bp.nodes.insert("B".into(), spec(0, 2));
    bp.edges.push(("A".into(), "B".into()));

    let (mut s, mut idx) = derive_substrate(&bp).unwrap();
    derive_node_ports(&mut s, &bp, &mut idx, &DerivePortsOptions::default()).unwrap();
    let mask = idx.scope_mask_for_edge(&s, "A", "B");
    let g = ChannelGraph::from_substrate(&s);

    let out = route_node_sides(
        &g,
        "A",
        &[PortSide::CrossLow, PortSide::CrossHigh],
        "B",
        &[PortSide::CrossLow, PortSide::CrossHigh],
        &Occupancy::new(),
        &mask,
    )
    .unwrap();
    assert_eq!(out.status, SolverStatus::Converged, "横向边候选侧选路应可布");
    assert!(!out.tracks.is_empty());

    // 无匹配端口的节点 → EmptyCandidates
    let err = route_node_sides(
        &g,
        "Ghost",
        &[PortSide::MainLow],
        "B",
        &[PortSide::MainLow],
        &Occupancy::new(),
        &mask,
    );
    assert_eq!(err.unwrap_err(), EndpointError::EmptyCandidates);
}

// ---------------------------------------------------------------------------
// 路径级作用域自反证（verify_route_scope，22 号文 §8 证明义务）
// ---------------------------------------------------------------------------

#[test]
fn verifier_passes_genuine_nested_group_route() {
    // 真实 route 产出（嵌套组穿出，≥ 2 gate）必须全过独立验证器：
    // 场景同 derive_nested_groups_route_through_gate_chain
    let mut bp = ChannelBlueprint::default();
    bp.nodes.insert("I".into(), spec(1, 1));
    bp.nodes.insert("J".into(), spec(2, 1));
    bp.nodes.insert("M1".into(), spec(3, 0));
    bp.nodes.insert("M2".into(), spec(3, 2));
    bp.nodes.insert("X".into(), spec(5, 1));
    bp.groups.insert(
        "outer".into(),
        GroupSpec {
            members: vec!["M1".into(), "M2".into()],
            parent: None,
        },
    );
    bp.groups.insert(
        "inner".into(),
        GroupSpec {
            members: vec!["I".into(), "J".into()],
            parent: Some("outer".into()),
        },
    );
    bp.edges.push(("I".into(), "X".into()));

    let (mut s, idx) = derive_substrate(&bp).unwrap();
    let (src, dst) = idx.edge_port_tracks(&bp, "I", "X").unwrap();
    let (pa, pb) = attach_edge(&mut s, src, dst, "I", "X");
    let mask = idx.scope_mask_for_edge(&s, "I", "X");
    let g = ChannelGraph::from_substrate(&s);

    let out = route(&g, pa, pb, &Occupancy::new(), &mask).unwrap();
    assert_eq!(out.status, SolverStatus::Converged);
    assert!(out.gates.len() >= 2);

    let v = verify_route_scope(
        &s,
        &out.tracks,
        &out.gates,
        idx.node_scope("I"),
        idx.node_scope("X"),
    );
    assert!(v.is_empty(), "真实路径必须自反证全过：{v:?}");

    // 同一路径伪造成「与两端无关的组」视角（两端都当根）：
    // 组内轨道全部变成借道 → 必须报 ForeignScope
    let v = verify_route_scope(&s, &out.tracks, &out.gates, None, None);
    assert!(
        v.iter()
            .any(|x| matches!(x, RouteScopeViolation::ForeignScope { .. })),
        "无关组视角下同路径必须被判借道：{v:?}"
    );

    // Infeasible 空路径自然通过
    assert!(verify_route_scope(&s, &[], &[], None, None).is_empty());
}

#[test]
fn verifier_flags_borrowed_passage_and_gate_inconsistencies() {
    // 手工基底：组 g1 内一段 t1，根 scope 两段 t0/t2（不经 route，
    // 直接喂伪造路径——验证器不依赖搜索器才能判违规）
    let mut s = Substrate::new();
    s.add_group(gid(1), None, (0, 1), (0, 1)).unwrap();
    s.add_track(tid(0), CROSS, None, 1.0, 0, (0, 0)).unwrap();
    s.add_track(tid(1), CROSS, Some(gid(1)), 1.0, 1, (1, 3))
        .unwrap();
    s.add_track(tid(2), CROSS, None, 1.0, 3, (0, 0)).unwrap();

    // 两端均在根：t1 属无关组 → 借道；scope 变化 ×2 无 gate → MissingGate ×2
    let v = verify_route_scope(&s, &[tid(0), tid(1), tid(2)], &[], None, None);
    assert_eq!(
        v,
        vec![
            RouteScopeViolation::ForeignScope {
                track: tid(1),
                scope: gid(1)
            },
            RouteScopeViolation::MissingGate {
                from: tid(0),
                to: tid(1)
            },
            RouteScopeViolation::MissingGate {
                from: tid(1),
                to: tid(2)
            },
        ]
    );

    // 端点在组内（u_scope = g1）：t1 合法，但 gate 序列不存在的闸口
    // 配不上轨道对 → GateMismatch；多余闸口 → UnexpectedGate
    let v = verify_route_scope(
        &s,
        &[tid(0), tid(1)],
        &[gate(9), gate(8)],
        Some(gid(1)),
        None,
    );
    assert_eq!(
        v,
        vec![
            RouteScopeViolation::GateMismatch {
                gate: gate(9),
                from: tid(0),
                to: tid(1)
            },
            RouteScopeViolation::UnexpectedGate(gate(8)),
        ]
    );

    // 未知轨道：只报解析违规，不继续失真的转移检查
    let v = verify_route_scope(&s, &[tid(0), tid(42)], &[], None, None);
    assert_eq!(v, vec![RouteScopeViolation::UnknownTrack(tid(42))]);
}
