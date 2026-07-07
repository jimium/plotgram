//! 增量重路由：只更新受影响的边。

use crate::ast::Diagram;
use crate::layout::{EdgeRoutingStrategy, LayoutResult};
use std::collections::HashSet;
use std::mem;

/// 保留占比低于此阈值时回退全量重路由（与 orthogonal `reroute_edges_touching_nodes` 对称）。
const MIN_PRESERVE_RATIO: f64 = 0.15;

pub(crate) fn reroute_subset(
    result: &mut LayoutResult,
    diagram: &Diagram,
    router: &dyn EdgeRoutingStrategy,
    affected_edges: &HashSet<usize>,
) {
    if affected_edges.is_empty() {
        return;
    }

    let n = diagram.relations.len();
    if n == 0 {
        return;
    }

    let mut preserve = HashSet::new();
    for i in 0..n {
        if !affected_edges.contains(&i) {
            preserve.insert(i);
        }
    }

    let layout = LayoutResult {
        nodes: mem::take(&mut result.nodes),
        groups: mem::take(&mut result.groups),
        edges: mem::take(&mut result.edges),
        total_width: result.total_width,
        total_height: result.total_height,
        hints: mem::take(&mut result.hints),
    };

    let fresh = if preserve.is_empty()
        || (preserve.len() as f64 / n as f64) < MIN_PRESERVE_RATIO
    {
        router.route(diagram, layout)
    } else {
        router.route_preserve(diagram, layout, &preserve)
    };

    *result = fresh;
}
