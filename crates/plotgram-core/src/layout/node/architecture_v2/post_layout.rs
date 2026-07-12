//! 架构图布局后处理：单 group 行居中等 L1 特例。

use crate::ast::Diagram;
use crate::layout::LayoutResult;
use std::collections::HashMap;

/// 架构图单 group 行居中：当某 macro rank 只有一个 group 时，
/// 将其水平居中到 sibling 包围盒（或画布）宽度，避免窄行贴左留下大片空白。
///
/// 多个顶层 group 时同样对「单 group 行」居中（RankBand Center 语义）。
///
/// 确定性：按 group id 字典序处理，不依赖 HashMap 迭代序。
pub(crate) fn center_single_group_rows(diagram: &Diagram, layout: &mut LayoutResult) {
    if layout.groups.is_empty() {
        return;
    }

    let top_ids: Vec<String> = diagram
        .groups
        .iter()
        .filter(|g| g.parent_id.is_none())
        .map(|g| g.id.as_str().to_string())
        .collect();
    if top_ids.is_empty() {
        return;
    }

    let mut rows: Vec<(f64, Vec<String>)> = Vec::new();
    for id in &top_ids {
        if let Some(g) = layout.groups.get(id) {
            let row_idx = rows
                .iter()
                .position(|(row_y, _)| (row_y - g.y).abs() < 0.5);
            match row_idx {
                Some(idx) => rows[idx].1.push(id.clone()),
                None => rows.push((g.y, vec![id.clone()])),
            }
        }
    }

    let full_right = layout
        .groups
        .values()
        .map(|g| g.x + g.width)
        .fold(0.0_f64, f64::max);
    let full_left = layout
        .groups
        .values()
        .map(|g| g.x)
        .fold(f64::INFINITY, f64::min);
    let full_width = full_right - full_left;
    if full_width <= 0.0 {
        return;
    }

    let mut node_to_top: HashMap<String, String> = HashMap::new();
    for entity in &diagram.entities {
        let Some(start_gid) = entity.group_id.as_ref() else {
            continue;
        };
        let mut cur = start_gid.as_str().to_string();
        loop {
            if top_ids.contains(&cur) {
                node_to_top.insert(entity.id.as_str().to_string(), cur);
                break;
            }
            let Some(g) = diagram.find_group(&cur) else {
                break;
            };
            match &g.parent_id {
                Some(p) => cur = p.as_str().to_string(),
                None => break,
            }
        }
    }

    let mut group_to_top: HashMap<String, String> = HashMap::new();
    for group in &diagram.groups {
        let mut cur = group.id.as_str().to_string();
        loop {
            if top_ids.contains(&cur) {
                group_to_top.insert(group.id.as_str().to_string(), cur);
                break;
            }
            let Some(g) = diagram.find_group(&cur) else {
                break;
            };
            match &g.parent_id {
                Some(p) => cur = p.as_str().to_string(),
                None => break,
            }
        }
    }

    for (_, row_ids) in &rows {
        if row_ids.len() != 1 {
            continue;
        }
        let top_id = &row_ids[0];
        let Some(g) = layout.groups.get(top_id) else {
            continue;
        };
        let block_width = g.width;
        if block_width >= full_width {
            continue;
        }
        let target_x = full_left + (full_width - block_width) / 2.0;
        let shift = target_x - g.x;
        if shift.abs() < 0.5 {
            continue;
        }

        if let Some(g) = layout.groups.get_mut(top_id) {
            g.x += shift;
        }
        for (node_id, nl) in layout.nodes.iter_mut() {
            if node_to_top.get(node_id).map(String::as_str) == Some(top_id.as_str()) {
                nl.x += shift;
            }
        }
        let mut nested: Vec<String> = layout
            .groups
            .keys()
            .filter(|gid| {
                gid.as_str() != top_id.as_str()
                    && group_to_top.get(*gid).map(String::as_str) == Some(top_id.as_str())
            })
            .cloned()
            .collect();
        nested.sort();
        for gid in nested {
            if let Some(g) = layout.groups.get_mut(&gid) {
                g.x += shift;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::layout::compute_layout_with_plan;
    use crate::pipeline::parse_prepare_validate;
    use crate::prepare::StyleRequest;

    #[test]
    fn stress_nested_cloud_gets_left_gutter_budget() {
        let source = include_str!(
            "../../../../../../showcase/architecture/c.layout-stress-nested.pgm"
        );
        let output = parse_prepare_validate(source, &StyleRequest::default());
        let prepared = output.diagram.expect("valid diagram");
        let layout = compute_layout_with_plan(prepared.inner(), prepared.layout_plan())
            .expect("layout");

        let cloud_left = layout
            .hints
            .group_routing
            .as_ref()
            .and_then(|h| h.side_gutters.get("cloud"))
            .map(|g| g.left)
            .unwrap_or(0.0);
        assert!(
            cloud_left > 0.0,
            "cloud should have positive EGB left gutter, got {cloud_left}"
        );
    }

    #[test]
    fn stress_nested_no_node_overlap() {
        use crate::layout::lint::{lint_layout, LintMetricsSummary};

        let source = include_str!(
            "../../../../../../showcase/architecture/c.layout-stress-nested.pgm"
        );
        let output = parse_prepare_validate(source, &StyleRequest::default());
        let prepared = output.diagram.expect("valid diagram");
        let diagram = prepared.inner();
        let layout = compute_layout_with_plan(diagram, prepared.layout_plan())
            .expect("layout");
        let summary = LintMetricsSummary::from_report(&lint_layout(diagram, &layout));
        assert_eq!(
            summary.node_overlap, 0,
            "unexpected node overlaps in stress-nested layout"
        );
        let a = layout.nodes.get("db_master").expect("db_master");
        let b = layout.nodes.get("db_replica").expect("db_replica");
        let gap = if a.x <= b.x {
            b.x - (a.x + a.width)
        } else {
            a.x - (b.x + b.width)
        };
        // 「主从同步」标签约 52px 宽，间距必须能放下边+label
        assert!(
            gap >= 52.0,
            "db_master/db_replica gap too tight for edge label: gap={gap:.1} a=({:.1},w={:.1}) b=({:.1},w={:.1})",
            a.x,
            a.width,
            b.x,
            b.width
        );
    }

    #[test]
    fn stress_nested_has_no_sibling_group_overlap() {
        use crate::layout::lint::{lint_layout, LintMetricsSummary};

        let source = include_str!(
            "../../../../../../showcase/architecture/c.layout-stress-nested.pgm"
        );
        let output = parse_prepare_validate(source, &StyleRequest::default());
        let prepared = output.diagram.expect("valid diagram");
        let diagram = prepared.inner();
        let layout = compute_layout_with_plan(diagram, prepared.layout_plan())
            .expect("layout");
        let summary = LintMetricsSummary::from_report(&lint_layout(diagram, &layout));
        assert_eq!(
            summary.group_overlap, 0,
            "unexpected group overlaps in stress-nested layout"
        );
    }

    #[test]
    fn stress_nested_nodes_stay_inside_leaf_groups() {
        let source = include_str!(
            "../../../../../../showcase/architecture/c.layout-stress-nested.pgm"
        );
        let output = parse_prepare_validate(source, &StyleRequest::default());
        let prepared = output.diagram.expect("valid diagram");
        let diagram = prepared.inner();
        let layout = compute_layout_with_plan(diagram, prepared.layout_plan())
            .expect("layout");

        let ds = layout.groups.get("data_subnet").expect("data_subnet");
        let ds_bottom = ds.y + ds.height;
        for id in ["db_master", "db_replica", "mq", "redis"] {
            let n = layout.nodes.get(id).expect(id);
            assert!(
                n.y + n.height <= ds_bottom + 1.0,
                "{id} bottom {:.1} exceeds data_subnet bottom {:.1}",
                n.y + n.height,
                ds_bottom
            );
        }

        let cloud = layout.groups.get("cloud").expect("cloud");
        let cloud_bottom = cloud.y + cloud.height;
        assert!(
            ds_bottom <= cloud_bottom + 1.0,
            "data_subnet bottom {:.1} exceeds cloud bottom {:.1}",
            ds_bottom,
            cloud_bottom
        );
        // 父组底边不应与子组完全贴死（至少保留容器 bottom padding 的一半量级）
        assert!(
            cloud_bottom - ds_bottom >= 8.0,
            "cloud should keep bottom padding below data_subnet, gap={:.1}",
            cloud_bottom - ds_bottom
        );
    }

    #[test]
    fn stress_nested_child_groups_stay_inside_parents() {
        use crate::layout::lint::{lint_layout, LintRuleId, LintMetricsSummary};

        let source = include_str!(
            "../../../../../../showcase/architecture/c.layout-stress-nested.pgm"
        );
        let output = parse_prepare_validate(source, &StyleRequest::default());
        let prepared = output.diagram.expect("valid diagram");
        let diagram = prepared.inner();
        let layout = compute_layout_with_plan(diagram, prepared.layout_plan())
            .expect("layout");
        let report = lint_layout(diagram, &layout);
        let outside: Vec<_> = report
            .violations
            .iter()
            .filter(|v| v.rule == LintRuleId::ChildGroupOutsideParent)
            .collect();
        assert!(
            outside.is_empty(),
            "child groups outside parent: {:?}",
            outside
                .iter()
                .map(|v| v.message.as_str())
                .collect::<Vec<_>>()
        );
        let summary = LintMetricsSummary::from_report(&report);
        assert_eq!(summary.group_overlap, 0);
    }

    #[test]
    fn stress_nested_edges_attach_to_node_ports() {
        use crate::layout::group::PORT_STUB_CLEARANCE;

        let source = include_str!(
            "../../../../../../showcase/architecture/c.layout-stress-nested.pgm"
        );
        let output = parse_prepare_validate(source, &StyleRequest::default());
        let prepared = output.diagram.expect("valid diagram");
        let diagram = prepared.inner();
        let layout = compute_layout_with_plan(diagram, prepared.layout_plan())
            .expect("layout");

        let max_stub = PORT_STUB_CLEARANCE + 4.0;
        for (i, edge) in layout.edges.iter().enumerate() {
            let rel = &diagram.relations[i];
            let from = layout.nodes.get(rel.from.as_str()).expect("from node");
            let to = layout.nodes.get(rel.to.as_str()).expect("to node");
            let start = edge.path_start().expect("path start");
            let end = edge.path_end().expect("path end");
            let ds = dist_point_to_rect(start.x, start.y, from.x, from.y, from.width, from.height);
            let de = dist_point_to_rect(end.x, end.y, to.x, to.y, to.width, to.height);
            assert!(
                ds <= max_stub,
                "edge {} start detached from {} by {:.1}px (max {:.1})",
                i,
                rel.from.as_str(),
                ds,
                max_stub
            );
            assert!(
                de <= max_stub,
                "edge {} end detached from {} by {:.1}px (max {:.1})",
                i,
                rel.to.as_str(),
                de,
                max_stub
            );
        }
    }

    fn dist_point_to_rect(px: f64, py: f64, x: f64, y: f64, w: f64, h: f64) -> f64 {
        let dx = (x - px).max(0.0).max(px - (x + w));
        let dy = (y - py).max(0.0).max(py - (y + h));
        (dx * dx + dy * dy).sqrt()
    }
}
