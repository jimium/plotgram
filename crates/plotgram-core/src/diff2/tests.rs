//! Intent Diff 测试套件
//!
//! 覆盖：
//! - Round-trip：parse → format → parse → diff 为空
//! - 闭环：diff(A, B) → patch(A, Δ) → format → parse → diff with B 为空
//! - Validate：patch 结果通过 prepare → validate 无 errors
//! - 各场景：entity / relation / group / style_decl / diagram 属性变更
//! - ChangeSet JSON 序列化 round-trip
//! - Group membership 重建

use crate::ast::RawDiagram;
use crate::diff2::{diff, format, patch, ChangeSet};
use crate::pipeline::prepare;
use crate::prepare::StyleRequest;
use crate::validation;

// ─── 辅助函数 ──────────────────────────────────────────────────────

fn parse_dsl(source: &str) -> RawDiagram {
    crate::parse(source).unwrap_or_else(|e| panic!("DSL 解析失败: {e}\n源码:\n{source}"))
}

/// 断言 round-trip：parse → format → parse → diff 为空
fn assert_round_trip(source: &str) {
    let original = parse_dsl(source);
    let formatted = format(&original);
    let reparsed = parse_dsl(&formatted);
    let changes = diff(&original, &reparsed);
    assert!(
        changes.is_empty(),
        "round-trip diff 不为空:\n  原始:\n{}\n  格式化后:\n{}\n  变更: {:#?}",
        source,
        formatted,
        changes
    );
}

/// 断言闭环：diff(A, B) → patch(A, Δ) → format → parse → diff with B 为空
fn assert_closed_loop(source_a: &str, source_b: &str) {
    let a = parse_dsl(source_a);
    let b = parse_dsl(source_b);

    let changes = diff(&a, &b);
    let result = patch(&a, &changes);
    assert!(result.is_ok(), "patch 有错误: {:?}", result.errors);

    let formatted = format(&result.diagram);
    let reparsed = parse_dsl(&formatted);
    let final_diff = diff(&reparsed, &b);
    assert!(
        final_diff.is_empty(),
        "闭环 diff 不为空:\n  A:\n{}\n  B:\n{}\n  格式化后:\n{}\n  变更: {:#?}",
        source_a,
        source_b,
        formatted,
        final_diff
    );
}

/// 断言 patch 结果通过 prepare → validate 无 errors
fn assert_patch_validates(source_a: &str, source_b: &str) {
    let a = parse_dsl(source_a);
    let b = parse_dsl(source_b);

    let changes = diff(&a, &b);
    let result = patch(&a, &changes);
    assert!(result.is_ok(), "patch 有错误: {:?}", result.errors);

    let output = prepare(result.diagram, &StyleRequest::default())
        .unwrap_or_else(|e| panic!("prepare 失败: {e}"));
    let validation = validation::validate(&output.diagram);
    assert!(
        validation.errors.is_empty(),
        "validate 有 errors: {:?}",
        validation.errors
    );
}

// ─── Round-trip 测试 ───────────────────────────────────────────────

#[test]
fn round_trip_minimal() {
    assert_round_trip(
        r##"diagram flowchart {
    entity a "A"
    entity b "B"
    a -> b
}"##,
    );
}

#[test]
fn round_trip_empty() {
    assert_round_trip("diagram flowchart {\n}");
}

#[test]
fn round_trip_diagram_attributes() {
    assert_round_trip(
        r##"diagram flowchart {
    title: "测试图表"
    config {
        direction: left-to-right
        theme: common.clean-light
        render_style: excalidraw
    }
    entity a "A"
    a -> a
}"##,
    );
}

#[test]
fn round_trip_config_block() {
    assert_round_trip(
        r##"diagram flowchart {
    config {
        layout: sugiyama-v2 {
            group_padding: 20
        }
        edge_routing: bezier {
            tension: 0.55
        }
    }
    entity a "A"
    entity b "B"
    a -> b
}"##,
    );
}

