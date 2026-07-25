//! Group bounds 重算 / 扩壳 / 收缩。
//!
//! 本文件从 `mod.rs` 拆分而来，仅做代码搬家，无行为变更。

use crate::ast::Diagram;
use crate::layout::engines::common::group_bounds::{
    compute_group_bounds, compute_group_bounds_with_side_gutters,
    container_padding_for_leaf, GroupPadding,
};
use crate::layout::{GroupLayout, LayoutResult, NodeLayout};
use std::collections::HashMap;

/// 重叠消解 / PRS 后的安全网：向外扩展 group 矩形以包住内容（不动节点）。
///
/// - leaf group：包住直接成员节点，保留 `leaf_padding`
/// - 容器组：包住直接子组，保留 `container_padding`
///
/// 与 [`recompute_group_bounds`] 不同：不整体重算原点，只向外扩，尽量保留
/// L1 左缘对齐等整形结果。
pub fn expand_groups_to_contain_contents(
    diagram: &Diagram,
    groups: &mut HashMap<String, GroupLayout>,
    nodes: &HashMap<String, NodeLayout>,
    leaf_padding: GroupPadding,
    container_padding: GroupPadding,
) {
    crate::layout::group::write_counter::record_group_write_at("expand_groups_to_contain_contents");
    let mut group_ids: Vec<String> = diagram
        .groups
        .iter()
        .map(|g| g.id.as_str().to_string())
        .collect();
    // 深组优先：先扩 leaf，再扩祖先容器。
    group_ids.sort_by(|a, b| {
        let da = group_depth(diagram, a);
        let db = group_depth(diagram, b);
        db.cmp(&da).then_with(|| a.cmp(b))
    });

    for gid in group_ids {
        let Some(gdef) = diagram.find_group(&gid) else {
            continue;
        };
        let Some(gl) = groups.get(&gid).cloned() else {
            continue;
        };

        let mut min_x = f64::MAX;
        let mut min_y = f64::MAX;
        let mut max_x = f64::MIN;
        let mut max_y = f64::MIN;
        let mut has_content = false;

        for eid in &gdef.entity_ids {
            let Some(nl) = nodes.get(eid.as_str()) else {
                continue;
            };
            has_content = true;
            min_x = min_x.min(nl.x);
            min_y = min_y.min(nl.y);
            max_x = max_x.max(nl.x + nl.width);
            max_y = max_y.max(nl.y + nl.height);
        }
        for child_id in &gdef.child_group_ids {
            let Some(child) = groups.get(child_id.as_str()) else {
                continue;
            };
            has_content = true;
            min_x = min_x.min(child.x);
            min_y = min_y.min(child.y);
            max_x = max_x.max(child.x + child.width);
            max_y = max_y.max(child.y + child.height);
        }
        if !has_content {
            continue;
        }

        let pad = if gdef.entity_ids.is_empty() {
            container_padding
        } else {
            leaf_padding
        };
        let need_left = min_x - pad.left;
        let need_top = min_y - pad.top;
        let need_right = max_x + pad.right;
        let need_bottom = max_y + pad.bottom;

        if let Some(gl) = groups.get_mut(&gid) {
            let left = gl.x.min(need_left);
            let top = gl.y.min(need_top);
            let right = (gl.x + gl.width).max(need_right);
            let bottom = (gl.y + gl.height).max(need_bottom);
            gl.x = left;
            gl.y = top;
            gl.width = right - left;
            gl.height = bottom - top;
        }
    }
}

