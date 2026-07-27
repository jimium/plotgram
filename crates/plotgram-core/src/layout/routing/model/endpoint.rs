//! 路径构建用的轻量端点描述（R1：自 OVG `slot` 外提）。
//!
//! 供 slot 分配与 path building 共用；`solution::EndpointAssignment::project_endpoint`
//! 与 ortho `EndpointPair` 均指向本类型，避免 model 反依赖 `edge_routing_orthogonal`。

use crate::layout::geometry::Point;
use crate::layout::types::Port;

/// Endpoint descriptor for slot assignment and path building.
///
/// Carries everything the slot-assignment pass (sorting by target) and the
/// path-building pass (anchor / side / node_id) need, so that `EndpointPair`
/// can be handed to `select_best_path` without extra parameters.
#[derive(Clone)]
pub struct Endpoint {
    pub edge_index: usize,
    pub is_from: bool,
    /// Opposite node center, used for slot sorting along the side.
    pub target_x: f64,
    pub target_y: f64,
    pub lane: usize,
    /// Node id this endpoint sits on.
    pub node_id: String,
    /// Connection side (port) on the node.
    pub side: Port,
    /// Resolved slot anchor coordinates (filled in during slot assignment).
    pub anchor: Point,
}
