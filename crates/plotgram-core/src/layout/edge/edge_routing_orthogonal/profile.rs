//! 图种相关的正交路由策略预设。

#[path = "profile_flowchart.rs"]
mod profile_flowchart;
#[path = "profile_architecture.rs"]
mod profile_architecture;

use crate::types::DiagramType;

/// 路径打分权重倍率（相对 DefaultScorer 基准项）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScoringWeights {
    pub path_length: f64,
    pub bend: f64,
    pub obstacle: f64,
    pub corridor_misalignment: f64,
    pub channel_load: f64,
}

impl Default for ScoringWeights {
    fn default() -> Self {
        Self {
            path_length: 1.0,
            bend: 1.0,
            obstacle: 1.0,
            corridor_misalignment: 1.0,
            channel_load: 1.0,
        }
    }
}

/// 图种相关的正交路由策略预设（不可变配置 + 阶段开关）。
#[derive(Clone, Debug, PartialEq)]
pub struct OrthoRoutingProfile {
    pub diagram_type: DiagramType,
    /// 平行边最小间距
    pub parallel_gap: f64,
    /// 是否启用走廊规划后 lane 偏移
    pub corridor_lane_offsets: bool,
    /// 是否在 lane 阶段分离无关 trunk
    pub separate_unrelated_trunks: bool,
    /// trunk 共享是否走语义门控（architecture = true）
    pub semantic_merge: bool,
    /// 打分权重倍率
    pub scoring: ScoringWeights,
    /// 是否偏好 P1-1 trunk+fork 路径形态（flowchart fan-out）
    pub prefer_trunk_fork: bool,
}

impl OrthoRoutingProfile {
    /// 按图种选择预设；State / Er / Custom 继承 flowchart 默认。
    pub fn for_diagram_type(diagram_type: DiagramType) -> Self {
        match diagram_type {
            DiagramType::Architecture => profile_architecture::default_profile(),
            _ => profile_flowchart::default_profile(),
        }
    }

    /// 供 `edge_merge_policy::edges_may_share_trunk` 使用的等效图种门控。
    pub fn merge_policy_diagram_type(&self) -> DiagramType {
        if self.semantic_merge {
            DiagramType::Architecture
        } else {
            DiagramType::Flowchart
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::constants::ORTHO_PARALLEL_GAP;

    #[test]
    fn flowchart_profile_defaults() {
        let p = OrthoRoutingProfile::for_diagram_type(DiagramType::Flowchart);
        assert_eq!(p.diagram_type, DiagramType::Flowchart);
        assert!((p.parallel_gap - ORTHO_PARALLEL_GAP).abs() < f64::EPSILON);
        assert!(!p.semantic_merge);
        assert!(!p.corridor_lane_offsets);
        assert!(!p.separate_unrelated_trunks);
        assert!(p.prefer_trunk_fork);
    }

    #[test]
    fn architecture_profile_defaults() {
        let p = OrthoRoutingProfile::for_diagram_type(DiagramType::Architecture);
        assert_eq!(p.diagram_type, DiagramType::Architecture);
        assert!(p.parallel_gap >= ORTHO_PARALLEL_GAP);
        assert!(p.semantic_merge);
        assert!(p.corridor_lane_offsets);
        assert!(p.separate_unrelated_trunks);
        assert!(!p.prefer_trunk_fork);
    }

    #[test]
    fn state_and_custom_use_flowchart_profile() {
        let state = OrthoRoutingProfile::for_diagram_type(DiagramType::State);
        let custom = OrthoRoutingProfile::for_diagram_type(DiagramType::Custom("x".into()));
        let flow = OrthoRoutingProfile::for_diagram_type(DiagramType::Flowchart);
        assert_eq!(state.semantic_merge, flow.semantic_merge);
        assert_eq!(custom.prefer_trunk_fork, flow.prefer_trunk_fork);
    }
}
