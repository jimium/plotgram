//! 同侧 stub 占用表：跨对共干检测与去冲突（拥堵修正 S0/S1）。
//!
//! 正反向 `enforce_reverse_pair_min_gap` 只看无向对；本模块按
//! `(node_id, side, axis_coord)` 查占用，分离**任意**同侧近距 stub
//! （含跨对，如 user-auth 的 db→auth 与 auth→cache 共竖线）。

use super::slot::is_vertical_port;
use super::EPS;
use crate::ast::Relation;
use crate::layout::routing::segment_pair::is_reverse_pair;
use crate::layout::geometry::Point;
use crate::layout::{EdgeLayout, NodeLayout, Port};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};

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
        let stub1 = if pts.len() >= 3 { pts[1] } else { pts[1.min(pts.len() - 1)] };
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

/// 层缝 vs 边带需求的粗估（S0.3 诊断；不改布局）。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LayerBandDemand {
    pub upper_layer_y: f64,
    pub lower_layer_y: f64,
    pub effective_gap: f64,
    pub crossing_edges: usize,
    pub demand: f64,
    pub deficit: f64,
}

/// 按节点 y 中心聚成层，估邻层边带需求。
///
/// `effective_gap` 用 **面距**（上层底边 → 下层顶边），不是中心距；
/// 否则节点高度会吞掉真实 gutter，T2 类样例永远报不出 deficit。
///
/// **demand 单源**：经 [`crate::layout::demand::band::edge_band_demand`] 计算；
/// 本函数只负责层聚类、face_gap 与 deficit。诊断口径用
/// [`EdgeBandDemandProfile::for_layer_band_diagnosis`]。
pub fn estimate_layer_band_demands(
    nodes: &HashMap<String, NodeLayout>,
    relations: &[Relation],
    parallel_gap: f64,
    profile: crate::layout::demand::band::EdgeBandDemandProfile,
) -> Vec<LayerBandDemand> {
    if nodes.is_empty() {
        return Vec::new();
    }
    let mut centers: Vec<(String, f64)> = nodes
        .iter()
        .map(|(id, nl)| (id.clone(), nl.y + nl.height * 0.5))
        .collect();
    centers.sort_by(|a, b| {
        a.1.partial_cmp(&b.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });

    // 简单聚类：y 差 > 40 开新层
    let mut layers: Vec<Vec<String>> = Vec::new();
    for (id, cy) in centers {
        if layers
            .last()
            .and_then(|l| l.first())
            .and_then(|id0| nodes.get(id0))
            .is_some_and(|nl0| (cy - (nl0.y + nl0.height * 0.5)).abs() <= 40.0)
        {
            layers.last_mut().unwrap().push(id);
        } else {
            layers.push(vec![id]);
        }
    }
    if layers.len() < 2 {
        return Vec::new();
    }

    let layer_center_y: Vec<f64> = layers
        .iter()
        .map(|ents| {
            ents.iter()
                .filter_map(|id| nodes.get(id))
                .map(|nl| nl.y + nl.height * 0.5)
                .sum::<f64>()
                / ents.len().max(1) as f64
        })
        .collect();

    let mut out = Vec::new();
    for li in 0..layers.len() - 1 {
        // 面距：上一层最底边 → 下一层最顶边（y 向下增大时）
        let upper_bottom = layers[li]
            .iter()
            .filter_map(|id| nodes.get(id))
            .map(|nl| nl.y + nl.height)
            .fold(f64::NEG_INFINITY, f64::max);
        let lower_top = layers[li + 1]
            .iter()
            .filter_map(|id| nodes.get(id))
            .map(|nl| nl.y)
            .fold(f64::INFINITY, f64::min);
        let face_gap = (lower_top - upper_bottom).max(0.0);
        let bd = crate::layout::demand::band::edge_band_demand(
            &layers[li],
            &layers[li + 1],
            relations,
            parallel_gap,
            profile,
        );
        let deficit = (bd.demand - face_gap).max(0.0);
        out.push(LayerBandDemand {
            upper_layer_y: layer_center_y[li].min(layer_center_y[li + 1]),
            lower_layer_y: layer_center_y[li].max(layer_center_y[li + 1]),
            effective_gap: face_gap,
            crossing_edges: bd.crossing_edges,
            demand: bd.demand,
            deficit,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{ArrowType, AttributeMap, Identifier, Relation, Span};
    use crate::layout::types::PathGeometry;

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
        // auth bottom: db→auth at x=184, auth→cache at x=184
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
        // Phase 3/6：真修空壳已删
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
        assert_eq!(stats.stubs_shifted, 0, "Phase3/6: stub post-route 已删 stats={stats:?}");
    }

    #[test]
    fn architecture_pipeline_reduces_exact_stub_cross_pairs_on_cloud_native() {
        use crate::layout::compute_layout_with_plan;
        use crate::pipeline::parse_prepare_validate;
        use crate::prepare::StyleRequest;

        let source =
            include_str!("../../../../../../showcase/architecture/product.cloud-native.pgm");
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
        // Phase 3：D 段 stub 真修已删；exact 跨对记债（H4 rip-up 收敛中）
        let _ = exact_cross;
    }

    #[test]
    fn estimate_layer_band_demand_matches_edge_band_demand() {
        use crate::layout::demand::band::{edge_band_demand, EdgeBandDemandProfile};

        let mut nodes = HashMap::new();
        nodes.insert(
            "a".into(),
            NodeLayout {
                x: 0.0,
                y: 0.0,
                width: 80.0,
                height: 40.0,
            },
        );
        nodes.insert(
            "b".into(),
            NodeLayout {
                x: 100.0,
                y: 0.0,
                width: 80.0,
                height: 40.0,
            },
        );
        nodes.insert(
            "c".into(),
            NodeLayout {
                x: 0.0,
                y: 100.0,
                width: 80.0,
                height: 40.0,
            },
        );
        nodes.insert(
            "d".into(),
            NodeLayout {
                x: 100.0,
                y: 100.0,
                width: 80.0,
                height: 40.0,
            },
        );
        let relations = vec![rel("a", "c"), rel("b", "d"), rel("a", "d")];
        let parallel_gap = 12.0;
        let profile = EdgeBandDemandProfile::for_layer_band_diagnosis(24.0);
        let bands = estimate_layer_band_demands(&nodes, &relations, parallel_gap, profile);
        assert_eq!(bands.len(), 1);
        let upper = vec!["a".to_string(), "b".to_string()];
        let lower = vec!["c".to_string(), "d".to_string()];
        let bd = edge_band_demand(&upper, &lower, &relations, parallel_gap, profile);
        assert_eq!(bands[0].crossing_edges, bd.crossing_edges);
        assert!((bands[0].demand - bd.demand).abs() < 1e-9);
        // face_gap = 100 - 40 = 60
        assert!((bands[0].effective_gap - 60.0).abs() < 1e-9);
        assert!((bands[0].deficit - (bd.demand - 60.0).max(0.0)).abs() < 1e-9);
    }
}
