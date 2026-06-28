//! 边路由后的统一后处理：waypoint 量化 + 分组边框排斥。

use crate::layout::grid_snap;
use crate::layout::group;
use crate::layout::{EdgeLayout, EdgeSnapConfig, GroupLayout};
use std::collections::HashMap;

/// 对边路径执行 grid snap 与分组边框排斥（L3 waypoint snap + Border Shell repulse）。
///
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
