//! 折线边穿节点检测。

use crate::ast::Diagram;
use crate::layout::geometry::{Point, Rect};
use crate::layout::{LayoutResult, NodeLayout};
use std::collections::HashMap;

use super::geometry::segment_intersects_aabb;
use super::overlap::analyze_edge_overlaps;
use super::{NodePushInfo, RefineConfig, RefineMetrics};

/// 分析折线边路径穿过非端点节点的情况。
pub fn analyze_edge_node_crossings(
    result: &LayoutResult,
    diagram: &Diagram,
    config: &RefineConfig,
) -> RefineMetrics {
    let mut metrics = RefineMetrics::default();
    let relations = &diagram.relations;

    for (edge_idx, edge) in result.edges.iter().enumerate() {
        if !edge.is_polyline() || edge.path_len() < 2 {
            continue;
        }
        let Some(rel) = relations.get(edge_idx) else {
            continue;
        };
        let from_id = rel.from.as_str();
        let to_id = rel.to.as_str();

        let path = edge.path_points();
        for window in path.windows(2) {
            let p1 = window[0];
            let p2 = window[1];

            for (node_id, nl) in &result.nodes {
                if node_id == from_id || node_id == to_id {
                    continue;
                }
                if segment_intersects_aabb(p1, p2, Rect::new(
                    nl.x + config.node_shrink,
                    nl.y + config.node_shrink,
                    nl.width - 2.0 * config.node_shrink,
                    nl.height - 2.0 * config.node_shrink,
                )) {
                    metrics.edge_node_crossings += 1;
                    accumulate_push(&mut metrics.problem_nodes, node_id, edge_idx, p1, p2, nl);
                }
            }
        }
    }

    metrics
}

/// 综合分析：节点穿障 + 边-边重叠。
pub(crate) fn analyze_crossings(
    result: &LayoutResult,
    diagram: &Diagram,
    config: &RefineConfig,
) -> RefineMetrics {
    let mut metrics = analyze_edge_node_crossings(result, diagram, config);
    metrics.edge_overlaps = analyze_edge_overlaps(result);
    metrics
}

fn accumulate_push(
    problem_nodes: &mut HashMap<String, NodePushInfo>,
    node_id: &str,
    edge_idx: usize,
    p1: Point,
    p2: Point,
    nl: &NodeLayout,
) {
    let info = problem_nodes.entry(node_id.to_string()).or_default();
    info.crossing_count += 1;

    if !info.edge_indices.contains(&edge_idx) {
        info.edge_indices.push(edge_idx);
    }

    let dx = p2.x - p1.x;
    let dy = p2.y - p1.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len > f64::EPSILON {
        let nx = -dy / len;
        let ny = dx / len;
        let cx = nl.x + nl.width / 2.0;
        let cy = nl.y + nl.height / 2.0;
        let vx = cx - p1.x;
        let vy = cy - p1.y;
        let dot = vx * nx + vy * ny;
        let sign = if dot >= 0.0 { 1.0 } else { -1.0 };
        info.push_fx += sign * nx;
        info.push_fy += sign * ny;
    }
}
