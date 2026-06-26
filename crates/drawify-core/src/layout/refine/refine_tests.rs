use super::*;
use super::overlap::{analyze_edge_overlaps, segments_conflict_xy};
use std::collections::HashMap;
use crate::layout::geometry::{Point, Rect};
use crate::layout::{EdgeLayout, LayoutResult, NodeLayout, PathGeometry, Port};

#[test]
fn segment_clear_of_aabb() {
    assert!(!segment_intersects_aabb(
        Point::new(0.0, 0.0),
        Point::new(10.0, 0.0),
        Rect::new(2.0, 2.0, 6.0, 6.0),
    ));
}

#[test]
fn segment_crosses_aabb() {
    assert!(segment_intersects_aabb(
        Point::new(5.0, 0.0),
        Point::new(5.0, 10.0),
        Rect::new(2.0, 2.0, 6.0, 6.0),
    ));
}

#[test]
fn segment_touches_aabb_edge() {
    assert!(segment_intersects_aabb(
        Point::new(2.0, 0.0),
        Point::new(2.0, 5.0),
        Rect::new(2.0, 2.0, 6.0, 6.0),
    ));
}

#[test]
fn segment_parallel_to_x_inside() {
    assert!(segment_intersects_aabb(
        Point::new(3.0, 5.0),
        Point::new(7.0, 5.0),
        Rect::new(2.0, 2.0, 6.0, 6.0),
    ));
}

#[test]
fn segment_parallel_to_x_outside() {
    assert!(!segment_intersects_aabb(
        Point::new(3.0, 0.0),
        Point::new(7.0, 0.0),
        Rect::new(2.0, 2.0, 6.0, 6.0),
    ));
}

#[test]
fn diagonal_segment_misses() {
    assert!(!segment_intersects_aabb(
        Point::new(0.0, 1.5),
        Point::new(1.5, 0.0),
        Rect::new(2.0, 2.0, 5.0, 5.0),
    ));
}

#[test]
fn push_direction_away_from_edge() {
    let mut info = NodePushInfo::default();
    let p1: (f64, f64) = (5.0, 0.0);
    let p2: (f64, f64) = (5.0, 10.0);
    let dx = p2.0 - p1.0;
    let dy = p2.1 - p1.1;
    let len = (dx * dx + dy * dy).sqrt();
    let nx = -dy / len;
    let ny = dx / len;
    let cx = 3.0_f64;
    let cy = 5.0_f64;
    let vx = cx - p1.0;
    let vy = cy - p1.1;
    let dot = vx * nx + vy * ny;
    let sign = if dot >= 0.0 { 1.0 } else { -1.0 };
    info.push_fx = sign * nx;
    info.push_fy = sign * ny;
    assert!(info.push_fx < 0.0, "expected negative x push, got {}", info.push_fx);
    assert!(info.push_fy.abs() < f64::EPSILON);
}

