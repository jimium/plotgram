//! Plan IR 单测：指纹稳定/敏感、diff、RouteOutcome 收录、合流、serde 往返。

use super::diff::{Change, diff};
use super::{
    EdgePorts, GroupScopeSpec, Plan, PlanError, PortRef, Provenance, Slot, SubstrateSketch,
};
use crate::layout::atlas::channel::{
    Bundle, ChannelGraph, Occupancy, PortSide, PortSlotId, ScopeMask, Substrate, TrackId,
    TrackOrient, detect_bundles, route,
};
use crate::layout::kernel::cost::SolverStatus;

fn tid(n: u32) -> TrackId {
    TrackId(n)
}
fn port(n: u32) -> PortSlotId {
    PortSlotId(n)
}
fn slot(rank: usize, order: usize) -> Slot {
    Slot { rank, order }
}
fn port_ref(node: &str, side: PortSide, slot_index: u32) -> PortRef {
    PortRef {
        node: node.into(),
        side,
        slot_index,
        slot_id: None,
        side_order: 0,
        along_offset: 0.0,
    }
}

/// 手工构造一个覆盖全字段的小 Plan（两节点一组一边一束）。
fn sample_plan() -> Plan {
    let mut p = Plan {
        substrate: SubstrateSketch {
            rank_count: 2,
            order_count: 2,
        },
        ..Plan::default()
    };
    p.node_slots.insert("a".into(), slot(0, 0));
    p.node_slots.insert("b".into(), slot(1, 1));
    p.group_scopes.insert(
        "g".into(),
        GroupScopeSpec {
            parent: None,
            ranks: (0, 1),
            orders: (1, 1),
        },
    );
    p.ports.insert(
        0,
        EdgePorts {
            from: port_ref("a", PortSide::MainHigh, 0),
            to: port_ref("b", PortSide::MainLow, 0),
        },
    );
    p.channels.insert(0, vec![tid(0), tid(1)]);
    p.gates.insert(0, vec![]);
    p.bundles.push(Bundle {
        edges: vec![0, 1],
        suffix: vec![tid(1)],
    });
    p.provenance.insert(0, Provenance::Manual);
    p
}

// ---------------------------------------------------------------------------
// 指纹：稳定性与敏感性
// ---------------------------------------------------------------------------

#[test]
fn fingerprint_is_insertion_order_independent() {
    // 同内容、不同插入顺序 → 指纹相等（BTreeMap 归一 + 规范编码）
    let a = sample_plan();
    let mut b = Plan {
        substrate: a.substrate.clone(),
        ..Plan::default()
    };
    // 逆序插入
    b.node_slots.insert("b".into(), slot(1, 1));
    b.node_slots.insert("a".into(), slot(0, 0));
    b.group_scopes = a.group_scopes.clone();
    b.ports = a.ports.clone();
    b.channels = a.channels.clone();
    b.gates = a.gates.clone();
    b.bundles = a.bundles.clone();
    b.provenance = a.provenance.clone();

    assert_eq!(a.fingerprint(), b.fingerprint());
    assert_eq!(a.fingerprint(), a.clone().fingerprint());
}

#[test]
fn fingerprint_is_sensitive_to_each_field() {
    let base = sample_plan();
    let f0 = base.fingerprint();

    // node slot 移动
    let mut p = base.clone();
    p.node_slots.insert("a".into(), slot(0, 1));
    assert_ne!(p.fingerprint(), f0, "slot 移动应改变指纹");

    // channels 变更
    let mut p = base.clone();
    p.channels.insert(0, vec![tid(0)]);
    assert_ne!(p.fingerprint(), f0, "channel 序列变更应改变指纹");

    // gates 变更
    let mut p = base.clone();
    p.gates.insert(0, vec![crate::layout::atlas::channel::GateId(7)]);
    assert_ne!(p.fingerprint(), f0, "gate 序列变更应改变指纹");

    // ports 变更（语义身份变化）
    let mut p = base.clone();
    p.ports.get_mut(&0).unwrap().from.slot_index = 3;
    assert_ne!(p.fingerprint(), f0, "端口 slot_index 变更应改变指纹");

    // side_order 变更（M1 决策）
    let mut p = base.clone();
    p.ports.get_mut(&0).unwrap().from.side_order = 1;
    assert_ne!(p.fingerprint(), f0, "端口 side_order 变更应改变指纹");

    // along_offset 变更（M1 决策）
    let mut p = base.clone();
    p.ports.get_mut(&0).unwrap().from.along_offset = 9.0;
    assert_ne!(p.fingerprint(), f0, "端口 along_offset 变更应改变指纹");

    // bundle 增删
    let mut p = base.clone();
    p.bundles.clear();
    assert_ne!(p.fingerprint(), f0, "bundle 移除应改变指纹");

    // substrate 摘要变化
    let mut p = base.clone();
    p.substrate.rank_count = 3;
    assert_ne!(p.fingerprint(), f0, "substrate 摘要变更应改变指纹");
}

