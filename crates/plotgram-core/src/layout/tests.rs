use super::*;
use crate::layout::geometry::Point;
use crate::ast::{AttributeValue, Diagram, DiagramAttribute, Position, SourceInfo, Span, TextValue};
use crate::layout::algorithm_config::SUGIYAMA_LAYOUT_OPTIONS;
use crate::layout::edge::edge_routing_bezier::BEZIER_OPTIONS;
use crate::layout::edge::edge_routing_orthogonal::ORTHOGONAL_OPTIONS;
use crate::types::DiagramType;
use crate::profile::profile_for;

fn sample_diagram(diagram_type: DiagramType) -> Diagram {
    Diagram::new(
        diagram_type,
        SourceInfo {
            file: None,
            line_count: 1,
        },
    )
    }

fn atom_attr(key: &str, value: &str) -> DiagramAttribute {
    DiagramAttribute {
        key: key.to_string(),
        value: AttributeValue::String(TextValue::unquoted(value.to_string())),
        span: Span::new(Position::new(1, 1), Position::new(1, 1)),
    }
}

fn config_attr(key: &str, algo: &str, options: &[(&str, AttributeValue)]) -> DiagramAttribute {
    DiagramAttribute {
        key: key.to_string(),
        value: AttributeValue::Config {
            algo: algo.to_string(),
            options: options
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        },
        span: Span::new(Position::new(1, 1), Position::new(1, 1)),
    }
}

// ── LayoutPlan::resolve config block tests ──

#[test]
fn resolve_uses_profile_defaults_when_attrs_missing() {
    let diagram = sample_diagram(DiagramType::Flowchart);
    let profile = profile_for(&diagram.diagram_type);
    let plan = LayoutPlan::resolve(&diagram, profile);

    assert_eq!(plan.layout_algo, "flowchart");
    assert_eq!(plan.edge_routing, "orthogonal");
}

#[test]
fn resolve_edge_options_from_config_block() {
    let mut diagram = sample_diagram(DiagramType::Flowchart);
    diagram.attributes.push(config_attr(
        "edge_routing",
        "orthogonal",
        &[
            ("slot_pitch", AttributeValue::Number(55.0)),
            ("channel_margin", AttributeValue::Number(22.0)),
        ],
    ));
    let profile = profile_for(&diagram.diagram_type);
    let plan = LayoutPlan::resolve(&diagram, profile);

    assert_eq!(
        plan.edge_options.get_or_default(&ORTHOGONAL_OPTIONS[0]),
        55.0
    );
    assert_eq!(
        plan.edge_options.get_or_default(&ORTHOGONAL_OPTIONS[1]),
        22.0
    );
}

#[test]
fn resolve_bezier_tension_from_config_block() {
    let mut diagram = sample_diagram(DiagramType::Flowchart);
    diagram.attributes.push(config_attr(
        "edge_routing",
        "bezier",
        &[("tension", AttributeValue::Number(1.2))],
    ));
    let profile = profile_for(&diagram.diagram_type);
    let plan = LayoutPlan::resolve(&diagram, profile);

    assert_eq!(plan.edge_routing, "bezier");
    assert_eq!(plan.edge_options.get_or_default(&BEZIER_OPTIONS[0]), 1.2);
}

#[test]
fn resolve_layout_group_padding_from_config_block() {
    let mut diagram = sample_diagram(DiagramType::Flowchart);
    diagram.attributes.push(config_attr(
        "layout",
        "flowchart",
        &[("group_padding", AttributeValue::Number(40.0))],
    ));
    let profile = profile_for(&diagram.diagram_type);
    let plan = LayoutPlan::resolve(&diagram, profile);

    assert_eq!(
        plan.layout_options.get_or_default(&SUGIYAMA_LAYOUT_OPTIONS[0]),
        40.0
    );
}

#[test]
fn invalid_layout_option_emits_warning() {
    let mut diagram = sample_diagram(DiagramType::Flowchart);
    diagram.attributes.push(config_attr(
        "layout",
        "flowchart",
        &[("group_padding", AttributeValue::Number(-5.0))],
    ));
    let profile = profile_for(&diagram.diagram_type);
    let plan = LayoutPlan::resolve(&diagram, profile);
    let mut result = crate::error::ValidationResult::new();
    crate::layout::validate_layout_plan_warnings(&diagram, &plan, &mut result);
    assert!(!result.warnings.is_empty());
}