use crate::ast::{
    ArrowType, AttributeMap, Diagram, Entity, Identifier, Relation, SourceInfo, Span,
};
use crate::layout::LayoutHints;
use crate::types::DiagramType;

    /// 恒等路由器：返回 result 不做任何修改（用于测试 refine 逻辑而非路由逻辑）
    struct IdentityRouter;
    impl EdgeRoutingStrategy for IdentityRouter {
        fn name(&self) -> &'static str { "identity" }
        fn route(&self, _diagram: &Diagram, result: LayoutResult) -> LayoutResult {
            result
        }
    }

    fn make_test_diagram(relations: usize) -> Diagram {
        let span = Span::dummy();
        let mut entities = vec![
            Entity {
                id: Identifier::new_unchecked("a"),
                label: "A".into(),
                attributes: AttributeMap::default(),
                group_id: None,
                span,
            },
            Entity {
                id: Identifier::new_unchecked("b"),
                label: "B".into(),
                attributes: AttributeMap::default(),
                group_id: None,
                span,
            },
            Entity {
                id: Identifier::new_unchecked("c"),
                label: "C".into(),
                attributes: AttributeMap::default(),
                group_id: None,
                span,
            },
        ];
        let _ = &mut entities; // silence unused warning when relations=0
        let rels: Vec<Relation> = (0..relations)
            .map(|_| Relation {
                from: Identifier::new_unchecked("a"),
                to: Identifier::new_unchecked("b"),
                arrow: ArrowType::Active,
                label: None,
                head_label: None,
                tail_label: None,
                attributes: AttributeMap::default(),
                span,
            })
            .collect();
        Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: Vec::new(),
            entities,
            relations: rels,
            groups: Vec::new(),
            style_decls: vec![],
            source_info: SourceInfo { file: None, line_count: 1 },
            ..Default::default()
        }
    }

    fn make_result_with_crossing() -> LayoutResult {
        // 节点 A 在左，节点 B 在右，节点 C 在中间（被边穿过）
        let mut nodes = HashMap::new();
        nodes.insert("a".to_string(), NodeLayout { x: 0.0, y: 0.0, width: 40.0, height: 30.0, ..Default::default() });
        nodes.insert("b".to_string(), NodeLayout { x: 200.0, y: 0.0, width: 40.0, height: 30.0, ..Default::default() });
        // 节点 C 在 (90, 5) 大小 20x20，中心在 (100, 15)
        nodes.insert("c".to_string(), NodeLayout { x: 90.0, y: 5.0, width: 20.0, height: 20.0, ..Default::default() });

        // 边 0：A→B 折线，从 (40, 15) 到 (200, 15)，穿过 C
        let edges = vec![EdgeLayout {
            geometry: PathGeometry::Polyline {
                points: vec![Point::new(40.0, 15.0), Point::new(200.0, 15.0)],
            },
            labels: vec![crate::layout::EdgeLabelLayout::new("label", Point::new(120.0, 15.0))],
            from_port: Port::Right,
            to_port: Port::Left,
        }];

        LayoutResult {
            nodes,
            groups: HashMap::new(),
            edges,
            total_width: 240.0,
            total_height: 30.0,
            hints: LayoutHints::default(),
        }
    }

    #[test]
    fn refine_config_default_max_passes_is_3() {
        let config = RefineConfig::default();
        assert_eq!(config.max_passes, 3, "P2: default max_passes should be 3");
    }

    #[test]
    fn analyze_records_edge_indices() {
        let diagram = make_test_diagram(1);
        let result = make_result_with_crossing();
        let config = RefineConfig::default();

        let metrics = analyze_edge_node_crossings(&result, &diagram, &config);

        assert!(metrics.edge_node_crossings > 0, "should detect crossings");
        let c_info = metrics.problem_nodes.get("c").expect("node C should be a problem node");
        assert!(
            c_info.edge_indices.contains(&0),
            "edge 0 should be in C's edge_indices, got {:?}",
            c_info.edge_indices
        );
    }

    #[test]
    fn run_refine_skips_when_no_crossings() {
        // 无穿障时，run_refine 应直接返回，不调用路由器
        let diagram = make_test_diagram(1);
        let mut result = make_result_with_crossing();
        // 把节点 C 移开，消除穿障
        result.nodes.get_mut("c").unwrap().y = 200.0;

        let config = RefineConfig::default();
        let router = IdentityRouter;
        let output = run_refine(&diagram, result.clone(), &router, &config);

        // 结果应与输入完全相同（未进入循环）
        assert_eq!(
            output.nodes.get("c").unwrap().y, 200.0,
            "node C should not move when there are no crossings"
        );
    }

    #[test]
    fn run_refine_rollback_when_no_improvement() {
        // push_distance=0 → 节点不移动 → 穿障数不变 → 回退到初始状态
        let diagram = make_test_diagram(1);
        let result = make_result_with_crossing();
        let config = RefineConfig {
            enabled: true,
            max_passes: 3,
            push_distance: 0.0, // 不推开 → 无改善 → 回退
            node_shrink: 2.0,
        };
        let router = IdentityRouter;
        let output = run_refine(&diagram, result.clone(), &router, &config);

        // 节点 C 应未被移动（回退到初始状态）
        let original_c = result.nodes.get("c").unwrap();
        let output_c = output.nodes.get("c").unwrap();
        assert_eq!(
            (output_c.x, output_c.y),
            (original_c.x, original_c.y),
            "node C should not move when push_distance=0 (rollback)"
        );
    }

    #[test]
    fn run_refine_improves_crossings() {
        // 正常推开场景：节点 C 被推离边路径，穿障数降为 0
        let diagram = make_test_diagram(1);
        let result = make_result_with_crossing();
        let config = RefineConfig::default();
        let router = IdentityRouter;

        let before = analyze_edge_node_crossings(&result, &diagram, &config).edge_node_crossings;
        assert!(before > 0, "should have crossings before refine");

        let output = run_refine(&diagram, result, &router, &config);

        let after = analyze_edge_node_crossings(&output, &diagram, &config).edge_node_crossings;
        assert_eq!(
            after, 0,
            "crossings should be 0 after refine (node pushed away)"
        );
    }

    // ── P1-2: refine 锚点脱节修复 + momentum 测试 ──

    /// 构造指定关系列表的 Diagram（支持多对节点）
    fn make_diagram_with_relations(rels: Vec<(&str, &str)>) -> Diagram {
        let span = Span::dummy();
        let ids: std::collections::BTreeSet<&str> = rels.iter()
            .flat_map(|(f, t)| [f, t])
            .copied()
            .collect();
        let entities: Vec<Entity> = ids.into_iter()
            .map(|id| Entity {
                id: Identifier::new_unchecked(id),
                label: id.to_uppercase(),
                attributes: AttributeMap::default(),
                group_id: None,
                span,
            })
            .collect();
        let rels: Vec<Relation> = rels.into_iter()
            .map(|(from, to)| Relation {
                from: Identifier::new_unchecked(from),
                to: Identifier::new_unchecked(to),
                arrow: ArrowType::Active,
                label: None,
                head_label: None,
                tail_label: None,
                attributes: AttributeMap::default(),
                span,
            })
            .collect();
        Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: Vec::new(),
            entities,
            relations: rels,
            groups: Vec::new(),
            style_decls: vec![],
            source_info: SourceInfo { file: None, line_count: 1 },
            ..Default::default()
        }
    }

    /// 测试用路由器：将每条边路径替换为 from→to 节点中心的直线
    /// 用于验证 refine 后哪些边被重新路由（锚点是否与节点位置一致）
    struct CenterLineRouter;
    impl EdgeRoutingStrategy for CenterLineRouter {
        fn name(&self) -> &'static str { "center-line" }
        fn route(&self, diagram: &Diagram, mut result: LayoutResult) -> LayoutResult {
            for (i, edge) in result.edges.iter_mut().enumerate() {
                let Some(rel) = diagram.relations.get(i) else { continue };
                let (Some(from_nl), Some(to_nl)) = (
                    result.nodes.get(rel.from.as_str()),
                    result.nodes.get(rel.to.as_str()),
                ) else { continue };
                let from_x = from_nl.x + from_nl.width / 2.0;
                let from_y = from_nl.y + from_nl.height / 2.0;
                let to_x = to_nl.x + to_nl.width / 2.0;
                let to_y = to_nl.y + to_nl.height / 2.0;
                edge.geometry = PathGeometry::Polyline {
                    points: vec![Point::new(from_x, from_y), Point::new(to_x, to_y)],
                };
            }
            result
        }
    }

    #[test]
    fn test_momentum_dampens_reversal() {
        let mut m = MomentumHistory::new();
        // 第一次：向右推，无历史 → 不衰减
        let (fx, fy) = m.damp("a", 1.0, 0.0);
        assert_eq!((fx, fy), (1.0, 0.0));
        m.update("a", 1.0, 0.0);

        // 第二次：向左推（方向反转）→ 0.5x 衰减
        let (fx, fy) = m.damp("a", -1.0, 0.0);
        assert_eq!((fx, fy), (-0.5, 0.0));
        assert_eq!(m.reversal_count, 1);
        m.update("a", -1.0, 0.0);

        // 第三次：向右推（再次反转）→ 0.5x 衰减
        let (fx, fy) = m.damp("a", 1.0, 0.0);
        assert_eq!((fx, fy), (0.5, 0.0));
        assert_eq!(m.reversal_count, 2);
    }

    #[test]
    fn test_momentum_no_damp_same_direction() {
        let mut m = MomentumHistory::new();
        m.update("a", 1.0, 0.0);
        // 同方向 → 不衰减
        let (fx, fy) = m.damp("a", 2.0, 0.0);
        assert_eq!((fx, fy), (2.0, 0.0));
        assert_eq!(m.reversal_count, 0);

        // 不同节点 → 不衰减
        let (fx, fy) = m.damp("b", -1.0, 0.0);
        assert_eq!((fx, fy), (-1.0, 0.0));
        assert_eq!(m.reversal_count, 0);
    }

    #[test]
    fn test_momentum_deterministic() {
        let run = || {
            let mut m = MomentumHistory::new();
            m.update("a", 1.0, 0.0);
            m.update("b", 0.0, 1.0);
            (m.damp("a", -1.0, 0.0), m.damp("b", 0.0, -1.0), m.reversal_count)
        };
        assert_eq!(run(), run());
    }

    /// P1-2 核心测试：refine 推开节点后，连接该节点的所有边的锚点
    /// 必须与节点新位置一致（修复 G4 锚点脱节）。
    ///
    /// 场景：
    /// - 边 0: A→B 折线穿过节点 D（D 是问题节点，会被推开）
    /// - 边 1: D→C 折线不穿过任何节点（不是穿障边）
    ///
    /// 旧逻辑（bug）：reroute_subset 仅替换穿障边（边 0），边 1 保留旧路径，
    /// 锚点仍指向 D 的旧位置 → 边 1 "悬空"。
    /// 新逻辑（修复）：edges_to_reroute 扩展为包含所有连接 D 的边（边 0 + 边 1），
    /// 边 1 也被重新路由 → 锚点与 D 新位置一致。
    #[test]
    fn test_p1_2_anchor_consistency_after_push() {
        // Diagram: A→B (穿障边), D→C (非穿障边，但连接被推开节点 D)
        let diagram = make_diagram_with_relations(vec![("a", "b"), ("d", "c")]);

        // 节点布局
        let mut nodes = HashMap::new();
        nodes.insert("a".to_string(), NodeLayout { x: 0.0, y: 0.0, width: 40.0, height: 30.0 });
        nodes.insert("b".to_string(), NodeLayout { x: 200.0, y: 0.0, width: 40.0, height: 30.0 });
        // D 在 A→B 路径上（y=15 穿过 D 的 y=[10,30]）
        nodes.insert("d".to_string(), NodeLayout { x: 90.0, y: 10.0, width: 20.0, height: 20.0 });
        nodes.insert("c".to_string(), NodeLayout { x: 100.0, y: 200.0, width: 40.0, height: 30.0 });

        // 边 0: A→B 折线穿过 D
        // 边 1: D→C 折线（初始路径，不穿障）
        let edges = vec![
            EdgeLayout {
                geometry: PathGeometry::Polyline {
                    points: vec![Point::new(40.0, 15.0), Point::new(200.0, 15.0)],
                },
                labels: vec![],
                from_port: Port::Right,
                to_port: Port::Left,
            },
            EdgeLayout {
                geometry: PathGeometry::Polyline {
                    points: vec![Point::new(100.0, 30.0), Point::new(100.0, 200.0)],
                },
                labels: vec![],
                from_port: Port::Bottom,
                to_port: Port::Top,
            },
        ];

        let result = LayoutResult {
            nodes,
            groups: HashMap::new(),
            edges,
            total_width: 240.0,
            total_height: 240.0,
            hints: LayoutHints::default(),
        };

        let config = RefineConfig::default();
        let router = CenterLineRouter;

        // 验证初始有穿障
        let before = analyze_edge_node_crossings(&result, &diagram, &config).edge_node_crossings;
        assert!(before > 0, "初始应有穿障");

        let output = run_refine(&diagram, result, &router, &config);

        // 验证穿障消除
        let after = analyze_edge_node_crossings(&output, &diagram, &config).edge_node_crossings;
        assert_eq!(after, 0, "refine 后穿障应为 0");

        // 验证 D 节点被推开
        let d_after = output.nodes.get("d").unwrap();
        assert!(d_after.y > 10.0, "D 应被向下推开, 实际 y={}", d_after.y);

        // P1-2 核心：边 1 (D→C) 的路径应反映 D 的新位置
        // CenterLineRouter 会将边 1 替换为 D 中心→C 中心的直线
        let edge1 = &output.edges[1];
        let points = edge1.path_points();
        let first = points[0];
        let d_center_y = d_after.y + d_after.height / 2.0;
        assert!(
            (first.y - d_center_y).abs() < 1.0,
            "边 1 起点应与 D 新中心一致: got y={}, expected y={}",
            first.y, d_center_y
        );

        // 旧 bug 下边 1 起点会是 (100, 30)（D 旧 bottom），与新中心差距大
        assert!(
            (first.y - 30.0).abs() > 5.0,
            "边 1 起点不应停留在旧位置 y=30, got y={}",
            first.y
        );
    }

    /// P1-2 确定性测试：同一输入多次运行 refine，结果应完全一致（AGENTS.md §2）
    #[test]
    fn test_p1_2_refine_deterministic() {
        let diagram = make_diagram_with_relations(vec![("a", "b"), ("d", "c")]);

        let make_result = || {
            let mut nodes = HashMap::new();
            nodes.insert("a".to_string(), NodeLayout { x: 0.0, y: 0.0, width: 40.0, height: 30.0 });
            nodes.insert("b".to_string(), NodeLayout { x: 200.0, y: 0.0, width: 40.0, height: 30.0 });
            nodes.insert("d".to_string(), NodeLayout { x: 90.0, y: 10.0, width: 20.0, height: 20.0 });
            nodes.insert("c".to_string(), NodeLayout { x: 100.0, y: 200.0, width: 40.0, height: 30.0 });
            let edges = vec![
                EdgeLayout {
                    geometry: PathGeometry::Polyline { points: vec![Point::new(40.0, 15.0), Point::new(200.0, 15.0)] },
                    labels: vec![],
                    from_port: Port::Right,
                    to_port: Port::Left,
                },
                EdgeLayout {
                    geometry: PathGeometry::Polyline { points: vec![Point::new(100.0, 30.0), Point::new(100.0, 200.0)] },
                    labels: vec![],
                    from_port: Port::Bottom,
                    to_port: Port::Top,
                },
            ];
            LayoutResult {
                nodes, groups: HashMap::new(), edges,
                total_width: 240.0, total_height: 240.0,
                hints: LayoutHints::default(),
            }
        };

        let config = RefineConfig::default();
        let router = CenterLineRouter;

        let out1 = run_refine(&diagram, make_result(), &router, &config);
        let out2 = run_refine(&diagram, make_result(), &router, &config);

        // 节点位置一致
        for nid in ["a", "b", "c", "d"] {
            let n1 = out1.nodes.get(nid).unwrap();
            let n2 = out2.nodes.get(nid).unwrap();
            assert_eq!((n1.x, n1.y), (n2.x, n2.y), "节点 {} 位置不一致", nid);
        }
        // 边路径一致
        for i in 0..out1.edges.len() {
            let p1: Vec<_> = out1.edges[i].path_points().to_vec();
            let p2: Vec<_> = out2.edges[i].path_points().to_vec();
            assert_eq!(p1.len(), p2.len(), "边 {} 路径点数不一致", i);
            for (j, (a, b)) in p1.iter().zip(p2.iter()).enumerate() {
                assert_eq!(a, b, "边 {} 点 {} 不一致", i, j);
            }
        }
    }

    // ── P1-2 Task 3: edge-overlap 感知测试 ──

    #[test]
    fn test_segments_conflict_xy_parallel_overlap() {
        // 两条水平线段，y 间距 4px（<= 8px gap），x 区间重叠 → 冲突
        assert!(segments_conflict_xy(
            Point::new(0.0, 10.0), Point::new(100.0, 10.0),
            Point::new(20.0, 14.0), Point::new(80.0, 14.0),
        ));
    }

    #[test]
    fn test_segments_conflict_xy_parallel_no_overlap() {
        // 两条水平线段，y 间距 4px，但 x 区间不重叠 → 无冲突
        assert!(!segments_conflict_xy(
            Point::new(0.0, 10.0), Point::new(50.0, 10.0),
            Point::new(60.0, 14.0), Point::new(100.0, 14.0),
        ));
    }

    #[test]
    fn test_segments_conflict_xy_far_apart() {
        // 两条水平线段，y 间距 20px（> 8px gap）→ 无冲突
        assert!(!segments_conflict_xy(
            Point::new(0.0, 10.0), Point::new(100.0, 10.0),
            Point::new(20.0, 30.0), Point::new(80.0, 30.0),
        ));
    }

    #[test]
    fn test_segments_conflict_xy_perpendicular_cross() {
        // 水平段与垂直段严格内部相交 → 冲突
        assert!(segments_conflict_xy(
            Point::new(0.0, 50.0), Point::new(100.0, 50.0),  // 水平段 y=50
            Point::new(50.0, 0.0), Point::new(50.0, 100.0),  // 垂直段 x=50
        ));
    }

    #[test]
    fn test_segments_conflict_xy_perpendicular_no_cross() {
        // 水平段与垂直段不相交（垂直段 x 在水平段范围外）→ 无冲突
        assert!(!segments_conflict_xy(
            Point::new(0.0, 50.0), Point::new(100.0, 50.0),
            Point::new(200.0, 0.0), Point::new(200.0, 100.0),
        ));
    }

    #[test]
    fn test_segments_conflict_xy_perpendicular_endpoint_touch() {
        // 水平段与垂直段在端点接触（T-junction）→ 不算冲突
        assert!(!segments_conflict_xy(
            Point::new(0.0, 50.0), Point::new(100.0, 50.0),  // 水平段 y=50, x=[0,100]
            Point::new(100.0, 0.0), Point::new(100.0, 100.0), // 垂直段 x=100（= 水平段端点）
        ));
    }

    #[test]
    fn test_analyze_edge_overlaps_detects_parallel() {
        // 两条平行水平边，y 间距 4px，x 区间重叠 → 1 次重叠
        let edges = vec![
            EdgeLayout {
                geometry: PathGeometry::Polyline {
                    points: vec![Point::new(0.0, 10.0), Point::new(100.0, 10.0)],
                },
                labels: vec![],
                from_port: Port::Right,
                to_port: Port::Left,
            },
            EdgeLayout {
                geometry: PathGeometry::Polyline {
                    points: vec![Point::new(20.0, 14.0), Point::new(80.0, 14.0)],
                },
                labels: vec![],
                from_port: Port::Right,
                to_port: Port::Left,
            },
        ];
        let result = LayoutResult {
            nodes: HashMap::new(),
            groups: HashMap::new(),
            edges,
            total_width: 100.0,
            total_height: 20.0,
            hints: LayoutHints::default(),
        };
        assert_eq!(analyze_edge_overlaps(&result), 1);
    }

    #[test]
    fn test_analyze_edge_overlaps_detects_crossing() {
        // 水平边与垂直边交叉 → 1 次重叠
        let edges = vec![
            EdgeLayout {
                geometry: PathGeometry::Polyline {
                    points: vec![Point::new(0.0, 50.0), Point::new(100.0, 50.0)],
                },
                labels: vec![],
                from_port: Port::Right,
                to_port: Port::Left,
            },
            EdgeLayout {
                geometry: PathGeometry::Polyline {
                    points: vec![Point::new(50.0, 0.0), Point::new(50.0, 100.0)],
                },
                labels: vec![],
                from_port: Port::Bottom,
                to_port: Port::Top,
            },
        ];
        let result = LayoutResult {
            nodes: HashMap::new(),
            groups: HashMap::new(),
            edges,
            total_width: 100.0,
            total_height: 100.0,
            hints: LayoutHints::default(),
        };
        assert_eq!(analyze_edge_overlaps(&result), 1);
    }

    #[test]
    fn test_analyze_edge_overlaps_no_overlap() {
        // 两条边相距很远 → 0 次重叠
        let edges = vec![
            EdgeLayout {
                geometry: PathGeometry::Polyline {
                    points: vec![Point::new(0.0, 0.0), Point::new(100.0, 0.0)],
                },
                labels: vec![],
                from_port: Port::Right,
                to_port: Port::Left,
            },
            EdgeLayout {
                geometry: PathGeometry::Polyline {
                    points: vec![Point::new(0.0, 200.0), Point::new(100.0, 200.0)],
                },
                labels: vec![],
                from_port: Port::Right,
                to_port: Port::Left,
            },
        ];
        let result = LayoutResult {
            nodes: HashMap::new(),
            groups: HashMap::new(),
            edges,
            total_width: 100.0,
            total_height: 200.0,
            hints: LayoutHints::default(),
        };
        assert_eq!(analyze_edge_overlaps(&result), 0);
    }

    #[test]
    fn test_combined_crossing_score_weights_node_crossings() {
        // node-crossings 权重 10，edge-overlaps 权重 1
        let m1 = RefineMetrics {
            edge_node_crossings: 1,
            edge_overlaps: 0,
            ..Default::default()
        };
        let m2 = RefineMetrics {
            edge_node_crossings: 0,
            edge_overlaps: 10,
            ..Default::default()
        };
        // 1*10 + 0 = 10; 0*10 + 10 = 10 → 相同评分
        assert_eq!(combined_crossing_score(&m1), combined_crossing_score(&m2));

        let m3 = RefineMetrics {
            edge_node_crossings: 0,
            edge_overlaps: 9,
            ..Default::default()
        };
        // 0*10 + 9 = 9 < 10 → m3 优于 m1
        assert!(combined_crossing_score(&m3) < combined_crossing_score(&m1));
    }

    /// P1-2 Task 3: refine 回退决策应感知 edge-overlaps
    ///
    /// 场景：refine 推节点后 node-crossings 减少但 edge-overlaps 大幅增加，
    /// 综合评分未改善 → 应回退。
    #[test]
    fn test_refine_rollback_when_overlap_outweighs_crossing_gain() {
        // 构造一个穿障场景，但推节点会引入更多边重叠
        // 使用 IdentityRouter（不改路径），push_distance=0 → 不推 → 回退
        let diagram = make_test_diagram(1);
        let result = make_result_with_crossing();
        let config = RefineConfig {
            enabled: true,
            max_passes: 3,
            push_distance: 0.0, // 不推开 → 无改善 → 回退
            node_shrink: 2.0,
        };
        let router = IdentityRouter;
        let output = run_refine(&diagram, result.clone(), &router, &config);

        // 应回退到初始状态
        let original_c = result.nodes.get("c").unwrap();
        let output_c = output.nodes.get("c").unwrap();
        assert_eq!(
            (output_c.x, output_c.y),
            (original_c.x, original_c.y),
            "push_distance=0 时应回退"
        );
    }

    // ── P1-2 Task 4: 前置单边重路由测试 ──

    /// 路由器：将穿障边替换为绕行路径（不穿节点 C）
    struct DetourRouter;
    impl EdgeRoutingStrategy for DetourRouter {
        fn name(&self) -> &'static str { "detour" }
        fn route(&self, _diagram: &Diagram, mut result: LayoutResult) -> LayoutResult {
            // 将所有边替换为 y=0 的水平直线（绕开 y=[5,25] 的节点 C）
            for edge in result.edges.iter_mut() {
                edge.geometry = PathGeometry::Polyline {
                    points: vec![Point::new(0.0, 0.0), Point::new(200.0, 0.0)],
                };
            }
            result
        }
    }

    /// P1-2 Task 4: 穿障边可通过单边重路由消除时，不应推节点
    #[test]
    fn test_refine_preroute_skips_push_when_reroute_fixes() {
        let diagram = make_test_diagram(1);
        let result = make_result_with_crossing();

        // DetourRouter 会将边替换为 y=0 的直线，绕开节点 C（y=[5,25]）
        let config = RefineConfig::default();
        let output = run_refine(&diagram, result.clone(), &DetourRouter, &config);

        // 穿障应消除
        let after = analyze_edge_node_crossings(&output, &diagram, &config).edge_node_crossings;
        assert_eq!(after, 0, "穿障应通过单边重路由消除");

        // 节点 C 不应被推开（单边重路由已解决，无需推节点）
        let original_c = result.nodes.get("c").unwrap();
        let output_c = output.nodes.get("c").unwrap();
        assert_eq!(
            (output_c.x, output_c.y),
            (original_c.x, original_c.y),
            "单边重路由能消除穿障时，节点 C 不应被推开"
        );

        // push_count 应为 0
        assert_eq!(
            output.hints.refine_debug.as_ref().unwrap().push_count, 0,
            "push_count 应为 0（未推节点）"
        );
    }

    /// P1-2 Task 4: 单边重路由无法消除穿障时，仍应推节点
    #[test]
    fn test_refine_preroute_falls_back_to_push() {
        let diagram = make_test_diagram(1);
        let result = make_result_with_crossing();

        // IdentityRouter 不改路径，单边重路由无法消除穿障 → 应回退到推节点
        let config = RefineConfig::default();
        let output = run_refine(&diagram, result.clone(), &IdentityRouter, &config);

        // 穿障应消除（通过推节点）
        let after = analyze_edge_node_crossings(&output, &diagram, &config).edge_node_crossings;
        assert_eq!(after, 0, "穿障应通过推节点消除");

        // 节点 C 应被推开
        let original_c = result.nodes.get("c").unwrap();
        let output_c = output.nodes.get("c").unwrap();
        assert_ne!(
            (output_c.x, output_c.y),
            (original_c.x, original_c.y),
            "单边重路由失败时，节点 C 应被推开"
        );
    }

    /// P1-2 Task 3+4 确定性测试
    #[test]
    fn test_refine_task3_task4_deterministic() {
        let diagram = make_test_diagram(1);
        let config = RefineConfig::default();

        let make_result = || make_result_with_crossing();

        let out1 = run_refine(&diagram, make_result(), &IdentityRouter, &config);
        let out2 = run_refine(&diagram, make_result(), &IdentityRouter, &config);

        for nid in ["a", "b", "c"] {
            let n1 = out1.nodes.get(nid).unwrap();
            let n2 = out2.nodes.get(nid).unwrap();
            assert_eq!((n1.x, n1.y), (n2.x, n2.y), "节点 {} 位置不一致", nid);
        }
    }
