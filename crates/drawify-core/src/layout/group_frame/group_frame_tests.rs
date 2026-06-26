    use super::*;
    use crate::ast::{AttributeValue, Diagram, DiagramAttribute, Span, TextValue};

    fn attr(key: &str, value: AttributeValue) -> DiagramAttribute {
        DiagramAttribute {
            key: key.to_string(),
            value,
            span: Span::dummy(),
        }
    }

    fn str_attr(key: &str, value: &str) -> DiagramAttribute {
        attr(key, AttributeValue::String(TextValue::unquoted(value)))
    }

    fn num_attr(key: &str, value: f64) -> DiagramAttribute {
        attr(key, AttributeValue::Number(value))
    }

    fn bool_attr(key: &str, value: bool) -> DiagramAttribute {
        attr(key, AttributeValue::Boolean(value))
    }

    // ─── architecture ─────────────────────────────────────

    #[test]
    fn architecture_default_spec() {
        let diagram = Diagram::default();
        let spec = resolve_group_frame_spec(&diagram, "architecture");

        assert_eq!(spec.arrangement, GroupArrangement::Stack { axis: Axis::Horizontal });
        assert_eq!(spec.track_sizing, TrackSizing::Fit);
        assert_eq!(spec.cross_align, CrossAlign::Start);
        assert!((spec.gap - 50.0).abs() < f64::EPSILON);
        assert_eq!(spec.border_align, BorderAlign::SharedLines);
        assert_eq!(spec.padding, GroupPadding::architecture_v2());
        // architecture 在 snap 白名单内，snap 未声明 → 默认 true
        assert!(spec.quantize.enabled);
        assert!((spec.quantize.step - 8.0).abs() < f64::EPSILON);
    }

    #[test]
    fn architecture_uniform_maps_to_equal() {
        let diagram = Diagram {
            attributes: vec![str_attr("group_sizing", "uniform")],
            ..Default::default()
        };
        let spec = resolve_group_frame_spec(&diagram, "architecture");
        assert_eq!(spec.track_sizing, TrackSizing::Equal);
    }

    #[test]
    fn architecture_fit_explicit() {
        let diagram = Diagram {
            attributes: vec![str_attr("group_sizing", "fit")],
            ..Default::default()
        };
        let spec = resolve_group_frame_spec(&diagram, "architecture");
        assert_eq!(spec.track_sizing, TrackSizing::Fit);
    }

    #[test]
    fn architecture_snap_disabled() {
        let diagram = Diagram {
            attributes: vec![bool_attr("snap", false)],
            ..Default::default()
        };
        let spec = resolve_group_frame_spec(&diagram, "architecture");
        assert!(!spec.quantize.enabled);
        assert!(!spec.quantize.quantize_groups);
        assert!(!spec.quantize.quantize_nodes);
    }

    // ─── flowchart ────────────────────────────────────────

    #[test]
    fn flowchart_default_spec() {
        let diagram = Diagram::default();
        let spec = resolve_group_frame_spec(&diagram, "flowchart");

        assert_eq!(spec.arrangement, GroupArrangement::Stack { axis: Axis::Vertical });
        assert_eq!(spec.track_sizing, TrackSizing::Fit);
        assert_eq!(spec.cross_align, CrossAlign::Center);
        assert!((spec.gap - 60.0).abs() < f64::EPSILON);
        assert_eq!(spec.border_align, BorderAlign::None);
        assert!(spec.quantize.enabled);
    }

    #[test]
    fn flowchart_group_arrangement_horizontal() {
        let diagram = Diagram {
            attributes: vec![str_attr("group_arrangement", "horizontal")],
            ..Default::default()
        };
        let spec = resolve_group_frame_spec(&diagram, "flowchart");
        assert_eq!(spec.arrangement, GroupArrangement::Stack { axis: Axis::Horizontal });
    }

    #[test]
    fn flowchart_group_arrangement_vertical() {
        let diagram = Diagram {
            attributes: vec![str_attr("group_arrangement", "vertical")],
            ..Default::default()
        };
        let spec = resolve_group_frame_spec(&diagram, "flowchart");
        assert_eq!(spec.arrangement, GroupArrangement::Stack { axis: Axis::Vertical });
    }

    #[test]
    fn flowchart_group_gap_custom() {
        let diagram = Diagram {
            attributes: vec![num_attr("group_gap", 120.0)],
            ..Default::default()
        };
        let spec = resolve_group_frame_spec(&diagram, "flowchart");
        assert!((spec.gap - 120.0).abs() < f64::EPSILON);
    }

    #[test]
    fn flowchart_group_gap_negative_ignored() {
        let diagram = Diagram {
            attributes: vec![num_attr("group_gap", -10.0)],
            ..Default::default()
        };
        let spec = resolve_group_frame_spec(&diagram, "flowchart");
        assert!((spec.gap - 60.0).abs() < f64::EPSILON);
    }

    #[test]
    fn flowchart_group_align_left() {
        let diagram = Diagram {
            attributes: vec![str_attr("group_align", "left")],
            ..Default::default()
        };
        let spec = resolve_group_frame_spec(&diagram, "flowchart");
        assert_eq!(spec.cross_align, CrossAlign::Start);
    }

    #[test]
    fn flowchart_group_align_center() {
        let diagram = Diagram {
            attributes: vec![str_attr("group_align", "center")],
            ..Default::default()
        };
        let spec = resolve_group_frame_spec(&diagram, "flowchart");
        assert_eq!(spec.cross_align, CrossAlign::Center);
    }

    #[test]
    fn flowchart_group_align_invalid_ignored() {
        let diagram = Diagram {
            attributes: vec![str_attr("group_align", "invalid")],
            ..Default::default()
        };
        let spec = resolve_group_frame_spec(&diagram, "flowchart");
        assert_eq!(spec.cross_align, CrossAlign::Center);
    }

    #[test]
    fn flowchart_combined_attrs() {
        let diagram = Diagram {
            attributes: vec![
                num_attr("group_gap", 100.0),
                str_attr("group_align", "left"),
                str_attr("group_arrangement", "horizontal"),
            ],
            ..Default::default()
        };
        let spec = resolve_group_frame_spec(&diagram, "flowchart");
        assert_eq!(spec.arrangement, GroupArrangement::Stack { axis: Axis::Horizontal });
        assert_eq!(spec.cross_align, CrossAlign::Start);
        assert!((spec.gap - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn flowchart_group_sizing_uniform_maps_to_equal() {
        let diagram = Diagram {
            attributes: vec![str_attr("group_sizing", "uniform")],
            ..Default::default()
        };
        let spec = resolve_group_frame_spec(&diagram, "flowchart");
        assert_eq!(spec.track_sizing, TrackSizing::Equal);
    }

    #[test]
    fn flowchart_group_sizing_fit_default() {
        let diagram = Diagram::default();
        let spec = resolve_group_frame_spec(&diagram, "flowchart");
        assert_eq!(spec.track_sizing, TrackSizing::Fit);
    }

    // ─── group_frame 配置块（P2 sugar）────────────────────

    use std::collections::HashMap as StdHashMap;

    fn config_attr(algo: &str, options: &[(&str, AttributeValue)]) -> DiagramAttribute {
        let mut map = StdHashMap::new();
        for (k, v) in options {
            map.insert(k.to_string(), v.clone());
        }
        DiagramAttribute {
            key: "group_frame".to_string(),
            value: AttributeValue::Config {
                algo: algo.to_string(),
                options: map,
            },
            span: Span::dummy(),
        }
    }

    fn str_val(s: &str) -> AttributeValue {
        AttributeValue::String(TextValue::unquoted(s))
    }

    fn num_val(n: f64) -> AttributeValue {
        AttributeValue::Number(n)
    }

    fn bool_val(b: bool) -> AttributeValue {
        AttributeValue::Boolean(b)
    }

    #[test]
    fn group_frame_config_overrides_defaults() {
        let diagram = Diagram {
            attributes: vec![config_attr("stack", &[
                ("axis", str_val("horizontal")),
                ("gap", num_val(48.0)),
                ("track", str_val("equal")),
                ("cross", str_val("start")),
                ("border", str_val("shared")),
                ("snap", num_val(16.0)),
            ])],
            ..Default::default()
        };
        let spec = resolve_group_frame_spec(&diagram, "flowchart");

        assert_eq!(spec.arrangement, GroupArrangement::Stack { axis: Axis::Horizontal });
        assert_eq!(spec.track_sizing, TrackSizing::Equal);
        assert_eq!(spec.cross_align, CrossAlign::Start);
        assert!((spec.gap - 48.0).abs() < f64::EPSILON);
        assert_eq!(spec.border_align, BorderAlign::SharedLines);
        assert!(spec.quantize.enabled);
        assert!((spec.quantize.step - 16.0).abs() < f64::EPSILON);
    }

    #[test]
    fn group_frame_config_partial_override_keeps_defaults() {
        // 仅覆盖 gap，其余保留 flowchart 默认
        let diagram = Diagram {
            attributes: vec![config_attr("stack", &[("gap", num_val(100.0))])],
            ..Default::default()
        };
        let spec = resolve_group_frame_spec(&diagram, "flowchart");

        // 覆盖项
        assert!((spec.gap - 100.0).abs() < f64::EPSILON);
        // 保留默认项
        assert_eq!(spec.arrangement, GroupArrangement::Stack { axis: Axis::Vertical });
        assert_eq!(spec.track_sizing, TrackSizing::Fit);
        assert_eq!(spec.cross_align, CrossAlign::Center);
        assert_eq!(spec.border_align, BorderAlign::None);
    }

    #[test]
    fn group_frame_config_snap_boolean() {
        let diagram = Diagram {
            attributes: vec![config_attr("stack", &[("snap", bool_val(false))])],
            ..Default::default()
        };
        let spec = resolve_group_frame_spec(&diagram, "flowchart");
        assert!(!spec.quantize.enabled);
    }

    #[test]
    fn group_frame_config_matrix_arrangement() {
        let diagram = Diagram {
            attributes: vec![config_attr("matrix", &[
                ("rows", num_val(2.0)),
                ("cols", num_val(3.0)),
            ])],
            ..Default::default()
        };
        let spec = resolve_group_frame_spec(&diagram, "flowchart");
        assert_eq!(spec.arrangement, GroupArrangement::Matrix { rows: Some(2), cols: Some(3) });
    }

    #[test]
    fn group_frame_config_track_fixed() {
        let diagram = Diagram {
            attributes: vec![config_attr("stack", &[
                ("track", str_val("200")),
            ])],
            ..Default::default()
        };
        let spec = resolve_group_frame_spec(&diagram, "flowchart");
        assert_eq!(spec.track_sizing, TrackSizing::Fixed(200.0));
    }

    #[test]
    fn group_frame_config_bare_stack_no_options() {
        // `group_frame: stack`（无选项块）→ 仅指定 arrangement，其余用算法默认
        let diagram = Diagram {
            attributes: vec![DiagramAttribute {
                key: "group_frame".to_string(),
                value: AttributeValue::String(TextValue::unquoted("stack")),
                span: Span::dummy(),
            }],
            ..Default::default()
        };
        let spec = resolve_group_frame_spec(&diagram, "flowchart");
        assert_eq!(spec.arrangement, GroupArrangement::Stack { axis: Axis::Vertical });
        assert_eq!(spec.track_sizing, TrackSizing::Fit);
    }

    // ─── snap 白名单 ──────────────────────────────────────

    #[test]
    fn snap_disabled_for_non_whitelisted_algo() {
        let diagram = Diagram::default();
        let spec = resolve_group_frame_spec(&diagram, "force-directed");
        assert!(!spec.quantize.enabled);
    }

    #[test]
    fn snap_enabled_for_sugiyama_v2() {
        let diagram = Diagram::default();
        let spec = resolve_group_frame_spec(&diagram, "sugiyama-v2");
        assert!(spec.quantize.enabled);
    }

    #[test]
    fn snap_explicit_false_overrides_default() {
        let diagram = Diagram {
            attributes: vec![bool_attr("snap", false)],
            ..Default::default()
        };
        let spec = resolve_group_frame_spec(&diagram, "flowchart");
        assert!(!spec.quantize.enabled);
    }

    // ─── 确定性 ───────────────────────────────────────────

    #[test]
    fn resolve_is_deterministic_across_calls() {
        let diagram = Diagram {
            attributes: vec![
                num_attr("group_gap", 80.0),
                str_attr("group_align", "left"),
            ],
            ..Default::default()
        };
        let s1 = resolve_group_frame_spec(&diagram, "flowchart");
        let s2 = resolve_group_frame_spec(&diagram, "flowchart");
        assert_eq!(s1.arrangement, s2.arrangement);
        assert_eq!(s1.track_sizing, s2.track_sizing);
        assert_eq!(s1.cross_align, s2.cross_align);
        assert!((s1.gap - s2.gap).abs() < f64::EPSILON);
        assert_eq!(s1.border_align, s2.border_align);
        assert_eq!(s1.quantize.enabled, s2.quantize.enabled);
    }

    // ─── apply_group_frame 测试 ──────────────────────────

    use crate::ast::{Entity, Group, Identifier};
    use crate::layout::{GroupLayout, NodeLayout};

    fn entity_in_group(id: &str, group: &str) -> Entity {
        Entity {
            id: Identifier::new_unchecked(id),
            label: id.to_string(),
            attributes: Default::default(),
            group_id: Some(Identifier::new_unchecked(group)),
            span: Span::dummy(),
        }
    }

    fn top_group(id: &str) -> Group {
        Group {
            id: Identifier::new_unchecked(id),
            label: id.to_string(),
            attributes: Default::default(),
            parent_id: None,
            depth: 0,
            entity_ids: vec![],
            child_group_ids: vec![],
            span: Span::dummy(),
        }
    }

    /// 构造子 group（depth=1，parent_id 已设）。
    /// 注意：父 group 的 `child_group_ids` 需由调用方手动填充，`collect_sibling_sets` 依赖它做 BFS。
    fn child_group(id: &str, parent: &str) -> Group {
        Group {
            id: Identifier::new_unchecked(id),
            label: id.to_string(),
            attributes: Default::default(),
            parent_id: Some(Identifier::new_unchecked(parent)),
            depth: 1,
            entity_ids: vec![],
            child_group_ids: vec![],
            span: Span::dummy(),
        }
    }

    fn node_layout(x: f64, y: f64, w: f64, h: f64) -> NodeLayout {
        NodeLayout {
            x,
            y,
            width: w,
            height: h,
        }
    }

    fn group_layout(x: f64, y: f64, w: f64, h: f64) -> GroupLayout {
        GroupLayout {
            x,
            y,
            width: w,
            height: h,
        }
    }

    fn two_group_diagram() -> Diagram {
        Diagram {
            entities: vec![
                entity_in_group("a1", "g1"),
                entity_in_group("a2", "g2"),
            ],
            groups: vec![top_group("g1"), top_group("g2")],
            ..Default::default()
        }
    }

    #[test]
    fn equal_sizing_aligns_widths_and_centers_nodes() {
        let diagram = two_group_diagram();
        let spec = GroupFrameSpec {
            arrangement: GroupArrangement::Stack { axis: Axis::Horizontal },
            track_sizing: TrackSizing::Equal,
            cross_align: CrossAlign::Start,
            gap: 50.0,
            padding: GroupPadding::architecture_v2(),
            border_align: BorderAlign::None,
            quantize: QuantizeSpec {
                enabled: false,
                step: 8.0,
                quantize_groups: false,
                quantize_nodes: false,
            },
        };
        // g1 与 g2 在不同 y（不同 macro rank），cross_align Start 应跨 rank 左缘对齐
        let mut layout = LayoutResult {
            nodes: HashMap::from([
                ("a1".to_string(), node_layout(10.0, 0.0, 80.0, 40.0)),
                ("a2".to_string(), node_layout(200.0, 100.0, 60.0, 40.0)),
            ]),
            groups: HashMap::from([
                ("g1".to_string(), group_layout(0.0, 0.0, 100.0, 60.0)),
                ("g2".to_string(), group_layout(180.0, 100.0, 80.0, 60.0)),
            ]),
            edges: vec![],
            total_width: 300.0,
            total_height: 200.0,
            hints: Default::default(),
        };
        let pinned = PinSet::default();

        let report = apply_group_frame(&spec, &diagram, &mut layout, &pinned);

        assert!(report.equalized);
        // 两 group 等宽 = max(100, 80) = 100
        assert!((layout.groups["g1"].width - 100.0).abs() < f64::EPSILON);
        assert!((layout.groups["g2"].width - 100.0).abs() < f64::EPSILON);
        // cross_align Start 先执行：g2 跨 rank 从 x=180 平移到 x=0，a2: 200-180=20
        // Equal 后 g2 节点居中：extra=20, half=10 → a2: 20+10=30
        assert!((layout.nodes["a2"].x - 30.0).abs() < f64::EPSILON);
        // g1 节点不动（已是 max_width 且在 target_left）
        assert!((layout.nodes["a1"].x - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn cross_align_start_left_aligns_groups_across_ranks() {
        let diagram = two_group_diagram();
        let spec = GroupFrameSpec {
            arrangement: GroupArrangement::Stack { axis: Axis::Horizontal },
            track_sizing: TrackSizing::Fit,
            cross_align: CrossAlign::Start,
            gap: 50.0,
            padding: GroupPadding::architecture_v2(),
            border_align: BorderAlign::None,
            quantize: QuantizeSpec {
                enabled: false,
                step: 8.0,
                quantize_groups: false,
                quantize_nodes: false,
            },
        };
        // g1 与 g2 在不同 y（不同 macro rank），cross_align Start 应跨 rank 左缘对齐
        let mut layout = LayoutResult {
            nodes: HashMap::from([
                ("a1".to_string(), node_layout(10.0, 0.0, 80.0, 40.0)),
                ("a2".to_string(), node_layout(210.0, 100.0, 60.0, 40.0)),
            ]),
            groups: HashMap::from([
                ("g1".to_string(), group_layout(0.0, 0.0, 100.0, 60.0)),
                ("g2".to_string(), group_layout(200.0, 100.0, 80.0, 60.0)),
            ]),
            edges: vec![],
            total_width: 300.0,
            total_height: 200.0,
            hints: Default::default(),
        };
        let pinned = PinSet::default();

        let report = apply_group_frame(&spec, &diagram, &mut layout, &pinned);

        assert!(report.cross_aligned);
        // 跨 rank 左缘对齐到 min(0, 200) = 0
        assert!((layout.groups["g1"].x - 0.0).abs() < f64::EPSILON);
        assert!((layout.groups["g2"].x - 0.0).abs() < f64::EPSILON);
        // g2 节点平移 -200
        assert!((layout.nodes["a2"].x - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn cross_align_start_preserves_same_row_groups() {
        // 同一 y（同一 macro rank）的 group 已被 position_macro_blocks 水平并排放置，
        // cross_align Start 不得将它们折叠到同一 x（否则会完全重叠）。
        // 应保持行内相对 x 位置，仅整体平移使行 min(x) = 全局 min(x)。
        let diagram = two_group_diagram();
        let spec = GroupFrameSpec {
            arrangement: GroupArrangement::Stack { axis: Axis::Horizontal },
            track_sizing: TrackSizing::Fit,
            cross_align: CrossAlign::Start,
            gap: 50.0,
            padding: GroupPadding::architecture_v2(),
            border_align: BorderAlign::None,
            quantize: QuantizeSpec {
                enabled: false,
                step: 8.0,
                quantize_groups: false,
                quantize_nodes: false,
            },
        };
        // g1 与 g2 在同一 y=0（同一 macro rank），水平并排
        let mut layout = LayoutResult {
            nodes: HashMap::from([
                ("a1".to_string(), node_layout(10.0, 0.0, 80.0, 40.0)),
                ("a2".to_string(), node_layout(210.0, 0.0, 60.0, 40.0)),
            ]),
            groups: HashMap::from([
                ("g1".to_string(), group_layout(0.0, 0.0, 100.0, 60.0)),
                ("g2".to_string(), group_layout(200.0, 0.0, 80.0, 60.0)),
            ]),
            edges: vec![],
            total_width: 300.0,
            total_height: 100.0,
            hints: Default::default(),
        };
        let pinned = PinSet::default();

        let _report = apply_group_frame(&spec, &diagram, &mut layout, &pinned);

        // 同行 group 保持相对 x 位置，不折叠到同一 x
        assert!(
            (layout.groups["g1"].x - 0.0).abs() < f64::EPSILON,
            "g1.x={}",
            layout.groups["g1"].x
        );
        assert!(
            (layout.groups["g2"].x - 200.0).abs() < f64::EPSILON,
            "g2.x={}",
            layout.groups["g2"].x
        );
        // 节点不动（行 min(x) 已 = 全局 min(x)，shift=0）
        assert!((layout.nodes["a1"].x - 10.0).abs() < f64::EPSILON);
        assert!((layout.nodes["a2"].x - 210.0).abs() < f64::EPSILON);
    }

    #[test]
    fn border_align_shared_lines_aligns_left_edges() {
        let diagram = two_group_diagram();
        let spec = GroupFrameSpec {
            arrangement: GroupArrangement::Stack { axis: Axis::Horizontal },
            track_sizing: TrackSizing::Fit,
            cross_align: CrossAlign::Center,
            gap: 50.0,
            padding: GroupPadding::architecture_v2(),
            border_align: BorderAlign::SharedLines,
            quantize: QuantizeSpec {
                enabled: false,
                step: 8.0,
                quantize_groups: false,
                quantize_nodes: false,
            },
        };
        // 两 group 左缘相差 3px（< step=8），应被共线。
        // 放在不同 y 行避免触发 overlap resolution（本测试关注 border_align）。
        let mut layout = LayoutResult {
            nodes: HashMap::from([
                ("a1".to_string(), node_layout(10.0, 0.0, 80.0, 40.0)),
                ("a2".to_string(), node_layout(213.0, 100.0, 60.0, 40.0)),
            ]),
            groups: HashMap::from([
                ("g1".to_string(), group_layout(0.0, 0.0, 100.0, 60.0)),
                ("g2".to_string(), group_layout(3.0, 100.0, 80.0, 60.0)),
            ]),
            edges: vec![],
            total_width: 300.0,
            total_height: 200.0,
            hints: Default::default(),
        };
        let pinned = PinSet::default();

        let report = apply_group_frame(&spec, &diagram, &mut layout, &pinned);

        assert!(report.borders_aligned > 0);
        // 左缘共线（中位数 = 0 或 3，取排序后中位数）
        assert!(
            (layout.groups["g1"].x - layout.groups["g2"].x).abs() < f64::EPSILON,
            "g1.x={} g2.x={}",
            layout.groups["g1"].x,
            layout.groups["g2"].x
        );
        // 节点不动（border_align 只改框）
        assert!((layout.nodes["a1"].x - 10.0).abs() < f64::EPSILON);
        assert!((layout.nodes["a2"].x - 213.0).abs() < f64::EPSILON);
    }

    #[test]
    fn quantize_snaps_group_bounds_to_grid() {
        let diagram = two_group_diagram();
        let spec = GroupFrameSpec {
            arrangement: GroupArrangement::Stack { axis: Axis::Horizontal },
            track_sizing: TrackSizing::Fit,
            cross_align: CrossAlign::Center,
            gap: 50.0,
            padding: GroupPadding::architecture_v2(),
            border_align: BorderAlign::None,
            quantize: QuantizeSpec {
                enabled: true,
                step: 8.0,
                quantize_groups: true,
                quantize_nodes: false,
            },
        };
        let mut layout = LayoutResult {
            nodes: HashMap::from([
                ("a1".to_string(), node_layout(10.0, 0.0, 80.0, 40.0)),
                ("a2".to_string(), node_layout(210.0, 0.0, 60.0, 40.0)),
            ]),
            groups: HashMap::from([
                ("g1".to_string(), group_layout(5.0, 3.0, 97.0, 55.0)),
                ("g2".to_string(), group_layout(205.0, 3.0, 77.0, 55.0)),
            ]),
            edges: vec![],
            total_width: 300.0,
            total_height: 100.0,
            hints: Default::default(),
        };
        let pinned = PinSet::default();

        let report = apply_group_frame(&spec, &diagram, &mut layout, &pinned);

        assert!(report.groups_quantized > 0);
        // g1: x=5→floor(5/8)*8=0, y=3→0, right=102→ceil=104, bottom=58→ceil=64
        assert!((layout.groups["g1"].x - 0.0).abs() < f64::EPSILON);
        assert!((layout.groups["g1"].y - 0.0).abs() < f64::EPSILON);
        assert!((layout.groups["g1"].width - 104.0).abs() < f64::EPSILON);
        assert!((layout.groups["g1"].height - 64.0).abs() < f64::EPSILON);
    }

    #[test]
    fn idempotent_equal_plus_start() {
        let diagram = two_group_diagram();
        let spec = GroupFrameSpec {
            arrangement: GroupArrangement::Stack { axis: Axis::Horizontal },
            track_sizing: TrackSizing::Equal,
            cross_align: CrossAlign::Start,
            gap: 50.0,
            padding: GroupPadding::architecture_v2(),
            border_align: BorderAlign::None,
            quantize: QuantizeSpec {
                enabled: false,
                step: 8.0,
                quantize_groups: false,
                quantize_nodes: false,
            },
        };
        let make_layout = || LayoutResult {
            nodes: HashMap::from([
                ("a1".to_string(), node_layout(10.0, 0.0, 80.0, 40.0)),
                ("a2".to_string(), node_layout(210.0, 0.0, 60.0, 40.0)),
            ]),
            groups: HashMap::from([
                ("g1".to_string(), group_layout(0.0, 0.0, 100.0, 60.0)),
                ("g2".to_string(), group_layout(200.0, 0.0, 80.0, 60.0)),
            ]),
            edges: vec![],
            total_width: 300.0,
            total_height: 100.0,
            hints: Default::default(),
        };
        let pinned = PinSet::default();

        let mut layout = make_layout();
        apply_group_frame(&spec, &diagram, &mut layout, &pinned);
        let snapshot = layout.clone();
        // 第二次执行
        apply_group_frame(&spec, &diagram, &mut layout, &pinned);

        // 幂等：第二次结果与第一次一致
        for (id, g) in &snapshot.groups {
            let g2 = &layout.groups[id];
            assert!((g.x - g2.x).abs() < f64::EPSILON, "group {} x: {} vs {}", id, g.x, g2.x);
            assert!((g.width - g2.width).abs() < f64::EPSILON, "group {} width", id);
        }
        for (id, n) in &snapshot.nodes {
            let n2 = &layout.nodes[id];
            assert!((n.x - n2.x).abs() < f64::EPSILON, "node {} x: {} vs {}", id, n.x, n2.x);
        }
    }

    #[test]
    fn pinset_protects_nodes_from_shift() {
        let diagram = two_group_diagram();
        let spec = GroupFrameSpec {
            arrangement: GroupArrangement::Stack { axis: Axis::Horizontal },
            track_sizing: TrackSizing::Fit,
            cross_align: CrossAlign::Start,
            gap: 50.0,
            padding: GroupPadding::architecture_v2(),
            border_align: BorderAlign::None,
            quantize: QuantizeSpec {
                enabled: false,
                step: 8.0,
                quantize_groups: false,
                quantize_nodes: false,
            },
        };
        let mut layout = LayoutResult {
            nodes: HashMap::from([
                ("a1".to_string(), node_layout(10.0, 0.0, 80.0, 40.0)),
                ("a2".to_string(), node_layout(210.0, 100.0, 60.0, 40.0)),
            ]),
            groups: HashMap::from([
                ("g1".to_string(), group_layout(0.0, 0.0, 100.0, 60.0)),
                ("g2".to_string(), group_layout(200.0, 100.0, 80.0, 60.0)),
            ]),
            edges: vec![],
            total_width: 300.0,
            total_height: 200.0,
            hints: Default::default(),
        };
        // a2 被 Pin 保护
        let mut pinned = PinSet::default();
        pinned.full.insert("a2".to_string());

        apply_group_frame(&spec, &diagram, &mut layout, &pinned);

        // a2 不动（PinSet 保护，跨 rank 平移被跳过）
        assert!((layout.nodes["a2"].x - 210.0).abs() < f64::EPSILON);
        // group 框仍跨 rank 左缘对齐
        assert!((layout.groups["g2"].x - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn no_groups_is_noop() {
        let diagram = Diagram::default();
        let spec = GroupFrameSpec {
            arrangement: GroupArrangement::Stack { axis: Axis::Vertical },
            track_sizing: TrackSizing::Equal,
            cross_align: CrossAlign::Start,
            gap: 60.0,
            padding: GroupPadding::architecture_v2(),
            border_align: BorderAlign::SharedLines,
            quantize: QuantizeSpec::default(),
        };
        let mut layout = LayoutResult {
            nodes: HashMap::from([("n1".to_string(), node_layout(10.0, 10.0, 50.0, 30.0))]),
            groups: HashMap::new(),
            edges: vec![],
            total_width: 100.0,
            total_height: 100.0,
            hints: Default::default(),
        };
        let pinned = PinSet::default();
        let report = apply_group_frame(&spec, &diagram, &mut layout, &pinned);
        assert_eq!(report.top_group_count, 0);
        assert!(!report.equalized);
        assert!(!report.cross_aligned);
    }

    // ─── P3 Matrix ───────────────────────────────────────

    fn four_group_diagram() -> Diagram {
        Diagram {
            entities: vec![
                entity_in_group("a1", "g1"),
                entity_in_group("a2", "g2"),
                entity_in_group("a3", "g3"),
                entity_in_group("a4", "g4"),
            ],
            groups: vec![
                top_group("g1"),
                top_group("g2"),
                top_group("g3"),
                top_group("g4"),
            ],
            ..Default::default()
        }
    }

    fn matrix_layout_initial() -> LayoutResult {
        // 4 groups 大致 2x2 但未对齐：g1(0,0) g2(200,0) g3(0,200) g4(200,200)
        LayoutResult {
            nodes: HashMap::from([
                ("a1".to_string(), node_layout(10.0, 10.0, 80.0, 40.0)),
                ("a2".to_string(), node_layout(210.0, 10.0, 60.0, 40.0)),
                ("a3".to_string(), node_layout(10.0, 210.0, 70.0, 40.0)),
                ("a4".to_string(), node_layout(210.0, 210.0, 50.0, 30.0)),
            ]),
            groups: HashMap::from([
                ("g1".to_string(), group_layout(0.0, 0.0, 100.0, 60.0)),
                ("g2".to_string(), group_layout(200.0, 0.0, 80.0, 60.0)),
                ("g3".to_string(), group_layout(0.0, 200.0, 90.0, 70.0)),
                ("g4".to_string(), group_layout(200.0, 200.0, 70.0, 50.0)),
            ]),
            edges: vec![],
            total_width: 400.0,
            total_height: 400.0,
            hints: Default::default(),
        }
    }

    fn matrix_spec(
        rows: Option<u32>,
        cols: Option<u32>,
        track: TrackSizing,
        cross: CrossAlign,
        gap: f64,
    ) -> GroupFrameSpec {
        GroupFrameSpec {
            arrangement: GroupArrangement::Matrix { rows, cols },
            track_sizing: track,
            cross_align: cross,
            gap,
            padding: GroupPadding::architecture_v2(),
            border_align: BorderAlign::None,
            quantize: QuantizeSpec {
                enabled: false,
                step: 8.0,
                quantize_groups: false,
                quantize_nodes: false,
            },
        }
    }

    #[test]
    fn matrix_fit_start_places_groups_in_grid() {
        let diagram = four_group_diagram();
        // 自动推断 2x2
        let spec = matrix_spec(None, None, TrackSizing::Fit, CrossAlign::Start, 20.0);
        let mut layout = matrix_layout_initial();
        let pinned = PinSet::default();

        let report = apply_group_frame(&spec, &diagram, &mut layout, &pinned);

        assert!(report.matrix_applied);
        // col_widths = [max(100,90), max(80,70)] = [100, 80]
        // row_heights = [max(60,60), max(70,50)] = [60, 70]
        // col_x = [0, 120]; row_y = [0, 80]
        // g1(0,0) g2(120,0) g3(0,80) g4(120,80)
        let g = &layout.groups;
        assert!((g["g1"].x - 0.0).abs() < 0.5 && (g["g1"].y - 0.0).abs() < 0.5);
        assert!((g["g2"].x - 120.0).abs() < 0.5 && (g["g2"].y - 0.0).abs() < 0.5);
        assert!((g["g3"].x - 0.0).abs() < 0.5 && (g["g3"].y - 80.0).abs() < 0.5);
        assert!((g["g4"].x - 120.0).abs() < 0.5 && (g["g4"].y - 80.0).abs() < 0.5);
        // 节点随 group 平移：g2 dx=-80, g3 dy=-120, g4 dx=-80 dy=-120
        let n = &layout.nodes;
        assert!((n["a1"].x - 10.0).abs() < 0.5 && (n["a1"].y - 10.0).abs() < 0.5);
        assert!((n["a2"].x - 130.0).abs() < 0.5 && (n["a2"].y - 10.0).abs() < 0.5);
        assert!((n["a3"].x - 10.0).abs() < 0.5 && (n["a3"].y - 90.0).abs() < 0.5);
        assert!((n["a4"].x - 130.0).abs() < 0.5 && (n["a4"].y - 90.0).abs() < 0.5);
    }

    #[test]
    fn matrix_equal_track_equalizes_columns_and_rows() {
        let diagram = four_group_diagram();
        let spec = matrix_spec(None, None, TrackSizing::Equal, CrossAlign::Start, 20.0);
        let mut layout = matrix_layout_initial();
        let pinned = PinSet::default();

        apply_group_frame(&spec, &diagram, &mut layout, &pinned);

        // Equal: col_widths 全 = max(100,80,90,70)=100; row_heights 全 = max(60,60,70,50)=70
        // col_x = [0, 120]; row_y = [0, 90]
        let g = &layout.groups;
        assert!((g["g1"].x - 0.0).abs() < 0.5 && (g["g1"].y - 0.0).abs() < 0.5);
        assert!((g["g2"].x - 120.0).abs() < 0.5 && (g["g2"].y - 0.0).abs() < 0.5);
        assert!((g["g3"].x - 0.0).abs() < 0.5 && (g["g3"].y - 90.0).abs() < 0.5);
        assert!((g["g4"].x - 120.0).abs() < 0.5 && (g["g4"].y - 90.0).abs() < 0.5);
    }

    #[test]
    fn matrix_center_cross_align_centers_in_cell() {
        let diagram = four_group_diagram();
        let spec = matrix_spec(None, None, TrackSizing::Fit, CrossAlign::Center, 20.0);
        let mut layout = matrix_layout_initial();
        let pinned = PinSet::default();

        apply_group_frame(&spec, &diagram, &mut layout, &pinned);

        // Fit: col_widths=[100,80], row_heights=[60,70]; col_x=[0,120], row_y=[0,80]
        // Center:
        //  g1: (0+(100-100)/2, 0+(60-60)/2) = (0, 0)
        //  g2: (120+(80-80)/2, 0+(60-60)/2) = (120, 0)
        //  g3: (0+(100-90)/2, 80+(70-70)/2) = (5, 80)
        //  g4: (120+(80-70)/2, 80+(70-50)/2) = (125, 90)
        let g = &layout.groups;
        assert!((g["g1"].x - 0.0).abs() < 0.5 && (g["g1"].y - 0.0).abs() < 0.5);
        assert!((g["g2"].x - 120.0).abs() < 0.5 && (g["g2"].y - 0.0).abs() < 0.5);
        assert!((g["g3"].x - 5.0).abs() < 0.5 && (g["g3"].y - 80.0).abs() < 0.5);
        assert!((g["g4"].x - 125.0).abs() < 0.5 && (g["g4"].y - 90.0).abs() < 0.5);
    }

    #[test]
    fn matrix_explicit_rows_cols_single_row() {
        let diagram = four_group_diagram();
        // 1 行 4 列
        let spec = matrix_spec(Some(1), Some(4), TrackSizing::Fit, CrossAlign::Start, 10.0);
        let mut layout = matrix_layout_initial();
        let pinned = PinSet::default();

        apply_group_frame(&spec, &diagram, &mut layout, &pinned);

        // 排序后 ordered = [g1,g2,g3,g4]（按 y,x）
        // col_widths = [100,80,90,70]; row_heights=[max(60,60,70,50)]=70
        // col_x = [0, 110, 200, 300]; row_y=[0]
        // 全部 y=0
        let g = &layout.groups;
        assert!((g["g1"].x - 0.0).abs() < 0.5);
        assert!((g["g2"].x - 110.0).abs() < 0.5);
        assert!((g["g3"].x - 200.0).abs() < 0.5);
        assert!((g["g4"].x - 300.0).abs() < 0.5);
        for id in ["g1", "g2", "g3", "g4"] {
            assert!((g[id].y - 0.0).abs() < 0.5);
        }
    }

    #[test]
    fn matrix_idempotent() {
        let diagram = four_group_diagram();
        let spec = matrix_spec(None, None, TrackSizing::Equal, CrossAlign::Start, 20.0);
        let mut layout = matrix_layout_initial();
        let pinned = PinSet::default();

        apply_group_frame(&spec, &diagram, &mut layout, &pinned);
        // 快照
        let snap_groups: Vec<(String, f64, f64, f64, f64)> = layout
            .groups
            .iter()
            .map(|(k, g)| (k.clone(), g.x, g.y, g.width, g.height))
            .collect();
        let snap_nodes: Vec<(String, f64, f64)> = layout
            .nodes
            .iter()
            .map(|(k, n)| (k.clone(), n.x, n.y))
            .collect();

        // 第二次执行
        let report = apply_group_frame(&spec, &diagram, &mut layout, &pinned);
        // 第二次应近似 no-op（dx/dy < 0.5 跳过）
        assert!(!report.matrix_applied);

        for (k, x, y, w, h) in &snap_groups {
            let g = &layout.groups[k];
            assert!((g.x - x).abs() < 0.5, "group {} x drift", k);
            assert!((g.y - y).abs() < 0.5, "group {} y drift", k);
            assert!((g.width - w).abs() < 0.5, "group {} w drift", k);
            assert!((g.height - h).abs() < 0.5, "group {} h drift", k);
        }
        for (k, x, y) in &snap_nodes {
            let n = &layout.nodes[k];
            assert!((n.x - x).abs() < 0.5, "node {} x drift", k);
            assert!((n.y - y).abs() < 0.5, "node {} y drift", k);
        }
    }

    #[test]
    fn matrix_pinset_protection() {
        let diagram = four_group_diagram();
        let spec = matrix_spec(None, None, TrackSizing::Fit, CrossAlign::Start, 20.0);
        let mut layout = matrix_layout_initial();
        // a3 被 Pin 保护
        let mut pinned = PinSet::default();
        pinned.full.insert("a3".to_string());

        apply_group_frame(&spec, &diagram, &mut layout, &pinned);

        // a3 不动（PinSet 保护），但 g3 框仍移动到 (0, 80)
        assert!((layout.nodes["a3"].x - 10.0).abs() < 0.5);
        assert!((layout.nodes["a3"].y - 210.0).abs() < 0.5);
        assert!((layout.groups["g3"].x - 0.0).abs() < 0.5);
        assert!((layout.groups["g3"].y - 80.0).abs() < 0.5);
        // 其他节点正常平移
        assert!((layout.nodes["a4"].x - 130.0).abs() < 0.5);
        assert!((layout.nodes["a4"].y - 90.0).abs() < 0.5);
    }

    #[test]
    fn infer_matrix_dims_auto_square() {
        assert_eq!(infer_matrix_dims(None, None, 4), (2, 2));
        // sqrt(6)≈2.45 → cols=3, rows=ceil(6/3)=2
        assert_eq!(infer_matrix_dims(None, None, 6), (2, 3));
        // sqrt(9)=3 → cols=3, rows=3
        assert_eq!(infer_matrix_dims(None, None, 9), (3, 3));
        // sqrt(1)=1 → cols=1, rows=1
        assert_eq!(infer_matrix_dims(None, None, 1), (1, 1));
    }

    #[test]
    fn infer_matrix_dims_explicit() {
        // rows 指定 → cols = ceil(n/rows)
        assert_eq!(infer_matrix_dims(Some(2), None, 5), (2, 3));
        // cols 指定 → rows = ceil(n/cols)
        assert_eq!(infer_matrix_dims(None, Some(3), 7), (3, 3));
        // 两者均指定
        assert_eq!(infer_matrix_dims(Some(2), Some(3), 4), (2, 3));
        // rows=1 → 单行
        assert_eq!(infer_matrix_dims(Some(1), None, 4), (1, 4));
    }

    // ─── P3 嵌套 sub-frame ──────────────────────────────

    /// 单顶层 group `g1`，含子 group `g1a`/`g1b`（各带一个节点）。
    /// `collect_sibling_sets` 产出：[g1], [g1a, g1b]。
    fn nested_diagram_single_parent() -> Diagram {
        let mut g1 = top_group("g1");
        g1.child_group_ids = vec![
            Identifier::new_unchecked("g1a"),
            Identifier::new_unchecked("g1b"),
        ];
        Diagram {
            entities: vec![
                entity_in_group("a1", "g1a"),
                entity_in_group("a2", "g1b"),
            ],
            groups: vec![
                g1,
                child_group("g1a", "g1"),
                child_group("g1b", "g1"),
            ],
            ..Default::default()
        }
    }

    /// 两顶层 group `g1`/`g2`，各含两个子 group。
    /// `collect_sibling_sets` 产出：[g1, g2], [g1a, g1b], [g2a, g2b]。
    fn nested_diagram_two_parents() -> Diagram {
        let mut g1 = top_group("g1");
        g1.child_group_ids = vec![
            Identifier::new_unchecked("g1a"),
            Identifier::new_unchecked("g1b"),
        ];
        let mut g2 = top_group("g2");
        g2.child_group_ids = vec![
            Identifier::new_unchecked("g2a"),
            Identifier::new_unchecked("g2b"),
        ];
        Diagram {
            entities: vec![
                entity_in_group("a1", "g1a"),
                entity_in_group("a2", "g1b"),
                entity_in_group("b1", "g2a"),
                entity_in_group("b2", "g2b"),
            ],
            groups: vec![
                g1,
                g2,
                child_group("g1a", "g1"),
                child_group("g1b", "g1"),
                child_group("g2a", "g2"),
                child_group("g2b", "g2"),
            ],
            ..Default::default()
        }
    }

    /// 嵌套场景初始 layout：g1(0,0,100,200) 包含 g1a(10,10,80,60) 与 g1b(10,100,60,60)。
    fn nested_layout_initial() -> LayoutResult {
        LayoutResult {
            nodes: HashMap::from([
                ("a1".to_string(), node_layout(20.0, 20.0, 60.0, 40.0)),
                ("a2".to_string(), node_layout(20.0, 110.0, 40.0, 40.0)),
            ]),
            groups: HashMap::from([
                ("g1".to_string(), group_layout(0.0, 0.0, 100.0, 200.0)),
                ("g1a".to_string(), group_layout(10.0, 10.0, 80.0, 60.0)),
                ("g1b".to_string(), group_layout(10.0, 100.0, 60.0, 60.0)),
            ]),
            edges: vec![],
            total_width: 200.0,
            total_height: 250.0,
            hints: Default::default(),
        }
    }

    fn nested_stack_spec(track: TrackSizing, cross: CrossAlign) -> GroupFrameSpec {
        GroupFrameSpec {
            arrangement: GroupArrangement::Stack { axis: Axis::Vertical },
            track_sizing: track,
            cross_align: cross,
            gap: 10.0,
            padding: GroupPadding::architecture_v2(),
            border_align: BorderAlign::None,
            quantize: QuantizeSpec {
                enabled: false,
                step: 8.0,
                quantize_groups: false,
                quantize_nodes: false,
            },
        }
    }

    #[test]
    fn nested_subframe_equalizes_child_groups() {
        let diagram = nested_diagram_single_parent();
        // Stack(V) + Equal + Start：顶层 [g1] 单元素 no-op；嵌套 [g1a, g1b] 拉齐宽度
        let spec = nested_stack_spec(TrackSizing::Equal, CrossAlign::Start);
        let mut layout = nested_layout_initial();
        let pinned = PinSet::default();

        let report = apply_group_frame(&spec, &diagram, &mut layout, &pinned);

        // 嵌套 sibling set 数 = 1（[g1a, g1b]）
        assert_eq!(report.top_group_count, 1);
        assert_eq!(report.nested_frames_applied, 1);
        assert!(report.equalized);
        // 顶层 [g1] 单元素，cross_align 不触发
        assert!(!report.cross_aligned);

        // 嵌套 Equal：max_width = max(80, 60) = 80
        // g1a 已是 80（no-op）；g1b 60→80，extra=20，half=10
        // a2 居中：x 20→30；g1b.width 60→80
        assert!((layout.groups["g1a"].width - 80.0).abs() < 0.5);
        assert!((layout.groups["g1b"].width - 80.0).abs() < 0.5);
        assert!((layout.nodes["a2"].x - 30.0).abs() < 0.5);
        // a1 不动（g1a 已是 max_width）
        assert!((layout.nodes["a1"].x - 20.0).abs() < 0.5);
        // 父 group g1 不受嵌套 sub-frame 影响
        assert!((layout.groups["g1"].width - 100.0).abs() < 0.5);
    }

    #[test]
    fn nested_subframe_report_counts_sets() {
        let diagram = nested_diagram_two_parents();
        let spec = nested_stack_spec(TrackSizing::Fit, CrossAlign::Start);
        let mut layout = LayoutResult {
            nodes: HashMap::from([
                ("a1".to_string(), node_layout(20.0, 20.0, 60.0, 40.0)),
                ("a2".to_string(), node_layout(20.0, 110.0, 60.0, 40.0)),
                ("b1".to_string(), node_layout(220.0, 20.0, 60.0, 40.0)),
                ("b2".to_string(), node_layout(220.0, 110.0, 60.0, 40.0)),
            ]),
            groups: HashMap::from([
                ("g1".to_string(), group_layout(0.0, 0.0, 100.0, 200.0)),
                ("g2".to_string(), group_layout(200.0, 0.0, 100.0, 200.0)),
                ("g1a".to_string(), group_layout(10.0, 10.0, 80.0, 60.0)),
                ("g1b".to_string(), group_layout(10.0, 100.0, 80.0, 60.0)),
                ("g2a".to_string(), group_layout(210.0, 10.0, 80.0, 60.0)),
                ("g2b".to_string(), group_layout(210.0, 100.0, 80.0, 60.0)),
            ]),
            edges: vec![],
            total_width: 400.0,
            total_height: 250.0,
            hints: Default::default(),
        };
        let pinned = PinSet::default();

        let report = apply_group_frame(&spec, &diagram, &mut layout, &pinned);

        // 3 个 sibling set：[g1,g2] + [g1a,g1b] + [g2a,g2b]
        // top_group_count = 2（顶层 set 长度）
        // nested_frames_applied = 2（idx > 0 的 set 数）
        assert_eq!(report.top_group_count, 2);
        assert_eq!(report.nested_frames_applied, 2);
    }

    #[test]
    fn nested_subframe_idempotent() {
        let diagram = nested_diagram_single_parent();
        let spec = nested_stack_spec(TrackSizing::Equal, CrossAlign::Start);
        let mut layout = nested_layout_initial();
        let pinned = PinSet::default();

        apply_group_frame(&spec, &diagram, &mut layout, &pinned);

        // 快照第一次结果
        let snap_groups: Vec<(String, f64, f64, f64, f64)> = ["g1", "g1a", "g1b"]
            .iter()
            .map(|k| {
                let g = &layout.groups[*k];
                (k.to_string(), g.x, g.y, g.width, g.height)
            })
            .collect();
        let snap_nodes: Vec<(String, f64, f64)> = ["a1", "a2"]
            .iter()
            .map(|k| {
                let n = &layout.nodes[*k];
                (k.to_string(), n.x, n.y)
            })
            .collect();

        // 第二次执行
        apply_group_frame(&spec, &diagram, &mut layout, &pinned);

        for (k, x, y, w, h) in &snap_groups {
            let g = &layout.groups[k];
            assert!((g.x - x).abs() < 0.5, "group {} x drift", k);
            assert!((g.y - y).abs() < 0.5, "group {} y drift", k);
            assert!((g.width - w).abs() < 0.5, "group {} w drift", k);
            assert!((g.height - h).abs() < 0.5, "group {} h drift", k);
        }
        for (k, x, y) in &snap_nodes {
            let n = &layout.nodes[k];
            assert!((n.x - x).abs() < 0.5, "node {} x drift", k);
            assert!((n.y - y).abs() < 0.5, "node {} y drift", k);
        }
    }

    #[test]
    fn nested_subframe_pinset_protection() {
        let diagram = nested_diagram_single_parent();
        let spec = nested_stack_spec(TrackSizing::Equal, CrossAlign::Start);
        let mut layout = nested_layout_initial();
        // a2 被 Pin 保护
        let mut pinned = PinSet::default();
        pinned.full.insert("a2".to_string());

        apply_group_frame(&spec, &diagram, &mut layout, &pinned);

        // g1b 框仍拉齐到 80
        assert!((layout.groups["g1b"].width - 80.0).abs() < 0.5);
        // a2 不动（PinSet 保护）
        assert!((layout.nodes["a2"].x - 20.0).abs() < 0.5);
        // a1 不受影响
        assert!((layout.nodes["a1"].x - 20.0).abs() < 0.5);
    }