#[test]
fn round_trip_entity_attributes() {
    assert_round_trip(
        r##"diagram flowchart {
    entity api "API 服务" {
        type: service
        status: healthy
        owner: "平台团队"
        style.fill: "#E3F2FD"
        style.stroke: "#1976D2"
        style.stroke_width: 2
        meta.version: "2.1.0"
        meta.port: 8080
    }
    entity db "数据库" {
        type: database
    }
    api -> db "查询"
}"##,
    );
}

#[test]
fn round_trip_all_arrow_types() {
    assert_round_trip(
        r##"diagram flowchart {
    entity a "A"
    entity b "B"
    entity c "C"
    a -> b "主动"
    b --> c "被动"
    c <-> a "双向"
}"##,
    );
}

#[test]
fn round_trip_relation_with_attributes() {
    assert_round_trip(
        r##"diagram flowchart {
    entity a "A"
    entity b "B"
    a -> b "请求" {
        style.stroke: "#C62828"
        style.dashed: true
        meta.latency: "200ms"
    }
}"##,
    );
}

#[test]
fn round_trip_groups_nested() {
    assert_round_trip(
        r##"diagram architecture {
    group frontend "前端" {
        layout: horizontal
        entity web "Web"
        entity mobile "Mobile"
    }
    group backend "后端" {
        border_style: dashed
        color: "blue"
        group api_layer "API 层" {
            entity gateway "网关"
        }
        entity worker "Worker"
    }
    web -> gateway
    mobile -> gateway
    gateway -> worker
}"##,
    );
}

#[test]
fn round_trip_style_decls() {
    assert_round_trip(
        r##"diagram flowchart {
    node_style service {
        fill: "#E3F2FD"
        stroke: "#1976D2"
        shape: rounded_rect
        stroke_width: 2
    }
    node_style database {
        fill: "#FFF3E0"
        stroke: "#E65100"
        shape: cylinder
    }
    edge_style error {
        stroke: "#C62828"
        stroke_width: 2.5
        dashed: true
    }
    entity api "API" {
        type: service
    }
    entity db "DB" {
        type: database
    }
    api -> db "查询"
}"##,
    );
}

#[test]
fn round_trip_string_escaping() {
    assert_round_trip(
        r##"diagram flowchart {
    title: "包含\"引号\"和\\反斜杠"
    entity a "标签\n换行"
    a -> a "自环"
}"##,
    );
}

#[test]
fn round_trip_number_formats() {
    assert_round_trip(
        r##"diagram flowchart {
    entity a "A" {
        style.stroke_width: 2
        style.width: 100.5
        style.height: 0
    }
    entity b "B" {
        style.stroke_width: 2.5
    }
    a -> b
}"##,
    );
}

#[test]
fn round_trip_boolean_values() {
    assert_round_trip(
        r##"diagram flowchart {
    entity a "A" {
        style.dashed: true
    }
    entity b "B" {
        style.dashed: false
    }
    a -> b
}"##,
    );
}

// ─── 闭环测试 ──────────────────────────────────────────────────────

#[test]
fn closed_loop_entity_add() {
    let a = r##"diagram flowchart {
    entity a "A"
    entity b "B"
    a -> b
}"##;
    let b = r##"diagram flowchart {
    entity a "A"
    entity b "B"
    entity c "C"
    a -> b
    b -> c
}"##;
    assert_closed_loop(a, b);
}

#[test]
fn closed_loop_entity_remove() {
    let a = r##"diagram flowchart {
    entity a "A"
    entity b "B"
    entity c "C"
    a -> b
    b -> c
}"##;
    let b = r##"diagram flowchart {
    entity a "A"
    entity b "B"
    a -> b
}"##;
    assert_closed_loop(a, b);
}

#[test]
fn closed_loop_entity_modify_label() {
    let a = r##"diagram flowchart {
    entity a "旧标签"
    entity b "B"
    a -> b
}"##;
    let b = r##"diagram flowchart {
    entity a "新标签"
    entity b "B"
    a -> b
}"##;
    assert_closed_loop(a, b);
}

#[test]
fn closed_loop_entity_modify_attributes() {
    let a = r##"diagram flowchart {
    entity api "API" {
        type: service
        status: healthy
    }
    entity db "DB"
    api -> db
}"##;
    let b = r##"diagram flowchart {
    entity api "API" {
        type: service
        status: degraded
        owner: "SRE 团队"
    }
    entity db "DB"
    api -> db
}"##;
    assert_closed_loop(a, b);
}

