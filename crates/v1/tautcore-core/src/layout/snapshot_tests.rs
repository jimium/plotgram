//! 布局输出快照测试（P4：insta 替代 magic-number 断言）。
//!
//! 重构布局算法时，坐标变化通过 `cargo insta review` 批量审阅/接受，
//! 无需逐个修改硬编码数值。运行：
//! ```bash
//! cargo test -p tautcore-core --lib -- layout::snapshot_tests
//! cargo insta review   # 审阅变更
//! ```

#[cfg(test)]
mod tests {
    use crate::pipeline::prepare::parse_prepare_validate;
    use crate::prepare::StyleRequest;

    /// 从 DSL 源码计算布局，返回可序列化的摘要（节点坐标 + 画布尺寸）。
    fn layout_summary(source: &str) -> serde_json::Value {
        let output = parse_prepare_validate(source, &StyleRequest::default());
        assert!(output.is_valid(), "parse_prepare_validate failed: {:?}", output.errors);
        let prepared = output.diagram.unwrap();
        let diagram = prepared.inner();
        let plan = prepared.layout_plan();
        let result =
            crate::layout::pipeline::entry::compute_layout_with_plan(diagram, plan)
                .expect("layout failed");

        // 按 id 排序保证确定性
        let mut nodes: Vec<_> = result
            .nodes
            .iter()
            .map(|(id, n)| {
                serde_json::json!({
                    "id": id,
                    "x": (n.x * 100.0).round() / 100.0,
                    "y": (n.y * 100.0).round() / 100.0,
                    "w": (n.width * 100.0).round() / 100.0,
                    "h": (n.height * 100.0).round() / 100.0,
                })
            })
            .collect();
        nodes.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));

        serde_json::json!({
            "canvas_w": (result.total_width * 100.0).round() / 100.0,
            "canvas_h": (result.total_height * 100.0).round() / 100.0,
            "nodes": nodes,
        })
    }

    #[test]
    fn flowchart_two_nodes_snapshot() {
        let summary = layout_summary(
            r#"diagram flowchart {
                entity a "开始"
                entity b "结束"
                a -> b
            }"#,
        );
        insta::assert_json_snapshot!(summary);
    }

    #[test]
    fn flowchart_diamond_snapshot() {
        let summary = layout_summary(
            r#"diagram flowchart {
                entity start "开始"
                entity check "检查"
                entity yes "是"
                entity no "否"
                start -> check
                check -> yes "通过"
                check -> no "拒绝"
            }"#,
        );
        insta::assert_json_snapshot!(summary);
    }

    #[test]
    fn architecture_groups_snapshot() {
        let summary = layout_summary(
            r#"diagram architecture {
                group frontend "前端" {
                    entity web "Web"
                    entity mobile "Mobile"
                }
                group backend "后端" {
                    entity api "API"
                    entity db "DB"
                }
                web -> api
                mobile -> api
                api -> db
            }"#,
        );
        insta::assert_json_snapshot!(summary);
    }
}
