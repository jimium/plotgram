//! 画布总尺寸计算共享逻辑。
//!
//! 抽取自 `grid_snap.rs` / `force_directed.rs` / `sugiyama_v2/postprocess.rs` /
//! `architecture_v2/layout/postprocess.rs` 四处字节级相同的 `bounds_from_layout`
//! (architecture_v2 版叫 `compute_total_size`,用常量 PADDING 替代参数)。
//!
//! 统一后行为完全一致:遍历节点和分组的 bbox,取 max_x/max_y,加 padding。

use crate::layout::{GroupLayout, NodeLayout};
use std::collections::HashMap;

/// 计算画布总尺寸(width, height),包含节点 + 分组,加 padding。
///
/// 节点和分组的右下角分别取 max,再取两者较大值,最后加 padding。
/// 空节点/空分组返回 (padding, padding)。
pub fn canvas_size(
    nodes: &HashMap<String, NodeLayout>,
    groups: &HashMap<String, GroupLayout>,
    padding: f64,
) -> (f64, f64) {
    let node_max_x = nodes
        .values()
        .map(|n| n.x + n.width)
        .fold(0.0_f64, f64::max);
    let node_max_y = nodes
        .values()
        .map(|n| n.y + n.height)
        .fold(0.0_f64, f64::max);
    let group_max_x = groups
        .values()
        .map(|g| g.x + g.width)
        .fold(0.0_f64, f64::max);
    let group_max_y = groups
        .values()
        .map(|g| g.y + g.height)
        .fold(0.0_f64, f64::max);

    (
        node_max_x.max(group_max_x) + padding,
        node_max_y.max(group_max_y) + padding,
    )
}
