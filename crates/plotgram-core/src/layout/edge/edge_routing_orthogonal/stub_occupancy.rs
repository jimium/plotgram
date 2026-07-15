//! 同侧 stub 占用表：跨对共干检测与去冲突（拥堵修正 S0/S1）。
//!
//! 正反向 `enforce_reverse_pair_min_gap` 只看无向对；本模块按
//! `(node_id, side, axis_coord)` 查占用，分离**任意**同侧近距 stub
//! （含跨对，如 user-auth 的 db→auth 与 auth→cache 共竖线）。

use super::slot::is_vertical_port;
use super::{EPS, PORT_CLEARANCE, SLOT_MARGIN_RATIO};
use crate::ast::Relation;
use crate::layout::edge::segment_pair::is_reverse_pair;
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

fn set_axis(p: &mut Point, side: Port, coord: f64) {
    if is_vertical_port(side) {
        p.x = coord;
    } else {
        p.y = coord;
    }
}

fn side_span(nl: &NodeLayout, side: Port) -> (f64, f64) {
    if is_vertical_port(side) {
        (
            nl.x + nl.width * SLOT_MARGIN_RATIO,
            nl.x + nl.width * (1.0 - SLOT_MARGIN_RATIO),
        )
    } else {
        (
            nl.y + nl.height * SLOT_MARGIN_RATIO,
            nl.y + nl.height * (1.0 - SLOT_MARGIN_RATIO),
        )
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

/// 平移路径在指定端的 stub 柱坐标（沿端口切线）。
fn shift_stub_column(points: &mut [Point], at_from: bool, side: Port, new_coord: f64) {
    if points.len() < 2 {
        return;
    }
    if at_from {
        let old = axis_coord(points[0], side);
        for p in points.iter_mut() {
            if (axis_coord(*p, side) - old).abs() < 1.0 {
                set_axis(p, side, new_coord);
            } else {
                break;
            }
        }
    } else {
        let last = points.len() - 1;
        let old = axis_coord(points[last], side);
        for i in (0..=last).rev() {
            if (axis_coord(points[i], side) - old).abs() < 1.0 {
                set_axis(&mut points[i], side, new_coord);
            } else {
                break;
            }
        }
    }
}

fn pick_free_coord(
    desired: f64,
    occupied: &[f64],
    min_gap: f64,
    lo: f64,
    hi: f64,
) -> Option<f64> {
    if occupied
        .iter()
        .all(|&o| (o - desired).abs() + EPS >= min_gap)
        && desired >= lo - EPS
        && desired <= hi + EPS
    {
        return Some(desired.clamp(lo, hi));
    }
    let steps = [
        1.0, -1.0, 2.0, -2.0, 3.0, -3.0, 4.0, -4.0, 5.0, -5.0, 6.0, -6.0,
    ];
    for mult in steps {
        let cand = (desired + mult * min_gap).clamp(lo, hi);
        if occupied
            .iter()
            .all(|&o| (o - cand).abs() + EPS >= min_gap)
        {
            return Some(cand);
        }
    }
    None
}

/// C 阶段：仅分离**跨无向对**且间距过近的同侧 stub（优先 exact 共柱）。
/// 正反向同对交给 `enforce_reverse_pair_min_gap`；不做整侧贪心重排（会扰动 architecture）。
pub fn resolve_stub_occupancy_conflicts(
    edges: &mut [EdgeLayout],
    relations: &[Relation],
    from_side: &[Port],
    to_side: &[Port],
    nodes: &HashMap<String, NodeLayout>,
    min_gap: f64,
) -> StubOccupancyStats {
    let mut stats = StubOccupancyStats::default();
    let records = collect_stub_occupancy(edges, relations, from_side, to_side);
    stats.records = records.len();
    let before = find_stub_occupancy_conflicts(&records, relations, min_gap);
    stats.conflict_pairs_before = before.len();
    stats.cross_pair_conflicts_before = before.iter().filter(|c| !c.reverse_pair).count();

    // 只修跨对 exact 共柱（gap≈0）；tight 交由 lane / S2 gutter，避免全图扰动
    let mut targets: Vec<&StubOccupancyConflict> = before
        .iter()
        .filter(|c| !c.reverse_pair && c.gap < 1.0)
        .collect();
    targets.sort_by(|a, b| {
        a.gap
            .partial_cmp(&b.gap)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.edge_a.cmp(&b.edge_a))
            .then_with(|| a.edge_b.cmp(&b.edge_b))
    });

    let mut shifted_edges: Vec<usize> = Vec::new();
    for c in targets {
        // 已因先前平移而不再冲突则跳过
        let live = collect_stub_occupancy(edges, relations, from_side, to_side);
        let still = find_stub_occupancy_conflicts(&live, relations, min_gap)
            .into_iter()
            .any(|x| {
                x.node_id == c.node_id
                    && x.side == c.side
                    && x.edge_a == c.edge_a
                    && x.edge_b == c.edge_b
                    && !x.reverse_pair
            });
        if !still {
            continue;
        }

        let Some(nl) = nodes.get(&c.node_id) else {
            stats.unresolved_conflicts += 1;
            stats.degraded += 1;
            continue;
        };
        let (lo, hi) = side_span(nl, c.side);

        // 占用：同侧其它 stub（不含将要移动的边）
        let move_ei = c.edge_b; // 稳定：较大 edge_index
        let occupied: Vec<f64> = live
            .iter()
            .filter(|r| r.node_id == c.node_id && r.side == c.side && r.edge_index != move_ei)
            .map(|r| r.axis_coord)
            .collect();

        let rec = live
            .iter()
            .find(|r| r.edge_index == move_ei && r.node_id == c.node_id && r.side == c.side);
        let Some(rec) = rec else {
            continue;
        };
        let Some(new_c) = pick_free_coord(rec.axis_coord, &occupied, min_gap, lo, hi) else {
            stats.unresolved_conflicts += 1;
            stats.degraded += 1;
            continue;
        };
        if (new_c - rec.axis_coord).abs() < EPS {
            continue;
        }
        let Some(edge) = edges.get_mut(move_ei) else {
            continue;
        };
        let mut pts: Vec<Point> = edge.path_points().into_owned();
        if pts.len() < 2 {
            continue;
        }
        shift_stub_column(&mut pts, rec.at_from, c.side, new_c);
        ensure_minimal_stub_len(&mut pts, rec.at_from, c.side);
        edge.set_polyline_points(pts);
        stats.stubs_shifted += 1;
        shifted_edges.push(move_ei);
    }

    let after = find_stub_occupancy_conflicts(
        &collect_stub_occupancy(edges, relations, from_side, to_side),
        relations,
        min_gap,
    );
    stats.unresolved_conflicts = after
        .iter()
        .filter(|c| !c.reverse_pair && c.gap < 1.0)
        .count();
    let _ = shifted_edges;
    stats
}

fn ensure_minimal_stub_len(points: &mut Vec<Point>, at_from: bool, side: Port) {
    use super::path::port_outward;
    if points.len() < 2 {
        return;
    }
    let (ox, oy) = port_outward(side);
    if at_from {
        let a = points[0];
        let b = points[1];
        let proj = (b.x - a.x) * ox + (b.y - a.y) * oy;
        if proj < PORT_CLEARANCE * 0.5 {
            points[1] = Point::new(a.x + ox * PORT_CLEARANCE, a.y + oy * PORT_CLEARANCE);
        }
    } else {
        let n = points.len();
        let a = points[n - 1];
        let b = points[n - 2];
        let proj = (b.x - a.x) * ox + (b.y - a.y) * oy;
        if proj < PORT_CLEARANCE * 0.5 {
            points[n - 2] = Point::new(a.x + ox * PORT_CLEARANCE, a.y + oy * PORT_CLEARANCE);
        }
    }
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
pub fn estimate_layer_band_demands(
    nodes: &HashMap<String, NodeLayout>,
    relations: &[Relation],
    parallel_gap: f64,
    label_band: f64,
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

    let layer_of: HashMap<String, usize> = layers
        .iter()
        .enumerate()
        .flat_map(|(li, ents)| ents.iter().map(move |id| (id.clone(), li)))
        .collect();
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
        let mut cross = 0usize;
        for rel in relations {
            let Some(&a) = layer_of.get(rel.from.as_str()) else {
                continue;
            };
            let Some(&b) = layer_of.get(rel.to.as_str()) else {
                continue;
            };
            if (a == li && b == li + 1) || (b == li && a == li + 1) {
                cross += 1;
            }
        }
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
        let profile = crate::layout::edge_band_demand::EdgeBandDemandProfile {
            parallel_scale: 0.5,
            fanin_scale: 0.0,
            label_band,
            label_per_edge: 0.0,
            max_extra: f64::INFINITY,
            side_channel_scale: 0.0,
            side_channel_base: 0.0,
            side_channel_max: 0.0,
        };
        // 诊断仍用粗公式（与 S0 可比）；布局写权走 edge_band_demand 完整 profile
        let demand = (cross as f64) * parallel_gap * profile.parallel_scale + profile.label_band;
        let deficit = (demand - face_gap).max(0.0);
        out.push(LayerBandDemand {
            upper_layer_y: layer_center_y[li].min(layer_center_y[li + 1]),
            lower_layer_y: layer_center_y[li].max(layer_center_y[li + 1]),
            effective_gap: face_gap,
            crossing_edges: cross,
            demand,
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
        let mut nodes = HashMap::new();
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
        let stats = resolve_stub_occupancy_conflicts(
            &mut edges,
            &relations,
            &from_side,
            &to_side,
            &nodes,
            8.0,
        );
        assert!(stats.stubs_shifted >= 1, "stats={stats:?}");
        let after = find_stub_occupancy_conflicts(
            &collect_stub_occupancy(&edges, &relations, &from_side, &to_side),
            &relations,
            8.0,
        );
        let auth_bottom: Vec<_> = after
            .iter()
            .filter(|c| c.node_id == "auth" && c.side == Port::Bottom)
            .collect();
        assert!(
            auth_bottom.is_empty(),
            "auth bottom conflicts remain: {auth_bottom:?}"
        );
    }
}