#[test]
fn closed_loop_entity_modify_group_id() {
    let a = r##"diagram flowchart {
    entity a "A"
    entity b "B"
    a -> b
}"##;
    let b = r##"diagram flowchart {
    group g "Group" {
        entity a "A"
    }
    entity b "B"
    a -> b
}"##;
    assert_closed_loop(a, b);
}

#[test]
fn closed_loop_relation_add_remove() {
    let a = r##"diagram flowchart {
    entity a "A"
    entity b "B"
    entity c "C"
    a -> b
}"##;
    let b = r##"diagram flowchart {
    entity a "A"
    entity b "B"
    entity c "C"
    a -> b
    b -> c "调用"
    c --> a "返回"
}"##;
    assert_closed_loop(a, b);
}

#[test]
fn closed_loop_relation_label_change_is_remove_add() {
    // label 是 relation 身份的一部分，变更 label = remove + add
    let a = r##"diagram flowchart {
    entity a "A"
    entity b "B"
    a -> b "旧标签"
}"##;
    let b = r##"diagram flowchart {
    entity a "A"
    entity b "B"
    a -> b "新标签"
}"##;
    assert_closed_loop(a, b);
}

#[test]
fn closed_loop_relation_arrow_modify() {
    let a = r##"diagram flowchart {
    entity a "A"
    entity b "B"
    a -> b "请求"
}"##;
    let b = r##"diagram flowchart {
    entity a "A"
    entity b "B"
    a --> b "请求"
}"##;
    assert_closed_loop(a, b);
}

#[test]
fn closed_loop_relation_attributes_modify() {
    let a = r##"diagram flowchart {
    entity a "A"
    entity b "B"
    a -> b "请求" {
        style.stroke: "#FF0000"
    }
}"##;
    let b = r##"diagram flowchart {
    entity a "A"
    entity b "B"
    a -> b "请求" {
        style.stroke: "#00FF00"
        style.dashed: true
    }
}"##;
    assert_closed_loop(a, b);
}

#[test]
fn closed_loop_group_add() {
    let a = r##"diagram flowchart {
    entity a "A"
    entity b "B"
    a -> b
}"##;
    let b = r##"diagram flowchart {
    group g "Group" {
        entity a "A"
    }
    entity b "B"
    a -> b
}"##;
    assert_closed_loop(a, b);
}

#[test]
fn closed_loop_group_remove() {
    let a = r##"diagram flowchart {
    group g "Group" {
        entity a "A"
    }
    entity b "B"
    a -> b
}"##;
    let b = r##"diagram flowchart {
    entity a "A"
    entity b "B"
    a -> b
}"##;
    assert_closed_loop(a, b);
}

#[test]
fn closed_loop_group_modify() {
    let a = r##"diagram architecture {
    group g "旧名称" {
        layout: horizontal
        entity a "A"
    }
    entity b "B"
    a -> b
}"##;
    let b = r##"diagram architecture {
    group g "新名称" {
        layout: vertical
        border_style: dashed
        entity a "A"
    }
    entity b "B"
    a -> b
}"##;
    assert_closed_loop(a, b);
}

#[test]
fn closed_loop_nested_group_add() {
    let a = r##"diagram architecture {
    group backend "后端" {
        entity api "API"
    }
    entity web "Web"
    web -> api
}"##;
    let b = r##"diagram architecture {
    group backend "后端" {
        group api_layer "API 层" {
            entity api "API"
        }
    }
    entity web "Web"
    web -> api
}"##;
    assert_closed_loop(a, b);
}

#[test]
fn closed_loop_style_decl_add() {
    let a = r##"diagram flowchart {
    entity api "API" {
        type: service
    }
    entity db "DB" {
        type: database
    }
    api -> db
}"##;
    let b = r##"diagram flowchart {
    node_style service {
        fill: "#E3F2FD"
        stroke: "#1976D2"
    }
    entity api "API" {
        type: service
    }
    entity db "DB" {
        type: database
    }
    api -> db
}"##;
    assert_closed_loop(a, b);
}

