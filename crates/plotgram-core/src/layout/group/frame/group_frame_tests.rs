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
        // G-pre：architecture 默认 Fit + None（朴素容器）
        assert_eq!(spec.track_sizing, TrackSizing::Fit);
        assert_eq!(spec.cross_align, CrossAlign::Center);
        assert!((spec.gap - 40.0).abs() < f64::EPSILON);
        assert_eq!(spec.border_align, BorderAlign::None);
        assert_eq!(spec.padding, GroupPadding::architecture());
        // snap 未声明 → 默认 true
        assert!(spec.quantize.enabled);
        assert!((spec.quantize.step - 8.0).abs() < f64::EPSILON);
    }

    #[test]
    fn architecture_default_fit_track() {
        let diagram = Diagram::default();
        let spec = resolve_group_frame_spec(&diagram, "architecture");
        assert_eq!(spec.track_sizing, TrackSizing::Fit);
        assert_eq!(spec.cross_align, CrossAlign::Center);
        assert_eq!(spec.border_align, BorderAlign::None);
    }

    #[test]
    fn architecture_group_frame_dsl_ignored() {
        // G-pre：`group_frame` DSL 不再被 resolve 消费
        let diagram = Diagram {
            attributes: vec![config_attr("stack", &[("track", str_val("equal"))])],
            ..Default::default()
        };
        let spec = resolve_group_frame_spec(&diagram, "architecture");
        assert_eq!(spec.track_sizing, TrackSizing::Fit);
        assert_eq!(spec.border_align, BorderAlign::None);
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
    }

    // ─── flowchart ────────────────────────────────────────

    #[test]
    fn flowchart_default_spec() {
        let diagram = Diagram::default();
        let spec = resolve_group_frame_spec(&diagram, "flowchart");

        assert_eq!(spec.arrangement, GroupArrangement::Stack { axis: Axis::Vertical });
        assert_eq!(spec.track_sizing, TrackSizing::Fit);
        assert_eq!(spec.cross_align, CrossAlign::Center);
        assert!((spec.gap - 48.0).abs() < f64::EPSILON);
        assert_eq!(spec.border_align, BorderAlign::None);
        assert!(spec.quantize.enabled);
    }

    #[test]
    fn flowchart_group_frame_dsl_ignored_keeps_defaults() {
        // G-pre：`group_frame` DSL 忽略，仍为算法默认 Vertical/Fit/Center/48/None
        let diagram = Diagram {
            attributes: vec![config_attr(
                "stack",
                &[
                    ("axis", str_val("horizontal")),
                    ("gap", num_val(120.0)),
                    ("cross", str_val("start")),
                    ("track", str_val("equal")),
                ],
            )],
            ..Default::default()
        };
        let spec = resolve_group_frame_spec(&diagram, "flowchart");
        assert_eq!(spec.arrangement, GroupArrangement::Stack { axis: Axis::Vertical });
        assert_eq!(spec.cross_align, CrossAlign::Center);
        assert_eq!(spec.track_sizing, TrackSizing::Fit);
        assert!((spec.gap - 48.0).abs() < f64::EPSILON);
        assert_eq!(spec.border_align, BorderAlign::None);
    }

    #[test]
    fn flowchart_gap_non_positive_keeps_default() {
        let diagram = Diagram {
            attributes: vec![config_attr("stack", &[("gap", num_val(-10.0))])],
            ..Default::default()
        };
        let spec = resolve_group_frame_spec(&diagram, "flowchart");
        assert!((spec.gap - 48.0).abs() < f64::EPSILON);
    }

    #[test]
    fn flowchart_track_fit_default() {
        let diagram = Diagram::default();
        let spec = resolve_group_frame_spec(&diagram, "flowchart");
        assert_eq!(spec.track_sizing, TrackSizing::Fit);
    }

    // ─── group_frame 配置块 ────────────────────

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

    #[test]
    fn group_frame_config_ignored_keeps_algorithm_defaults() {
        // G-pre：配置块全部忽略，仍为 flowchart 算法默认
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

        assert_eq!(spec.arrangement, GroupArrangement::Stack { axis: Axis::Vertical });
        assert_eq!(spec.track_sizing, TrackSizing::Fit);
        assert_eq!(spec.cross_align, CrossAlign::Center);
        assert!((spec.gap - 48.0).abs() < f64::EPSILON);
        assert_eq!(spec.border_align, BorderAlign::None);
        // snap 步长仍走顶层 `snap` 属性默认，不读 group_frame 内 snap
        assert!(spec.quantize.enabled);
        assert!((spec.quantize.step - 8.0).abs() < f64::EPSILON);
    }

    #[test]
    fn group_frame_config_matrix_and_presets_ignored() {
        // matrix / strips / tiles / lanes 等短名均不再展开
        for attr in [
            config_attr("matrix", &[("rows", num_val(2.0)), ("cols", num_val(3.0))]),
            config_attr("tiles", &[("cols", num_val(3.0))]),
            config_attr("lanes", &[]),
            config_attr("strips", &[("gap", num_val(60.0))]),
            DiagramAttribute {
                key: "group_frame".to_string(),
                value: AttributeValue::String(TextValue::unquoted("strips")),
                span: Span::dummy(),
            },
        ] {
            let diagram = Diagram {
                attributes: vec![attr],
                ..Default::default()
            };
            let spec = resolve_group_frame_spec(&diagram, "flowchart");
            assert_eq!(
                spec.arrangement,
                GroupArrangement::Stack { axis: Axis::Vertical }
            );
            assert_eq!(spec.track_sizing, TrackSizing::Fit);
            assert_eq!(spec.border_align, BorderAlign::None);
        }
    }

    #[test]
    fn group_frame_config_bare_stack_no_options() {
        // `group_frame: stack` 同样忽略 → 算法默认
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

    #[test]
    fn architecture_preset_dsl_ignored_keeps_fit_none() {
        let diagram = Diagram {
            attributes: vec![config_attr(
                "strips",
                &[("gap", num_val(60.0)), ("cross", str_val("start"))],
            )],
            ..Default::default()
        };
        let spec = resolve_group_frame_spec(&diagram, "architecture");
        assert_eq!(spec.arrangement, GroupArrangement::Stack { axis: Axis::Horizontal });
        assert_eq!(spec.track_sizing, TrackSizing::Fit);
        assert_eq!(spec.border_align, BorderAlign::None);
        assert_eq!(spec.cross_align, CrossAlign::Center);
        assert!((spec.gap - 40.0).abs() < f64::EPSILON);
    }

    // ─── snap 默认启用（与算法无关，仅受 `snap` 属性控制） ──────

    #[test]
    fn snap_enabled_by_default_regardless_of_algo() {
        // P0 后 quantize.enabled 只读 `snap` 属性（默认 true），不再按算法白名单判定
        for algo in ["flowchart", "er", "state", "architecture"] {
            let diagram = Diagram::default();
            let spec = resolve_group_frame_spec(&diagram, algo);
            assert!(
                spec.quantize.enabled,
                "algo `{algo}` should default to quantize.enabled=true"
            );
        }
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
            attributes: vec![config_attr(
                "stack",
                &[("gap", num_val(80.0)), ("cross", str_val("left"))],
            )],
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

