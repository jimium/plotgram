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
        // Iteration 2：提高障碍权重，强化穿组/擦边代价
        scoring: ScoringWeights {
            path_length: 1.0,
            bend: 1.0,
            obstacle: 1.5,
            corridor_misalignment: 1.2,
            channel_load: 1.0,
        },
        prefer_trunk_fork: false,
    }
}
