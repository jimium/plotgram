//! 几何意图微调（P1.5）。
//!
//! 在 `strategy.compute_with_overlay` 产出 `LayoutResult` 之后、`grid_snap` 之前执行。
//! 消费 [`crate::layout::intent::GeometricIntent`]，对节点坐标做局部修正：
//!
//! - `Pin`：锁定节点当前坐标，标记为 pinned 跳过后续 grid snap（仅轴约束，不含绝对坐标）。
//! - `AlignVertical`：多节点 x 中心对齐到均值，单轮重叠消除，对齐节点加入 pinned。
//! - `AlignHorizontal`：多节点 y 中心对齐到均值，单轮重叠消除，对齐节点加入 pinned。
//!
//! 跨组对齐仅对齐同组节点，标记 `Partial`。
//! 重叠消除仅做一轮，失败则标记 `Partial`，不级联。
//!
//! 详见 `docs/architecture/layout-intent-optimized.md` §5.3。

use crate::ast::Diagram;
use crate::layout::intent::{
    GeometricIntent, IntentStatus, LayoutIntentOverlay, PinAxis, PinSet, RefinementReport,
};
use crate::layout::node::common::group_map::build_node_to_top_group;
use crate::layout::{LayoutResult, NodeLayout};
use std::collections::HashMap;

/// 穿障修正后对齐完整性检查的容差（节点中心偏移超过此值视为对齐被破坏）。
const ALIGN_BREAK_TOLERANCE: f64 = 1.0;

/// 对齐后单轮重叠消除的层内间距（与 grid_snap 默认 node_gap 一致量级）。
const ALIGN_OVERLAP_GAP: f64 = 24.0;

/// 执行几何意图微调，返回每条意图的执行结果。
///
/// `pinned` 会被填充：被 `Pin` / `Align*` 保护的节点加入对应集合，
/// 供后续 `grid_snap::snap_layout_to_grid` 跳过。
pub fn apply_geometric_refinement(
    result: &mut LayoutResult,
    overlay: &LayoutIntentOverlay,
    pinned: &mut PinSet,
    diagram: &Diagram,
) -> RefinementReport {
    let mut report = RefinementReport::default();

    for (i, intent) in overlay.geometric.iter().enumerate() {
        let (kind, status, message) = match intent {
            GeometricIntent::Pin { node, axis } => {
                let status = apply_pin(result, node, *axis, pinned);
                let msg = if status == IntentStatus::NotFound {
                    Some(format!("node '{node}' not found"))
                } else {
                    None
                };
                ("pin", status, msg)
            }
            GeometricIntent::AlignVertical { nodes } => {
                let (status, msg) = apply_align_vertical(result, nodes, pinned, diagram);
                ("align_vertical", status, msg)
            }
            GeometricIntent::AlignHorizontal { nodes } => {
                let (status, msg) = apply_align_horizontal(result, nodes, pinned, diagram);
                ("align_horizontal", status, msg)
            }
        };
        report.push(i, kind, status, message);
    }

    report
}

/// `Pin`：锁定节点当前坐标，标记为 pinned 跳过后续 snap。
fn apply_pin(
    result: &LayoutResult,
    node_id: &str,
    axis: PinAxis,
    pinned: &mut PinSet,
) -> IntentStatus {
    if !result.nodes.contains_key(node_id) {
        return IntentStatus::NotFound;
    }
    match axis {
        PinAxis::Both => {
            pinned.full.insert(node_id.to_string());
        }
        PinAxis::X => {
            pinned.x_only.insert(node_id.to_string());
        }
        PinAxis::Y => {
            pinned.y_only.insert(node_id.to_string());
        }
    }
    IntentStatus::Satisfied
}

/// `AlignVertical`：将 nodes 的 x 中心对齐到均值。
///
/// 跨组节点仅对齐同组节点，标记 `Partial`。对齐后做单轮重叠消除（y 轴方向）。
/// 对齐节点加入 `pinned.aligned`，跳过后续 grid snap。
fn apply_align_vertical(
    result: &mut LayoutResult,
    nodes: &[String],
    pinned: &mut PinSet,
    diagram: &Diagram,
) -> (IntentStatus, Option<String>) {
    apply_align_axis(result, nodes, pinned, diagram, /* vertical = */ true)
}

