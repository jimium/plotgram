use super::{OrthoRoutingProfile, ScoringWeights};
use crate::layout::constants::ORTHO_PARALLEL_GAP;
use crate::types::DiagramType;

pub fn default_profile() -> OrthoRoutingProfile {
    OrthoRoutingProfile {
        diagram_type: DiagramType::Flowchart,
        parallel_gap: ORTHO_PARALLEL_GAP,
        corridor_lane_offsets: false,
        separate_unrelated_trunks: false,
        semantic_merge: false,
        scoring: ScoringWeights::default(),
        prefer_trunk_fork: true,
    }
}