#[test]
fn closed_loop_style_decl_modify() {
    let a = r##"diagram flowchart {
    node_style service {
        fill: "#E3F2FD"
        stroke: "#1976D2"
    }
    entity api "API" {
        type: service
    }
    api -> api
}"##;
    let b = r##"diagram flowchart {
    node_style service {
        fill: "#C8E6C9"
        stroke: "#1976D2"
        shape: rounded_rect
    }
    entity api "API" {
        type: service
    }
    api -> api
}"##;
    assert_closed_loop(a, b);
}

#[test]
fn closed_loop_style_decl_remove() {
    let a = r##"diagram flowchart {
    node_style service {
        fill: "#E3F2FD"
    }
    edge_style error {
        stroke: "#C62828"
    }
    entity api "API" {
        type: service
    }
    api -> api
}"##;
    let b = r##"diagram flowchart {
    entity api "API" {
        type: service
    }
    api -> api
}"##;
    assert_closed_loop(a, b);
}

#[test]
fn closed_loop_diagram_type_change() {
    let a = r##"diagram flowchart {
    entity a "A"
    a -> a
}"##;
    let b = r##"diagram state {
    entity a "A"
    a -> a
}"##;
    assert_closed_loop(a, b);
}

#[test]
fn closed_loop_diagram_attributes_change() {
    let a = r##"diagram flowchart {
    title: "旧标题"
    config {
        direction: top-to-bottom
    }
    entity a "A"
    a -> a
}"##;
    let b = r##"diagram flowchart {
    title: "新标题"
    config {
        direction: left-to-right
        theme: common.clean-light
    }
    entity a "A"
    a -> a
}"##;
    assert_closed_loop(a, b);
}

#[test]
fn closed_loop_config_block_change() {
    let a = r##"diagram flowchart {
    config {
        layout: sugiyama-v2 {
            group_padding: 20
        }
    }
    entity a "A"
    entity b "B"
    a -> b
}"##;
    let b = r##"diagram flowchart {
    config {
        layout: sugiyama-v2 {
            group_padding: 40
        }
        edge_routing: orthogonal {
            slot_pitch: 40
        }
    }
    entity a "A"
    entity b "B"
    a -> b
}"##;
    assert_closed_loop(a, b);
}

#[test]
fn closed_loop_mixed_changes() {
    let a = r##"diagram flowchart {
    config {
        direction: top-to-bottom
    }
    entity a "A" {
        type: service
    }
    entity b "B"
    entity c "C"
    a -> b
    b -> c
}"##;
    let b = r##"diagram flowchart {
    title: "新图"
    config {
        direction: left-to-right
    }
    node_style service {
        fill: "#E3F2FD"
    }
    group g "Group" {
        entity a "A" {
            type: service
            status: healthy
        }
    }
    entity c "C"
    entity d "D"
    a -> c "调用"
    c -> d
}"##;
    assert_closed_loop(a, b);
}

#[test]
fn closed_loop_no_changes() {
    let a = r##"diagram flowchart {
    entity a "A"
    entity b "B"
    a -> b "请求"
}"##;
    // A 和 B 完全相同
    assert_closed_loop(a, a);
}

// ─── Validate 测试 ─────────────────────────────────────────────────

#[test]
fn patch_result_validates_entity_changes() {
    assert_patch_validates(
        r##"diagram flowchart {
    entity a "A"
    entity b "B"
    a -> b
}"##,
        r##"diagram flowchart {
    entity a "A"
    entity b "B"
    entity c "C"
    a -> b
    b -> c
    c -> a
}"##,
    );
}

#[test]
fn patch_result_validates_group_changes() {
    assert_patch_validates(
        r##"diagram architecture {
    entity a "A"
    entity b "B"
    a -> b
}"##,
        r##"diagram architecture {
    group g "Group" {
        entity a "A"
    }
    entity b "B"
    a -> b
}"##,
    );
}

#[test]
fn patch_result_validates_style_decl_changes() {
    assert_patch_validates(
        r##"diagram flowchart {
    entity api "API" {
        type: service
    }
    entity db "DB" {
        type: database
    }
    api -> db
}"##,
        r##"diagram flowchart {
    node_style service {
        fill: "#E3F2FD"
    }
    entity api "API" {
        type: service
    }
    entity db "DB" {
        type: database
    }
    api -> db
}"##,
    );
}

