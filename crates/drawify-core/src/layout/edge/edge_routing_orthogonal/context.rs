//! Routing context and endpoint pair for orthogonal edge routing.
//!
//! These types bundle the per-edge and per-diagram state that `select_best_path`
//! and the candidate scorer need, so that path building functions can take a
//! single context reference instead of a long parameter list.

use crate::layout::geometry::Point;
use crate::layout::group::{GroupCorridor, GroupRoutingContext};
use crate::layout::{GroupLayout, NodeLayout};
use std::collections::HashMap;

use super::{OrthoConfig, RoutedSegment};
use super::slot::Endpoint;

/// Shared, read-only routing context for a single `route_edges_orthogonal` call.
///
/// Holds references to the diagram-level node/group maps, the already-routed
/// segments (for overlap detection), and the resolved config. All per-edge
/// path-building functions receive `&RoutingContext` instead of repeating
/// these parameters.
pub struct RoutingContext<'a> {
    pub nodes: &'a HashMap<String, NodeLayout>,
    pub group_ctx: &'a GroupRoutingContext,
    pub routed_segments: &'a [RoutedSegment],
    pub cfg: &'a OrthoConfig,
}

/// A pair of resolved endpoints (from / to) for a single edge.
///
/// Each `Endpoint` carries its anchor coordinates, connection side, and node id,
/// so `select_best_path` can reconstruct everything it needs from `&EndpointPair`.
pub struct EndpointPair {
    pub from: Endpoint,
    pub to: Endpoint,
}

impl EndpointPair {
    #[inline]
    pub fn from_id(&self) -> &str {
        &self.from.node_id
    }

    #[inline]
    pub fn to_id(&self) -> &str {
        &self.to.node_id
    }

    #[inline]
    pub fn from_anchor(&self) -> Point {
        self.from.anchor
    }

    #[inline]
    pub fn to_anchor(&self) -> Point {
        self.to.anchor
    }
}