#[test]
fn fingerprint_ignores_transient_slot_id() {
    // PortSlotId 是基底重建的运行态编号，不参与指纹（语义身份不变）
    let base = sample_plan();
    let mut p = base.clone();
    p.ports.get_mut(&0).unwrap().from.slot_id = Some(port(42));
    assert_eq!(p.fingerprint(), base.fingerprint());
}

#[test]
fn fingerprint_and_semantic_eq_use_decision_scope() {
    let base = sample_plan();

    // provenance 变化：溯源是元数据非决策，指纹/semantic_eq 不变，== 仍区分
    let mut p = base.clone();
    p.provenance.insert(0, Provenance::LegacyAdapter);
    assert_eq!(p.fingerprint(), base.fingerprint());
    assert!(p.semantic_eq(&base));
    assert_ne!(p, base, "== 是结构相等，应区分 provenance");

    // bundles 顺序：集合语义（与 diff 口径一致），规范序后指纹不变
    let mut a = base.clone();
    a.bundles.push(Bundle {
        edges: vec![2, 3],
        suffix: vec![tid(5)],
    });
    let mut b = a.clone();
    b.bundles.reverse();
    assert_ne!(a.bundles, b.bundles);
    assert_eq!(a.fingerprint(), b.fingerprint());
    assert!(a.semantic_eq(&b));

    // slot_id：semantic_eq 与指纹同口径，== 仍区分
    let mut p = base.clone();
    p.ports.get_mut(&0).unwrap().from.slot_id = Some(port(42));
    assert!(p.semantic_eq(&base));
    assert_ne!(p, base);
}

// ---------------------------------------------------------------------------
// diff
// ---------------------------------------------------------------------------

#[test]
fn diff_of_identical_plans_is_empty() {
    let a = sample_plan();
    let d = diff(&a, &a.clone());
    assert!(d.is_empty());
    assert_eq!(format!("{d}"), "PlanDiff: 无差异");
}

#[test]
fn diff_reports_field_level_changes() {
    let a = sample_plan();
    let mut b = a.clone();
    // slot 移动 + 新增节点
    b.node_slots.insert("a".into(), slot(0, 1));
    b.node_slots.insert("c".into(), slot(1, 0));
    // 边 0 换 channel；新边 1 只有 ports
    b.channels.insert(0, vec![tid(0), tid(2)]);
    b.ports.insert(
        1,
        EdgePorts {
            from: port_ref("b", PortSide::CrossLow, 0),
            to: port_ref("c", PortSide::CrossHigh, 0),
        },
    );
    // bundle 变化
    b.bundles = vec![Bundle {
        edges: vec![0, 2],
        suffix: vec![tid(2)],
    }];

    let d = diff(&a, &b);
    assert!(!d.is_empty());
    assert!(!d.substrate_changed);

    // 节点差异按键升序：a（变更）、c（新增）
    assert_eq!(d.node_slots.len(), 2);
    assert_eq!(d.node_slots[0].0, "a");
    assert_eq!(
        d.node_slots[0].1,
        Change::Changed(slot(0, 0), slot(0, 1))
    );
    assert_eq!(d.node_slots[1].0, "c");
    assert!(matches!(d.node_slots[1].1, Change::Added(_)));

    // 边差异按 EdgeId 升序：边 0 只有 channels 变、边 1 只有 ports 增
    assert_eq!(d.edges.len(), 2);
    assert_eq!(d.edges[0].edge, 0);
    assert!(d.edges[0].ports.is_none() && d.edges[0].gates.is_none());
    assert_eq!(
        d.edges[0].channels,
        Some(Change::Changed(vec![tid(0), tid(1)], vec![tid(0), tid(2)]))
    );
    assert_eq!(d.edges[1].edge, 1);
    assert!(matches!(d.edges[1].ports, Some(Change::Added(_))));
    assert!(d.edges[1].channels.is_none());

    // bundle 集合差
    assert_eq!(d.bundles_added.len(), 1);
    assert_eq!(d.bundles_removed.len(), 1);
    assert!(d.group_scopes.is_empty());
}

