//! 路由后处理：shell gutters、边框排斥、路径投影。

pub mod border_repulse;
pub mod path_projection;
pub mod shell_expand;

/// 可保留边占比低于该阈值时退回全图重路由（Slice F2c：增量复用门槛）。
pub const MIN_PRESERVE_RATIO: f64 = 0.10;

pub use border_repulse::{
    repulse_edges_only, snap_and_repulse_edges, snap_and_repulse_edges_with_guard,
};
pub use path_projection::{
    project_path_off_group_borders, project_path_off_group_borders_with_stub,
    repulse_edges_from_group_borders,
};
pub use shell_expand::{
    commit_side_gutters_into_groups, feedforward_shell_from_orthosketch,
    route_shell_overflow_remaining,
};
