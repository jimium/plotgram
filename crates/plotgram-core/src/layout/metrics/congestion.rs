//! 拥堵诊断指标（S0）：stub 占用冲突 + 层缝 deficit。

use crate::ast::Diagram;
use crate::layout::edge::edge_routing_orthogonal::{
    collect_stub_occupancy, estimate_layer_band_demands, find_stub_occupancy_conflicts,
};
use crate::layout::edge::segment_pair::parallel_gap_for_diagram;
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
    let bands = estimate_layer_band_demands(&result.nodes, &diagram.relations, min_gap, LABEL_BAND_EST);
    let max_def = bands.iter().map(|b| b.deficit).fold(0.0_f64, f64::max);
    let def_n = bands.iter().filter(|b| b.deficit > 1.0).count();
    let ortho = result.hints.orthogonal_debug.as_ref();
    let edge_crossing = compute_lint_metrics(diagram, result).edge_crossing;

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
    }
}
