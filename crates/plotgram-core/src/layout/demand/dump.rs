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