/// `AlignHorizontal`：将 nodes 的 y 中心对齐到均值。
fn apply_align_horizontal(
    result: &mut LayoutResult,
    nodes: &[String],
    pinned: &mut PinSet,
    diagram: &Diagram,
) -> (IntentStatus, Option<String>) {
    apply_align_axis(result, nodes, pinned, diagram, /* vertical = */ false)
}

/// 对齐核心逻辑：`vertical=true` 对齐 x 中心，`vertical=false` 对齐 y 中心。
fn apply_align_axis(
    result: &mut LayoutResult,
    nodes: &[String],
    pinned: &mut PinSet,
    diagram: &Diagram,
    vertical: bool,
) -> (IntentStatus, Option<String>) {
    // 过滤存在的节点
    let existing: Vec<&String> = nodes.iter().filter(|id| result.nodes.contains_key(*id)).collect();
    if existing.is_empty() {
        return (IntentStatus::NotFound, Some("no nodes found for alignment".into()));
    }

    // 跨组检测：若节点分属不同顶层组，仅对齐同组节点，标记 Partial
    let group_map = build_node_to_top_group(diagram);
    let mut groups_seen: std::collections::HashSet<Option<&String>> = std::collections::HashSet::new();
    for id in &existing {
        groups_seen.insert(group_map.get(*id));
    }
    let cross_group = groups_seen.len() > 1;

    // 选定对齐目标节点集：跨组时仅保留第一个组（按出现顺序）的节点
    let target_nodes: Vec<&String> = if cross_group {
        let first_group = group_map.get(existing[0]).cloned();
        existing
            .iter()
            .copied()
            .filter(|id| group_map.get(*id) == first_group.as_ref())
            .collect()
    } else {
        existing.clone()
    };

    if target_nodes.len() < 2 {
        let msg = if cross_group {
            Some("cross-group alignment: fewer than 2 nodes in first group".into())
        } else {
            Some("fewer than 2 nodes to align".into())
        };
        return (IntentStatus::Partial, msg);
    }

    // 计算均值
    let mean = compute_center_mean(&result.nodes, &target_nodes, vertical);

    // 对齐：设置每个节点的中心到均值
    for id in &target_nodes {
        if let Some(node) = result.nodes.get_mut(*id) {
            set_center(node, mean, vertical);
            if vertical {
                pinned.aligned_vertical.insert((*id).clone());
            } else {
                pinned.aligned_horizontal.insert((*id).clone());
            }
        }
    }

    // 单轮重叠消除（沿非对齐轴方向）
    let overlap_resolved = resolve_alignment_overlap(&mut result.nodes, &target_nodes, vertical);

    let status = if cross_group {
        IntentStatus::Partial
    } else if overlap_resolved {
        IntentStatus::Satisfied
    } else {
        IntentStatus::Partial
    };

    let msg = if cross_group {
        Some(format!(
            "cross-group alignment: aligned {} nodes in first group, skipped {}",
            target_nodes.len(),
            existing.len() - target_nodes.len()
        ))
    } else if !overlap_resolved {
        Some("overlap resolution incomplete after single pass".into())
    } else {
        None
    };

    (status, msg)
}

/// 计算节点集在对齐轴上的中心均值。
fn compute_center_mean(
    nodes: &HashMap<String, NodeLayout>,
    ids: &[&String],
    vertical: bool,
) -> f64 {
    let sum: f64 = ids
        .iter()
        .filter_map(|id| nodes.get(*id))
        .map(|n| center(n, vertical))
        .sum();
    sum / ids.len() as f64
}

/// 获取节点在对齐轴上的中心。
fn center(node: &NodeLayout, vertical: bool) -> f64 {
    if vertical {
        node.x + node.width / 2.0
    } else {
        node.y + node.height / 2.0
    }
}

