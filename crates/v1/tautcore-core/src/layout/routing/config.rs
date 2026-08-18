//! 路由算法正式配置（Slice B）。
//!
//! 原 `TAUTCORE_*` 正式环境变量硬切为此结构体字段。
//! debug/trace 类 env（`*_DEBUG`）保留原样，不纳入。

/// 路由算法正式配置。
///
/// 所有影响路由算法行为的可调参数集中于此，由 pipeline 构造后注入路由管线。
/// 不再从 `std::env` 读取正式配置（AGENTS.md §8 卫生红线：正式算法路径不读 env）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoutingConfig {
    /// 按边难度注入边序（原 `TAUTCORE_EDGE_ORDER_SCORE`，默认 true）。
    pub edge_order_score: bool,
    /// OVG 可见性图（原 `TAUTCORE_OVG_ENABLED`，默认 true）。
    pub ovg_enabled: bool,
    /// 端口求解器 v2 反向 stub 惩罚（原 `TAUTCORE_PORT_SOLVER_V2`，默认 true）。
    pub port_solver_v2: bool,
    /// 同侧 slot 压力错开（原 `TAUTCORE_PORT_PRESSURE_SLOT`，默认 true）。
    pub port_pressure_slot: bool,
    /// 廊级 OVER soft 惩罚（原 `TAUTCORE_CORRIDOR_SOFT`，默认 true）。
    pub corridor_soft: bool,
    /// 远离惩罚每 px 费率（原 `TAUTCORE_AWAY_PENALTY`，默认 2.0）。
    pub away_penalty_rate: f64,
    /// stub 出组惩罚量（原 `TAUTCORE_STUB_EXIT_PENALTY`，默认 120.0）。
    pub stub_exit_penalty: f64,
    /// 边级压力预算（原 `TAUTCORE_EDGE_PRESSURE_BUDGET`，默认 true）。
    pub edge_pressure_budget: bool,
    /// Phase 5 / D4-4：启用 ShareTrunk 语义合流（由 prepare/recipe 注入，不经 OrthoProfile）。
    pub share_trunk: bool,
}

impl Default for RoutingConfig {
    fn default() -> Self {
        Self {
            edge_order_score: true,
            ovg_enabled: true,
            port_solver_v2: true,
            port_pressure_slot: true,
            corridor_soft: true,
            away_penalty_rate: 2.0,
            stub_exit_penalty: 120.0,
            edge_pressure_budget: true,
            share_trunk: false,
        }
    }
}
