//! 跨管线共享的代价与求解状态词汇。
//!
//! 这些类型**无几何、跨管线通用**，同时服务：
//! - 旧路由内核 [`crate::layout::kernel::route`]（`lex_astar` 的词典序代价）
//! - 坐标求解器 [`crate::layout::kernel::coordinate`]（`SolverStatus`）
//! - 新架构 [`crate::layout::atlas`]（通道图选路代价）
//!
//! **归属说明**：本模块住在中立的 `kernel` 层、不属于任何将被删除的旧管线。
//! 旧 `route::model` 与 `coordinate::model` 均从此处重导出——Atlas 直接依赖本模块，
//! 不依赖 `kernel/route`，因此 Stage 4c 删除旧路由内核时共享词汇安然无恙。

use std::fmt;

/// 可比较的有序 f64（NaN 视为最大，保证确定性全序）。
#[derive(Debug, Clone, Copy)]
pub struct OrderedF64(pub f64);

impl PartialEq for OrderedF64 {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}

impl Eq for OrderedF64 {}

impl PartialOrd for OrderedF64 {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OrderedF64 {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

/// 词典序代价：高位优先，低位不得破坏高位。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct LexCost {
    /// Q1：硬约束残差（理想为 0；>0 表示降级解）。
    pub q1_hard_residual: OrderedF64,
    pub q2_crossings: u32,
    pub q3_bends: u32,
    pub q4_length: OrderedF64,
    pub q5_alignment: OrderedF64,
    pub q6_symmetry: OrderedF64,
}

impl Default for LexCost {
    fn default() -> Self {
        Self {
            q1_hard_residual: OrderedF64(0.0),
            q2_crossings: 0,
            q3_bends: 0,
            q4_length: OrderedF64(0.0),
            q5_alignment: OrderedF64(0.0),
            q6_symmetry: OrderedF64(0.0),
        }
    }
}

/// 求解状态（路由 / 坐标 / 通道图共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolverStatus {
    /// 正常收敛。
    Converged,
    /// 未收敛但返回最佳可行解。
    Degraded,
    /// 硬约束不可行。
    Infeasible,
}

impl fmt::Display for SolverStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Converged => write!(f, "converged"),
            Self::Degraded => write!(f, "degraded"),
            Self::Infeasible => write!(f, "infeasible"),
        }
    }
}
