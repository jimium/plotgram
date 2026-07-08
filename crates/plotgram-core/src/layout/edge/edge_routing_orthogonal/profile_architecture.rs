use super::{OrthoRoutingProfile, ScoringWeights};
use crate::layout::constants::ORTHO_PARALLEL_GAP_ARCHITECTURE;
use crate::types::DiagramType;

pub fn default_profile() -> OrthoRoutingProfile {
    OrthoRoutingProfile {
        diagram_type: DiagramType::Architecture,
        parallel_gap: ORTHO_PARALLEL_GAP_ARCHITECTURE,
        corridor_lane_offsets: true,
        separate_unrelated_trunks: true,
        semantic_merge: true,
        scoring: ScoringWeights::default(),
        prefer_trunk_fork: false,
    }
}
