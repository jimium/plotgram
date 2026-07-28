//! 路由侧 group 上下文：障碍壳、配置、hints。

pub mod border_shell;
pub mod config;
pub mod context;

pub use border_shell::{
    group_segment_violates_border_shell, segment_hugs_group_border,
    segment_intersects_group_shell, segment_near_misses_group_shell,
    segment_within_port_stub_zone,
};
pub use config::{routing_algo_for_diagram, GroupRoutingProfile};
pub use context::{build_node_to_groups, GroupRoutingContext, GroupRoutingHints};
pub use crate::layout::kernel::group::hierarchy::SiblingOrientation;
