//! Stage 3：`RelaxationLadder`（23 号文 3.6）。
//!
//! 度量相不可行时按级显式放松，每级带 [`Provenance`]，禁止静默压缩。
//! 生产挂钩：L1 缩 pitch → L2 反向相 I 边序 → L3 裙边 Demand 抬缝 → L4 Degraded。
//!
//! [`solve_main_with_ladder`](super::solve) 封装 L0–L3；flat 路径在 L2 前插入反向选路。

use super::provenance::Provenance;

/// 松弛级别（与 22 号文阶梯对齐；**勿**与 27 号文 L1–L8 合法化编号混淆）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RelaxLevel {
    /// 无放松，硬约束全满足。
    L0 = 0,
    /// 压 gate / lane pitch（`band_scale < 1`）。
    L1 = 1,
    /// 回相 I：反向边序重选路（改争用）。
    L2 = 2,
    /// 裙边 Demand 抬缝 / 启发式层顶（`inflate_layer_gaps`）。
    L3 = 3,
    /// 显式 Degraded（仍出图，带 provenance）。
    L4 = 4,
}

impl RelaxLevel {
    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

/// 单级松弛记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelaxStep {
    pub level: RelaxLevel,
    pub provenance: Provenance,
}

/// 一次度量相求解的松弛轨迹（可空 = 全程 L0）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RelaxationLadder {
    pub steps: Vec<RelaxStep>,
}

impl RelaxationLadder {
    pub fn pristine() -> Self {
        Self { steps: Vec::new() }
    }

    pub fn highest(&self) -> RelaxLevel {
        self.steps
            .iter()
            .map(|s| s.level)
            .max()
            .unwrap_or(RelaxLevel::L0)
    }

    pub fn push(&mut self, level: RelaxLevel, producer: &'static str, detail: impl Into<String>) {
        self.steps.push(RelaxStep {
            level,
            provenance: Provenance::with_detail(producer, detail),
        });
    }

    pub fn is_pristine(&self) -> bool {
        self.steps.is_empty() || self.highest() == RelaxLevel::L0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ladder_tracks_highest_level() {
        let mut lad = RelaxationLadder::pristine();
        assert!(lad.is_pristine());
        lad.push(RelaxLevel::L1, "metric/relax", "pitch");
        lad.push(RelaxLevel::L3, "metric/relax", "skirt");
        assert_eq!(lad.highest(), RelaxLevel::L3);
        assert!(!lad.is_pristine());
    }
}
