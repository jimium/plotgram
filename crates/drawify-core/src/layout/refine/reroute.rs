//! 增量重路由：只更新受影响的边。

use crate::ast::Diagram;
use crate::layout::{EdgeRoutingStrategy, LayoutResult};
use std::collections::HashSet;

pub(crate) fn reroute_subset(
    result: &mut LayoutResult,
    diagram: &Diagram,
    router: &dyn EdgeRoutingStrategy,
    affected_edges: &HashSet<usize>,
) {
    if affected_edges.is_empty() {
        return;
    }

    let fresh = router.route(diagram, result.clone());

    for &i in affected_edges {
        if let Some(new_edge) = fresh.edges.get(i) {
            result.edges[i] = new_edge.clone();
        }
    }

    result.total_width = fresh.total_width;
    result.total_height = fresh.total_height;
}