#[test]
fn auto_resolves_to_profile_default_layout() {
    let mut diagram = sample_diagram(DiagramType::Flowchart);
    diagram.attributes.push(config_attr("layout", "auto", &[]));
    let profile = profile_for(&diagram.diagram_type);
    let plan = LayoutPlan::resolve(&diagram, profile);

    assert_eq!(plan.layout_algo, "auto");
    assert_eq!(plan.resolved_auto_algo.as_deref(), Some("flowchart"));
}

#[test]
fn non_auto_has_no_resolved_auto_algo() {
    let diagram = sample_diagram(DiagramType::Flowchart);
    let profile = profile_for(&diagram.diagram_type);
    let plan = LayoutPlan::resolve(&diagram, profile);

    assert_eq!(plan.layout_algo, "flowchart");
    assert_eq!(plan.resolved_auto_algo, None);
}

#[test]
fn uses_profile_defaults_when_diagram_has_no_override() {
    let diagram = sample_diagram(DiagramType::Sequence);
    let profile = profile_for(&diagram.diagram_type);
    let plan = LayoutPlan::resolve(&diagram, profile);

    assert_eq!(plan.layout_algo, "sequence");
    assert_eq!(plan.edge_routing, "");
    assert!(compute_layout(&diagram).is_ok());
}

#[test]
fn sequence_rejects_edge_routing_attribute() {
    let mut diagram = sample_diagram(DiagramType::Sequence);
    diagram.attributes.push(atom_attr("edge_routing", "straight"));
    assert!(compute_layout(&diagram).is_err());
}

#[test]
fn explicit_overrides_still_take_priority() {
    let mut diagram = sample_diagram(DiagramType::Flowchart);
    diagram.attributes.push(atom_attr("layout", "er"));
    diagram.attributes.push(atom_attr("edge_routing", "bezier"));
    let profile = profile_for(&diagram.diagram_type);
    let plan = LayoutPlan::resolve(&diagram, profile);

    assert_eq!(plan.layout_algo, "er");
    assert_eq!(plan.edge_routing, "bezier");
}

#[test]
fn string_layout_attrs_are_resolved() {
    let mut diagram = sample_diagram(DiagramType::Flowchart);
    diagram.attributes.push(atom_attr("direction", "left-to-right"));
    diagram.attributes.push(atom_attr("layout", "er"));
    diagram.attributes.push(atom_attr("edge_routing", "bezier"));
    let profile = profile_for(&diagram.diagram_type);
    let plan = LayoutPlan::resolve(&diagram, profile);

    assert_eq!(diagram.direction(), "left-to-right");
    assert_eq!(plan.layout_algo, "er");
    assert_eq!(plan.edge_routing, "bezier");
}

#[test]
fn unknown_layout_algo_is_rejected() {
    let mut diagram = sample_diagram(DiagramType::Flowchart);
    diagram
        .attributes
        .push(atom_attr("layout", "not_a_real_algo"));

    match compute_layout(&diagram) {
        Err(err) => assert!(err.message.contains("not_a_real_algo")),
        Ok(_) => panic!("expected layout error"),
    }
}

#[test]
fn unsupported_layout_algo_for_diagram_type_is_rejected() {
    let mut diagram = sample_diagram(DiagramType::Sequence);
    diagram.attributes.push(atom_attr("layout", "mindmap"));

    assert!(compute_layout(&diagram).is_err());
}

#[test]
fn compute_layout_applies_grid_snap_for_sugiyama_v2() {
    use crate::ast::{AttributeMap, Entity, Identifier, Relation, ArrowType};

    let span = Span::new(Position::new(1, 1), Position::new(1, 1));
    let mut diagram = sample_diagram(DiagramType::Flowchart);
    diagram.attributes.push(atom_attr("layout", "flowchart"));
    for id in ["a", "b", "c", "d"] {
        diagram.entities.push(Entity {
            id: Identifier::new_unchecked(id),
            label: id.to_string(),
            attributes: AttributeMap::default(),
            group_id: None,
            span,
        });
    }
    for (from, to) in [("a", "b"), ("a", "c"), ("b", "d"), ("c", "d")] {
        diagram.relations.push(Relation {
            from: Identifier::new_unchecked(from),
            to: Identifier::new_unchecked(to),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span,
        });
    }

    let result = compute_layout(&diagram).expect("layout should succeed");
    assert_eq!(result.nodes.len(), 4);

    let b = &result.nodes["b"];
    let c = &result.nodes["c"];
    let b_cy = b.y + b.height / 2.0;
    let c_cy = c.y + c.height / 2.0;
    assert!(
        (b_cy - c_cy).abs() < f64::EPSILON,
        "siblings b and c should share rank-axis center after grid snap"
    );
}

