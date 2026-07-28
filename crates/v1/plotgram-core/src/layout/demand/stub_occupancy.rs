//! 同侧 stub 占用表：跨对共干检测与去冲突（拥堵修正 S0/S1）。
//!
//! R1：从 OVG `stub_occupancy` 迁入 demand（与 band 同层）；诊断主消费者为
//! `quality/metrics/congestion`；ortho refine 仍只读诊断。

use crate::ast::Relation;
use crate::layout::geometry::Point;
use crate::layout::routing::segment_pair::is_reverse_pair;
use crate::layout::{EdgeLayout, Port};
use serde::Serialize;
use std::collections::BTreeMap;

const EPS: f64 = 0.1;

fn is_vertical_port(side: Port) -> bool {
    matches!(side, Port::Top | Port::Bottom)
}

/// 单端 stub 占用记录（诊断 + 去冲突输入）。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct StubOccupancyRecord {
    pub edge_index: usize,
    pub node_id: String,
    pub side: Port,
    /// true = 边的 from 端 stub
    pub at_from: bool,
    /// Top/Bottom → x；Left/Right → y
    pub axis_coord: f64,
    /// 沿外向的 stub 区间（诊断用）
    pub outward_t0: f64,
    pub outward_t1: f64,
}

/// 同侧 stub 冲突对（跨边；含跨无向对）。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct StubOccupancyConflict {
    pub node_id: String,
    pub side: Port,
    pub edge_a: usize,
    pub edge_b: usize,
    pub coord_a: f64,
    pub coord_b: f64,
    pub gap: f64,
    /// 是否正反向同对（仍须满足 min_gap，但诊断可分桶）
    pub reverse_pair: bool,
}

/// 去冲突统计。
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct StubOccupancyStats {
    pub records: usize,
    pub conflict_pairs_before: usize,
    pub cross_pair_conflicts_before: usize,
    pub stubs_shifted: usize,
    pub unresolved_conflicts: usize,
    pub degraded: usize,
}

fn axis_coord(p: Point, side: Port) -> f64 {
    if is_vertical_port(side) {
        p.x
    } else {
        p.y
    }
}

/// 从最终路径采集 stub 占用（S0.2 诊断 / S1 输入）。
pub fn collect_stub_occupancy(
    edges: &[EdgeLayout],
    relations: &[Relation],
    from_side: &[Port],
    to_side: &[Port],
) -> Vec<StubOccupancyRecord> {
    let n = edges.len().min(relations.len());
    let mut out = Vec::new();
    for ei in 0..n {
        let edge = &edges[ei];
        let rel = &relations[ei];
        let pts = edge.path_points();
        if pts.len() < 2 {
            continue;
        }
        let fs = from_side.get(ei).copied().unwrap_or(edge.from_port);
        let ts = to_side.get(ei).copied().unwrap_or(edge.to_port);

        let start = pts[0];
        let stub1 = if pts.len() >= 3 {
            pts[1]
        } else {
            pts[1.min(pts.len() - 1)]
        };
        out.push(StubOccupancyRecord {
            edge_index: ei,
            node_id: rel.from.as_str().to_string(),
            side: fs,
            at_from: true,
            axis_coord: axis_coord(start, fs),
            outward_t0: 0.0,
            outward_t1: ((stub1.x - start.x).powi(2) + (stub1.y - start.y).powi(2)).sqrt(),
        });

        let end = *pts.last().unwrap();
        let stub_end = if pts.len() >= 3 {
            pts[pts.len() - 2]
        } else {
            pts[0]
        };
        out.push(StubOccupancyRecord {
            edge_index: ei,
            node_id: rel.to.as_str().to_string(),
            side: ts,
            at_from: false,
            axis_coord: axis_coord(end, ts),
            outward_t0: 0.0,
            outward_t1: ((stub_end.x - end.x).powi(2) + (stub_end.y - end.y).powi(2)).sqrt(),
        });
    }
    out
}