/// 设置节点在对齐轴上的中心。
fn set_center(node: &mut NodeLayout, value: f64, vertical: bool) {
    if vertical {
        node.x = value - node.width / 2.0;
    } else {
        node.y = value - node.height / 2.0;
    }
}

/// 单轮重叠消除：沿非对齐轴方向推开重叠节点。
///
/// `vertical=true`（对齐 x）时沿 y 轴推开；`vertical=false`（对齐 y）时沿 x 轴推开。
/// 返回 `true` 表示所有节点已无重叠，`false` 表示仍有重叠（单轮未完全消除）。
fn resolve_alignment_overlap(
    nodes: &mut HashMap<String, NodeLayout>,
    ids: &[&String],
    vertical: bool,
) -> bool {
    if ids.len() < 2 {
        return true;
    }

    // 沿非对齐轴排序
    let mut ordered: Vec<String> = ids.iter().map(|s| (*s).clone()).collect();
    ordered.sort_by(|a, b| {
        let ca = nodes.get(a).map(|n| cross_center(n, vertical)).unwrap_or(0.0);
        let cb = nodes.get(b).map(|n| cross_center(n, vertical)).unwrap_or(0.0);
        ca.partial_cmp(&cb).unwrap_or(std::cmp::Ordering::Equal)
    });

    // 前向扫描：确保相邻节点在非对齐轴上有足够间距
    let mut all_ok = true;
    for i in 1..ordered.len() {
        let (prev_id, curr_id) = (&ordered[i - 1], &ordered[i]);
        // 仅读取所需标量，避免克隆整个 NodeLayout
        let Some((prev_cross, prev_size)) = nodes.get(prev_id).map(|n| (cross_center(n, vertical), cross_size(n, vertical)))
        else { continue };
        let Some((curr_cross, curr_size)) = nodes.get(curr_id).map(|n| (cross_center(n, vertical), cross_size(n, vertical)))
        else { continue };

        let min_cross = prev_cross + prev_size / 2.0 + ALIGN_OVERLAP_GAP + curr_size / 2.0;
        if curr_cross < min_cross {
            // 推开当前节点
            if let Some(node) = nodes.get_mut(curr_id) {
                set_cross_center(node, min_cross, vertical);
            }
        }
    }

    // 检查是否仍有重叠
    for i in 1..ordered.len() {
        let (prev_id, curr_id) = (&ordered[i - 1], &ordered[i]);
        let Some((prev_cross, prev_size)) = nodes.get(prev_id).map(|n| (cross_center(n, vertical), cross_size(n, vertical)))
        else { continue };
        let Some((curr_cross, curr_size)) = nodes.get(curr_id).map(|n| (cross_center(n, vertical), cross_size(n, vertical)))
        else { continue };

        let min_cross = prev_cross + prev_size / 2.0 + ALIGN_OVERLAP_GAP + curr_size / 2.0;
        if curr_cross < min_cross {
            all_ok = false;
            break;
        }
    }

    all_ok
}

/// 非对齐轴的中心。
fn cross_center(node: &NodeLayout, vertical: bool) -> f64 {
    if vertical {
        node.y + node.height / 2.0
    } else {
        node.x + node.width / 2.0
    }
}

/// 非对齐轴的尺寸。
fn cross_size(node: &NodeLayout, vertical: bool) -> f64 {
    if vertical {
        node.height
    } else {
        node.width
    }
}

/// 设置非对齐轴的中心。
fn set_cross_center(node: &mut NodeLayout, value: f64, vertical: bool) {
    if vertical {
        node.y = value - node.height / 2.0;
    } else {
        node.x = value - node.width / 2.0;
    }
}

