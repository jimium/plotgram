//! D1.1 TrackOrder (L3): colour overlapping spans on each substrate track.
//!
//! End-bus BundlePlan members are skipped (intentional bus collinearity).
//! SharedCorridor members share one track_index on declared shared tracks.
//! Cross-track lane counts feed LayerGap Demand via the track's rank-gap line.

use std::collections::BTreeMap;

use plotgram_algo::interval_color::{color_intervals, Interval};
use plotgram_model::geometry::Rect;

use crate::layout::hierarchical::channel::{
    ChannelPath, ChannelRoutePlan, RouteTopology, TrackId, TrackOrient,
};
use crate::layout::hierarchical::compose::bundle::{end_bus_edge_ids, BundleKind, BundlePlan};
use crate::layout::hierarchical::compose::ports::EdgePorts;
use crate::layout::hierarchical::metric::anchor::port_anchor;
use crate::layout::hierarchical::model::{ElemKey, PlanGraph, RealGraph};

const JOG_EPS: f64 = 1e-6;

/// L3 assignment for one edge on one substrate track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HopTrack {
    pub track: TrackId,
    pub track_index: u32,
}

/// TrackOrder plan (indices only — pixels are Metric).
#[derive(Debug, Default, Clone)]
pub struct TrackOrderPlan {
    /// `(edge_id, track_id)` → lane index on that substrate track.
    pub assignments: BTreeMap<(String, TrackId), HopTrack>,
    /// Substrate track → number of lanes.
    pub track_counts: BTreeMap<TrackId, usize>,
    /// RankGap(r) → max concurrent lanes on Cross line r+1 (for Demand).
    pub rank_gap_track_counts: BTreeMap<u32, usize>,
}

fn endpoint_cross(
    plan: &PlanGraph,
    graph: &RealGraph,
    ports: &BTreeMap<String, EdgePorts>,
    frames: &[Rect],
    edge_id: &str,
    at_source: bool,
) -> f64 {
    let edge = graph
        .edges
        .iter()
        .find(|e| e.edge_id == edge_id)
        .expect("edge must exist");
    let node_idx = if at_source {
        edge.original_source
    } else {
        edge.original_target
    };
    let id = &graph.ids[node_idx];
    let elem = plan.index_of[&ElemKey::Real(id.clone())];
    let rp = &ports[edge_id];
    let port = if at_source { rp.source } else { rp.target };
    port_anchor(frames[elem], port).x
}

/// Assign L3 lanes from Channel paths + pixel spans.
pub fn assign_track_order(
    plan: &PlanGraph,
    graph: &RealGraph,
    ports: &BTreeMap<String, EdgePorts>,
    frames: &[Rect],
    bundles: &[BundlePlan],
    route_plan: &ChannelRoutePlan,
) -> TrackOrderPlan {
    let bus_edges = end_bus_edge_ids(bundles);

    // Group edge spans by substrate track (Cross tracks only for L3 pitch
    // demand; Main tracks get lanes too for Ink vertical corridors).
    let mut by_track: BTreeMap<TrackId, Vec<(String, f64, f64)>> = BTreeMap::new();
    for (edge_id, topo) in &route_plan.routes {
        if bus_edges.contains(edge_id) {
            continue;
        }
        let RouteTopology::Orthogonal(ChannelPath { tracks, .. }) = topo;
        let ax = endpoint_cross(plan, graph, ports, frames, edge_id, true);
        let bx = endpoint_cross(plan, graph, ports, frames, edge_id, false);
        let lo = ax.min(bx);
        let hi = ax.max(bx);
        for &tid in tracks {
            let Some(t) = route_plan.substrate.track(tid) else {
                continue;
            };
            match t.orient {
                TrackOrient::Cross => {
                    if (hi - lo).abs() < JOG_EPS {
                        // Straight vertical through this gap — still occupies
                        // one lane slot for Demand bookkeeping only when other
                        // edges share the rail; zero-width intervals may share.
                        by_track.entry(tid).or_default().push((edge_id.clone(), lo, hi));
                    } else {
                        by_track.entry(tid).or_default().push((edge_id.clone(), lo, hi));
                    }
                }
                TrackOrient::Main => {
                    // Vertical corridor: interval along main uses endpoint Y
                    // span as a soft separator (degenerate → share lane 0).
                    let ay = endpoint_main_y(plan, graph, ports, frames, edge_id, true);
                    let by = endpoint_main_y(plan, graph, ports, frames, edge_id, false);
                    by_track
                        .entry(tid)
                        .or_default()
                        .push((edge_id.clone(), ay.min(by), ay.max(by)));
                }
            }
        }
    }

    let mut assignments = BTreeMap::new();
    let mut track_counts = BTreeMap::new();
    let mut rank_gap_track_counts: BTreeMap<u32, usize> = BTreeMap::new();

    for (tid, members) in &by_track {
        let intervals: Vec<Interval> = members
            .iter()
            .map(|(_, lo, hi)| Interval::new(*lo, *hi))
            .collect();
        let colors = color_intervals(&intervals, 0.0);
        let count = colors.iter().copied().max().map_or(0, |m| m + 1);
        track_counts.insert(*tid, count);
        for (i, (edge_id, _, _)) in members.iter().enumerate() {
            assignments.insert(
                (edge_id.clone(), *tid),
                HopTrack {
                    track: *tid,
                    track_index: colors[i] as u32,
                },
            );
        }
        if let Some(t) = route_plan.substrate.track(*tid) {
            if matches!(t.orient, TrackOrient::Cross) {
                // Cross line k (1..=rank_count) sits in RankGap(k-1).
                if t.line >= 1 && t.line <= route_plan.index.rank_count {
                    let gap = (t.line - 1) as u32;
                    let e = rank_gap_track_counts.entry(gap).or_insert(0);
                    *e = (*e).max(count);
                }
            }
        }
    }

    // SharedCorridor: force members onto one lane per declared shared track
    // (intentional collinearity; Verifier exemption key).
    for bundle in bundles {
        if bundle.kind != BundleKind::SharedCorridor {
            continue;
        }
        for &tid_raw in &bundle.shared_track_ids {
            let tid = TrackId(tid_raw);
            let mut indices = Vec::new();
            for eid in &bundle.member_edges {
                if let Some(ht) = assignments.get(&(eid.clone(), tid)) {
                    indices.push(ht.track_index);
                }
            }
            if indices.is_empty() {
                continue;
            }
            let shared = *indices.iter().min().unwrap();
            for eid in &bundle.member_edges {
                if let Some(ht) = assignments.get_mut(&(eid.clone(), tid)) {
                    ht.track_index = shared;
                }
            }
        }
    }

    TrackOrderPlan {
        assignments,
        track_counts,
        rank_gap_track_counts,
    }
}

fn endpoint_main_y(
    plan: &PlanGraph,
    graph: &RealGraph,
    ports: &BTreeMap<String, EdgePorts>,
    frames: &[Rect],
    edge_id: &str,
    at_source: bool,
) -> f64 {
    let edge = graph
        .edges
        .iter()
        .find(|e| e.edge_id == edge_id)
        .expect("edge must exist");
    let node_idx = if at_source {
        edge.original_source
    } else {
        edge.original_target
    };
    let id = &graph.ids[node_idx];
    let elem = plan.index_of[&ElemKey::Real(id.clone())];
    let rp = &ports[edge_id];
    let port = if at_source { rp.source } else { rp.target };
    port_anchor(frames[elem], port).y
}