// ---------------------------------------------------------------------------
// RouteOutcome 收录
// ---------------------------------------------------------------------------

#[test]
fn record_route_fills_channels_gates_and_provenance() {
    // 单 CROSS 轨道、两端口异节点 → Converged 单轨道解（合法同轨边）
    let mut s = Substrate::new();
    s.add_track(tid(0), TrackOrient::Cross, None, 1.0, 0, (0, 0))
        .unwrap();
    s.attach_port(port(0), "a", PortSide::MainLow, 0, tid(0), 0)
        .unwrap();
    s.attach_port(port(1), "b", PortSide::MainLow, 0, tid(0), 0)
        .unwrap();
    let g = ChannelGraph::from_substrate(&s);
    let mask = ScopeMask::for_ports(&s, port(0), port(1)).unwrap();
    let out = route(&g, port(0), port(1), &Occupancy::new(), &mask).unwrap();
    assert_eq!(out.status, SolverStatus::Converged);

    let mut plan = Plan::default();
    plan.record_route(7, &out).unwrap();
    assert_eq!(plan.channels[&7], out.tracks);
    assert_eq!(plan.gates[&7], out.gates);
    assert_eq!(plan.provenance[&7], Provenance::ChannelRoute);
}

#[test]
fn record_route_invalidates_stale_bundles() {
    // 同上构造 Converged 结果
    let mut s = Substrate::new();
    s.add_track(tid(0), TrackOrient::Cross, None, 1.0, 0, (0, 0))
        .unwrap();
    s.attach_port(port(0), "a", PortSide::MainLow, 0, tid(0), 0)
        .unwrap();
    s.attach_port(port(1), "b", PortSide::MainLow, 0, tid(0), 0)
        .unwrap();
    let g = ChannelGraph::from_substrate(&s);
    let mask = ScopeMask::for_ports(&s, port(0), port(1)).unwrap();
    let out = route(&g, port(0), port(1), &Occupancy::new(), &mask).unwrap();
    assert_eq!(out.status, SolverStatus::Converged);

    // 已有 bundles 的 Plan 收录新边 → 旧合流失效（须重跑 detect_and_set_bundles）
    let mut plan = sample_plan();
    assert!(!plan.bundles.is_empty());
    plan.record_route(7, &out).unwrap();
    assert!(plan.bundles.is_empty(), "channels 变更后旧 bundles 必须失效");
}

#[test]
fn record_route_rejects_non_converged_and_leaves_plan_intact() {
    // 自环（同节点）→ route 显式 Infeasible（L7-T5）→ Plan 拒收且逐字段不变
    let mut s = Substrate::new();
    s.add_track(tid(0), TrackOrient::Cross, None, 1.0, 0, (0, 0))
        .unwrap();
    s.attach_port(port(0), "n0", PortSide::MainLow, 0, tid(0), 0)
        .unwrap();
    s.attach_port(port(1), "n0", PortSide::MainLow, 1, tid(0), 0)
        .unwrap();
    let g = ChannelGraph::from_substrate(&s);
    let mask = ScopeMask::for_ports(&s, port(0), port(1)).unwrap();
    let out = route(&g, port(0), port(1), &Occupancy::new(), &mask).unwrap();
    assert_eq!(out.status, SolverStatus::Infeasible);

    let mut plan = sample_plan();
    let before = plan.clone();
    let err = plan.record_route(9, &out).unwrap_err();
    assert_eq!(
        err,
        PlanError::RouteNotConverged(9, SolverStatus::Infeasible)
    );
    assert_eq!(plan, before, "拒收后 Plan 必须逐字段不变");
}

