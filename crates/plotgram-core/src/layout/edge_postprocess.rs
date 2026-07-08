//! 边路由后的统一后处理：waypoint 量化 + 分组边框排斥。
//!
//! 本模块提供两个独立函数：
//! - [`repulse_edges_only`]：仅执行分组边框排斥（几何投影），不含量化。在路由后、组框修复后执行。
//! - [`snap_and_repulse_edges`]：执行像素量化 + 边框排斥 + 简化。在管道最末尾执行（仅一次）。

use crate::layout::grid_snap;
use crate::layout::group;
use crate::layout::{EdgeLayout, EdgeSnapConfig, GroupLayout};
use std::collections::HashMap;

/// 仅执行分组边框排斥（不含量化）。
///
/// 在路由完成后、组框修复后执行，将贴边路径推开到合法通道。
/// 当 snap 未启用时无操作。
pub fn repulse_edges_only(
    edges: &mut [EdgeLayout],
    groups: &HashMap<String, GroupLayout>,
    config: &EdgeSnapConfig,
) {
    if !config.enabled {
        return;
    }
    group::repulse_edges_from_group_borders(
        edges,
        groups,
        config.shell_pad,
        config.grid_step,
        config.repulse_max_rounds,
    );
}

/// 对边路径执行 grid snap 与分组边框排斥（L3 waypoint snap + Border Shell repulse）。
///
/// 在管道最末尾执行，仅运行一次。
/// 当 snap 未启用时无操作。
pub fn snap_and_repulse_edges(
    edges: &mut [EdgeLayout],
    groups: &HashMap<String, GroupLayout>,
    config: &EdgeSnapConfig,
) {
    if !config.enabled {
        return;
    }
    grid_snap::snap_edge_waypoints(edges, groups, config);
    group::repulse_edges_from_group_borders(
        edges,
        groups,
        config.shell_pad,
        config.grid_step,
        config.repulse_max_rounds,
    );
}