/// 穿障修正后对齐完整性检查（设计 §5.3.1 "首期仅观测，不回滚"）。
///
/// 在 `refine::run_refine` 之后调用，逐条检查 `overlay.geometric` 中的
/// `AlignVertical` / `AlignHorizontal` 意图：若该意图所声明的节点集合在
/// 对齐轴上的中心极差超过 `ALIGN_BREAK_TOLERANCE`，则**仅降级该条意图**
/// 从 `Satisfied` 为 `Partial`，不影响其他对齐意图。
///
/// `pinned` 记录了哪些节点被对齐以及对齐轴，用于快速跳过无对齐的情况；
/// `report` 会被原地降级。
///
/// 注意：此函数不回滚 refine 的修改，仅观测并更新报告。
pub fn check_alignment_after_refine(
    result: &LayoutResult,
    pinned: &PinSet,
    overlay: &LayoutIntentOverlay,
    report: &mut RefinementReport,
) {
    // 快速跳过：没有任何对齐节点时无需遍历意图
    if pinned.aligned_vertical.is_empty() && pinned.aligned_horizontal.is_empty() {
        return;
    }

    for (i, intent) in overlay.geometric.iter().enumerate() {
        match intent {
            GeometricIntent::AlignVertical { nodes } => {
                if !alignment_intact(result, nodes, /* vertical = */ true) {
                    report.downgrade_to_partial(
                        i,
                        "alignment broken by refine::run_refine (observed, not rolled back)",
                    );
                }
            }
            GeometricIntent::AlignHorizontal { nodes } => {
                if !alignment_intact(result, nodes, /* vertical = */ false) {
                    report.downgrade_to_partial(
                        i,
                        "alignment broken by refine::run_refine (observed, not rolled back)",
                    );
                }
            }
            GeometricIntent::Pin { .. } => {
                // Pin 不涉及对齐完整性检查
            }
        }
    }
}

