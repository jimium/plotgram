//! 正交路由策略预设（Phase 6：无 `DiagramType`——由图种无关标量/开关表达）。

/// 路径打分权重倍率（相对 DefaultScorer 基准项）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScoringWeights {
    pub path_length: f64,
    pub bend: f64,
    pub obstacle: f64,
    pub corridor_misalignment: f64,
    /// P1-2: 通道对齐软约束权重
    pub channel_alignment: f64,
    /// P2-2: 交叉惩罚权重
    pub crossing: f64,
    /// A-2：远离惩罚权重
    pub away: f64,
}

impl Default for ScoringWeights {
    fn default() -> Self {
        Self {
            path_length: 1.0,
            bend: 1.0,
            obstacle: 1.0,
            corridor_misalignment: 1.0,
            channel_alignment: 1.0,
            crossing: 1.0,
            away: 1.0,
        }
    }
}

/// 正交路由策略预设（不可变配置 + 阶段开关）。
///
/// Phase 6：不再持有 `DiagramType`；architecture 族由 [`Self::architecture`] /
/// `arch_family` 标量表达，由 recipe 在 ortho 边界外解析图种后注入。
#[derive(Clone, Debug, PartialEq)]
pub struct OrthoRoutingProfile {
    /// 是否 architecture 族预设（平行缝更大、语义合流、分离无关 trunk）。
    pub arch_family: bool,
    /// 平行边最小间距
    pub parallel_gap: f64,
    /// 是否在 lane 阶段分离无关 trunk
    pub separate_unrelated_trunks: bool,
    /// trunk 共享是否走语义门控
    pub semantic_merge: bool,
    /// 打分权重倍率
    pub scoring: ScoringWeights,
    /// 是否偏好 trunk+fork 路径形态
    pub prefer_trunk_fork: bool,
}

impl OrthoRoutingProfile {
    /// flowchart / 默认族。
    pub fn flowchart() -> Self {
        Self {
            arch_family: false,
            parallel_gap: crate::layout::constants::ORTHO_PARALLEL_GAP,
            separate_unrelated_trunks: false,
            semantic_merge: false,
            scoring: ScoringWeights::default(),
            prefer_trunk_fork: true,
        }
    }

    /// architecture 族。
    pub fn architecture() -> Self {
        Self {
            arch_family: true,
            parallel_gap: crate::layout::constants::ORTHO_PARALLEL_GAP_ARCHITECTURE,
            separate_unrelated_trunks: true,
            semantic_merge: true,
            scoring: ScoringWeights {
                path_length: 1.0,
                bend: 1.0,
                obstacle: 1.5,
                corridor_misalignment: 1.2,
                channel_alignment: 1.0,
                crossing: 1.0,
                away: 1.0,
            },
            prefer_trunk_fork: false,
        }
    }

    /// 由 recipe/契约注入：ShareTrunk / arch 预设。
    pub fn from_arch_family(arch_family: bool) -> Self {
        if arch_family {
            Self::architecture()
        } else {
            Self::flowchart()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::constants::{ORTHO_PARALLEL_GAP, ORTHO_PARALLEL_GAP_ARCHITECTURE};

    #[test]
    fn flowchart_profile_defaults() {
        let p = OrthoRoutingProfile::flowchart();
        assert!(!p.arch_family);
        assert!((p.parallel_gap - ORTHO_PARALLEL_GAP).abs() < f64::EPSILON);
        assert!(!p.semantic_merge);
        assert!(!p.separate_unrelated_trunks);
        assert!(p.prefer_trunk_fork);
    }

    #[test]
    fn architecture_profile_defaults() {
        let p = OrthoRoutingProfile::architecture();
        assert!(p.arch_family);
        assert!((p.parallel_gap - ORTHO_PARALLEL_GAP_ARCHITECTURE).abs() < f64::EPSILON);
        assert!(p.parallel_gap > ORTHO_PARALLEL_GAP);
        assert!(p.semantic_merge);
        assert!(p.separate_unrelated_trunks);
        assert!(!p.prefer_trunk_fork);
    }
}