#[test]
fn patch_result_validates_diagram_type_change() {
    assert_patch_validates(
        r##"diagram flowchart {
    entity a "A"
    a -> a
}"##,
        r##"diagram state {
    entity a "A"
    a -> a
}"##,
    );
}

#[test]
fn patch_result_validates_config_block() {
    assert_patch_validates(
        r##"diagram flowchart {
    entity a "A"
    entity b "B"
    a -> b
}"##,
        r##"diagram flowchart {
    config {
        layout: sugiyama-v2 {
            group_padding: 20
        }
        edge_routing: bezier {
            tension: 0.55
        }
    }
    entity a "A"
    entity b "B"
    a -> b
}"##,
    );
}

// ─── ChangeSet JSON 序列化 ─────────────────────────────────────────

#[test]
fn changeset_json_round_trip() {
    let a = parse_dsl(
        r##"diagram flowchart {
    entity a "A"
    entity b "B"
    a -> b
}"##,
    );
    let b = parse_dsl(
        r##"diagram flowchart {
    entity a "A"
    entity c "C"
    a -> c "调用"
}"##,
    );

    let changes = diff(&a, &b);

    // 序列化为 JSON
    let json = serde_json::to_string(&changes).expect("ChangeSet 序列化失败");

    // 反序列化
    let deserialized: ChangeSet =
        serde_json::from_str(&json).expect("ChangeSet 反序列化失败");

    // 用反序列化的 ChangeSet 做 patch，结果应与直接 patch 相同
    let result1 = patch(&a, &changes);
    let result2 = patch(&a, &deserialized);

    assert!(result1.is_ok());
    assert!(result2.is_ok());

    // 两次 patch 结果应语义等价
    let diff_between = diff(&result1.diagram, &result2.diagram);
    assert!(
        diff_between.is_empty(),
        "JSON round-trip 后 patch 结果不一致: {:#?}",
        diff_between
    );
}

#[test]
fn changeset_json_structure() {
    let a = parse_dsl(
        r##"diagram flowchart {
    entity a "A"
}"##,
    );
    let b = parse_dsl(
        r##"diagram flowchart {
    entity a "A"
    entity b "B"
}"##,
    );

    let changes = diff(&a, &b);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes.changes[0].op, crate::diff2::ChangeOp::Add);

    let json = serde_json::to_value(&changes).unwrap();
    // 应有 changes 数组
    assert!(json.get("changes").is_some());
    let arr = json["changes"].as_array().unwrap();
    assert_eq!(arr.len(), 1);
    // 第一条应是 add 操作
    assert_eq!(arr[0]["op"], "add");
    assert_eq!(arr[0]["path"]["target"], "entity");
}

// ─── Group membership 重建测试 ─────────────────────────────────────

#[test]
fn group_membership_rebuilt_after_patch() {
    let a = parse_dsl(
        r##"diagram architecture {
    entity a "A"
    entity b "B"
}"##,
    );
    let b = parse_dsl(
        r##"diagram architecture {
    group g "Group" {
        entity a "A"
    }
    entity b "B"
}"##,
    );

    let changes = diff(&a, &b);
    let result = patch(&a, &changes);
    assert!(result.is_ok());

    let diagram = result.diagram.inner();

    // group g 应存在
    let group = diagram
        .groups
        .iter()
        .find(|g| g.id.as_str() == "g")
        .expect("group g 不存在");

    // entity_ids 应包含 a
    assert!(
        group.entity_ids.iter().any(|id| id.as_str() == "a"),
        "group.entity_ids 应包含 a, 实际: {:?}",
        group.entity_ids
    );

    // entity a 的 group_id 应为 g
    let entity_a = diagram
        .entities
        .iter()
        .find(|e| e.id.as_str() == "a")
        .unwrap();
    assert_eq!(
        entity_a.group_id.as_ref().map(|g| g.as_str()),
        Some("g")
    );

    // entity b 的 group_id 应为 None
    let entity_b = diagram
        .entities
        .iter()
        .find(|e| e.id.as_str() == "b")
        .unwrap();
    assert!(entity_b.group_id.is_none());
}

