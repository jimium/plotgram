//! Env 门控 dump：边难度与 pre-route 压力。

use crate::ast::Diagram;
use crate::layout::LayoutResult;

use super::corridor::compute_corridor_model;
use super::features::{collect_edge_features, score_edges, DifficultyProfile};
use super::types::BandDemand;

/// 压力快照（诊断 / enrich 共用）。
#[derive(Debug, Clone)]
pub struct PressureSnapshot {
    pub bands: Vec<BandDemand>,
    pub corridor: super::corridor::CorridorModel,
    pub features: Vec<super::types::EdgeFeatures>,
    pub scores: Vec<(usize, f64)>,
}

impl PressureSnapshot {
    pub fn compute(diagram: &Diagram, result: &LayoutResult) -> Self {
        let corridor = compute_corridor_model(diagram, result);
        let features = collect_edge_features(diagram, result, Some(&corridor));
        let scores = score_edges(&features, &DifficultyProfile::default());
        let bands = features_to_bands_proxy(diagram, result);
        Self {
            bands,
            corridor,
            features,
            scores,
        }
    }

    pub fn max_edge_score(&self) -> f64 {
        self.scores.first().map(|(_, s)| *s).unwrap_or(0.0)
    }

    pub fn corridors_over(&self) -> usize {
        self.corridor.demands.iter().filter(|d| d.is_over()).count()
    }
}

fn features_to_bands_proxy(diagram: &Diagram, result: &LayoutResult) -> Vec<BandDemand> {
    use crate::layout::edge::edge_routing_orthogonal::estimate_layer_band_demands;
    use crate::layout::edge::segment_pair::parallel_gap_for_diagram;
    use crate::layout::edge_band_demand::EdgeBandDemandProfile;
    let parallel_gap = parallel_gap_for_diagram(diagram.diagram_type.clone());
    estimate_layer_band_demands(
        &result.nodes,
        &diagram.relations,
        parallel_gap,
        EdgeBandDemandProfile::for_layer_band_diagnosis(24.0),
    )
    .into_iter()
    .map(|b| BandDemand {
        upper_layer_y: b.upper_layer_y,
        lower_layer_y: b.lower_layer_y,
        effective_gap: b.effective_gap,
        crossing_edges: b.crossing_edges,
        demand: b.demand,
        deficit: b.deficit,
    })
    .collect()
}

pub fn dump_edge_difficulty_if_enabled(diagram: &Diagram, result: &LayoutResult) {
    if std::env::var_os("PLOTGRAM_DUMP_EDGE_DIFFICULTY").is_none() {
        return;
    }
    let snap = PressureSnapshot::compute(diagram, result);
    log_pressure_snapshot("edge-difficulty", &snap);
}

pub fn dump_pre_route_pressure_if_enabled(diagram: &Diagram, result: &LayoutResult) {
    if std::env::var_os("PLOTGRAM_DUMP_PRE_ROUTE_PRESSURE").is_none()
        && std::env::var_os("PLOTGRAM_DUMP_EDGE_DIFFICULTY").is_none()
    {
        return;
    }
    let snap = PressureSnapshot::compute(diagram, result);
    log_pressure_snapshot("pre-route-pressure", &snap);
}

/// 已算好的快照直接转储（避免 pre-route 双算）。
pub fn log_pressure_snapshot_for_pre_route(snap: &PressureSnapshot) {
    log_pressure_snapshot("pre-route-pressure", snap);
}

fn log_pressure_snapshot(tag: &str, snap: &PressureSnapshot) {
    crate::perf_log!(
        "[{}] bands={} max_deficit={:.1} corridors={} over={} cross_scope={} max_score={:.2}",
        tag,
        snap.bands.len(),
        snap.bands
            .iter()
            .map(|b| b.deficit)
            .fold(0.0_f64, f64::max),
        snap.corridor.demands.len(),
        snap.corridors_over(),
        snap.corridor.cross_scope_edges,
        snap.max_edge_score()
    );
    for (idx, score) in snap.scores.iter().take(16) {
        if let Some(f) = snap.features.iter().find(|f| f.edge_index == *idx) {
            crate::perf_log!(
                "  edge[{}] score={:.2} {}→{} span={} risk={:?} band_def={:.1} corr_ov={:.0} port={} hits={} grid_ov={}",
                idx,
                score,
                f.from,
                f.to,
                f.span_ranks,
                f.corridor_risk,
                f.band_deficit,
                f.corridor_overflow,
                f.port_pressure,
                f.obstacle_hits,
                f.grid_overflow
            );
        }
    }
    for c in snap
        .corridor
        .demands
        .iter()
        .filter(|c| c.load > 0)
        .take(12)
    {
        let flag = if c.capacity == 0 {
            " DEGEN"
        } else if c.is_over() {
            " OVER"
        } else {
            ""
        };
        crate::perf_log!(
            "  corridor[{}] {:?} {}↔{} load={}/{} gap={:.0}{}",
            c.corridor_index,
            c.axis,
            c.group_a,
            c.group_b,
            c.load,
            c.capacity,
            c.gap,
            flag
        );
    }
}