/// 检查指定节点集合在对齐轴上的中心是否仍保持一致（极差 ≤ 容差）。
///
/// `vertical=true` 检查 x 中心（`AlignVertical`），`vertical=false` 检查 y 中心（`AlignHorizontal`）。
/// 节点数 < 2 或节点不存在时视为"对齐未破坏"（无需降级）。
fn alignment_intact(result: &LayoutResult, nodes: &[String], vertical: bool) -> bool {
    let centers: Vec<f64> = nodes
        .iter()
        .filter_map(|id| result.nodes.get(id))
        .map(|n| center(n, vertical))
        .collect();
    if centers.len() < 2 {
        return true;
    }
    let min = centers.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = centers.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    max - min <= ALIGN_BREAK_TOLERANCE
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Diagram, SourceInfo};
    use crate::layout::{LayoutHints, NodeLayout};
    use crate::types::DiagramType;

    fn make_layout(nodes: &[(&str, f64, f64, f64, f64)]) -> LayoutResult {
        let nodes: HashMap<String, NodeLayout> = nodes
            .iter()
            .map(|(id, x, y, w, h)| {
                (
                    id.to_string(),
                    NodeLayout {
                        x: *x,
                        y: *y,
                        width: *w,
                        height: *h,
                        ..Default::default()
                    },
                )
            })
            .collect();
        LayoutResult {
            nodes,
            groups: HashMap::new(),
            edges: vec![],
            total_width: 400.0,
            total_height: 300.0,
            hints: LayoutHints::default(),
        }
    }

    fn empty_diagram() -> Diagram {
        Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: vec![],
            entities: vec![],
            relations: vec![],
            groups: vec![],
            style_decls: vec![],
            source_info: SourceInfo {
                file: None,
                line_count: 1,
            },
            ..Default::default()
        }
    }

    #[test]
    fn pin_marks_node_in_pinned_set() {
        let mut result = make_layout(&[("a", 10.0, 20.0, 80.0, 40.0)]);
        let overlay = LayoutIntentOverlay {
            topology: vec![],
            geometric: vec![GeometricIntent::Pin {
                node: "a".into(),
                axis: PinAxis::Both,
            }],
        };
        let mut pinned = PinSet::default();
        let report = apply_geometric_refinement(&mut result, &overlay, &mut pinned, &empty_diagram());

        assert_eq!(report.satisfied, 1);
        assert!(pinned.full.contains("a"));
        // 坐标不变
        assert_eq!(result.nodes["a"].x, 10.0);
        assert_eq!(result.nodes["a"].y, 20.0);
    }

    #[test]
    fn pin_x_only_marks_x_axis() {
        let mut result = make_layout(&[("a", 10.0, 20.0, 80.0, 40.0)]);
        let overlay = LayoutIntentOverlay {
            topology: vec![],
            geometric: vec![GeometricIntent::Pin {
                node: "a".into(),
                axis: PinAxis::X,
            }],
        };
        let mut pinned = PinSet::default();
        apply_geometric_refinement(&mut result, &overlay, &mut pinned, &empty_diagram());

        assert!(pinned.x_only.contains("a"));
        assert!(!pinned.full.contains("a"));
        assert!(pinned.is_x_pinned("a"));
        assert!(!pinned.is_y_pinned("a"));
    }

    #[test]
    fn pin_not_found_node() {
        let mut result = make_layout(&[("a", 10.0, 20.0, 80.0, 40.0)]);
        let overlay = LayoutIntentOverlay {
            topology: vec![],
            geometric: vec![GeometricIntent::Pin {
                node: "ghost".into(),
                axis: PinAxis::Both,
            }],
        };
        let mut pinned = PinSet::default();
        let report = apply_geometric_refinement(&mut result, &overlay, &mut pinned, &empty_diagram());

        assert_eq!(report.not_found, 1);
        assert!(pinned.is_empty());
    }

    #[test]
    fn align_vertical_aligns_x_centers() {
        // a at x=10 (center 50), b at x=120 (center 160), c at x=200 (center 240)
        // mean = (50+160+240)/3 = 150
        let mut result = make_layout(&[
            ("a", 10.0, 0.0, 80.0, 40.0),
            ("b", 120.0, 100.0, 80.0, 40.0),
            ("c", 200.0, 200.0, 80.0, 40.0),
        ]);
        let overlay = LayoutIntentOverlay {
            topology: vec![],
            geometric: vec![GeometricIntent::AlignVertical {
                nodes: vec!["a".into(), "b".into(), "c".into()],
            }],
        };
        let mut pinned = PinSet::default();
        let report = apply_geometric_refinement(&mut result, &overlay, &mut pinned, &empty_diagram());

        assert_eq!(report.satisfied, 1);
        let mean_x = 150.0;
        for id in &["a", "b", "c"] {
            let node = &result.nodes[*id];
            assert!(
                (node.x + node.width / 2.0 - mean_x).abs() < 0.01,
                "node {id} x center = {}, expected {mean_x}",
                node.x + node.width / 2.0
            );
            assert!(pinned.aligned_vertical.contains(*id));
        }
    }

    #[test]
    fn align_horizontal_aligns_y_centers() {
        // a at y=0 (center 20), b at y=100 (center 120), c at y=200 (center 220)
        // mean = (20+120+220)/3 = 120
        let mut result = make_layout(&[
            ("a", 0.0, 0.0, 80.0, 40.0),
            ("b", 100.0, 100.0, 80.0, 40.0),
            ("c", 200.0, 200.0, 80.0, 40.0),
        ]);
        let overlay = LayoutIntentOverlay {
            topology: vec![],
            geometric: vec![GeometricIntent::AlignHorizontal {
                nodes: vec!["a".into(), "b".into(), "c".into()],
            }],
        };
        let mut pinned = PinSet::default();
        let report = apply_geometric_refinement(&mut result, &overlay, &mut pinned, &empty_diagram());

        assert_eq!(report.satisfied, 1);
        let mean_y = 120.0;
        for id in &["a", "b", "c"] {
            let node = &result.nodes[*id];
            assert!(
                (node.y + node.height / 2.0 - mean_y).abs() < 0.01,
                "node {id} y center = {}, expected {mean_y}",
                node.y + node.height / 2.0
            );
        }
    }

    #[test]
    fn align_vertical_resolves_overlap_on_y() {
        // a, b, c all at same y → after align_vertical, y should be pushed apart
        let mut result = make_layout(&[
            ("a", 10.0, 0.0, 80.0, 40.0),
            ("b", 120.0, 0.0, 80.0, 40.0),
            ("c", 200.0, 0.0, 80.0, 40.0),
        ]);
        let overlay = LayoutIntentOverlay {
            topology: vec![],
            geometric: vec![GeometricIntent::AlignVertical {
                nodes: vec!["a".into(), "b".into(), "c".into()],
            }],
        };
        let mut pinned = PinSet::default();
        let report = apply_geometric_refinement(&mut result, &overlay, &mut pinned, &empty_diagram());

        // Overlap should be resolved (single pass sufficient here)
        assert_eq!(report.satisfied, 1);
        // b should be pushed below a
        let a_y = result.nodes["a"].y;
        let b_y = result.nodes["b"].y;
        let c_y = result.nodes["c"].y;
        assert!(b_y > a_y, "b ({b_y}) should be below a ({a_y})");
        assert!(c_y > b_y, "c ({c_y}) should be below b ({b_y})");
    }

    #[test]
    fn align_with_single_node_returns_partial() {
        let mut result = make_layout(&[("a", 10.0, 0.0, 80.0, 40.0)]);
        let overlay = LayoutIntentOverlay {
            topology: vec![],
            geometric: vec![GeometricIntent::AlignVertical {
                nodes: vec!["a".into()],
            }],
        };
        let mut pinned = PinSet::default();
        let report = apply_geometric_refinement(&mut result, &overlay, &mut pinned, &empty_diagram());

        assert_eq!(report.partial, 1);
    }

    #[test]
    fn align_with_nonexistent_nodes_returns_not_found() {
        let mut result = make_layout(&[("a", 10.0, 0.0, 80.0, 40.0)]);
        let overlay = LayoutIntentOverlay {
            topology: vec![],
            geometric: vec![GeometricIntent::AlignVertical {
                nodes: vec!["ghost1".into(), "ghost2".into()],
            }],
        };
        let mut pinned = PinSet::default();
        let report = apply_geometric_refinement(&mut result, &overlay, &mut pinned, &empty_diagram());

        assert_eq!(report.not_found, 1);
    }

    #[test]
    fn pinset_queries() {
        let mut ps = PinSet::default();
        ps.full.insert("a".into());
        ps.x_only.insert("b".into());
        ps.aligned_vertical.insert("c".into());
        ps.aligned_horizontal.insert("d".into());

        assert!(ps.is_x_pinned("a"));
        assert!(ps.is_y_pinned("a"));
        assert!(ps.is_x_pinned("b"));
        assert!(!ps.is_y_pinned("b"));
        // aligned_vertical: x 中心对齐 → 两轴都跳过 snap
        assert!(ps.is_x_pinned("c"));
        assert!(ps.is_y_pinned("c"));
        // aligned_horizontal: y 中心对齐 → 两轴都跳过 snap
        assert!(ps.is_x_pinned("d"));
        assert!(ps.is_y_pinned("d"));
        assert!(!ps.is_x_pinned("e"));
        assert!(!ps.is_empty());
    }

    #[test]
    fn pinset_empty_default() {
        let ps = PinSet::default();
        assert!(ps.is_empty());
        assert!(!ps.is_x_pinned("any"));
    }

    // ── 穿障修正后对齐完整性检查 ─────────────────────────────

    #[test]
    fn check_alignment_after_refine_no_breakage_keeps_satisfied() {
        // 对齐后节点未移动 → 保持 Satisfied
        let mut result = make_layout(&[
            ("a", 110.0, 0.0, 80.0, 40.0),
            ("b", 110.0, 100.0, 80.0, 40.0),
        ]);
        let overlay = LayoutIntentOverlay {
            topology: vec![],
            geometric: vec![GeometricIntent::AlignVertical {
                nodes: vec!["a".into(), "b".into()],
            }],
        };
        let mut pinned = PinSet::default();
        let mut report = apply_geometric_refinement(&mut result, &overlay, &mut pinned, &empty_diagram());
        assert_eq!(report.satisfied, 1);

        // 模拟 refine 未破坏对齐（节点未移动）
        check_alignment_after_refine(&result, &pinned, &overlay, &mut report);
        assert_eq!(report.satisfied, 1, "alignment intact → should stay Satisfied");
        assert_eq!(report.partial, 0);
    }

    #[test]
    fn check_alignment_after_refine_breakage_downgrades_to_partial() {
        // 对齐后模拟 refine 推开节点 → 降级为 Partial
        let mut result = make_layout(&[
            ("a", 110.0, 0.0, 80.0, 40.0),
            ("b", 110.0, 100.0, 80.0, 40.0),
        ]);
        let overlay = LayoutIntentOverlay {
            topology: vec![],
            geometric: vec![GeometricIntent::AlignVertical {
                nodes: vec!["a".into(), "b".into()],
            }],
        };
        let mut pinned = PinSet::default();
        let mut report = apply_geometric_refinement(&mut result, &overlay, &mut pinned, &empty_diagram());
        assert_eq!(report.satisfied, 1);

        // 模拟 refine 推开节点 b 的 x 坐标
        result.nodes.get_mut("b").unwrap().x = 200.0;

        check_alignment_after_refine(&result, &pinned, &overlay, &mut report);
        assert_eq!(report.satisfied, 0, "alignment broken → should downgrade to Partial");
        assert_eq!(report.partial, 1);
        assert_eq!(report.results[0].status, IntentStatus::Partial);
        assert!(report.results[0]
            .message
            .as_deref()
            .unwrap()
            .contains("broken"));
    }

    #[test]
    fn check_alignment_after_refine_no_aligned_nodes_no_op() {
        let result = make_layout(&[("a", 10.0, 0.0, 80.0, 40.0)]);
        let pinned = PinSet::default();
        let overlay = LayoutIntentOverlay::default();
        let mut report = RefinementReport::default();
        check_alignment_after_refine(&result, &pinned, &overlay, &mut report);
        assert!(report.is_empty());
    }

    #[test]
    fn check_alignment_after_refine_horizontal_breakage() {
        let mut result = make_layout(&[
            ("a", 0.0, 100.0, 80.0, 40.0),
            ("b", 100.0, 100.0, 80.0, 40.0),
        ]);
        let overlay = LayoutIntentOverlay {
            topology: vec![],
            geometric: vec![GeometricIntent::AlignHorizontal {
                nodes: vec!["a".into(), "b".into()],
            }],
        };
        let mut pinned = PinSet::default();
        let mut report = apply_geometric_refinement(&mut result, &overlay, &mut pinned, &empty_diagram());
        assert_eq!(report.satisfied, 1);

        // 模拟 refine 推开节点 b 的 y 坐标
        result.nodes.get_mut("b").unwrap().y = 200.0;

        check_alignment_after_refine(&result, &pinned, &overlay, &mut report);
        assert_eq!(report.partial, 1);
        assert_eq!(report.satisfied, 0);
    }

    #[test]
    fn downgrade_to_partial_does_not_override_worse_status() {
        // 若意图已是 Conflicted/NotFound，不应被降级为 Partial
        let mut result = make_layout(&[("a", 10.0, 0.0, 80.0, 40.0)]);
        let overlay = LayoutIntentOverlay {
            topology: vec![],
            geometric: vec![GeometricIntent::AlignVertical {
                nodes: vec!["ghost".into()], // NotFound
            }],
        };
        let mut pinned = PinSet::default();
        let mut report = apply_geometric_refinement(&mut result, &overlay, &mut pinned, &empty_diagram());
        assert_eq!(report.not_found, 1);

        // 即使调用 check_alignment_after_refine，NotFound 状态不应被覆盖
        check_alignment_after_refine(&result, &pinned, &overlay, &mut report);
        assert_eq!(report.not_found, 1);
        assert_eq!(report.partial, 0);
    }
}