// ---------------------------------------------------------------------------
// 合流
// ---------------------------------------------------------------------------

#[test]
fn detect_and_set_bundles_matches_direct_call() {
    // 边 0/1 共享后缀 [2,3]，边 2 无共享
    let mut plan = Plan::default();
    plan.channels.insert(0, vec![tid(1), tid(2), tid(3)]);
    plan.channels.insert(1, vec![tid(4), tid(2), tid(3)]);
    plan.channels.insert(2, vec![tid(5)]);
    plan.detect_and_set_bundles(2);

    let paths: Vec<(usize, &[TrackId])> = plan
        .channels
        .iter()
        .map(|(&e, t)| (e, t.as_slice()))
        .collect();
    assert_eq!(plan.bundles, detect_bundles(&paths, 2));
    assert_eq!(plan.bundles.len(), 1);
    assert_eq!(plan.bundles[0].edges, vec![0, 1]);
    assert_eq!(plan.bundles[0].suffix, vec![tid(2), tid(3)]);
}

// ---------------------------------------------------------------------------
// 不变量检查
// ---------------------------------------------------------------------------

#[test]
fn validate_checks_lightweight_invariants() {
    assert_eq!(sample_plan().validate(), Ok(()));

    // channels 有边但缺 gates / provenance
    let mut p = sample_plan();
    p.gates.remove(&0);
    assert_eq!(p.validate(), Err(PlanError::MissingEdgeRecord(0, "gates")));
    let mut p = sample_plan();
    p.provenance.remove(&0);
    assert_eq!(
        p.validate(),
        Err(PlanError::MissingEdgeRecord(0, "provenance"))
    );

    // 组 parent 悬空
    let mut p = sample_plan();
    p.group_scopes.get_mut("g").unwrap().parent = Some("ghost".into());
    assert_eq!(
        p.validate(),
        Err(PlanError::DanglingGroupParent("g".into(), "ghost".into()))
    );

    // 覆盖区间倒置
    let mut p = sample_plan();
    p.group_scopes.get_mut("g").unwrap().ranks = (1, 0);
    assert_eq!(p.validate(), Err(PlanError::InvertedGroupSpan("g".into())));

    // 端口引用未知节点
    let mut p = sample_plan();
    p.ports.get_mut(&0).unwrap().to.node = "zombie".into();
    assert_eq!(
        p.validate(),
        Err(PlanError::UnknownPortNode(0, "zombie".into()))
    );
}

// ---------------------------------------------------------------------------
// serde 往返
// ---------------------------------------------------------------------------

#[test]
fn serde_roundtrip_preserves_fingerprint() {
    let a = sample_plan();
    let json = serde_json::to_string(&a).unwrap();
    let b: Plan = serde_json::from_str(&json).unwrap();
    assert_eq!(a, b);
    assert_eq!(a.fingerprint(), b.fingerprint());
}

// ---------------------------------------------------------------------------
// M1 side_order / M3 lane 端点序
// ---------------------------------------------------------------------------

#[test]
fn assign_port_side_orders_by_peer_slot() {
    // 节点 s 底边连向左(order=0)与右(order=2)两目标 → side_order 跟对端 order
    let cases = [
        ("left_first", 0usize, 2usize, 0u32, 1u32),
        ("right_first_slots", 2usize, 0usize, 1u32, 0u32),
    ];
    for (name, order_b, order_c, expect_edge0, expect_edge1) in cases {
        let mut p = Plan {
            substrate: SubstrateSketch {
                rank_count: 2,
                order_count: 3,
            },
            ..Plan::default()
        };
        p.node_slots.insert("s".into(), slot(0, 1));
        p.node_slots.insert("b".into(), slot(1, order_b));
        p.node_slots.insert("c".into(), slot(1, order_c));
        // 故意先插入 EdgeId=1 再 EdgeId=0，验证不按插入序
        p.ports.insert(
            1,
            EdgePorts {
                from: port_ref("s", PortSide::MainHigh, 0),
                to: port_ref("c", PortSide::MainLow, 0),
            },
        );
        p.ports.insert(
            0,
            EdgePorts {
                from: port_ref("s", PortSide::MainHigh, 0),
                to: port_ref("b", PortSide::MainLow, 0),
            },
        );
        p.assign_port_side_orders();
        assert_eq!(
            p.ports[&0].from.side_order, expect_edge0,
            "{name}: edge0"
        );
        assert_eq!(
            p.ports[&1].from.side_order, expect_edge1,
            "{name}: edge1"
        );
    }
}

