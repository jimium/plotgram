//! Gate 分配（MCF）记债骨架——Post-S7 Wave3 / AGENTS §7 创新模式。
//!
//! **现状**：生产路径用 [`super::channel::Occupancy`] 顺序 `commit` +
//! [`GateCapacity::Unbounded`]（D6：容量是输出）。完整最小费用流 Gate 分配
//! 尚未实现。
//!
//! **创新模式登记**（落地前须遵守）：
//! - 目标维度：多 gate 争用下的相 I 可行性与 lane demand 公平性（product-gate）
//! - 可接受临时退化：stress 质量 WARN；product 穿组仍硬 0
//! - 退出判据： Occupancy 顺序敏感性用例上 MCF 帕累托不劣，且 det=true
//!
//! 本模块仅提供类型占位与诊断钩子，**不**改变生产选路。

use super::channel::{EdgeId, GateId};
use std::collections::BTreeMap;

/// 一条边在各 gate 上的分配权重（未来 MCF 流量）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GateAssignment {
    /// gate → 流量（当前 stub 恒 1.0 当边穿越该 gate）
    pub flow: BTreeMap<GateId, f64>,
}

/// 从 Plan.gates 构造平凡赋值（每穿越 gate 流量 1）——诊断用，非 MCF。
pub fn trivial_assignment_from_gates(
    gates: &BTreeMap<EdgeId, Vec<GateId>>,
) -> BTreeMap<EdgeId, GateAssignment> {
    let mut out = BTreeMap::new();
    for (&eid, gs) in gates {
        let mut flow = BTreeMap::new();
        for &g in gs {
            *flow.entry(g).or_insert(0.0) += 1.0;
        }
        out.insert(eid, GateAssignment { flow });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::atlas::channel::GateId;

    #[test]
    fn trivial_assignment_counts_crossings() {
        let mut gates = BTreeMap::new();
        gates.insert(0, vec![GateId(1), GateId(2)]);
        gates.insert(1, vec![GateId(1)]);
        let a = trivial_assignment_from_gates(&gates);
        assert_eq!(a[&0].flow[&GateId(1)], 1.0);
        assert_eq!(a[&0].flow[&GateId(2)], 1.0);
        assert_eq!(a[&1].flow[&GateId(1)], 1.0);
    }
}