#[test]
fn nested_group_membership_rebuilt() {
    let a = parse_dsl(
        r##"diagram architecture {
    group parent "Parent" {
        entity a "A"
    }
    entity b "B"
}"##,
    );
    let b = parse_dsl(
        r##"diagram architecture {
    group parent "Parent" {
        group child "Child" {
            entity a "A"
        }
    }
    entity b "B"
}"##,
    );

    let changes = diff(&a, &b);
    let result = patch(&a, &changes);
    assert!(result.is_ok());

    let diagram = result.diagram.inner();

    let parent = diagram
        .groups
        .iter()
        .find(|g| g.id.as_str() == "parent")
        .unwrap();
    let child = diagram
        .groups
        .iter()
        .find(|g| g.id.as_str() == "child")
        .unwrap();

    // parent 的 child_group_ids 应包含 child
    assert!(
        parent
            .child_group_ids
            .iter()
            .any(|id| id.as_str() == "child"),
        "parent.child_group_ids 应包含 child"
    );

    // child 的 parent_id 应为 parent
    assert_eq!(
        child.parent_id.as_ref().map(|g| g.as_str()),
        Some("parent")
    );

    // child 的 depth 应为 1
    assert_eq!(child.depth, 1);

    // parent 的 depth 应为 0
    assert_eq!(parent.depth, 0);

    // child 的 entity_ids 应包含 a
    assert!(
        child.entity_ids.iter().any(|id| id.as_str() == "a"),
        "child.entity_ids 应包含 a"
    );

    // parent 的 entity_ids 应为空（a 现在在 child 里）
    assert!(
        parent.entity_ids.is_empty(),
        "parent.entity_ids 应为空, 实际: {:?}",
        parent.entity_ids
    );
}

// ─── Diff 精确性测试 ───────────────────────────────────────────────

#[test]
fn diff_empty_when_identical() {
    let a = parse_dsl(
        r##"diagram flowchart {
    entity a "A"
    a -> a
}"##,
    );
    let changes = diff(&a, &a);
    assert!(changes.is_empty());
}

#[test]
fn diff_counts_entity_add() {
    let a = parse_dsl(
        r##"diagram flowchart {
    entity a "A"
}"##,
    );
    let b = parse_dsl(
        r##"diagram flowchart {
    entity a "A"
    entity b "B"
}"##,
    );
    let changes = diff(&a, &b);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes.changes[0].op, crate::diff2::ChangeOp::Add);
    assert_eq!(changes.changes[0].path.target, crate::diff2::ChangeTarget::Entity);
}

#[test]
fn diff_relation_label_change_is_remove_add() {
    let a = parse_dsl(
        r##"diagram flowchart {
    entity a "A"
    entity b "B"
    a -> b "old"
}"##,
    );
    let b = parse_dsl(
        r##"diagram flowchart {
    entity a "A"
    entity b "B"
    a -> b "new"
}"##,
    );
    let changes = diff(&a, &b);
    // label 变更 = remove 旧 relation + add 新 relation
    assert_eq!(changes.len(), 2);
    let has_remove = changes.changes.iter().any(|c| {
        c.op == crate::diff2::ChangeOp::Remove
            && c.path.target == crate::diff2::ChangeTarget::Relation
    });
    let has_add = changes.changes.iter().any(|c| {
        c.op == crate::diff2::ChangeOp::Add
            && c.path.target == crate::diff2::ChangeTarget::Relation
    });
    assert!(has_remove, "应有 relation Remove: {:#?}", changes);
    assert!(has_add, "应有 relation Add: {:#?}", changes);
}

#[test]
fn diff_relation_arrow_change_is_modify() {
    let a = parse_dsl(
        r##"diagram flowchart {
    entity a "A"
    entity b "B"
    a -> b "请求"
}"##,
    );
    let b = parse_dsl(
        r##"diagram flowchart {
    entity a "A"
    entity b "B"
    a --> b "请求"
}"##,
    );
    let changes = diff(&a, &b);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes.changes[0].op, crate::diff2::ChangeOp::Modify);
    assert_eq!(
        changes.changes[0].path.target,
        crate::diff2::ChangeTarget::Relation
    );
    assert_eq!(changes.changes[0].path.attr_key.as_deref(), Some("arrow"));
}

