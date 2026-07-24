//! 空间需求与路由契约的结构化分离。
//!
//! L3 目标：将 `SpaceBudget` 拆分为两个只读数据对象：
//! - [`SpacingDemandStore`]：布局求解输入（pair gaps、vertical rank gap）
//! - [`RoutingContract`]：路由只读消费（port clearance、corridor boost）
//!
//! 布局 solver 消费 SpacingDemandStore 编译出的 MinSeparation 硬约束；
//! 路由器只读 RoutingContract 决定 clearance 和走廊预算。
//! 节点冻结后不再有 budget guard 直接推点。

use std::collections::BTreeMap;

// ─── SpacingDemandStore ───────────────────────────────────────────────────────

/// 布局空间需求存储：编译进 CoordinateProblem 的输入。
///
/// 包含同层 pair 最小间距和竖直 rank 缝下界。
/// Builder 将这些需求编译为 P0 `MinSeparation` 硬约束。
#[derive(Debug, Clone, Default)]
pub struct SpacingDemandStore {
    /// 无特殊边时的默认同层间距。
    pub default_node_gap: f64,
    /// 规范化 pair `(min_id, max_id)` → 最小边距（节点外缘到外缘）。
    pub pair_gaps: BTreeMap<(String, String), f64>,
    /// 竖直 rank 缝下界；`None` 时使用 `default_node_gap`。
    pub min_vertical_rank_gap: Option<f64>,
}

impl SpacingDemandStore {
    pub fn new(default_node_gap: f64) -> Self {
        Self {
            default_node_gap,
            pair_gaps: BTreeMap::new(),
            min_vertical_rank_gap: None,
        }
    }

    /// 设置 pair 最小间距（取 max）。
    pub fn set_pair_gap(&mut self, a: &str, b: &str, gap: f64) {
        let key = canonical_pair(a, b);
        let entry = self.pair_gaps.entry(key).or_insert(self.default_node_gap);
        *entry = entry.max(gap);
    }

    /// 查询两节点外缘之间的最小间距。
    pub fn min_gap(&self, a: &str, b: &str) -> f64 {
        self.pair_gaps
            .get(&canonical_pair(a, b))
            .copied()
            .unwrap_or(self.default_node_gap)
    }

    /// 竖直 rank 缝下界。
    pub fn vertical_rank_gap(&self) -> f64 {
        self.min_vertical_rank_gap
            .unwrap_or(self.default_node_gap)
            .max(self.default_node_gap)
    }

    /// 是否有非默认的 pair 需求。
    pub fn has_custom_demands(&self) -> bool {
        !self.pair_gaps.is_empty() || self.min_vertical_rank_gap.is_some()
    }
}

// ─── RoutingContract ──────────────────────────────────────────────────────────

/// 路由只读契约：路由器消费的空间约定。
///
/// 路由器读取此对象决定 port clearance 和走廊预算，
/// 不直接修改节点坐标。
#[derive(Debug, Clone)]
pub struct RoutingContract {
    /// 端口外向 stub 长度。
    pub port_clearance: f64,
    /// 路由 0 候选时请求抬高走廊/车道预算。
    pub corridor_boost_requested: bool,
}

impl Default for RoutingContract {
    fn default() -> Self {
        Self {
            port_clearance: crate::layout::group::constants::PORT_STUB_CLEARANCE,
            corridor_boost_requested: false,
        }
    }
}

impl RoutingContract {
    pub fn new(port_clearance: f64) -> Self {
        Self {
            port_clearance,
            corridor_boost_requested: false,
        }
    }

    pub fn request_corridor_boost(&mut self) {
        self.corridor_boost_requested = true;
    }

    pub fn take_corridor_boost(&mut self) -> bool {
        let v = self.corridor_boost_requested;
        self.corridor_boost_requested = false;
        v
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn canonical_pair(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spacing_demand_store_pair_gap() {
        let mut store = SpacingDemandStore::new(40.0);
        store.set_pair_gap("a", "b", 60.0);
        assert_eq!(store.min_gap("a", "b"), 60.0);
        assert_eq!(store.min_gap("b", "a"), 60.0); // symmetric
        assert_eq!(store.min_gap("a", "c"), 40.0); // default
    }

    #[test]
    fn spacing_demand_store_vertical_gap() {
        let mut store = SpacingDemandStore::new(40.0);
        assert_eq!(store.vertical_rank_gap(), 40.0);
        store.min_vertical_rank_gap = Some(80.0);
        assert_eq!(store.vertical_rank_gap(), 80.0);
    }

    #[test]
    fn routing_contract_corridor_boost() {
        let mut contract = RoutingContract::default();
        assert!(!contract.corridor_boost_requested);
        contract.request_corridor_boost();
        assert!(contract.take_corridor_boost());
        assert!(!contract.corridor_boost_requested); // consumed
    }
}
