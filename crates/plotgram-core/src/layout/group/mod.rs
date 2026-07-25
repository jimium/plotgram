//! 分组子系统（收缩后）：走廊、写权、常量；Frame Spec 见 recipes::frame_spec。
//!
//! - 物化：[`crate::layout::kernel::group`]
//! - 路由上下文：[`crate::layout::routing::group_ctx`]
//! - Frame Spec：[`crate::layout::recipes::frame_spec`]

pub mod constants;
pub mod corridor;
pub mod rect;
pub mod write_counter;

pub use constants::{GROUP_BORDER_SHELL_PAD, PORT_STUB_CLEARANCE};
pub use corridor::{
    build_corridors_from_groups, build_sibling_corridors, build_stacking_corridors,
    corridor_misalignment_penalty, merge_corridors, prefer_corridor_coord, CorridorAxis,
    GroupCorridor,
};
pub use crate::layout::routing::group_ctx::{
    build_node_to_groups, group_segment_violates_border_shell, routing_algo_for_diagram,
    segment_hugs_group_border, segment_intersects_group_shell, segment_near_misses_group_shell,
    segment_within_port_stub_zone, GroupRoutingContext, GroupRoutingHints, GroupRoutingProfile,
};
pub use crate::layout::routing::post_route::{
    project_path_off_group_borders, project_path_off_group_borders_with_stub,
    repulse_edges_from_group_borders,
};
pub use crate::layout::kernel::group::hierarchy::{build_group_hierarchy, GroupHierarchy, SiblingOrientation};
pub use rect::{finalize_routing_groups, routing_group_padding};
#[cfg(debug_assertions)]
pub use rect::debug_assert_routing_groups_contain_members;

/// 过渡：旧 `group::border_shell` / `context` / `config` / `hierarchy` 路径。
pub use crate::layout::routing::group_ctx::border_shell;
pub use crate::layout::routing::group_ctx::config;
pub use crate::layout::routing::group_ctx::context;
pub use crate::layout::kernel::group::hierarchy;

#[cfg(test)]
mod integration_tests {
    use std::collections::HashMap;

    use crate::layout::GroupLayout;

    use super::corridor::{build_corridors_from_groups, CorridorAxis};
    use super::constants::EPS;

    #[test]
    fn routing_group_rect_invariant_with_corridors() {
        let mut groups = HashMap::new();
        groups.insert(
            "lane_a".to_string(),
            GroupLayout {
                x: 0.0,
                y: 0.0,
                width: 120.0,
                height: 200.0,
            },
        );
        groups.insert(
            "lane_b".to_string(),
            GroupLayout {
                x: 200.0,
                y: 0.0,
                width: 120.0,
                height: 200.0,
            },
        );
        let corridors = build_corridors_from_groups(&groups);
        assert_eq!(corridors.len(), 1);
        assert_eq!(corridors[0].axis, CorridorAxis::Vertical);
        assert!((corridors[0].coord - 160.0).abs() < EPS);
    }
}