#[test]
fn compute_layout_snaps_orthogonal_edge_waypoints() {
    use crate::ast::{AttributeMap, Entity, Identifier, Relation, ArrowType};

    let span = Span::new(Position::new(1, 1), Position::new(1, 1));
    let mut diagram = sample_diagram(DiagramType::Flowchart);
    diagram.attributes.push(atom_attr("layout", "flowchart"));
    for id in ["a", "b"] {
        diagram.entities.push(Entity {
            id: Identifier::new_unchecked(id),
            label: id.to_string(),
            attributes: AttributeMap::default(),
            group_id: None,
            span,
        });
    }
    diagram.relations.push(Relation {
        from: Identifier::new_unchecked("a"),
        to: Identifier::new_unchecked("b"),
        arrow: ArrowType::Active,
        label: None,
        head_label: None,
        tail_label: None,
        attributes: AttributeMap::default(),
        span,
    });

    let result = compute_layout(&diagram).expect("layout should succeed");
    assert_eq!(result.edges.len(), 1);
    let edge = &result.edges[0];
    let path: Vec<Point> = edge.path_points().into_owned();
    if path.len() > 2 {
        for i in 1..path.len() - 1 {
            let prev = path[i - 1];
            let curr = path[i];
            let next = path[i + 1];
            let dx_prev = (curr.x - prev.x).abs();
            let dy_prev = (curr.y - prev.y).abs();
            let dx_next = (next.x - curr.x).abs();
            let dy_next = (next.y - curr.y).abs();
            let prev_vertical = dx_prev < 0.1 && dy_prev >= 0.1;
            let next_vertical = dx_next < 0.1 && dy_next >= 0.1;
            let prev_horizontal = dy_prev < 0.1 && dx_prev >= 0.1;
            let next_horizontal = dy_next < 0.1 && dx_next >= 0.1;

            if prev_vertical && next_vertical {
                assert!(
                    (curr.x - prev.x).abs() < 0.1,
                    "vertical waypoint x={} must align with x={}",
                    curr.x,
                    prev.x
                );
                assert!(
                    (curr.y / 8.0).fract().abs() < 1e-6
                        || (curr.y / 8.0).fract().abs() > 1.0 - 1e-6,
                    "vertical waypoint y={} should be on 8px grid",
                    curr.y
                );
            } else if prev_horizontal && next_horizontal {
                assert!(
                    (curr.y - prev.y).abs() < 0.1,
                    "horizontal waypoint y={} must align with y={}",
                    curr.y,
                    prev.y
                );
                assert!(
                    (curr.x / 8.0).fract().abs() < 1e-6
                        || (curr.x / 8.0).fract().abs() > 1.0 - 1e-6,
                    "horizontal waypoint x={} should be on 8px grid",
                    curr.x
                );
            }
        }
    }
    assert!(!edge.is_bezier());
}

#[test]
fn compute_layout_respects_snap_false_attribute() {
    use crate::ast::{AttributeMap, Entity, Identifier, Relation, ArrowType};

    let span = Span::new(Position::new(1, 1), Position::new(1, 1));
    let mut diagram = sample_diagram(DiagramType::Flowchart);
    diagram.attributes.push(atom_attr("layout", "flowchart"));
    diagram.attributes.push(DiagramAttribute {
        key: "snap".into(),
        value: AttributeValue::Boolean(false),
        span,
    });
    diagram.entities.push(Entity {
        id: Identifier::new_unchecked("a"),
        label: "a".into(),
        attributes: AttributeMap::default(),
        group_id: None,
        span,
    });
    diagram.entities.push(Entity {
        id: Identifier::new_unchecked("b"),
        label: "b".into(),
        attributes: AttributeMap::default(),
        group_id: None,
        span,
    });
    diagram.relations.push(Relation {
        from: Identifier::new_unchecked("a"),
        to: Identifier::new_unchecked("b"),
        arrow: ArrowType::Active,
        label: None,
        head_label: None,
        tail_label: None,
        attributes: AttributeMap::default(),
        span,
    });

    assert!(compute_layout(&diagram).is_ok());
}

