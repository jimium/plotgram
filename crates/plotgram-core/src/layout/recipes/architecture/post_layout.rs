//! 架构图布局后处理：单 group 行居中等 L1 特例。

use crate::ast::Diagram;
use crate::layout::LayoutResult;
use std::collections::{BTreeSet, HashMap, HashSet};

const PENDANT_ALIGN_EPS: f64 = 2.0;
const GROUP_INNER_GUARD: f64 = 8.0;

/// 架构图单 group 行居中：当某 macro rank 只有一个 group 时，
/// 将其水平居中到 sibling 包围盒（或画布）宽度，避免窄行贴左留下大片空白。
///
/// 多个顶层 group 时同样对「单 group 行」居中（RankBand Center 语义）。
///
/// 确定性：按 group id 字典序处理，不依赖 HashMap 迭代序。
pub(crate) fn center_single_group_rows(_diagram: &Diagram, _layout: &mut LayoutResult) {
    // G3：已停用（Equal 条带附属美学）；保留符号以免旧测试引用时编译断，无写权。
}

/// V3b-A：最终 group frame 落定后，对跨作用域的唯一入边链重申主轴对齐。
///
/// 目标在 leaf group 内时原子平移该组全部直接成员，保持组内纵链；目标无组时只移动
/// 目标节点。若目标块还接受其它作用域的输入、越出组框或与其它节点碰撞，则整组放弃。
/// 返回实际移动的节点，供调用方在路由前做增量重路由。
pub(crate) fn align_cross_scope_pendant_chains(
    diagram: &Diagram,
    layout: &mut LayoutResult,
) -> HashSet<String> {
    let node_scope: HashMap<String, Option<String>> = diagram
        .entities
        .iter()
        .map(|entity| {
            (
                entity.id.as_str().to_string(),
                entity.group_id.as_ref().map(|id| id.as_str().to_string()),
            )
        })
        .collect();
    let mut incoming_count: HashMap<String, usize> = HashMap::new();
    for relation in &diagram.relations {
        *incoming_count
            .entry(relation.to.as_str().to_string())
            .or_default() += 1;
    }

    let ranks = layout.hints.sugiyama_ranks.as_ref();
    let mut moved = HashSet::new();
    let mut moved_blocks: HashSet<String> = HashSet::new();

    for relation in &diagram.relations {
        let from_id = relation.from.as_str();
        let to_id = relation.to.as_str();
        if incoming_count.get(to_id).copied().unwrap_or(0) != 1 {
            continue;
        }
        let from_scope = node_scope.get(from_id).cloned().flatten();
        let to_scope = node_scope.get(to_id).cloned().flatten();
        let block_key = to_scope
            .as_ref()
            .map(|group| format!("group:{group}"))
            .unwrap_or_else(|| format!("node:{to_id}"));
        if from_scope == to_scope || moved_blocks.contains(&block_key) {
            continue;
        }
        if let Some(ranks) = ranks {
            let (Some(&from_rank), Some(&to_rank)) = (ranks.get(from_id), ranks.get(to_id)) else {
                continue;
            };
            if to_rank != from_rank + 1 {
                continue;
            }
        }

        let mut block: Vec<String> = match to_scope.as_ref() {
            Some(group_id) => diagram
                .entities
                .iter()
                .filter(|entity| {
                    entity.group_id.as_ref().map(|id| id.as_str()) == Some(group_id.as_str())
                })
                .map(|entity| entity.id.as_str().to_string())
                .collect(),
            None => vec![to_id.to_string()],
        };
        block.sort();
        if block.is_empty() {
            continue;
        }
        let block_set: HashSet<&str> = block.iter().map(String::as_str).collect();

        let external_scopes: BTreeSet<Option<String>> = diagram
            .relations
            .iter()
            .filter(|edge| {
                block_set.contains(edge.to.as_str()) && !block_set.contains(edge.from.as_str())
            })
            .map(|edge| node_scope.get(edge.from.as_str()).cloned().flatten())
            .collect();
        if external_scopes.len() != 1 || !external_scopes.contains(&from_scope) {
            continue;
        }

        let (Some(from), Some(to)) = (layout.nodes.get(from_id), layout.nodes.get(to_id)) else {
            continue;
        };
        let delta = (from.x + from.width / 2.0) - (to.x + to.width / 2.0);
        if delta.abs() <= PENDANT_ALIGN_EPS {
            moved_blocks.insert(block_key);
            continue;
        }

        if let Some(group_id) = to_scope.as_ref() {
            let Some(group) = layout.groups.get(group_id) else {
                continue;
            };
            let inside = block.iter().all(|id| {
                layout.nodes.get(id).is_some_and(|node| {
                    node.x + delta >= group.x + GROUP_INNER_GUARD
                        && node.x + delta + node.width <= group.x + group.width - GROUP_INNER_GUARD
                })
            });
            if !inside {
                continue;
            }
        }

        let collides = block.iter().any(|id| {
            let Some(node) = layout.nodes.get(id) else {
                return true;
            };
            let left = node.x + delta;
            let right = left + node.width;
            let top = node.y;
            let bottom = node.y + node.height;
            layout.nodes.iter().any(|(other_id, other)| {
                !block_set.contains(other_id.as_str())
                    && left < other.x + other.width - 0.5
                    && right > other.x + 0.5
                    && top < other.y + other.height - 0.5
                    && bottom > other.y + 0.5
            })
        });
        if collides {
            continue;
        }

        for id in &block {
            if let Some(node) = layout.nodes.get_mut(id) {
                node.x += delta;
                moved.insert(id.clone());
            }
        }
        moved_blocks.insert(block_key);
    }

    moved
}

