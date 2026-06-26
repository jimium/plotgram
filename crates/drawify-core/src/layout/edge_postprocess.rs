//! 边路由后的统一后处理：waypoint 量化 + 分组边框排斥。

use crate::layout::grid_snap::{self, GridSnapConfig};
use crate::layout::group;
use crate::layout::{EdgeLayout, GroupLayout};
use std::collections::HashMap;

/// 对边路径执行 grid snap 与分组边框排斥（L3 waypoint snap + Border Shell repulse）。
///
/// 当 snap 未启用时无操作。
pub fn snap_and_repulse_edges(
    edges: &mut [EdgeLayout],
    groups: &HashMap<String, GroupLayout>,
    algo: &str,
    diagram: &crate::ast::Diagram,
) {
    if !grid_snap::should_snap(algo) {
        return;
    }
    let snap_config = GridSnapConfig::for_diagram(algo, diagram);
    if !snap_config.enabled {
        return;
    }
    let group_profile = group::GroupRoutingProfile::for_algo(algo);
    grid_snap::snap_edge_waypoints(
        edges,
        groups,
        &snap_config,
        group_profile.border_shell_pad,
        group_profile.stub_clearance,
    );
    group::repulse_edges_from_group_borders(
        edges,
        groups,
        group_profile.border_shell_pad,
        snap_config.grid_step,
        group_profile.repulse_max_rounds,
    );
}