#[test]
fn group_padding_option_affects_group_bounds() {
    use crate::ast::{AttributeMap, AttributeValue, Entity, Group, Identifier, Relation, ArrowType};

    let span = Span::new(Position::new(1, 1), Position::new(1, 1));
    let mut diagram = sample_diagram(DiagramType::Flowchart);
    diagram.attributes.push(DiagramAttribute {
        key: "layout".into(),
        value: AttributeValue::Config {
            algo: "flowchart".into(),
            options: [("group_padding".to_string(), AttributeValue::Number(50.0))]
                .into_iter()
                .collect(),
        },
        span,
    });
    diagram.entities = vec![
        Entity {
            id: Identifier::new_unchecked("a"),
            label: "a".into(),
            attributes: AttributeMap::default(),
            group_id: Some(Identifier::new_unchecked("g")),
            span,
        },
        Entity {
            id: Identifier::new_unchecked("b"),
            label: "b".into(),
            attributes: AttributeMap::default(),
            group_id: Some(Identifier::new_unchecked("g")),
            span,
        },
    ];
    diagram.relations.push(Relation {
        from: Identifier::new_unchecked("a"),
        to: Identifier::new_unchecked("b"),
        arrow: ArrowType::Active,
        label: None,
        head_label: None,
        tail_label: None,
        attributes: AttributeMap::default(),
        span,
    });
    diagram.groups.push(Group {
        id: Identifier::new_unchecked("g"),
        label: "g".into(),
        attributes: AttributeMap::default(),
        parent_id: None,
        depth: 0,
        entity_ids: vec![
            Identifier::new_unchecked("a"),
            Identifier::new_unchecked("b"),
        ],
        child_group_ids: vec![],
        span,
    });

    let default_layout = compute_layout(&diagram).expect("default layout");
    let default_group = default_layout
        .groups
        .get("g")
        .expect("group bounds");

    diagram.attributes.clear();
    diagram.attributes.push(atom_attr("layout", "flowchart"));
    let baseline_layout = compute_layout(&diagram).expect("baseline layout");
    let baseline_group = baseline_layout.groups.get("g").expect("baseline group");

    assert!(
        default_group.width > baseline_group.width,
        "larger group_padding should expand group width: {} vs {}",
        default_group.width,
        baseline_group.width
    );
}

// ── resolve_effective_direction 测试 ──

#[test]
fn effective_direction_flowchart_default() {
    let diagram = sample_diagram(DiagramType::Flowchart);
    assert_eq!(resolve_effective_direction(&diagram), Some("top-to-bottom"));
}

#[test]
fn effective_direction_mindmap_default() {
    let diagram = sample_diagram(DiagramType::Mindmap);
    assert_eq!(resolve_effective_direction(&diagram), Some("left-to-right"));
}

#[test]
fn effective_direction_sequence_is_none() {
    let diagram = sample_diagram(DiagramType::Sequence);
    assert_eq!(resolve_effective_direction(&diagram), None);
}

#[test]
fn effective_direction_state_default() {
    let diagram = sample_diagram(DiagramType::State);
    assert_eq!(resolve_effective_direction(&diagram), Some("top-to-bottom"));
}

#[test]
fn effective_direction_explicit_overrides_default() {
    let mut diagram = sample_diagram(DiagramType::Flowchart);
    diagram.attributes.push(atom_attr("direction", "left-to-right"));
    assert_eq!(resolve_effective_direction(&diagram), Some("left-to-right"));
}

#[test]
fn effective_direction_custom_inherits_flowchart() {
    let diagram = sample_diagram(DiagramType::Custom("test".to_string()));
    assert_eq!(resolve_effective_direction(&diagram), Some("top-to-bottom"));
}

// ── direction × layout 交叉校验测试 ──

#[test]
fn flowchart_with_radial_direction_is_rejected() {
    let mut diagram = sample_diagram(DiagramType::Flowchart);
    diagram.attributes.push(atom_attr("direction", "radial"));
    let result = compute_layout(&diagram);
    assert!(result.is_err(), "flowchart + radial should be rejected");
    let err = result.unwrap_err();
    assert!(err.message.contains("does not support direction 'radial'"), "unexpected error: {}", err.message);
}

#[test]
fn mindmap_with_radial_direction_is_accepted() {
    let mut diagram = sample_diagram(DiagramType::Mindmap);
    diagram.attributes.push(atom_attr("direction", "radial"));
    assert!(compute_layout(&diagram).is_ok());
}

#[test]
fn sequence_with_direction_is_rejected() {
    let mut diagram = sample_diagram(DiagramType::Sequence);
    diagram.attributes.push(atom_attr("direction", "top-to-bottom"));
    let result = compute_layout(&diagram);
    assert!(result.is_err(), "sequence + direction should be rejected");
    let err = result.unwrap_err();
    assert!(err.message.contains("does not support the 'direction' attribute"), "unexpected error: {}", err.message);
}

