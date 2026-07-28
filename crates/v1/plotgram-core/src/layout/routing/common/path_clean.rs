//! 路径硬清洁检查（R1：从 OVG `scoring` 抽到 common，供 kernel feasibility 复用）。
//!
//! 与 [`super::geom_obstacle`] 对齐：穿节点 / 穿组严格内部。

use crate::layout::geometry::Point;
use crate::layout::group::GroupRoutingContext;
use crate::layout::{GroupLayout, NodeLayout};
use std::collections::HashMap;

/// 节点障碍膨胀（与历史 OVG scoring 一致）。
pub const NODE_OBSTACLE_PAD: f64 = crate::layout::constants::DEFAULT_NODE_MARGIN;
/// 组框障碍膨胀。
pub const GROUP_OBSTACLE_PAD: f64 = crate::layout::group::GROUP_BORDER_SHELL_PAD;

/// 路径是否不穿端点以外的节点实体。
pub fn path_is_clean(
    path: &[Point],
    from_id: &str,
    to_id: &str,
    nodes: &HashMap<String, NodeLayout>,
    _groups: &GroupRoutingContext,
    sorted_node_ids: &[String],
) -> bool {
    let last = path.len().saturating_sub(2);
    path.windows(2).enumerate().all(|(si, w)| {
        sorted_node_ids.iter().all(|id| {
            if (id == from_id && si == 0) || (id == to_id && si == last) {
                return true;
            }
            let Some(node) = nodes.get(id) else {
                return true;
            };
            let pad = if id == from_id || id == to_id {
                0.0
            } else {
                NODE_OBSTACLE_PAD
            };
            !super::geom_obstacle::segment_pierces_node(w[0], w[1], node, pad)
        })
    })
}

/// 路径是否不穿端点无关组的严格内部。
pub fn path_avoids_group_interiors(
    path: &[Point],
    from_id: &str,
    to_id: &str,
    groups: &GroupRoutingContext,
    sorted_group_ids: &[String],
) -> bool {
    let endpoints = groups.endpoint_group_set(from_id, to_id);
    path.windows(2).all(|w| {
        sorted_group_ids.iter().all(|id| {
            endpoints.contains(id.as_str())
                || groups
                    .groups
                    .get(id)
                    .is_none_or(|g| !segment_crosses_rect_interior(w[0], w[1], g))
        })
    })
}

fn segment_crosses_rect_interior(a: Point, b: Point, group: &GroupLayout) -> bool {
    super::geom_obstacle::segment_pierces_group_interior(a, b, group)
}
