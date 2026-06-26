//! 分组 Border Shell 与组间路由走廊子系统。
//!
//! - [`border_shell`]：路径段 vs 分组边框关系分类 + 贴边平行检测
//! - [`corridor`]：组间优先走廊（布局注入 + 几何 fallback）
//! - [`post_route`]：snap 后投影与 repulse 安全网
//! - [`rect`]：路由权威 `GroupLayout` 来源
//! - [`context`]：边路由阶段共享上下文

pub mod border_shell;
pub mod config;
pub mod constants;
pub mod context;
pub mod corridor;
pub mod post_route;
pub mod rect;

pub use border_shell::{
    group_segment_violates_border_shell, segment_hugs_group_border,
    segment_intersects_group_shell, segment_near_misses_group_shell,
    segment_within_port_stub_zone,
};
pub use config::{routing_algo_for_diagram, GroupRoutingProfile};
pub use constants::{GROUP_BORDER_SHELL_PAD, PORT_STUB_CLEARANCE};
pub use context::{build_node_to_groups, GroupRoutingContext, GroupRoutingHints};
pub use corridor::{
    build_corridors_from_groups, build_stacking_corridors, corridor_misalignment_penalty,
    merge_corridors, prefer_corridor_coord, CorridorAxis, GroupCorridor,
};
pub use post_route::{
    project_path_off_group_borders, project_path_off_group_borders_with_stub,
    repulse_edges_from_group_borders,
};
pub use rect::{finalize_routing_groups, routing_group_padding};
#[cfg(debug_assertions)]
pub use rect::debug_assert_routing_groups_contain_members;

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