#[test]
fn assign_port_along_offsets_orders_same_side() {
    // 同侧两端口：side_order 0/1 → along_offset 一负一正（相对中点）
    use std::collections::BTreeMap;
    let cases = [
        (0u32, 1u32), // edge0 order0, edge1 order1
    ];
    for (ord0, ord1) in cases {
        let mut p = Plan {
            substrate: SubstrateSketch {
                rank_count: 2,
                order_count: 2,
            },
            ..Plan::default()
        };
        p.node_slots.insert("s".into(), slot(0, 0));
        p.node_slots.insert("a".into(), slot(1, 0));
        p.node_slots.insert("b".into(), slot(1, 1));
        p.ports.insert(
            0,
            EdgePorts {
                from: {
                    let mut r = port_ref("s", PortSide::MainHigh, 0);
                    r.side_order = ord0;
                    r
                },
                to: port_ref("a", PortSide::MainLow, 0),
            },
        );
        p.ports.insert(
            1,
            EdgePorts {
                from: {
                    let mut r = port_ref("s", PortSide::MainHigh, 0);
                    r.side_order = ord1;
                    r
                },
                to: port_ref("b", PortSide::MainLow, 0),
            },
        );
        let mut rects = BTreeMap::new();
        rects.insert("s".into(), (0.0, 0.0, 100.0, 40.0));
        rects.insert("a".into(), (0.0, 80.0, 40.0, 40.0));
        rects.insert("b".into(), (60.0, 80.0, 40.0, 40.0));
        p.assign_port_along_offsets(&rects);
        let a0 = p.ports[&0].from.along_offset;
        let a1 = p.ports[&1].from.along_offset;
        assert!(
            a0 < a1,
            "side_order {ord0}<{ord1} → along {a0} < {a1}"
        );
        assert!((a0 + a1).abs() < 1e-9, "对称中点两侧偏移应互为相反数");
    }
}

#[test]
fn assign_lane_indices_follows_endpoint_slots_not_edge_id() {
    // Cross 走廊：边 0 端点 order (2,2)，边 1 端点 order (0,0)
    // 若按 EdgeId：edge0→lane0；按槽位：edge1 应 lane0
    let mut s = Substrate::new();
    s.add_track(tid(0), TrackOrient::Cross, None, 1.0, 1, (0, 2))
        .unwrap();

    let mut p = Plan {
        substrate: SubstrateSketch {
            rank_count: 2,
            order_count: 3,
        },
        ..Plan::default()
    };
    p.node_slots.insert("a".into(), slot(0, 2));
    p.node_slots.insert("b".into(), slot(1, 2));
    p.node_slots.insert("c".into(), slot(0, 0));
    p.node_slots.insert("d".into(), slot(1, 0));
    p.ports.insert(
        0,
        EdgePorts {
            from: port_ref("a", PortSide::MainHigh, 0),
            to: port_ref("b", PortSide::MainLow, 0),
        },
    );
    p.ports.insert(
        1,
        EdgePorts {
            from: port_ref("c", PortSide::MainHigh, 0),
            to: port_ref("d", PortSide::MainLow, 0),
        },
    );
    p.channels.insert(0, vec![tid(0)]);
    p.channels.insert(1, vec![tid(0)]);
    p.gates.insert(0, vec![]);
    p.gates.insert(1, vec![]);

    p.assign_lane_indices(&s);
    assert_eq!(p.lane_indices[&1][0], 0, "较小 order 的边应得 lane 0");
    assert_eq!(p.lane_indices[&0][0], 1, "较大 order 的边应得 lane 1");
}