/// Phase F：按「内容 + max(base_pad, egb)」收缩组框多余空壳（只缩不扩）。
///
/// - 不得小于 `leaf/container padding` 与 EGB 合成后的各侧预算。
/// - 含顶层叶子：nudge 后常留下对侧空壳，需按真实内容收回。
/// - 仅应由 Fit 策略路径调用（Equal/uniform 条带由调用方跳过）。
pub fn shrink_groups_to_required_padding(
    diagram: &Diagram,
    groups: &mut HashMap<String, GroupLayout>,
    nodes: &HashMap<String, NodeLayout>,
    leaf_padding: GroupPadding,
    container_padding: GroupPadding,
    side_gutters: Option<&std::collections::BTreeMap<String, crate::layout::engines::common::group_bounds::SideGutter>>,
) {
    crate::layout::group::write_counter::record_group_write_at("shrink_groups_to_required_padding");
    let mut group_ids: Vec<String> = diagram
        .groups
        .iter()
        .map(|g| g.id.as_str().to_string())
        .collect();
    // 深组优先：先缩 leaf，再缩祖先。
    group_ids.sort_by(|a, b| {
        let da = group_depth(diagram, a);
        let db = group_depth(diagram, b);
        db.cmp(&da).then_with(|| a.cmp(b))
    });

    for gid in group_ids {
        let Some(gdef) = diagram.find_group(&gid) else {
            continue;
        };

        let mut min_x = f64::MAX;
        let mut min_y = f64::MAX;
        let mut max_x = f64::MIN;
        let mut max_y = f64::MIN;
        let mut has_content = false;

        for eid in &gdef.entity_ids {
            let Some(nl) = nodes.get(eid.as_str()) else {
                continue;
            };
            has_content = true;
            min_x = min_x.min(nl.x);
            min_y = min_y.min(nl.y);
            max_x = max_x.max(nl.x + nl.width);
            max_y = max_y.max(nl.y + nl.height);
        }
        for child_id in &gdef.child_group_ids {
            let Some(child) = groups.get(child_id.as_str()) else {
                continue;
            };
            has_content = true;
            min_x = min_x.min(child.x);
            min_y = min_y.min(child.y);
            max_x = max_x.max(child.x + child.width);
            max_y = max_y.max(child.y + child.height);
        }
        if !has_content {
            continue;
        }

        let mut pad = if gdef.entity_ids.is_empty() {
            container_padding
        } else {
            leaf_padding
        };
        if let Some(budget) = side_gutters.and_then(|m| m.get(gid.as_str())) {
            pad = pad.max_per_side(*budget);
        }

        let tight_left = min_x - pad.left;
        let tight_top = min_y - pad.top;
        let tight_right = max_x + pad.right;
        let tight_bottom = max_y + pad.bottom;

        let is_top = gdef.parent_id.is_none();
        if let Some(gl) = groups.get_mut(&gid) {
            if is_top {
                // 顶层：锁定左缘（SharedLines）；上/右/下按内容+max(base,egb)收回
                let top = gl.y.max(tight_top);
                let right = (gl.x + gl.width).min(tight_right);
                let bottom = (gl.y + gl.height).min(tight_bottom);
                if right > gl.x + f64::EPSILON && bottom > top + f64::EPSILON {
                    gl.y = top;
                    gl.width = right - gl.x;
                    gl.height = bottom - top;
                }
            } else {
                let left = gl.x.max(tight_left);
                let top = gl.y.max(tight_top);
                let right = (gl.x + gl.width).min(tight_right);
                let bottom = (gl.y + gl.height).min(tight_bottom);
                if right > left + f64::EPSILON && bottom > top + f64::EPSILON {
                    gl.x = left;
                    gl.y = top;
                    gl.width = right - left;
                    gl.height = bottom - top;
                }
            }
        }
    }
}

/// 兼容旧调用：仅扩容器组包住子组（无额外 padding）。
pub fn expand_container_groups_to_fit_children(
    diagram: &Diagram,
    groups: &mut HashMap<String, GroupLayout>,
) {
    expand_groups_to_contain_contents(
        diagram,
        groups,
        &HashMap::new(),
        GroupPadding::default(),
        GroupPadding::default(),
    );
}

fn group_depth(diagram: &Diagram, group_id: &str) -> usize {
    let mut depth = 0usize;
    let mut cur = group_id.to_string();
    loop {
        let Some(g) = diagram.find_group(&cur) else {
            break;
        };
        match &g.parent_id {
            Some(p) => {
                depth += 1;
                cur = p.as_str().to_string();
            }
            None => break,
        }
    }
    depth
}

/// 从节点位置重算 group bounds（L3→L1 数据流桥梁）。
///
/// 供管线在 L3 node snap 之后、L1 group frame 之前调用。
pub fn recompute_group_bounds(
    diagram: &Diagram,
    layout: &mut LayoutResult,
    padding: GroupPadding,
) {
    crate::layout::group::write_counter::record_group_write_at("recompute_group_bounds");
    let side_gutters = layout
        .hints
        .group_routing
        .as_ref()
        .filter(|h| !h.side_gutters.is_empty())
        .map(|h| &h.side_gutters);
    layout.groups = if let Some(gutters) = side_gutters {
        compute_group_bounds_with_side_gutters(
            diagram,
            &layout.nodes,
            padding,
            container_padding_for_leaf(padding),
            Some(gutters),
        )
    } else {
        compute_group_bounds(diagram, &layout.nodes, padding)
    }
    .into();
}
