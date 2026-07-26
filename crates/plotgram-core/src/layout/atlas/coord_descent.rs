//! I.7 三块有界坐标下降——Post-S7 Wave3 / AGENTS §7 创新模式记债。
//!
//! **现状**：Ordering / Gate / Channel 三块坐标尚未做有界下降；生产仍靠
//! 度量相 LP + `RelaxationLadder`（L1–L4）。
//!
//! **创新模式登记**（落地前须遵守）：
//! - 目标维度：相 II 不可行率与 product 画布紧凑度（product-gate）
//! - 可接受临时退化：stress 质量 WARN；product 穿组仍硬 0、`det=true`
//! - 退出判据：在登记样例上相对当前 LP 帕累托不劣，且最高松弛级 ≤ L1
//!
//! 本模块仅登记与类型占位，**不**改变生产求解。

/// 三块下降的目标块（22 号文 I.7）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DescentBlock {
    Ordering,
    Gate,
    Channel,
}

/// 有界下降配置占位。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoundedDescentConfig {
    pub max_rounds: u32,
    pub max_step: f64,
}

impl Default for BoundedDescentConfig {
    fn default() -> Self {
        Self {
            max_rounds: 3,
            max_step: 8.0,
        }
    }
}

/// 诊断：当前未实现，恒返回 0 轮。
pub fn run_bounded_descent_stub(_cfg: &BoundedDescentConfig) -> u32 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_runs_zero_rounds() {
        assert_eq!(run_bounded_descent_stub(&BoundedDescentConfig::default()), 0);
        assert_eq!(DescentBlock::Ordering, DescentBlock::Ordering);
    }
}
