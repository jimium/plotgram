//! 拥堵诊断指标（S0）：stub 占用冲突 + 层缝 deficit + EdgeDifficulty 摘要。
//!
//! `max_layer_deficit`：诊断口径经 `edge_band_demand` 单源（D1 对齐）。

use crate::ast::Diagram;
use crate::layout::demand::{
    collect_edge_features, compute_corridor_model, score_edges, DifficultyProfile,
};
use crate::layout::edge::edge_routing_orthogonal::{
    collect_stub_occupancy, estimate_layer_band_demands, find_stub_occupancy_conflicts,
};
use crate::layout::edge::segment_pair::parallel_gap_for_diagram;
use crate::layout::edge_band_demand::EdgeBandDemandProfile;
use crate::layout::lint::compute_lint_metrics;
use crate::layout::metrics::node_fingerprint;
use crate::layout::LayoutResult;
use serde::{Deserialize, Serialize};

const LABEL_BAND_EST: f64 = 24.0;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CongestionSampleMetrics {
    pub file: String,
    pub diagram_type: String,
    pub node_fp: String,
    pub stub_records: usize,
    pub stub_conflict_pairs: usize,
    pub stub_cross_pair_conflicts: usize,
    pub stub_exact_cross_pairs: usize,
    pub max_layer_deficit: f64,
    pub layer_bands_with_deficit: usize,
    pub ortho_stub_shifted: Option<usize>,
    pub ortho_stub_conflicts: Option<usize>,
    /// S4：边交叉违规数（lint EdgeCrossing）
    pub edge_crossing: usize,
    pub ortho_feedback_rerouted: Option<usize>,
    /// D2：边难度评分最大值
    #[serde(default)]
    pub max_edge_score: f64,
    /// D2：score ≥ p90 的边数（至少 1 条样本时）
    #[serde(default)]
    pub edges_over_score_p90: usize,
    /// D2/D3：廊级 load > capacity 条数
    #[serde(default)]
    pub corridors_over: usize,
    /// P1：obstacle_hits 总和
    #[serde(default)]
    pub total_obstacle_hits: usize,
    /// P3：grid_overflow 总和
    #[serde(default)]
    pub total_grid_overflow: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CongestionBaselineSnapshot {
    pub date: String,
    pub note: String,
    pub samples: Vec<CongestionSampleMetrics>,
}

pub fn compute_congestion_sample_metrics(
    file: &str,
    diagram: &Diagram,
    result: &LayoutResult,
) -> CongestionSampleMetrics {
    let min_gap = parallel_gap_for_diagram(diagram.diagram_type.clone());
    let from_side: Vec<_> = result.edges.iter().map(|e| e.from_port).collect();
    let to_side: Vec<_> = result.edges.iter().map(|e| e.to_port).collect();
    let records = collect_stub_occupancy(&result.edges, &diagram.relations, &from_side, &to_side);
    let conflicts = find_stub_occupancy_conflicts(&records, &diagram.relations, min_gap);
    let cross = conflicts.iter().filter(|c| !c.reverse_pair).count();
    let exact_cross = conflicts
        .iter()
        .filter(|c| !c.reverse_pair && c.gap < 1.0)
        .count();
    // 诊断口径对齐：demand 经 edge_band_demand 单源（for_layer_band_diagnosis）
    let bands = estimate_layer_band_demands(
        &result.nodes,
        &diagram.relations,
        min_gap,
        EdgeBandDemandProfile::for_layer_band_diagnosis(LABEL_BAND_EST),
    );
    let max_def = bands.iter().map(|b| b.deficit).fold(0.0_f64, f64::max);
    let def_n = bands.iter().filter(|b| b.deficit > 1.0).count();
    let ortho = result.hints.orthogonal_debug.as_ref();
    let edge_crossing = compute_lint_metrics(diagram, result).edge_crossing;

    let corridor = compute_corridor_model(diagram, result);
    let features = collect_edge_features(diagram, result, Some(&corridor));
    let scores = score_edges(&features, &DifficultyProfile::default());
    let max_edge_score = scores.first().map(|(_, s)| *s).unwrap_or(0.0);
    let edges_over_score_p90 = if scores.is_empty() {
        0
    } else {
        let p90_idx = ((scores.len() as f64) * 0.1).floor() as usize;
        let threshold = scores
            .get(p90_idx)
            .map(|(_, s)| *s)
            .unwrap_or(max_edge_score);
        scores
            .iter()
            .filter(|(_, s)| *s >= threshold - 1e-9)
            .count()
    };
    let corridors_over = corridor.demands.iter().filter(|d| d.is_over()).count();
    let total_obstacle_hits: usize = features.iter().map(|f| f.obstacle_hits).sum();
    let total_grid_overflow: usize = features.iter().map(|f| f.grid_overflow).sum();

    CongestionSampleMetrics {
        file: file.to_string(),
        diagram_type: format!("{:?}", diagram.diagram_type),
        node_fp: node_fingerprint(result),
        stub_records: records.len(),
        stub_conflict_pairs: conflicts.len(),
        stub_cross_pair_conflicts: cross,
        stub_exact_cross_pairs: exact_cross,
        max_layer_deficit: max_def,
        layer_bands_with_deficit: def_n,
        ortho_stub_shifted: ortho.map(|o| o.stub_occupancy_shifted),
        ortho_stub_conflicts: ortho.map(|o| o.stub_occupancy_conflicts),
        edge_crossing,
        ortho_feedback_rerouted: ortho.map(|o| o.feedback_rerouted_after_trunk),
        max_edge_score,
        edges_over_score_p90,
        corridors_over,
        total_obstacle_hits,
        total_grid_overflow,
    }
}