#[test]
fn from_center_is_rejected_as_direction() {
    let mut diagram = sample_diagram(DiagramType::Flowchart);
    diagram.attributes.push(atom_attr("direction", "from_center"));
    let result = compute_layout(&diagram);
    assert!(result.is_err(), "from_center should be rejected");
}

// ── 平行边布局集成测试 ──

/// 构造含多条平行边的 flowchart。
fn make_parallel_edges_test_diagram() -> Diagram {
    use crate::ast::{AttributeMap, Entity, Identifier, Relation, ArrowType};

    let span = Span::new(Position::new(1, 1), Position::new(1, 1));
    let mut diagram = sample_diagram(DiagramType::Flowchart);
    diagram.attributes.push(atom_attr("direction", "left-to-right"));
    diagram.attributes.push(atom_attr("edge_routing", "orthogonal"));

    for id in ["a", "b"] {
        diagram.entities.push(Entity {
            id: Identifier::new_unchecked(id),
            label: id.to_string(),
            attributes: AttributeMap::default(),
            group_id: None,
            span,
        });
    }
    // 4 条同向平行边 a→b
    for _ in 0..4 {
        diagram.relations.push(Relation {
            from: Identifier::new_unchecked("a"),
            to: Identifier::new_unchecked("b"),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span,
        });
    }
    diagram
    }

#[test]
fn unknown_bundling_option_emits_warning() {
    use crate::ast::{AttributeMap, AttributeValue, DiagramAttribute};
    use crate::layout::algorithm_config::validate_algorithm_config_warnings;

    let mut diagram = make_parallel_edges_test_diagram();
    diagram.attributes.retain(|a| a.key != "edge_routing");
    let span = Span::new(Position::new(1, 1), Position::new(1, 1));
    diagram.attributes.push(DiagramAttribute {
        key: "edge_routing".into(),
        value: AttributeValue::Config {
            algo: "orthogonal".into(),
            options: std::collections::HashMap::from([(
                "bundling".to_string(),
                AttributeValue::Number(1.0),
            )]),
        },
        span,
    });

    let mut validation = crate::error::ValidationResult::new();
    validate_algorithm_config_warnings(&diagram, &mut validation);
    assert!(
        validation
            .warnings
            .iter()
            .any(|w| w.message.contains("未知选项 'bundling'")),
        "removed bundling option should be reported as unknown"
    );
}

#[test]
fn parallel_edges_layout_succeeds() {
    let diagram = make_parallel_edges_test_diagram();
    let profile = profile_for(&diagram.diagram_type);
    let plan = LayoutPlan::resolve(&diagram, profile);

    let result = compute_layout_with_plan(&diagram, &plan).expect("layout should succeed");
    assert_eq!(result.edges.len(), 4);
}

#[test]
fn parallel_edges_no_exact_overlap_and_shared_from_anchor() {
    let diagram = make_parallel_edges_test_diagram();
    let profile = profile_for(&diagram.diagram_type);
    let plan = LayoutPlan::resolve(&diagram, profile);
    let result = compute_layout_with_plan(&diagram, &plan).expect("layout should succeed");

    assert_eq!(result.edges.len(), 4);

    // 当前行为：fan-out 边分布在相近的 from 区域（非严格共锚）
    let starts: Vec<Point> = result
        .edges
        .iter()
        .map(|e| e.path_points().into_owned()[0])
        .collect();
    let anchor = starts[0];
    assert!(
        starts
            .iter()
            .all(|s| (s.x - anchor.x).abs() < 2.0 && (s.y - anchor.y).abs() < 30.0),
        "flowchart fan-out should have nearby from anchors, got {starts:?}"
    );

    let unrelated = count_unrelated_parallel_overlaps(&diagram, &result);
    assert_eq!(
        unrelated, 0,
        "four parallel edges must not have unrelated exact overlaps"
    );
}

#[test]
fn parallel_edges_layout_without_bundling() {
    let diagram = make_parallel_edges_test_diagram();
    let profile = profile_for(&diagram.diagram_type);
    let plan = LayoutPlan::resolve(&diagram, profile);

    let result = compute_layout_with_plan(&diagram, &plan).expect("layout should succeed");

    for (i, edge) in result.edges.iter().enumerate() {
        let path: Vec<Point> = edge.path_points().into_owned();
        assert!(
            path.len() >= 2,
            "edge {} path must have at least 2 points, got {}",
            i,
            path.len()
        );
        for p in &path {
            assert!(p.x.is_finite(), "edge {} has NaN/inf x in path", i);
            assert!(p.y.is_finite(), "edge {} has NaN/inf y in path", i);
        }
    }
}
