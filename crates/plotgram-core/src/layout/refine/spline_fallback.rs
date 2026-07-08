//! refine 最后一轮：对仍穿障的边降级为 spline 可见性图绕障。

use crate::ast::Diagram;
use crate::layout::edge::common::edge_geometry::{
    build_edge_labels, compute_bezier_controls, label_t_for_diagram,
    point_at_path_t,
};
use crate::layout::edge::common::routing_skeleton::{resolve_endpoints, RoutingContext};
use crate::layout::edge::common::self_loop::{route_self_loop, self_loop_indices, SelfLoopStyle};
use crate::layout::edge::edge_routing_spline::{
    build_full_path, fit_multi_segment_spline, sample_bezier,
};
use crate::layout::edge::visibility;
use crate::layout::geometry::Point;
use crate::layout::{EdgeLayout, LayoutResult, PathGeometry};
use crate::layout::edge::edge_routing_bezier::BezierConfig;
use std::collections::HashSet;

const SPLINE_SAMPLES_PER_SEGMENT: usize = 12;

/// 对指定边索引用 spline 可见性图重路由（混合路由兜底）。
pub(crate) fn reroute_edges_with_spline(
    result: &mut LayoutResult,
    diagram: &Diagram,
    edge_indices: &HashSet<usize>,
) {
    if edge_indices.is_empty() {
        return;
    }

    let relations = &diagram.relations;
    let routing_snapshot = LayoutResult {
        nodes: result.nodes.clone(),
        groups: result.groups.clone(),
        edges: result.edges.clone(),
        total_width: result.total_width,
        total_height: result.total_height,
        hints: result.hints.clone(),
    };
    let ctx = RoutingContext::new(diagram, &routing_snapshot);
    let self_loop_idx = self_loop_indices(relations);
    let tension = BezierConfig::default().tension;

    let mut sorted_node_ids: Vec<String> = routing_snapshot.nodes.keys().cloned().collect();
    sorted_node_ids.sort();
    let node_list: Vec<(usize, &crate::layout::NodeLayout)> = sorted_node_ids
        .iter()
        .filter_map(|id| routing_snapshot.nodes.get(id).map(|nl| (id.as_str(), nl)))
        .enumerate()
        .map(|(i, (_, nl))| (i, nl))
        .collect();
    let node_id_to_idx: std::collections::HashMap<&str, usize> = sorted_node_ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    let obstacle_index = visibility::ObstacleIndex::build(&node_list);

    let mut spline_count = 0usize;
    let mut updates: Vec<(usize, EdgeLayout)> = Vec::new();
    let mut ordered_edges: Vec<usize> = edge_indices.iter().copied().collect();
    ordered_edges.sort_unstable();
    for &i in &ordered_edges {
        let Some(rel) = relations.get(i) else {
            continue;
        };
        if rel.from.as_str() == rel.to.as_str() {
            if let Some(nl) = routing_snapshot.nodes.get(rel.from.as_str()) {
                let loop_idx = self_loop_idx.get(&i).copied().unwrap_or(0);
                updates.push((
                    i,
                    route_self_loop(rel, nl, loop_idx, SelfLoopStyle::Curved),
                ));
            }
            continue;
        }

        let Some((ep, label_off)) = resolve_endpoints(&ctx, rel, i) else {
            continue;
        };

        let from_idx = node_id_to_idx
            .get(ep.from_id.as_str())
            .copied()
            .unwrap_or(usize::MAX);
        let to_idx = node_id_to_idx
            .get(ep.to_id.as_str())
            .copied()
            .unwrap_or(usize::MAX);

        let detour_path = obstacle_index.shortest_path(ep.start, ep.end, &[from_idx, to_idx]);

        let (geometry, sampled_for_label) = if detour_path.is_empty() {
            let cp = compute_bezier_controls(
                ep.start.x,
                ep.start.y,
                ep.end.x,
                ep.end.y,
                ep.from_port,
                ep.to_port,
                tension,
            );
            let sampled = sample_bezier(ep.start, cp[0], cp[1], ep.end, SPLINE_SAMPLES_PER_SEGMENT);
            (
                PathGeometry::Bezier {
                    start: ep.start,
                    end: ep.end,
                    controls: cp,
                },
                sampled,
            )
        } else {
            let full_path = build_full_path(ep.start, &detour_path, ep.end);
            let sampled = fit_multi_segment_spline(&full_path, SPLINE_SAMPLES_PER_SEGMENT);
            (PathGeometry::Polyline { points: sampled.clone() }, sampled)
        };

        let middle_t = label_t_for_diagram(diagram, rel);
        let labels = build_edge_labels(rel, middle_t, Point::new(label_off.ox, label_off.oy), |t| {
            point_at_path_t(&sampled_for_label, t)
        });

        updates.push((
            i,
            EdgeLayout {
                geometry,
                labels,
                from_port: ep.from_port,
                to_port: ep.to_port,
            },
        ));
        spline_count += 1;
    }

    for (i, edge) in updates {
        if i < result.edges.len() {
            result.edges[i] = edge;
        }
    }

    if spline_count > 0 {
        if let Some(stats) = result.hints.refine_debug.as_mut() {
            stats.spline_fallback_count = spline_count;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        ArrowType, AttributeMap, Diagram, Entity, Identifier, Relation, SourceInfo, Span,
    };
    use crate::layout::{LayoutResult, NodeLayout, PathGeometry};
    use crate::types::DiagramType;

    #[test]
    fn spline_fallback_reroutes_selected_edge() {
        let span = Span::dummy();
        let diagram = Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: Vec::new(),
            entities: vec![
                Entity {
                    id: Identifier::new_unchecked("a"),
                    label: "a".into(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span,
                },
                Entity {
                    id: Identifier::new_unchecked("b"),
                    label: "b".into(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span,
                },
            ],
            relations: vec![Relation {
                from: Identifier::new_unchecked("a"),
                to: Identifier::new_unchecked("b"),
                arrow: ArrowType::Active,
                label: None,
                head_label: None,
                tail_label: None,
                attributes: AttributeMap::default(),
                span,
            }],
            groups: Vec::new(),
            style_decls: vec![],
            source_info: SourceInfo {
                file: None,
                line_count: 1,
            },
            ..Default::default()
        };

        let mut result = LayoutResult {
            nodes: [
                (
                    "a".to_string(),
                    NodeLayout {
                        x: 0.0,
                        y: 0.0,
                        width: 80.0,
                        height: 40.0,
                        ..Default::default()
                    },
                ),
                (
                    "b".to_string(),
                    NodeLayout {
                        x: 200.0,
                        y: 0.0,
                        width: 80.0,
                        height: 40.0,
                        ..Default::default()
                    },
                ),
            ]
            .into_iter()
            .collect(),
            groups: Default::default(),
            edges: vec![EdgeLayout {
                geometry: PathGeometry::Polyline {
                    points: vec![
                        Point::new(40.0, 20.0),
                        Point::new(240.0, 20.0),
                    ],
                },
                labels: Vec::new(),
                from_port: crate::layout::Port::Right,
                to_port: crate::layout::Port::Left,
            }],
            total_width: 300.0,
            total_height: 100.0,
            hints: Default::default(),
        };

        let mut set = HashSet::new();
        set.insert(0);
        reroute_edges_with_spline(&mut result, &diagram, &set);
        assert!(result.edges[0].path_len() >= 2);
    }
}
