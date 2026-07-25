//! 增量重路由：只更新受影响的边。
//!
//! R11a/b：增量接口已删除，统一走全图重路由。

use crate::ast::Diagram;
use crate::layout::{RoutingRecipeDyn, LayoutResult};
use std::collections::HashSet;

pub(crate) fn reroute_subset(
    result: &mut LayoutResult,
    diagram: &Diagram,
    router: &dyn RoutingRecipeDyn,
    affected_edges: &HashSet<usize>,
) {
    if affected_edges.is_empty() {
        return;
    }

    let n = diagram.relations.len();
    if n == 0 {
        return;
    }

    // Slice B：构造 PreparedRoutingInput → router.route(&input) → 写回 edges。
    let frozen = crate::layout::routing::coordinator::FrozenNodeProduct::capture(result);
    let input = crate::layout::routing::model::prepared::PreparedRoutingInput::prepare(
        &frozen,
        diagram,
        &result.hints,
        "",
        router.name(),
        Default::default(),
        crate::layout::routing::model::prepared::RoutingCanvas {
            width: result.total_width,
            height: result.total_height,
        },
    );
    let product = router.route(&input);
    result.edges = product.edges;
    if product.group_routing.is_some() {
        result.hints.group_routing = product.group_routing;
    }
    if product.route_annotations.is_some() {
        result.hints.route_annotations = product.route_annotations;
    }
    if product.orthogonal_debug.is_some() {
        result.hints.orthogonal_debug = product.orthogonal_debug;
    }
}