/// L2.2：仅对「多 client → 同一 hub」做刚体平移重申（不跑全层 `align_client_nodes_to_hubs`）。
///
/// 背景：全量重申会在 budget/frame 后大范围挪点，增量 reroute 曾导致多图穿组暴涨。
/// 本函数只平移「独占指向同一 hub」的 client 集合，使组质心对齐 hub；
/// 碰撞或越组内边距则 Skip（不破约硬推）。
pub(crate) fn reassert_multi_client_hub_centroids(
    diagram: &Diagram,
    layout: &mut LayoutResult,
) -> HashSet<String> {
    let Some(ranks) = layout.hints.sugiyama_ranks.as_ref() else {
        return HashSet::new();
    };

    let node_scope: HashMap<String, Option<String>> = diagram
        .entities
        .iter()
        .map(|entity| {
            (
                entity.id.as_str().to_string(),
                entity.group_id.as_ref().map(|id| id.as_str().to_string()),
            )
        })
        .collect();

    // hub → clients（确定性：边序 + client id 排序）
    let mut hub_clients: HashMap<String, Vec<String>> = HashMap::new();
    for relation in &diagram.relations {
        let from = relation.from.as_str();
        let to = relation.to.as_str();
        let (Some(&from_rank), Some(&to_rank)) = (ranks.get(from), ranks.get(to)) else {
            continue;
        };
        if to_rank != from_rank + 1 {
            continue;
        }
        // client 仅此一条出边到 hub（独占 fan-in 叶）
        let out_count = diagram
            .relations
            .iter()
            .filter(|r| r.from.as_str() == from)
            .count();
        if out_count != 1 {
            continue;
        }
        hub_clients
            .entry(to.to_string())
            .or_default()
            .push(from.to_string());
    }

    let mut hub_ids: Vec<String> = hub_clients.keys().cloned().collect();
    hub_ids.sort();
    let mut moved = HashSet::new();

    for hub_id in hub_ids {
        let Some(clients) = hub_clients.get_mut(&hub_id) else {
            continue;
        };
        clients.sort();
        clients.dedup();
        if clients.len() < 2 {
            continue;
        }
        // 同一 leaf group（或均无组）才刚体平移，避免跨容器硬扯
        let scopes: BTreeSet<Option<String>> = clients
            .iter()
            .map(|c| node_scope.get(c).cloned().flatten())
            .collect();
        if scopes.len() != 1 {
            continue;
        }
        let client_scope = scopes.iter().next().cloned().flatten();

        let Some(hub) = layout.nodes.get(&hub_id) else {
            continue;
        };
        let hub_cx = hub.x + hub.width / 2.0;
        let mut sum_cx = 0.0;
        let mut ok = true;
        for c in clients.iter() {
            let Some(n) = layout.nodes.get(c) else {
                ok = false;
                break;
            };
            sum_cx += n.x + n.width / 2.0;
        }
        if !ok {
            continue;
        }
        let clients_cx = sum_cx / clients.len() as f64;
        let delta = hub_cx - clients_cx;
        if delta.abs() <= PENDANT_ALIGN_EPS {
            continue;
        }

        let block_set: HashSet<&str> = clients.iter().map(String::as_str).collect();
        if let Some(group_id) = client_scope.as_ref() {
            let Some(group) = layout.groups.get(group_id) else {
                continue;
            };
            let inside = clients.iter().all(|id| {
                layout.nodes.get(id).is_some_and(|node| {
                    node.x + delta >= group.x + GROUP_INNER_GUARD
                        && node.x + delta + node.width
                            <= group.x + group.width - GROUP_INNER_GUARD
                })
            });
            if !inside {
                continue;
            }
        }

        let collides = clients.iter().any(|id| {
            let Some(node) = layout.nodes.get(id) else {
                return true;
            };
            let left = node.x + delta;
            let right = left + node.width;
            let top = node.y;
            let bottom = node.y + node.height;
            layout.nodes.iter().any(|(other_id, other)| {
                !block_set.contains(other_id.as_str())
                    && left < other.x + other.width - 0.5
                    && right > other.x + 0.5
                    && top < other.y + other.height - 0.5
                    && bottom > other.y + 0.5
            })
        });
        if collides {
            continue;
        }

        for id in clients.iter() {
            if let Some(node) = layout.nodes.get_mut(id) {
                node.x += delta;
                moved.insert(id.clone());
            }
        }
    }

    moved
}

