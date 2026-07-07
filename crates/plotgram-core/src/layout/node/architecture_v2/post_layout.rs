//! 架构图布局后处理：单 group 行居中等 L1 特例。

use crate::ast::Diagram;
use crate::layout::LayoutResult;
use std::collections::HashMap;

/// 架构图单 group 行居中：当某 macro rank 只有一个 group 时，
/// `apply_cross_align_start` 会将其左对齐到全局 min(x)，留下大片右侧空白。
/// 本函数在 `apply_group_frame` 之后将单 group 行水平居中到画布宽度。
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
