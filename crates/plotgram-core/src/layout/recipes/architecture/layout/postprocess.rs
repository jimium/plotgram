//! Phase 5+: 重叠消除、钳制与画布尺寸。

use crate::ast::Diagram;
use crate::layout::node::common::overlap::{
    BruteForceResolver, ChainedResolver, ForceDirectedResolver, OverlapConfig, OverlapResolver,
};
use crate::layout::{GroupLayout, NodeLayout};
use std::collections::HashMap;

use super::constants::{MIN_GROUP_GAP, NODE_GAP, PADDING};

pub(in super::super) fn remove_node_overlaps(
    nodes: &mut HashMap<String, NodeLayout>,
    _sizes: &HashMap<String, (f64, f64)>,
) {
    let config = OverlapConfig {
        // 与 NODE_GAP 对齐；8px 只够「不重叠」，放不下同层边标签
        margin: NODE_GAP,
        max_iterations: 30,
        step_factor: 0.5,
    };

    // 串联：力导向消除 + 逐对确定性消除
    let resolver = ChainedResolver::new(vec![
        Box::new(ForceDirectedResolver::with_current_centers(nodes)),
        Box::new(BruteForceResolver::new(10)),
    ]);
    resolver.resolve(nodes, _sizes, &config);
}

// ─── Phase 5.5: 钳制到非负区域 ───────────────────────────

/// 防御性保险：把任何落在画布外的节点位置平移回画布内。
///
/// 算法：找出所有节点最小的 x 坐标；如果小于 PADDING，整体平移；
/// 同时逐个把 x 钳到不低于 PADDING。
pub(in super::super) fn clamp_to_canvas(
    nodes: &mut HashMap<String, NodeLayout>,
    _sizes: &HashMap<String, (f64, f64)>,
) {
    if nodes.is_empty() {
        return;
    }

    let min_x = nodes.values().map(|n| n.x).fold(f64::INFINITY, f64::min);
    if min_x < PADDING {
        let shift = PADDING - min_x;
        for nl in nodes.values_mut() {
            nl.x += shift;
        }
    }

    // 再次逐个保险：x 至少为 PADDING
    for nl in nodes.values_mut() {
        if nl.x < PADDING {
            nl.x = PADDING;
        }
        if nl.y < PADDING {
            nl.y = PADDING;
        }
    }
}

/// 把任何越过画布顶部的分组边界向下平移，确保分组标签（包括标题区）完整显示。
///
/// 算法：找出所有分组最小的 y 坐标；如果小于 PADDING，把分组和其所有成员节点
/// 一同向下平移，使最顶部的分组顶部恰好对齐 PADDING。
pub(in super::super) fn clamp_groups_to_canvas(
    nodes: &mut HashMap<String, NodeLayout>,
    groups: &mut HashMap<String, GroupLayout>,
) {
    if groups.is_empty() {
        return;
    }

    let min_y = groups
        .values()
        .map(|g| g.y)
        .fold(f64::INFINITY, f64::min);
    if min_y >= PADDING {
        return;
    }

    let shift = PADDING - min_y;

    for g in groups.values_mut() {
        g.y += shift;
    }
    for nl in nodes.values_mut() {
        nl.y += shift;
    }
}

/// 按 y 顺序推开重叠的分组边框，并同步平移组内节点。
pub(in super::super) fn resolve_group_overlaps(
    diagram: &Diagram,
    nodes: &mut HashMap<String, NodeLayout>,
    groups: &mut HashMap<String, GroupLayout>,
) {
    if groups.len() < 2 {
        return;
    }

    let mut ordered: Vec<String> = groups.keys().cloned().collect();
    ordered.sort_by(|a, b| {
        groups[a]
            .y
            .partial_cmp(&groups[b].y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.cmp(b))
    });

    let mut cursor_bottom = f64::NEG_INFINITY;
    for gid in ordered {
        let required_top = cursor_bottom + MIN_GROUP_GAP;
        let shift = {
            let g = groups.get(&gid).unwrap();
            if g.y < required_top {
                required_top - g.y
            } else {
                0.0
            }
        };

        if shift > 0.0 {
            if let Some(g) = groups.get_mut(&gid) {
                g.y += shift;
            }
            if let Some(group) = diagram.groups.iter().find(|gr| gr.id.as_str() == gid) {
                for eid in &group.entity_ids {
                    if let Some(nl) = nodes.get_mut(eid.as_str()) {
                        nl.y += shift;
                    }
                }
            }
        }

        if let Some(g) = groups.get(&gid) {
            cursor_bottom = g.y + g.height;
        }
    }
}

// ═══════════════════════════════════════════════════════════
//  单元测试
// ═══════════════════════════════════════════════════════════
