use super::{OrthoRoutingProfile, ScoringWeights};
use crate::layout::constants::ORTHO_PARALLEL_GAP;
use crate::types::DiagramType;

pub fn default_profile() -> OrthoRoutingProfile {
    // Phase 4 计划将 parallel_gap 提高到 12–16px；当前保持与全局默认一致以通过 stress-nested baseline。
    OrthoRoutingProfile {
        diagram_type: DiagramType::Architecture,
        parallel_gap: ORTHO_PARALLEL_GAP,
        corridor_lane_offsets: true,
        separate_unrelated_trunks: true,
        semantic_merge: true,
        scoring: ScoringWeights::default(),
        prefer_trunk_fork: false,
    }
}
