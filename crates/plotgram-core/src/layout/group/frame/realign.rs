//! V2/refine 后恢复同 rank 分组行对齐。

use crate::layout::GroupLayout;
use std::collections::HashMap;

/// V2/refine 后 `recompute_group_bounds` 会从 min_node_y 重算 group y，
/// 若同 rank 的 group 内节点被推开的幅度不同，group y 会不一致，
/// 破坏 `apply_group_frame` 的行检测（0.5px 容差）。
///
/// 本函数按 recompute 前的 y 将同 rank group 聚类，recompute 后统一对齐到
/// rank 内最小 recomputed y，保留各 group 的 bottom 不缩。
///
/// 确定性：按 group id 字典序处理，不依赖 HashMap 迭代序。
pub fn realign_group_rows(
    groups: &mut HashMap<String, GroupLayout>,
    pre_recompute_y: &HashMap<String, f64>,
) {
    if groups.is_empty() || pre_recompute_y.is_empty() {
        return;
    }

    let mut sorted: Vec<(String, f64)> = pre_recompute_y
        .iter()
        .map(|(id, &y)| (id.clone(), y))
        .collect();
    sorted.sort_by(|a, b| {
        a.1.partial_cmp(&b.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });

    let mut clusters: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut current_y = f64::INFINITY;
    for (id, y) in &sorted {
        if current.is_empty() || (y - current_y).abs() < 1.0 {
            current.push(id.clone());
            current_y = if current.len() == 1 { *y } else { current_y };
        } else {
            if current.len() > 1 {
                clusters.push(std::mem::take(&mut current));
            }
            current.clear();
            current.push(id.clone());
            current_y = *y;
        }
    }
    if current.len() > 1 {
        clusters.push(current);
    }

    for cluster in &clusters {
        let min_new_y = cluster
            .iter()
            .filter_map(|id| groups.get(id).map(|g| g.y))
            .fold(f64::INFINITY, f64::min);
        if !min_new_y.is_finite() {
            continue;
        }
        let mut ids: Vec<&String> = cluster.iter().collect();
        ids.sort();
        for id in ids {
            if let Some(g) = groups.get_mut(id) {
                let bottom = g.y + g.height;
                g.y = min_new_y;
                g.height = (bottom - min_new_y).max(0.0);
            }
        }
    }
}
