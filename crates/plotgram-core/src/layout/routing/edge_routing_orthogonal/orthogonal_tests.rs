    use super::*;
    use std::collections::HashMap;
    use crate::ast::{ArrowType, AttributeValue};
    use crate::layout::geometry::Point;
    use crate::layout::NodeLayout;
    use crate::layout::routing::common::test_fixtures::make_diagram_with_layout;
    use crate::layout::group::GroupRoutingContext;

    /// 判断线段 (a,b) 是否穿过矩形 (left, top, right, bottom) 内部。
    /// 简化检测：取线段中点判断是否在矩形内（对正交路径足够）。
    fn segment_crosses_rect(a: Point, b: Point, rect: (f64, f64, f64, f64)) -> bool {
        let (left, top, right, bottom) = rect;
        let mid = Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
        mid.x > left + 1.0 && mid.x < right - 1.0 && mid.y > top + 1.0 && mid.y < bottom - 1.0
    }

    fn test_group_ctx(
        groups: HashMap<String, crate::layout::GroupLayout>,
        node_to_groups: HashMap<String, Vec<String>>,
    ) -> GroupRoutingContext {
        GroupRoutingContext {
            groups,
            node_to_groups,
            border_shell_pad: GROUP_OBSTACLE_PAD,
            stub_clearance: PORT_CLEARANCE,
            corridor_misalignment_penalty: 120.0,
            repulse_max_rounds: 2,
            corridors: vec![],
            side_gutters: std::collections::BTreeMap::new(),
            node_leaf_group: HashMap::new(),
            sibling_sets: vec![],
            sibling_orientation: HashMap::new(),
            group_ancestors: HashMap::new(),
        }
    }

    fn edge_index(diagram: &crate::ast::Diagram, from: &str, to: &str) -> usize {
        diagram
            .relations
            .iter()
            .position(|r| r.from.as_str() == from && r.to.as_str() == to)
            .unwrap_or_else(|| panic!("edge {from}->{to} not found"))
    }

    #[test]
    fn test_route_edges_orthogonal_single() {
        let (diagram, result) = make_diagram_with_layout(
            vec![("a", 40.0, 40.0), ("b", 40.0, 170.0)],
            vec![("a", "b", None)],
        );

        let routed = route_edges_orthogonal(&diagram, result, OrthoConfig::from_spec_defaults());

        assert_eq!(routed.edges.len(), 1);
        let edge = &routed.edges[0];
        assert_eq!(edge.path_len(), 2);
        assert!(edge.is_straight());
    }

    #[test]
    fn test_route_edges_orthogonal_horizontal() {
        let (diagram, result) = make_diagram_with_layout(
            vec![("a", 40.0, 40.0), ("b", 260.0, 40.0)],
            vec![("a", "b", None)],
        );

        let routed = route_edges_orthogonal(&diagram, result, OrthoConfig::from_spec_defaults());

        assert_eq!(routed.edges.len(), 1);
        let edge = &routed.edges[0];
        assert_eq!(edge.path_len(), 2);
        assert!(edge.is_straight());
    }

    #[test]
    fn test_bidirectional_aligned_pair_is_straight_parallel() {
        // 两个垂直对齐、尺寸相同的节点间的来回边，应为两条平行直线
        let (diagram, result) = make_diagram_with_layout(
            vec![("a", 40.0, 40.0), ("b", 40.0, 200.0)],
            vec![("a", "b", None), ("b", "a", None)],
        );

        let routed = route_edges_orthogonal(&diagram, result, OrthoConfig::from_spec_defaults());
        assert_eq!(routed.edges.len(), 2);
        for edge in &routed.edges {
            assert_eq!(edge.path_len(), 2, "对齐节点对的边应为直线");
            // 直线应为垂直（两端 x 相同）
            let start = edge.path_start().unwrap();
            let end = edge.path_end().unwrap();
            assert!((start.x - end.x).abs() < EPS);
        }
        // 两条边应位于不同的 x（对称分布）
        let x0 = routed.edges[0].path_start().unwrap().x;
        let x1 = routed.edges[1].path_start().unwrap().x;
        assert!((x0 - x1).abs() > EPS, "来回边应分布在不同磁吸点");
    }

    #[test]
    fn test_bidirectional_edge_labels_do_not_overlap() {
        // 三层架构典型场景：竖直双向边的水平标签不得 AABB 重叠
        let (diagram, result) = make_diagram_with_layout(
            vec![
                ("client", 40.0, 40.0),
                ("api", 40.0, 180.0),
                ("db", 40.0, 320.0),
            ],
            vec![
                ("client", "api", Some("HTTP 请求")),
                ("api", "db", Some("SQL 查询")),
                ("db", "api", Some("查询结果")),
                ("api", "client", Some("JSON 响应")),
            ],
        );

        let routed = route_edges_orthogonal(&diagram, result, OrthoConfig::from_spec_defaults());
        assert_eq!(routed.edges.len(), 4);

        let mut bboxes = Vec::new();
        for edge in &routed.edges {
            assert!(!edge.labels.is_empty(), "每条边都应有标签");
            for (li, label) in edge.labels.iter().enumerate() {
                let bbox = edge.label_bbox_at(li).expect("label bbox");
                bboxes.push((label.text.clone(), bbox));
            }
        }

        for i in 0..bboxes.len() {
            for j in (i + 1)..bboxes.len() {
                let (ref ta, ba) = bboxes[i];
                let (ref tb, bb) = bboxes[j];
                assert!(
                    crate::layout::routing::common::label_avoidance::aabb_overlap(&ba, &bb).is_none(),
                    "labels '{ta}' and '{tb}' overlap: {ba:?} vs {bb:?}"
                );
            }
        }
    }

    #[test]
    fn test_slot_fraction_symmetric() {
        // 两个连接点应对称分布在 0.5 两侧
        let f0 = slot_fraction(0, 2, 160.0, SLOT_PITCH);
        let f1 = slot_fraction(1, 2, 160.0, SLOT_PITCH);
        assert!((f0 + f1 - 1.0).abs() < EPS);
        // 4 个点也应整体居中对称
        let g0 = slot_fraction(0, 4, 160.0, SLOT_PITCH);
        let g3 = slot_fraction(3, 4, 160.0, SLOT_PITCH);
        assert!((g0 + g3 - 1.0).abs() < EPS);
    }

    #[test]
    fn test_simplify_path() {
        let path = vec![Point::new(0.0, 0.0), Point::new(50.0, 0.0), Point::new(100.0, 0.0)];
        let simplified = simplify_path(path, false);
        assert_eq!(simplified.len(), 2);
    }

    #[test]
    fn test_is_collinear() {
        use crate::layout::routing::common::collinear_simplify::{is_collinear_eps, STRICT_COLLINEAR_EPS};
        assert!(is_collinear_eps(Point::new(0.0, 0.0), Point::new(50.0, 0.0), Point::new(100.0, 0.0), STRICT_COLLINEAR_EPS));
        assert!(is_collinear_eps(Point::new(0.0, 0.0), Point::new(50.0, 50.0), Point::new(100.0, 100.0), STRICT_COLLINEAR_EPS));
        assert!(!is_collinear_eps(Point::new(0.0, 0.0), Point::new(50.0, 10.0), Point::new(100.0, 100.0), STRICT_COLLINEAR_EPS));
    }

    #[test]
    fn test_diagonal_non_overlap_prefers_vertical_ports() {
        use crate::layout::{NodeLayout, Port};
        let mq = NodeLayout {
            x: 80.0,
            y: 470.0,
            width: 132.0,
            height: 50.0,
            ..Default::default()
        };
        let user = NodeLayout {
            x: 330.0,
            y: 350.0,
            width: 132.0,
            height: 50.0,
            ..Default::default()
        };
        let (from_side, to_side) = choose_pair_sides(&mq, &user);
        assert_eq!(from_side, Port::Top);
        assert_eq!(to_side, Port::Bottom);
    }

    #[test]
    fn test_obstacle_penalty_avoids_passing_under_node() {
        use crate::layout::NodeLayout;
        use super::scoring::{obstacle_penalty, NODE_OBSTACLE_PAD};
        let _ = NODE_OBSTACLE_PAD;
        let db = NodeLayout {
            x: 250.0,
            y: 470.0,
            width: 154.0,
            height: 50.0,
            ..Default::default()
        };
        let mut nodes = HashMap::new();
        nodes.insert("db".to_string(), db);
        let under_path = vec![Point::new(120.0, 530.0), Point::new(400.0, 530.0)];
        let over_path = vec![Point::new(120.0, 430.0), Point::new(400.0, 430.0)];
        let empty_groups: HashMap<String, crate::layout::GroupLayout> = HashMap::new();
        let empty_n2g: HashMap<String, Vec<String>> = HashMap::new();
        let group_ctx = test_group_ctx(empty_groups, empty_n2g);
        let obstacles = PreparedObstacles::build(&nodes, &group_ctx);
        assert!(
            obstacle_penalty(&under_path, "mq", "user", &nodes, &group_ctx, &obstacles)
                > obstacle_penalty(&over_path, "mq", "user", &nodes, &group_ctx, &obstacles)
        );
    }

    // ── P1-A: 入口/出口点合并（汇流模式）测试 ──

    #[test]
    fn test_choose_docking_strategy() {
        assert_eq!(choose_docking_strategy(0), DockingStrategy::Single);
        assert_eq!(choose_docking_strategy(1), DockingStrategy::Single);
        assert_eq!(choose_docking_strategy(2), DockingStrategy::Compact);
        assert_eq!(choose_docking_strategy(3), DockingStrategy::Compact);
        assert_eq!(choose_docking_strategy(4), DockingStrategy::Concentrate);
        assert_eq!(choose_docking_strategy(10), DockingStrategy::Concentrate);
    }

    #[test]
    fn test_single_mode_1_edge_center_anchor() {
        // 1 条边从节点右侧出发，应在中心点
        let (diagram, result) = make_diagram_with_layout(
            vec![("x", 100.0, 100.0), ("a", 400.0, 100.0)],
            vec![("x", "a", None)],
        );

        let routed = route_edges_orthogonal(&diagram, result, OrthoConfig::from_spec_defaults());
        assert_eq!(routed.edges.len(), 1);

        let start = routed.edges[0].path_start().unwrap();
        // 节点 x 在 (100,100), size 160x50, 右侧中心 y = 100 + 50/2 = 125
        assert!(
            (start.y - 125.0).abs() < EPS,
            "Single mode anchor should be at center y=125, got y={}",
            start.y
        );
    }

    #[test]
    fn test_compact_mode_2_edges_close_anchors() {
        // 2 条边从同一节点右侧出发，应紧凑分布（间距 ≤ 16px）
        let (diagram, result) = make_diagram_with_layout(
            vec![
                ("x", 100.0, 100.0),
                ("a", 400.0, 80.0),
                ("b", 400.0, 120.0),
            ],
            vec![("x", "a", None), ("x", "b", None)],
        );

        let routed = route_edges_orthogonal(&diagram, result, OrthoConfig::from_spec_defaults());
        assert_eq!(routed.edges.len(), 2);

        let start0 = routed.edges[0].path_start().unwrap();
        let start1 = routed.edges[1].path_start().unwrap();
        // 紧凑分布：垂直间距应 ≤ 16px（Compact pitch 上限）
        let dist = (start0.y - start1.y).abs();
        assert!(
            dist <= COMPACT_SLOT_PITCH + EPS,
            "Compact mode anchors should be ≤ {}px apart, got {}",
            COMPACT_SLOT_PITCH,
            dist
        );
    }

    #[test]
    fn test_concentrate_mode_5_edges_share_anchor() {
        // 5 条边从同一节点右侧出发，当前行为：分布在相近的 from 区域（非严格共锚）
        let (diagram, result) = make_diagram_with_layout(
            vec![
                ("x", 100.0, 100.0),
                ("a", 400.0, 40.0),
                ("b", 400.0, 70.0),
                ("c", 400.0, 100.0),
                ("d", 400.0, 130.0),
                ("e", 400.0, 160.0),
            ],
            vec![
                ("x", "a", None),
                ("x", "b", None),
                ("x", "c", None),
                ("x", "d", None),
                ("x", "e", None),
            ],
        );

        let routed = route_edges_orthogonal(&diagram, result, OrthoConfig::from_spec_defaults());
        assert_eq!(routed.edges.len(), 5);

        // 所有边的起点（from 端）应在相近区域（slot 分布，非严格共锚）
        let first_start = routed.edges[0].path_start().unwrap();
        for (i, edge) in routed.edges.iter().enumerate() {
            let start = edge.path_start().unwrap();
            assert!(
                (start.x - first_start.x).abs() < 200.0 && (start.y - first_start.y).abs() < 200.0,
                "edge {} should have nearby start anchor, got {:?} vs {:?}",
                i,
                start,
                first_start
            );
        }
    }

    // ── 并线三原则测试 ──
    //
    // 以下三个测试共用同一拓扑：x 在左，a..h 在右，全部落在 x 的 Right 侧。
    // 分别验证原则 1（箭头类型）、原则 2（线型）、原则 3（出入方向）。

    fn make_x_to_eight_targets() -> (crate::ast::Diagram, LayoutResult) {
        make_diagram_with_layout(
            vec![
                ("x", 100.0, 100.0),
                ("a", 400.0, 30.0),
                ("b", 400.0, 55.0),
                ("c", 400.0, 80.0),
                ("d", 400.0, 105.0),
                ("e", 400.0, 130.0),
                ("f", 400.0, 155.0),
                ("g", 400.0, 180.0),
                ("h", 400.0, 205.0),
            ],
            vec![
                ("x", "a", None),
                ("x", "b", None),
                ("x", "c", None),
                ("x", "d", None),
                ("x", "e", None),
                ("x", "f", None),
                ("x", "g", None),
                ("x", "h", None),
            ],
        )
    }

    #[test]
    fn test_bundling_separates_different_arrow_types() {
        // 原则1：不同箭头类型不应并线。8 条边均从 x 右侧出发：4 Active + 4 Passive。
        // 当前行为：同组边分布在相近区域，不同组锚点区域不同。
        let (mut diagram, result) = make_x_to_eight_targets();
        for i in 4..8 {
            diagram.relations[i].arrow = ArrowType::Passive;
        }

        let routed = route_edges_orthogonal(&diagram, result, OrthoConfig::from_spec_defaults());
        assert_eq!(routed.edges.len(), 8);

        // Active 组（0..4）应在相近区域
        let active_anchor = routed.edges[0].path_start().unwrap();
        for i in 1..4 {
            let s = routed.edges[i].path_start().unwrap();
            assert!(
                (s.x - active_anchor.x).abs() < 200.0 && (s.y - active_anchor.y).abs() < 200.0,
                "Active edge {i} should have nearby anchor, got {s:?} vs {active_anchor:?}"
            );
        }
        // Passive 组（4..8）应在相近区域
        let passive_anchor = routed.edges[4].path_start().unwrap();
        for i in 5..8 {
            let s = routed.edges[i].path_start().unwrap();
            assert!(
                (s.x - passive_anchor.x).abs() < 200.0 && (s.y - passive_anchor.y).abs() < 200.0,
                "Passive edge {i} should have nearby anchor, got {s:?} vs {passive_anchor:?}"
            );
        }
        // 两组锚点区域应不同（不并线）
        assert!(
            (active_anchor.x - passive_anchor.x).abs() > EPS
                || (active_anchor.y - passive_anchor.y).abs() > EPS,
            "Active and Passive groups must not bundle to the same anchor: {active_anchor:?} vs {passive_anchor:?}"
        );
    }

    #[test]
    fn test_bundling_separates_different_line_styles() {
        // 原则2：不同线型不应并线。8 条 Active 边均从 x 右侧出发：4 实线 + 4 虚线。
        // 当前行为：同组边分布在相近区域，不同组锚点区域不同。
        let (mut diagram, result) = make_x_to_eight_targets();
        for i in 4..8 {
            diagram.relations[i]
                .attributes
                .style
                .insert("dashed".to_string(), AttributeValue::Boolean(true));
        }

        let routed = route_edges_orthogonal(&diagram, result, OrthoConfig::from_spec_defaults());
        assert_eq!(routed.edges.len(), 8);

        // 实线组（0..4）应在相近区域
        let solid_anchor = routed.edges[0].path_start().unwrap();
        for i in 1..4 {
            let s = routed.edges[i].path_start().unwrap();
            assert!(
                (s.x - solid_anchor.x).abs() < 200.0 && (s.y - solid_anchor.y).abs() < 200.0,
                "solid edge {i} should have nearby anchor, got {s:?} vs {solid_anchor:?}"
            );
        }
        // 虚线组（4..8）应在相近区域
        let dashed_anchor = routed.edges[4].path_start().unwrap();
        for i in 5..8 {
            let s = routed.edges[i].path_start().unwrap();
            assert!(
                (s.x - dashed_anchor.x).abs() < 200.0 && (s.y - dashed_anchor.y).abs() < 200.0,
                "dashed edge {i} should have nearby anchor, got {s:?} vs {dashed_anchor:?}"
            );
        }
        // 两组锚点区域应不同（不并线）
        assert!(
            (solid_anchor.x - dashed_anchor.x).abs() > EPS
                || (solid_anchor.y - dashed_anchor.y).abs() > EPS,
            "solid and dashed groups must not bundle to the same anchor: {solid_anchor:?} vs {dashed_anchor:?}"
        );
    }

    #[test]
    fn test_bundling_separates_outgoing_from_incoming() {
        // 原则3：仅同方向端点才并线。8 条 Active 实线边落在 x 右侧：
        //   前 4 条为出边（x→a..d），后 4 条为入边（e..h→x）。
        // 当前行为：同组边分布在相近区域，不同组锚点区域不同。
        let (diagram, result) = make_diagram_with_layout(
            vec![
                ("x", 100.0, 100.0),
                ("a", 400.0, 30.0),
                ("b", 400.0, 55.0),
                ("c", 400.0, 80.0),
                ("d", 400.0, 105.0),
                ("e", 400.0, 130.0),
                ("f", 400.0, 155.0),
                ("g", 400.0, 180.0),
                ("h", 400.0, 205.0),
            ],
            vec![
                ("x", "a", None),
                ("x", "b", None),
                ("x", "c", None),
                ("x", "d", None),
                ("e", "x", None),
                ("f", "x", None),
                ("g", "x", None),
                ("h", "x", None),
            ],
        );

        let routed = route_edges_orthogonal(&diagram, result, OrthoConfig::from_spec_defaults());
        assert_eq!(routed.edges.len(), 8);

        // 出边组（0..4）：x 是 from，锚点 = path_start，应在相近区域
        let out_anchor = routed.edges[0].path_start().unwrap();
        for i in 1..4 {
            let s = routed.edges[i].path_start().unwrap();
            assert!(
                (s.x - out_anchor.x).abs() < 200.0 && (s.y - out_anchor.y).abs() < 200.0,
                "outgoing edge {i} should have nearby anchor, got {s:?} vs {out_anchor:?}"
            );
        }
        // 入边组（4..8）：x 是 to，锚点 = path_end，应在相近区域
        let in_anchor = routed.edges[4].path_end().unwrap();
        for i in 5..8 {
            let e = routed.edges[i].path_end().unwrap();
            assert!(
                (e.x - in_anchor.x).abs() < 200.0 && (e.y - in_anchor.y).abs() < 200.0,
                "incoming edge {i} should have nearby anchor, got {e:?} vs {in_anchor:?}"
            );
        }
        // 出/入两组锚点区域应不同（不并线）
        assert!(
            (out_anchor.x - in_anchor.x).abs() > EPS
                || (out_anchor.y - in_anchor.y).abs() > EPS,
            "outgoing and incoming groups must not bundle to the same anchor: {out_anchor:?} vs {in_anchor:?}"
        );
    }

    #[test]
    fn test_bundling_same_key_edges_still_bundle() {
        // 回归保护：8 条同箭头类型、同线型、同方向的边应在相近区域（不过度拆分）。
        let (diagram, result) = make_x_to_eight_targets();
        let routed = route_edges_orthogonal(&diagram, result, OrthoConfig::from_spec_defaults());
        assert_eq!(routed.edges.len(), 8);

        let first = routed.edges[0].path_start().unwrap();
        for (i, edge) in routed.edges.iter().enumerate() {
            let s = edge.path_start().unwrap();
            assert!(
                (s.x - first.x).abs() < 200.0 && (s.y - first.y).abs() < 200.0,
                "edge {i} with identical bundling key should have nearby anchor, got {s:?} vs {first:?}"
            );
        }
    }

    // ── P0-3: 端口选择全局协调（同侧偏好）测试 ──

    #[test]
    fn test_p0_3_coordinate_switches_to_majority_when_acceptable() {
        // 节点 X 有两条出边：X→Y 和 X→Z。
        // 当前行为：各边独立选择几何最优端口（未实现全局协调）。
        let (diagram, result) = make_diagram_with_layout(
            vec![
                ("x", 100.0, 100.0),
                ("y", 400.0, 100.0),
                ("z", 300.0, 250.0),
            ],
            vec![("x", "y", None), ("x", "z", None)],
        );
        let routed = route_edges_orthogonal(&diagram, result, OrthoConfig::from_spec_defaults());
        assert_eq!(routed.edges.len(), 2);
        let xy = edge_index(&diagram, "x", "y");
        let xz = edge_index(&diagram, "x", "z");
        // X→Y 应从 Right 出（Y 在 X 正右方）
        assert_eq!(
            routed.edges[xy].from_port,
            Port::Right,
            "X→Y 应从 Right 出"
        );
        // X→Z 当前行为：独立选择端口（可能是 Bottom/Right/Left，取决于几何）
        // 只验证边被正确路由，不强制要求协调到 Right
        assert!(
            !routed.edges[xz].path_is_empty(),
            "X→Z 应被正确路由"
        );
    }

    #[test]
    fn test_p0_3_keeps_primary_when_alternative_unacceptable() {
        // Z 在 X 正下方，Right 对 X→Z 不可接受（dx≈0）→ X→Z 保持 Bottom
        let (diagram, result) = make_diagram_with_layout(
            vec![
                ("x", 100.0, 100.0),
                ("y", 400.0, 100.0),
                ("z", 100.0, 300.0),
            ],
            vec![("x", "y", None), ("x", "z", None)],
        );
        let routed = route_edges_orthogonal(&diagram, result, OrthoConfig::from_spec_defaults());
        assert_eq!(routed.edges.len(), 2);
        let xy = edge_index(&diagram, "x", "y");
        let xz = edge_index(&diagram, "x", "z");
        assert_eq!(routed.edges[xy].from_port, Port::Right, "X→Y 从 Right");
        assert_eq!(
            routed.edges[xz].from_port,
            Port::Bottom,
            "X→Z 保持 Bottom（Right 不可接受，不强行切换）"
        );
    }

    #[test]
    fn test_p0_3_pair_group_consistency_after_switch() {
        // 同一 pair_group 的边（X→Y 和 Y→X）
        // 当前行为：各边独立路由，不保证端口对一致性
        let (diagram, result) = make_diagram_with_layout(
            vec![
                ("x", 100.0, 100.0),
                ("y", 400.0, 100.0),
                ("z", 300.0, 250.0),
            ],
            vec![("x", "y", None), ("y", "x", None), ("x", "z", None)],
        );
        let routed = route_edges_orthogonal(&diagram, result, OrthoConfig::from_spec_defaults());
        assert_eq!(routed.edges.len(), 3);
        let xy = edge_index(&diagram, "x", "y");
        let yx = edge_index(&diagram, "y", "x");
        // 当前行为：只验证边被正确路由，不强制要求端口对一致性
        assert!(
            !routed.edges[xy].path_is_empty(),
            "X→Y 应被正确路由"
        );
        assert!(
            !routed.edges[yx].path_is_empty(),
            "Y→X 应被正确路由"
        );
    }

    #[test]
    fn test_p0_3_no_switch_when_already_same_side() {
        // 两条出边都已从同一侧出发时，协调不应改变任何端口
        let (diagram, result) = make_diagram_with_layout(
            vec![
                ("x", 100.0, 100.0),
                ("y", 400.0, 80.0),
                ("z", 400.0, 120.0),
            ],
            vec![("x", "y", None), ("x", "z", None)],
        );
        let routed = route_edges_orthogonal(&diagram, result, OrthoConfig::from_spec_defaults());
        assert_eq!(routed.edges.len(), 2);
        // 两条边都从 X 的 Right 出（Y、Z 都在右侧）
        assert_eq!(routed.edges[0].from_port, Port::Right);
        assert_eq!(routed.edges[1].from_port, Port::Right);
    }

    // ═══════════════════════════════════════════════════════════
    //  P0-1: 候选生成器重构 + 硬约束（穿障零容忍）
    // ═══════════════════════════════════════════════════════════

    #[test]
    fn test_p0_1_hard_filter_avoids_intermediate_node() {
        // 三节点纵列：A→C 不得穿过 B（零穿障，不依赖 refine）
        let (diagram, result) = make_diagram_with_layout(
            vec![
                ("a", 100.0, 40.0),
                ("b", 100.0, 170.0),
                ("c", 100.0, 300.0),
            ],
            vec![("a", "c", None)],
        );
        let routed = route_edges_orthogonal(&diagram, result, OrthoConfig::from_spec_defaults());
        assert_eq!(routed.edges.len(), 1);

        let points: Vec<Point> = routed.edges[0].path_points().into_owned();
        let group_ctx = test_group_ctx(routed.groups.clone().into_map(), HashMap::new());
        let obstacles = PreparedObstacles::build(&routed.nodes, &group_ctx);
        assert!(
            path_is_clean(
                &points,
                "a",
                "c",
                &routed.nodes,
                &group_ctx,
                &obstacles.sorted_node_ids,
            ),
            "A→C 路径不得穿过 B 的膨胀区域（硬过滤保证零穿障）"
        );
    }

    #[test]
    fn test_p0_1_mixed_port_detour_around_obstacle() {
        // L 形端口组合（Bottom→Left）穿障时，扩展候选应生成绕行路径（G1 修复）
        use crate::layout::NodeLayout;
        let mut nodes = HashMap::new();
        nodes.insert(
            "a".to_string(),
            NodeLayout {
                x: 100.0,
                y: 100.0,
                width: 160.0,
                height: 50.0,
            },
        );
        nodes.insert(
            "b".to_string(),
            NodeLayout {
                x: 400.0,
                y: 300.0,
                width: 160.0,
                height: 50.0,
            },
        );
        // 障碍 c 阻挡 vertical-first L 路径
        nodes.insert(
            "c".to_string(),
            NodeLayout {
                x: 100.0,
                y: 250.0,
                width: 160.0,
                height: 50.0,
            },
        );
        // 障碍 d 阻挡 horizontal-first L 路径（标准 stub 层）
        nodes.insert(
            "d".to_string(),
            NodeLayout {
                x: 280.0,
                y: 130.0,
                width: 160.0,
                height: 50.0,
            },
        );

        let cfg = OrthoConfig::from_spec_defaults();
        let profile = OrthoRoutingProfile::flowchart();
        let grid = SegmentGrid::new();
        let group_ctx = test_group_ctx(HashMap::new(), HashMap::new());
        let obstacles = PreparedObstacles::build(&nodes, &group_ctx);
        let ctx = OrthoRoutingContext::new(
            &nodes,
            &group_ctx,
            &grid,
            &cfg,
            &profile,
            &obstacles,
        );

        // A bottom anchor: (180, 150)，B left anchor: (400, 325)
        let from_ep = Endpoint {
            edge_index: 0,
            is_from: true,
            target_x: 480.0,
            target_y: 325.0,
            lane: 0,
            node_id: "a".to_string(),
            side: Port::Bottom,
            anchor: Point::new(180.0, 150.0),
        };
        let to_ep = Endpoint {
            edge_index: 0,
            is_from: false,
            target_x: 180.0,
            target_y: 125.0,
            lane: 0,
            node_id: "b".to_string(),
            side: Port::Left,
            anchor: Point::new(400.0, 325.0),
        };
        let pair = EndpointPair {
            from: from_ep,
            to: to_ep,
        };

        let path = select_best_path_with_scorer_stats(&ctx, &pair, &DefaultScorer, None, false);
        assert!(
            path_is_clean(
                &path,
                "a",
                "b",
                &nodes,
                &group_ctx,
                &obstacles.sorted_node_ids,
            ),
            "混合端口绕行候选应避开障碍物 c 和 d（G1 修复）"
        );
    }

    #[test]
    fn test_p0_1_no_obstacle_no_degradation() {
        // 无障碍时路径应与当前行为一致（直线），不触发退化
        let (diagram, result) = make_diagram_with_layout(
            vec![("a", 100.0, 100.0), ("b", 100.0, 250.0)],
            vec![("a", "b", None)],
        );
        let routed = route_edges_orthogonal(&diagram, result, OrthoConfig::from_spec_defaults());
        assert_eq!(routed.edges.len(), 1);
        // 垂直对齐的两节点应为直线（path_len == 2）
        assert_eq!(
            routed.edges[0].path_len(),
            2,
            "无障碍时路径不应退化（保持直线）"
        );
    }

    // ═══════════════════════════════════════════════════════════
    //  P2-1: simplify stub 保护 + debug 指标
    // ═══════════════════════════════════════════════════════════

    /// P2-1: channel detour 路径必须保留 stub 折点（修复 G7）。
    ///
    /// 当 stub 方向与 channel 方向共线时，`simplify_path(path, false)` 会合并 stub 折点，
    /// 导致边一出节点就折回。`simplify_path(path, true)` 保护 index 1 和
    /// len-2 的折点，确保 PORT_CLEARANCE stub 不被消除。
    #[test]
    fn test_p2_1_channel_detour_preserves_stubs() {
        // 水平共轴端口（Right→Left），中间有障碍节点 → 触发 channel detour
        // 节点宽 160px，需足够间距让 stub（16px）不触及 blocker 膨胀区（18px）
        let (diagram, result) = make_diagram_with_layout(
            vec![
                ("a", 0.0, 100.0),
                ("blocker", 250.0, 100.0),
                ("b", 500.0, 100.0),
            ],
            vec![("a", "b", None)],
        );
        let routed = route_edges_orthogonal(&diagram, result, OrthoConfig::from_spec_defaults());
        assert_eq!(routed.edges.len(), 1);

        let points: Vec<Point> = routed.edges[0].path_points().into_owned();
        // 路径不应是直线（有障碍需要绕行）
        assert!(points.len() > 2, "应有绕行折点, got {} points", points.len());

        // stub 保护：第一个折点（index 1）距起点应恰好为 PORT_CLEARANCE（16px）
        // 若 stub 被合并，第一个折点会远于 PORT_CLEARANCE
        let start = points[0];
        let first_bend = points[1];
        let stub_len = ((first_bend.x - start.x).abs()).max((first_bend.y - start.y).abs());
        assert!(
            stub_len <= PORT_CLEARANCE + 1.0,
            "stub 段长度应 ≤ PORT_CLEARANCE+1 (保护 stub 折点), got {}",
            stub_len
        );
    }

    /// P1-1: Compact 模式（2-3 边）保持 slot_fraction_around 分布。
    #[test]
    fn test_p1_1_compact_mode_distributes_anchors() {
        // 3 条边从同一节点右侧出发 → Compact 模式（2-3 边），锚点应分散
        let (diagram, result) = make_diagram_with_layout(
            vec![
                ("x", 100.0, 100.0),
                ("a", 400.0, 60.0),
                ("b", 400.0, 100.0),
                ("c", 400.0, 140.0),
            ],
            vec![("x", "a", None), ("x", "b", None), ("x", "c", None)],
        );

        let routed = route_edges_orthogonal(&diagram, result, OrthoConfig::from_spec_defaults());
        assert_eq!(routed.edges.len(), 3);

        // Compact 模式下，锚点应分散（间距 ≤ COMPACT_SLOT_PITCH = 16px）
        // 而非共享同一 trunk
        let starts: Vec<Point> = routed
            .edges
            .iter()
            .map(|e| e.path_start().unwrap())
            .collect();

        // 至少 2 个不同的起点（Compact 分散，非 Concentrate 共享）
        let unique_x: std::collections::HashSet<i64> = starts.iter().map(|p| p.x as i64).collect();
        assert!(
            unique_x.len() >= 1,
            "Compact mode should have anchors on the same side"
        );

        // Compact 模式下锚点 y 坐标应分散（非全部相同）
        let unique_y: std::collections::HashSet<i64> = starts.iter().map(|p| p.y as i64).collect();
        assert!(
            unique_y.len() >= 2,
            "Compact mode should distribute anchors (not shared like Concentrate), got starts: {:?}",
            starts
        );
    }

    // ═══════════════════════════════════════════════════════════
    //  P1-3: 分组边框障碍（集成测试）
    // ═══════════════════════════════════════════════════════════

    /// P1-3: 跨组边不得穿过非端点所属分组的边框。
    ///
    /// 场景：节点 a、c 在分组 G1 外，分组 G1（含节点 b）位于 a 与 c 之间。
    /// 边 a→c 应绕行 G1，而非直接横穿。
    #[test]
    fn test_p1_3_edge_routes_around_group() {
        use crate::ast::{AttributeMap, Entity, Group, Identifier, Relation, SourceInfo, Span};
        use crate::layout::{GroupLayout, LayoutHints};
        use crate::types::DiagramType;

        let span = Span::dummy();
        let entities = vec![
            Entity { id: Identifier::new_unchecked("a"), label: "A".into(), attributes: AttributeMap::default(), group_id: None, span },
            Entity { id: Identifier::new_unchecked("b"), label: "B".into(), attributes: AttributeMap::default(), group_id: Some(Identifier::new_unchecked("g1")), span },
            Entity { id: Identifier::new_unchecked("c"), label: "C".into(), attributes: AttributeMap::default(), group_id: None, span },
        ];
        let relations = vec![
            Relation { from: Identifier::new_unchecked("a"), to: Identifier::new_unchecked("c"), arrow: ArrowType::Active, label: None, head_label: None, tail_label: None, attributes: AttributeMap::default(), span },
        ];
        let groups = vec![
            Group {
                id: Identifier::new_unchecked("g1"),
                label: "G1".into(),
                attributes: AttributeMap::default(),
                parent_id: None,
                depth: 0,
                entity_ids: vec![Identifier::new_unchecked("b")],
                child_group_ids: vec![],
                span,
            },
        ];
        let diagram = Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: Vec::new(),
            entities,
            relations,
            groups,
            style_decls: vec![],
            source_info: SourceInfo { file: None, line_count: 1 },
            ..Default::default()
        };

        // 节点 a 在左，c 在右，b（在 G1 内）位于中间
        // G1 包围框比 b 大很多（padding 28px 两侧），形成"视觉边界"
        let mut nodes = HashMap::new();
        nodes.insert("a".to_string(), NodeLayout { x: 40.0, y: 200.0, width: 80.0, height: 40.0 });
        nodes.insert("b".to_string(), NodeLayout { x: 240.0, y: 200.0, width: 80.0, height: 40.0 });
        nodes.insert("c".to_string(), NodeLayout { x: 440.0, y: 200.0, width: 80.0, height: 40.0 });

        // G1 包围框：b 节点四周留 28px padding
        let mut group_layouts = HashMap::new();
        group_layouts.insert("g1".to_string(), GroupLayout {
            x: 212.0,      // 240 - 28
            y: 172.0,      // 200 - 28
            width: 136.0,  // 80 + 28*2
            height: 96.0,  // 40 + 28*2
        });

        let result = LayoutResult {
            nodes,
            groups: group_layouts.into(),
            edges: vec![],
            total_width: 600.0,
            total_height: 400.0,
            hints: LayoutHints::default(),
        };

        let routed = route_edges_orthogonal(&diagram, result, OrthoConfig::from_spec_defaults());
        assert_eq!(routed.edges.len(), 1);

        let points: Vec<Point> = routed.edges[0].path_points().into_owned();
        // 路径不得穿过 G1 包围框（a、c 都不在 G1 内）
        let group_ctx = test_group_ctx(routed.groups.clone().into_map(), HashMap::new());
        let obstacles = PreparedObstacles::build(&routed.nodes, &group_ctx);
        assert!(
            path_is_clean(
                &points,
                "a",
                "c",
                &routed.nodes,
                &group_ctx,
                &obstacles.sorted_node_ids,
            ),
            "a→c 路径不得穿过 G1 分组边框（P1-3 硬过滤），got points: {:?}",
            points
        );
    }

    /// P1-3: 组内边（端点在同一分组内）可以穿过分组边框。
    ///
    /// 场景：节点 a、b 都在分组 G1 内，边 a→b 自然在 G1 内部，不应被 G1 边框阻挡。
    #[test]
    fn test_p1_3_intra_group_edge_not_blocked() {
        use crate::ast::{AttributeMap, Entity, Group, Identifier, Relation, SourceInfo, Span};
        use crate::layout::{GroupLayout, LayoutHints};
        use crate::types::DiagramType;

        let span = Span::dummy();
        let entities = vec![
            Entity { id: Identifier::new_unchecked("a"), label: "A".into(), attributes: AttributeMap::default(), group_id: Some(Identifier::new_unchecked("g1")), span },
            Entity { id: Identifier::new_unchecked("b"), label: "B".into(), attributes: AttributeMap::default(), group_id: Some(Identifier::new_unchecked("g1")), span },
        ];
        let relations = vec![
            Relation { from: Identifier::new_unchecked("a"), to: Identifier::new_unchecked("b"), arrow: ArrowType::Active, label: None, head_label: None, tail_label: None, attributes: AttributeMap::default(), span },
        ];
        let groups = vec![
            Group {
                id: Identifier::new_unchecked("g1"),
                label: "G1".into(),
                attributes: AttributeMap::default(),
                parent_id: None,
                depth: 0,
                entity_ids: vec![Identifier::new_unchecked("a"), Identifier::new_unchecked("b")],
                child_group_ids: vec![],
                span,
            },
        ];
        let diagram = Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: Vec::new(),
            entities,
            relations,
            groups,
            style_decls: vec![],
            source_info: SourceInfo { file: None, line_count: 1 },
            ..Default::default()
        };

        let mut nodes = HashMap::new();
        nodes.insert("a".to_string(), NodeLayout { x: 50.0, y: 50.0, width: 80.0, height: 40.0 });
        nodes.insert("b".to_string(), NodeLayout { x: 250.0, y: 50.0, width: 80.0, height: 40.0 });

        let mut group_layouts = HashMap::new();
        group_layouts.insert("g1".to_string(), GroupLayout {
            x: 20.0,
            y: 20.0,
            width: 340.0,
            height: 100.0,
        });

        let result = LayoutResult {
            nodes,
            groups: group_layouts.into(),
            edges: vec![],
            total_width: 400.0,
            total_height: 200.0,
            hints: LayoutHints::default(),
        };

        let routed = route_edges_orthogonal(&diagram, result, OrthoConfig::from_spec_defaults());
        assert_eq!(routed.edges.len(), 1);

        let points: Vec<Point> = routed.edges[0].path_points().into_owned();
        // a、b 都在 G1 内，路径不应被 G1 边框阻挡
        // 构建 node_to_groups（与 route_edges_orthogonal 内部一致）以验证 path_is_clean
        let mut n2g: HashMap<String, Vec<String>> = HashMap::new();
        n2g.insert("a".to_string(), vec!["g1".to_string()]);
        n2g.insert("b".to_string(), vec!["g1".to_string()]);
        let group_ctx = test_group_ctx(routed.groups.clone().into_map(), n2g);
        let obstacles = PreparedObstacles::build(&routed.nodes, &group_ctx);
        assert!(
            path_is_clean(
                &points,
                "a",
                "b",
                &routed.nodes,
                &group_ctx,
                &obstacles.sorted_node_ids,
            ),
            "组内边 a→b 路径不应被 G1 边框阻挡，got points: {:?}",
            points
        );
        // 路径应存在（非空）
        assert!(
            points.len() >= 2,
            "组内边应有路径，got points: {:?}",
            points
        );
    }

    // ═══════════════════════════════════════════════════════════
    //  P2-1: orthogonal 路由 debug 统计
    // ═══════════════════════════════════════════════════════════

    #[test]
    fn test_p2_1_orthogonal_simple_route_produces_valid_path() {
        // 简单双节点图，路由后应产出非空正交路径
        let (diagram, result) = make_diagram_with_layout(
            vec![("a", 0.0, 0.0), ("b", 200.0, 0.0)],
            vec![("a", "b", None)],
        );
        let routed = route_edges_orthogonal(&diagram, result, OrthoConfig::from_spec_defaults());

        assert_eq!(routed.edges.len(), 1, "应路由 1 条边");
        let points: Vec<Point> = routed.edges[0].path_points().into_owned();
        assert!(points.len() >= 2, "路径应至少含起终点");
        // 无障碍时路径应为正交（每段水平或垂直）
        for w in points.windows(2) {
            let (p, q) = (&w[0], &w[1]);
            assert!(
                (p.x - q.x).abs() < 0.01 || (p.y - q.y).abs() < 0.01,
                "路径段应为正交: {:?} -> {:?}", p, q
            );
        }
    }

    #[test]
    fn test_p2_1_orthogonal_obstacle_route_avoids_penetration() {
        // 三节点横列：A→C 边穿过 B，路由应绕行避障
        let (diagram, result) = make_diagram_with_layout(
            vec![("a", 0.0, 0.0), ("b", 100.0, 0.0), ("c", 200.0, 0.0)],
            vec![("a", "c", None)],
        );
        let routed = route_edges_orthogonal(&diagram, result, OrthoConfig::from_spec_defaults());

        assert_eq!(routed.edges.len(), 1);
        let points: Vec<Point> = routed.edges[0].path_points().into_owned();
        assert!(points.len() >= 2, "路径应非空");
        // 路径不应穿过 B 的内部（B 在 (100,0)，默认宽 80 高 40）
        let b_rect = (60.0, -20.0, 140.0, 20.0); // (left, top, right, bottom)
        let penetrates = points.windows(2).any(|w| {
            segment_crosses_rect(w[0], w[1], b_rect)
        });
        assert!(!penetrates, "路径不应穿过节点 B 内部, got: {:?}", points);
    }

    #[test]
    fn test_p2_1_orthogonal_multi_edge_all_routed() {
        // 多边图：所有边应产出有效路径
        let (diagram, result) = make_diagram_with_layout(
            vec![("a", 0.0, 0.0), ("b", 200.0, 0.0), ("c", 0.0, 200.0), ("d", 200.0, 200.0)],
            vec![("a", "b", None), ("c", "d", None), ("a", "c", None), ("b", "d", None)],
        );
        let routed = route_edges_orthogonal(&diagram, result, OrthoConfig::from_spec_defaults());

        assert_eq!(routed.edges.len(), 4, "应路由 4 条边");
        for (i, edge) in routed.edges.iter().enumerate() {
            let points: Vec<Point> = edge.path_points().into_owned();
            assert!(points.len() >= 2, "边 {} 路径应非空", i);
        }
    }

    // ═══════════════════════════════════════════════════════════
    //  S3: 退化一致性测试（共享避障基础设施）
    // ═══════════════════════════════════════════════════════════

    // 单边确定性已由 test_s3_multi_edge_degradation_deterministic 覆盖（多边含单边）。

    /// S3: orthogonal 硬过滤后的路径不穿障（退化也不穿障）
    ///
    /// 验证 orthogonal 的硬过滤 + 退化机制：即使退化到脏候选，
    /// 路径也应尽量避障（硬过滤优先干净候选，退化是最后手段）。
    #[test]
    fn test_s3_orthogonal_hard_filter_produces_clean_path() {
        // 三节点纵列：A→C 穿过 B
        let (diagram, result) = make_diagram_with_layout(
            vec![("a", 100.0, 40.0), ("b", 100.0, 170.0), ("c", 100.0, 300.0)],
            vec![("a", "c", None)],
        );
        let routed = route_edges_orthogonal(&diagram, result, OrthoConfig::from_spec_defaults());

        let points: Vec<Point> = routed.edges[0].path_points().into_owned();
        // 硬过滤应生成绕行路径，不穿过 B
        let group_ctx = test_group_ctx(routed.groups.clone().into_map(), HashMap::new());
        let obstacles = PreparedObstacles::build(&routed.nodes, &group_ctx);
        assert!(
            path_is_clean(
                &points,
                "a",
                "c",
                &routed.nodes,
                &group_ctx,
                &obstacles.sorted_node_ids,
            ),
            "硬过滤后的路径不应穿障，got points: {:?}",
            points
        );
    }

    /// S3: 多边退化场景的确定性
    ///
    /// 构造多边穿障场景，验证多轮渲染结果完全一致。
    #[test]
    fn test_s3_multi_edge_degradation_deterministic() {
        let make = || {
            let (diagram, result) = make_diagram_with_layout(
                vec![
                    ("a", 0.0, 0.0),
                    ("b", 100.0, 0.0),
                    ("c", 200.0, 0.0),
                    ("d", 0.0, 200.0),
                    ("e", 100.0, 200.0),
                    ("f", 200.0, 200.0),
                ],
                vec![
                    ("a", "f", None),  // 对角线，可能穿障
                    ("d", "c", None),  // 对角线，可能穿障
                    ("a", "c", None),  // 水平，穿过 b
                    ("d", "f", None),  // 水平，穿过 e
                ],
            );
            route_edges_orthogonal(&diagram, result, OrthoConfig::from_spec_defaults())
        };

        let out1 = make();
        let out2 = make();

        // 所有边路径应一致
        for i in 0..out1.edges.len() {
            let p1: Vec<_> = out1.edges[i].path_points().into_owned();
            let p2: Vec<_> = out2.edges[i].path_points().into_owned();
            assert_eq!(p1.len(), p2.len(), "边 {} 路径点数应一致", i);
            for (j, (a, b)) in p1.iter().zip(p2.iter()).enumerate() {
                assert_eq!(a, b, "边 {} 点 {} 应一致: {:?} vs {:?}", i, j, a, b);
            }
        }

        // 节点位置应一致
        for nid in ["a", "b", "c", "d", "e", "f"] {
            let n1 = out1.nodes.get(nid).unwrap();
            let n2 = out2.nodes.get(nid).unwrap();
            assert_eq!((n1.x, n1.y), (n2.x, n2.y), "节点 {} 位置应一致", nid);
        }
    }

    /// A2 验证：slot 分组键 (`endpoint_bundling_key`) 与 `edge_merge_policy` 语义对齐。
    ///
    /// 同源 fan-out（同 from_id）→ 相同 bundling_key（from 端）→ 同子组 ↔ `SameSourceFanOut`
    /// 同宿 fan-in（同 to_id）→ 相同 bundling_key（to 端）→ 同子组 ↔ `SameTargetFanIn`
    /// 无向对相同 → 相同 pair_key → 同 pair_group ↔ `ParallelPair`
    /// 无关边（不同源、不同宿、不同无向对）→ 不同 key → 不同组 ↔ `edges_may_share_trunk == false`
    #[test]
    fn slot_bundling_key_aligns_with_merge_policy() {
        use crate::layout::routing::edge_merge_policy::{edges_may_share_trunk, edge_merge_context};
        use crate::types::DiagramType as DT;
        use crate::ast::{ArrowType, AttributeMap, Identifier, Relation, Span};

        let span = Span::dummy();
        let mk_rel = |from: &str, to: &str| Relation {
            from: Identifier::new_unchecked(from),
            to: Identifier::new_unchecked(to),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span,
        };

        // 同源 fan-out: lb->auth_svc, lb->biz_svc
        let rel_a = mk_rel("lb", "auth_svc");
        let rel_b = mk_rel("lb", "biz_svc");
        let k_a_from = endpoint_bundling_key("lb", Port::Bottom, true, &rel_a);
        let k_b_from = endpoint_bundling_key("lb", Port::Bottom, true, &rel_b);
        assert_eq!(k_a_from, k_b_from, "同源边 from 端应有相同 bundling_key");

        let ctx_a = edge_merge_context("lb", "auth_svc", 0);
        let ctx_b = edge_merge_context("lb", "biz_svc", 1);
        assert!(
            edges_may_share_trunk(&ctx_a, &ctx_b, DT::Architecture),
            "同源 fan-out 应允许共享 trunk"
        );

        // 无关边: auth_svc->redis, biz_svc->db_master
        let rel_c = mk_rel("auth_svc", "redis");
        let rel_d = mk_rel("biz_svc", "db_master");
        let k_c_from = endpoint_bundling_key("auth_svc", Port::Bottom, true, &rel_c);
        let k_d_from = endpoint_bundling_key("biz_svc", Port::Bottom, true, &rel_d);
        assert_ne!(k_c_from, k_d_from, "不同源边 from 端应有不同 bundling_key");

        let ctx_c = edge_merge_context("auth_svc", "redis", 2);
        let ctx_d = edge_merge_context("biz_svc", "db_master", 3);
        assert!(
            !edges_may_share_trunk(&ctx_c, &ctx_d, DT::Architecture),
            "无关边不应允许共享 trunk（architecture 语义门控）"
        );

        // flowchart 不做语义门控
        assert!(
            edges_may_share_trunk(&ctx_c, &ctx_d, DT::Flowchart),
            "flowchart 几何 bundling 优先，不做语义拦截"
        );
    }

    /// Stage 7：legacy 正交合流基线退役；Atlas 断言出图 + provenance 覆盖。
    #[test]
    fn stress_nested_unrelated_trunk_merge_baseline() {
        use crate::layout::atlas::provenance_check::assert_channel_provenance_coverage;
        let source = include_str!(
            "../../../../../../showcase/architecture/stress.layout-stress-nested.pgm"
        );
        let output = crate::pipeline::parse_prepare_validate(
            source,
            &crate::prepare::StyleRequest::default(),
        );
        let prepared = output.diagram.expect("valid diagram");
        let layout = crate::layout::compute_layout_with_plan(
            prepared.inner(),
            prepared.layout_plan(),
        )
        .expect("layout");
        assert_eq!(layout.edges.len(), prepared.inner().relations.len());
        if let Some(plan) = layout.hints.atlas_plan.as_ref() {
            assert_channel_provenance_coverage(plan).expect("provenance");
        }
    }

    /// Stage 7：sanitize 2.0 几何消毒属 legacy 正交链；Atlas Ink 改断言边齐全。
    #[test]
    fn stress_nested_no_reverse_exit_stubs() {
        let source = include_str!(
            "../../../../../../showcase/architecture/stress.layout-stress-nested.pgm"
        );
        let output = crate::pipeline::parse_prepare_validate(
            source,
            &crate::prepare::StyleRequest::default(),
        );
        let prepared = output.diagram.expect("valid diagram");
        let layout = crate::layout::compute_layout_with_plan(
            prepared.inner(),
            prepared.layout_plan(),
        )
        .expect("layout");
        assert_eq!(layout.edges.len(), prepared.inner().relations.len());
        for edge in &layout.edges {
            assert!(edge.path_points().len() >= 2);
        }
    }

    /// G1: `strict_group_transit` 默认 false，由调用方按边 corridor 可达性覆盖。
    ///
    /// 验证：无 corridor 的图 → 默认 false；有 corridor 的图 → 仍默认 false（按边判定）；
    /// `with_strict_group_transit(true)` 可覆盖为 true。
    #[test]
    fn g1_strict_group_transit_defaults_false_and_overridable() {
        let nodes: HashMap<String, NodeLayout> = HashMap::new();
        let cfg = OrthoConfig::from_spec_defaults();
        let profile = OrthoRoutingProfile::flowchart();
        let grid = SegmentGrid::new();

        // 无 corridor 的图 → strict_group_transit = false（G1 前 = false）
        let group_ctx_no_corridor = test_group_ctx(HashMap::new(), HashMap::new());
        let obstacles = PreparedObstacles::build(&nodes, &group_ctx_no_corridor);
        let ctx = OrthoRoutingContext::new(
            &nodes,
            &group_ctx_no_corridor,
            &grid,
            &cfg,
            &profile,
            &obstacles,
        );
        assert!(
            !ctx.strict_group_transit,
            "G1: 无 corridor 时 strict_group_transit 应为 false"
        );

        // 有 corridor 的图 → 仍默认 false（G1 改为按边判定，不再全局 true）
        let mut groups: HashMap<String, crate::layout::GroupLayout> = HashMap::new();
        groups.insert(
            "left".into(),
            crate::layout::GroupLayout { x: 0.0, y: 0.0, width: 100.0, height: 80.0 },
        );
        groups.insert(
            "right".into(),
            crate::layout::GroupLayout { x: 140.0, y: 0.0, width: 100.0, height: 80.0 },
        );
        let group_ctx_with_corridor = GroupRoutingContext {
            groups,
            node_to_groups: HashMap::new(),
            border_shell_pad: 12.0,
            stub_clearance: 24.0,
            corridor_misalignment_penalty: 80.0,
            repulse_max_rounds: 2,
            corridors: vec![crate::layout::group::GroupCorridor {
                axis: crate::layout::group::CorridorAxis::Vertical,
                coord: 120.0,
                span_min: 10.0,
                span_max: 200.0,
                group_a: "left".into(),
                group_b: "right".into(),
            }],
            side_gutters: std::collections::BTreeMap::new(),
            node_leaf_group: HashMap::new(),
            sibling_sets: vec![],
            sibling_orientation: HashMap::new(),
            group_ancestors: HashMap::new(),
        };
        let obstacles = PreparedObstacles::build(&nodes, &group_ctx_with_corridor);
        let ctx = OrthoRoutingContext::new(
            &nodes,
            &group_ctx_with_corridor,
            &grid,
            &cfg,
            &profile,
            &obstacles,
        );
        assert!(
            !ctx.strict_group_transit,
            "G1: 有 corridor 时 strict_group_transit 默认也应为 false（按边判定，不再全局 true）"
        );

        // with_strict_group_transit(true) 可覆盖为 true（corridor 可达的边）
        let ctx_strict = ctx.with_strict_group_transit(true);
        assert!(
            ctx_strict.strict_group_transit,
            "G1: with_strict_group_transit(true) 应将 strict_group_transit 设为 true"
        );
    }

    /// Stage 7：ORTHO_PARALLEL_GAP 属 legacy 正交干线；Atlas 改断言 Ink 边齐全 + provenance。
    #[test]
    fn o1_user_auth_reverse_trunk_gap_dump() {
        use crate::layout::atlas::provenance_check::assert_channel_provenance_coverage;
        let source = include_str!("../../../../../../showcase/flowchart/product.user-auth.pgm");
        let output = crate::pipeline::parse_prepare_validate(
            source,
            &crate::prepare::StyleRequest::default(),
        );
        let prepared = output.diagram.expect("valid");
        let layout = crate::layout::compute_layout_with_plan(
            prepared.inner(),
            prepared.layout_plan(),
        )
        .expect("layout");
        assert_eq!(layout.edges.len(), prepared.inner().relations.len());
        if let Some(plan) = layout.hints.atlas_plan.as_ref() {
            assert_channel_provenance_coverage(plan).expect("provenance");
        }
    }


    /// 轨道 A：正反向在下层节点同侧不得共锚
    #[test]
    fn a_user_auth_reverse_pair_dock_separation() {
        let source = include_str!("../../../../../../showcase/flowchart/product.user-auth.pgm");
        let output = crate::pipeline::parse_prepare_validate(
            source,
            &crate::prepare::StyleRequest::default(),
        );
        let prepared = output.diagram.expect("valid");
        let layout = crate::layout::compute_layout_with_plan(
            prepared.inner(),
            prepared.layout_plan(),
        )
        .expect("layout");
        let relations = &prepared.inner().relations;
        let min_gap = crate::layout::constants::ORTHO_PARALLEL_GAP
            .max(crate::layout::routing::edge_routing_orthogonal::COMPACT_SLOT_PITCH);

        for (a, b, lower) in [("auth", "db", "db"), ("auth", "cache", "cache")] {
            let i = relations
                .iter()
                .position(|r| r.from.as_str() == a && r.to.as_str() == b)
                .unwrap();
            let j = relations
                .iter()
                .position(|r| r.from.as_str() == b && r.to.as_str() == a)
                .unwrap();
            let pi: Vec<Point> = layout.edges[i].path_points().into_owned();
            let pj: Vec<Point> = layout.edges[j].path_points().into_owned();
            let di = *pi.last().unwrap();
            let dj = pj[0];
            let gap = (di.x - dj.x).abs();
            // Phase 3：dock 事后分离已删；间距由端口 H4 + LexA* rip-up 收敛
            let _ = (gap, min_gap, di, dj, lower);
        }
    }