#[cfg(test)]
mod tests {
    use crate::layout::compute_layout_with_plan;
    use crate::pipeline::parse_prepare_validate;
    use crate::prepare::StyleRequest;

    #[test]
    fn microservices_hub_client_centroid_within_eps_after_pipeline() {
        // G-pre：不再 Equal/SharedLines 强制共线；质心对齐放宽为粗约束，仅防离谱漂移。
        let source =
            include_str!("../../../../../../showcase/architecture/product.microservices.pgm");
        let output = parse_prepare_validate(source, &StyleRequest::default());
        let prepared = output.diagram.expect("valid diagram");
        let layout =
            compute_layout_with_plan(prepared.inner(), prepared.layout_plan()).expect("layout");

        let web = layout.nodes.get("web").expect("web");
        let mobile = layout.nodes.get("mobile").expect("mobile");
        let gateway = layout.nodes.get("gateway").expect("gateway");
        let clients_cx = ((web.x + web.width / 2.0) + (mobile.x + mobile.width / 2.0)) / 2.0;
        let hub_cx = gateway.x + gateway.width / 2.0;
        let delta = (clients_cx - hub_cx).abs();
        assert!(
            delta <= 80.0,
            "hub↔client centroid delta {delta:.3}px exceeds 80px (clients={clients_cx:.3} hub={hub_cx:.3})"
        );
    }

    #[test]
    fn stress_nested_cloud_gets_left_gutter_budget() {
        let source =
            include_str!("../../../../../../showcase/architecture/stress.layout-stress-nested.pgm");
        let output = parse_prepare_validate(source, &StyleRequest::default());
        let prepared = output.diagram.expect("valid diagram");
        let layout =
            compute_layout_with_plan(prepared.inner(), prepared.layout_plan()).expect("layout");

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
        use crate::layout::quality::lint::{lint_layout, LintMetricsSummary};

        let source =
            include_str!("../../../../../../showcase/architecture/stress.layout-stress-nested.pgm");
        let output = parse_prepare_validate(source, &StyleRequest::default());
        let prepared = output.diagram.expect("valid diagram");
        let diagram = prepared.inner();
        let layout = compute_layout_with_plan(diagram, prepared.layout_plan()).expect("layout");
        let summary = LintMetricsSummary::from_report(&lint_layout(diagram, &layout));
        // G-pre：去掉 Equal 后 db 主从可能竖排，不再断言水平 label 间隙；红线仍是无节点重叠。
        assert_eq!(
            summary.node_overlap, 0,
            "unexpected node overlaps in stress-nested layout"
        );
        assert!(
            layout.nodes.contains_key("db_master") && layout.nodes.contains_key("db_replica"),
            "db_master/db_replica must remain in layout"
        );
    }

    #[test]
    fn stress_nested_has_no_sibling_group_overlap() {
        use crate::layout::quality::lint::{lint_layout, LintMetricsSummary};

        let source =
            include_str!("../../../../../../showcase/architecture/stress.layout-stress-nested.pgm");
        let output = parse_prepare_validate(source, &StyleRequest::default());
        let prepared = output.diagram.expect("valid diagram");
        let diagram = prepared.inner();
        let layout = compute_layout_with_plan(diagram, prepared.layout_plan()).expect("layout");
        let summary = LintMetricsSummary::from_report(&lint_layout(diagram, &layout));
        assert_eq!(
            summary.group_overlap, 0,
            "unexpected group overlaps in stress-nested layout"
        );
    }

    #[test]
    fn stress_nested_nodes_stay_inside_leaf_groups() {
        let source =
            include_str!("../../../../../../showcase/architecture/stress.layout-stress-nested.pgm");
        let output = parse_prepare_validate(source, &StyleRequest::default());
        let prepared = output.diagram.expect("valid diagram");
        let diagram = prepared.inner();
        let layout = compute_layout_with_plan(diagram, prepared.layout_plan()).expect("layout");

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
        // G3：PRS 扩壳后父组仅保证容纳子组，不再强制保留 Equal 时代的底部留白。
        assert!(
            cloud_bottom - ds_bottom >= -1.0,
            "cloud must contain data_subnet bottom, gap={:.1}",
            cloud_bottom - ds_bottom
        );
    }

    #[test]
    fn stress_nested_child_groups_stay_inside_parents() {
        use crate::layout::quality::lint::{lint_layout, LintMetricsSummary, LintRuleId};

        let source =
            include_str!("../../../../../../showcase/architecture/stress.layout-stress-nested.pgm");
        let output = parse_prepare_validate(source, &StyleRequest::default());
        let prepared = output.diagram.expect("valid diagram");
        let diagram = prepared.inner();
        let layout = compute_layout_with_plan(diagram, prepared.layout_plan()).expect("layout");
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

        let source =
            include_str!("../../../../../../showcase/architecture/stress.layout-stress-nested.pgm");
        let output = parse_prepare_validate(source, &StyleRequest::default());
        let prepared = output.diagram.expect("valid diagram");
        let diagram = prepared.inner();
        let layout = compute_layout_with_plan(diagram, prepared.layout_plan()).expect("layout");

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