// ─── Format 输出验证 ───────────────────────────────────────────────

#[test]
fn format_produces_valid_dsl() {
    let raw = parse_dsl(
        r##"diagram flowchart {
    title: "测试"
    config {
        direction: left-to-right
    }
    node_style service {
        fill: "#E3F2FD"
    }
    entity a "A" {
        type: service
    }
    entity b "B"
    a -> b "调用"
}"##,
    );

    let formatted = format(&raw);

    // 格式化后的文本应能重新解析
    let reparsed = parse_dsl(&formatted);

    // 且语义等价
    let changes = diff(&raw, &reparsed);
    assert!(changes.is_empty(), "格式化后语义不等价: {:#?}", changes);
}

#[test]
fn format_empty_diagram() {
    let raw = parse_dsl("diagram flowchart {\n}");
    let formatted = format(&raw);
    assert!(
        formatted.contains("diagram flowchart {"),
        "应包含 diagram 声明: {}",
        formatted
    );
}

#[test]
fn format_sorts_attributes() {
    let raw = parse_dsl(
        r##"diagram flowchart {
    entity z "Z" {
        type: service
        style.fill: "#FFF"
        meta.x: 1
        status: healthy
        style.stroke: "#000"
        meta.a: 2
    }
    z -> z
}"##,
    );
    let formatted = format(&raw);

    // standard 属性应按 key 排序：status 在 type 之前
    let status_pos = formatted.find("status:").unwrap();
    let type_pos = formatted.find("type:").unwrap();
    assert!(
        status_pos < type_pos,
        "standard 属性应按 key 排序 (status < type)"
    );

    // style 属性应按 key 排序：fill 在 stroke 之前
    let fill_pos = formatted.find("style.fill:").unwrap();
    let stroke_pos = formatted.find("style.stroke:").unwrap();
    assert!(fill_pos < stroke_pos, "style 属性应按 key 排序 (fill < stroke)");

    // meta 属性应按 key 排序：a 在 x 之前
    let meta_a_pos = formatted.find("meta.a:").unwrap();
    let meta_x_pos = formatted.find("meta.x:").unwrap();
    assert!(meta_a_pos < meta_x_pos, "meta 属性应按 key 排序 (a < x)");
}

#[test]
fn format_outputs_doc_comment_as_is() {
    let raw = parse_dsl(
        r##"// 文件头文档注释
// 第二行

diagram flowchart {
    entity a "A"
    a -> a
}"##,
    );

    assert_eq!(
        raw.inner().doc_comment.as_deref(),
        Some("// 文件头文档注释\n// 第二行\n")
    );

    let formatted = format(&raw);
    assert!(
        formatted.starts_with("// 文件头文档注释\n// 第二行\ndiagram flowchart {"),
        "Formatter 应原样输出文件头文档注释: {}",
        formatted
    );
}

#[test]
fn round_trip_doc_comment() {
    let source = r##"// 文档注释
// 多行

diagram flowchart {
    entity a "A"
    a -> a
}"##;
    let original = parse_dsl(source);
    let formatted = format(&original);
    let reparsed = parse_dsl(&formatted);

    assert_eq!(
        original.inner().doc_comment,
        reparsed.inner().doc_comment,
        "format -> parse 应保留 doc_comment"
    );

    // 其余语义仍等价（diff 不比较注释内容，仅验证 AST 主体）
    let changes = diff(&original, &reparsed);
    assert!(changes.is_empty(), "格式化后主体语义不等价: {:#?}", changes);
}

#[test]
fn non_doc_comments_are_lost_after_format() {
    let raw = parse_dsl(
        r##"diagram flowchart {
    entity a "A" // 行尾注释
    a -> a // 另一个注释
}"##,
    );
    let formatted = format(&raw);
    assert!(
        !formatted.contains("行尾注释"),
        "非文件头注释经 Formatter 后应丢失: {}",
        formatted
    );
    assert!(
        !formatted.contains("另一个注释"),
        "非文件头注释经 Formatter 后应丢失: {}",
        formatted
    );
}