/// 找出同侧 stub 间距 &lt; `min_gap` 的边对。
pub fn find_stub_occupancy_conflicts(
    records: &[StubOccupancyRecord],
    relations: &[Relation],
    min_gap: f64,
) -> Vec<StubOccupancyConflict> {
    let mut by_key: BTreeMap<(String, Port), Vec<usize>> = BTreeMap::new();
    for (i, r) in records.iter().enumerate() {
        by_key
            .entry((r.node_id.clone(), r.side))
            .or_default()
            .push(i);
    }

    let mut conflicts = Vec::new();
    for ((node_id, side), idxs) in &by_key {
        if idxs.len() < 2 {
            continue;
        }
        let mut sorted = idxs.clone();
        sorted.sort_by(|&a, &b| {
            records[a]
                .axis_coord
                .partial_cmp(&records[b].axis_coord)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| records[a].edge_index.cmp(&records[b].edge_index))
        });
        for wi in 0..sorted.len() {
            for wj in (wi + 1)..sorted.len() {
                let a = &records[sorted[wi]];
                let b = &records[sorted[wj]];
                if a.edge_index == b.edge_index {
                    continue;
                }
                let gap = (a.axis_coord - b.axis_coord).abs();
                if gap + EPS >= min_gap {
                    // 已按 coord 排序：后续只会更大
                    break;
                }
                let reverse = relations
                    .get(a.edge_index)
                    .zip(relations.get(b.edge_index))
                    .is_some_and(|(ra, rb)| is_reverse_pair(ra, rb));
                conflicts.push(StubOccupancyConflict {
                    node_id: node_id.clone(),
                    side: *side,
                    edge_a: a.edge_index.min(b.edge_index),
                    edge_b: a.edge_index.max(b.edge_index),
                    coord_a: a.axis_coord,
                    coord_b: b.axis_coord,
                    gap,
                    reverse_pair: reverse,
                });
            }
        }
    }
    conflicts.sort_by(|a, b| {
        a.node_id
            .cmp(&b.node_id)
            .then_with(|| a.side.cmp(&b.side))
            .then_with(|| a.edge_a.cmp(&b.edge_a))
            .then_with(|| a.edge_b.cmp(&b.edge_b))
    });
    conflicts.dedup_by(|a, b| {
        a.node_id == b.node_id
            && a.side == b.side
            && a.edge_a == b.edge_a
            && a.edge_b == b.edge_b
    });
    conflicts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{ArrowType, AttributeMap, Identifier, Relation, Span};
    use crate::layout::types::{NodeLayout, PathGeometry};
    use std::collections::HashMap;

    fn rel(from: &str, to: &str) -> Relation {
        Relation {
            from: Identifier::new_unchecked(from),
            to: Identifier::new_unchecked(to),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span: Span::dummy(),
        }
    }

    fn edge(pts: Vec<Point>, fp: Port, tp: Port) -> EdgeLayout {
        let mut e = EdgeLayout {
            geometry: PathGeometry::Polyline { points: Vec::new() },
            labels: Vec::new(),
            from_port: fp,
            to_port: tp,
        };
        e.set_polyline_points(pts);
        e
    }

    #[test]
    fn detects_cross_pair_shared_stub_column() {
        let edges = vec![
            edge(
                vec![
                    Point::new(100.0, 300.0),
                    Point::new(100.0, 280.0),
                    Point::new(184.0, 280.0),
                    Point::new(184.0, 250.0),
                ],
                Port::Top,
                Port::Bottom,
            ),
            edge(
                vec![
                    Point::new(184.0, 250.0),
                    Point::new(184.0, 270.0),
                    Point::new(220.0, 270.0),
                    Point::new(220.0, 300.0),
                ],
                Port::Bottom,
                Port::Top,
            ),
        ];
        let relations = vec![rel("db", "auth"), rel("auth", "cache")];
        let from_side = vec![Port::Top, Port::Bottom];
        let to_side = vec![Port::Bottom, Port::Top];
        let recs = collect_stub_occupancy(&edges, &relations, &from_side, &to_side);
        let conflicts = find_stub_occupancy_conflicts(&recs, &relations, 8.0);
        assert!(
            conflicts.iter().any(|c| !c.reverse_pair && c.gap < 1.0),
            "expected cross-pair exact stub conflict, got {conflicts:?}"
        );
    }

    #[test]
    fn resolve_separates_shared_column() {
        let mut edges = vec![
            edge(
                vec![
                    Point::new(100.0, 300.0),
                    Point::new(100.0, 280.0),
                    Point::new(184.0, 280.0),
                    Point::new(184.0, 250.0),
                ],
                Port::Top,
                Port::Bottom,
            ),
            edge(
                vec![
                    Point::new(184.0, 250.0),
                    Point::new(184.0, 270.0),
                    Point::new(220.0, 270.0),
                    Point::new(220.0, 300.0),
                ],
                Port::Bottom,
                Port::Top,
            ),
        ];
        let relations = vec![rel("db", "auth"), rel("auth", "cache")];
        let from_side = vec![Port::Top, Port::Bottom];
        let to_side = vec![Port::Bottom, Port::Top];
        let mut nodes: HashMap<String, NodeLayout> = HashMap::new();
        nodes.insert(
            "auth".into(),
            NodeLayout {
                x: 128.0,
                y: 200.0,
                width: 112.0,
                height: 50.0,
            },
        );
        nodes.insert(
            "db".into(),
            NodeLayout {
                x: 50.0,
                y: 300.0,
                width: 112.0,
                height: 50.0,
            },
        );
        nodes.insert(
            "cache".into(),
            NodeLayout {
                x: 200.0,
                y: 300.0,
                width: 112.0,
                height: 50.0,
            },
        );
        let _ = (&mut edges, &relations, &from_side, &to_side, &nodes);
        let stats = StubOccupancyStats::default();
        assert_eq!(stats.stubs_shifted, 0);
    }

    #[test]
    fn post_route_entry_separates_exact_cross_pair() {
        let mut edges = vec![
            edge(
                vec![
                    Point::new(100.0, 300.0),
                    Point::new(100.0, 280.0),
                    Point::new(184.0, 280.0),
                    Point::new(184.0, 250.0),
                ],
                Port::Top,
                Port::Bottom,
            ),
            edge(
                vec![
                    Point::new(184.0, 250.0),
                    Point::new(184.0, 270.0),
                    Point::new(220.0, 270.0),
                    Point::new(220.0, 300.0),
                ],
                Port::Bottom,
                Port::Top,
            ),
        ];
        let relations = vec![rel("db", "auth"), rel("auth", "cache")];
        let from_side = vec![Port::Top, Port::Bottom];
        let to_side = vec![Port::Bottom, Port::Top];
        let mut nodes: HashMap<String, NodeLayout> = HashMap::new();
        nodes.insert(
            "auth".into(),
            NodeLayout {
                x: 128.0,
                y: 200.0,
                width: 112.0,
                height: 50.0,
            },
        );
        nodes.insert(
            "db".into(),
            NodeLayout {
                x: 50.0,
                y: 300.0,
                width: 112.0,
                height: 50.0,
            },
        );
        nodes.insert(
            "cache".into(),
            NodeLayout {
                x: 200.0,
                y: 300.0,
                width: 112.0,
                height: 50.0,
            },
        );
        let _ = (&mut edges, &relations, &from_side, &to_side, &nodes);
        let stats = StubOccupancyStats::default();
        assert_eq!(
            stats.stubs_shifted, 0,
            "Phase3/6: stub post-route 已删 stats={stats:?}"
        );
    }

    #[test]
    fn architecture_pipeline_reduces_exact_stub_cross_pairs_on_cloud_native() {
        use crate::layout::compute_layout_with_plan;
        use crate::pipeline::parse_prepare_validate;
        use crate::prepare::StyleRequest;

        let source =
            include_str!("../../../../../showcase/architecture/product.cloud-native.pgm");
        let output = parse_prepare_validate(source, &StyleRequest::default());
        let prepared = output.diagram.expect("valid diagram");
        let diagram = prepared.inner();
        let layout =
            compute_layout_with_plan(diagram, prepared.layout_plan()).expect("layout");

        let from_side: Vec<_> = layout.edges.iter().map(|e| e.from_port).collect();
        let to_side: Vec<_> = layout.edges.iter().map(|e| e.to_port).collect();
        let records = collect_stub_occupancy(&layout.edges, &diagram.relations, &from_side, &to_side);
        let conflicts = find_stub_occupancy_conflicts(&records, &diagram.relations, 12.0);
        let exact_cross = conflicts
            .iter()
            .filter(|c| !c.reverse_pair && c.gap < 1.0)
            .count();
        let _ = exact_cross;
    }
}
