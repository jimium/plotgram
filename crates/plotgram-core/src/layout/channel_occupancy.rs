//! B2 通道占用只读诊断（候选架构，先观测后决策）。
//!
//! - 路由后对照：跨 leaf 边是否有走廊链、走廊负载是否超过 **有效间隙** 容量。
//! - 与 lint 穿组 / 穿节点实际 dirty 边做命中率对照。
//! - **禁止改几何**；默认零输出，`PLOTGRAM_DUMP_CHANNEL_OCCUPANCY` 置位时 `perf_log`。
//!
//! ## 容量模型（v1）
//! 实现见 [`crate::layout::demand::corridor`]；本模块只做 lint 对照包装。

use crate::ast::Diagram;
use crate::layout::demand::{
    compute_corridor_model, corridor_capacity_v1, CorridorDemand,
};
use crate::layout::{LayoutResult};
use std::collections::HashSet;

/// 与 demand::CorridorDemand 同构（对外 API 稳定名）。
pub type CorridorOccupancy = CorridorDemand;

/// 单条边的占用风险标签。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ChannelRiskKind {
    /// 跨 leaf 但走廊邻接图上无链
    NoCorridorChain,
    /// 走廊链上存在超容廊段
    OverloadedCorridor,
}

/// 预测或实际的边级风险记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelEdgeRisk {
    pub edge_index: usize,
    pub from: String,
    pub to: String,
    pub from_leaf: Option<String>,
    pub to_leaf: Option<String>,
    pub kind: ChannelRiskKind,
}

/// B2 只读诊断报告。
#[derive(Debug, Clone, PartialEq)]
pub struct ChannelOccupancyReport {
    pub corridors: usize,
    pub cross_scope_edges: usize,
    pub with_chain: usize,
    pub predicted: Vec<ChannelEdgeRisk>,
    pub corridor_loads: Vec<CorridorOccupancy>,
    pub actual_group_interior: Vec<(usize, String, String)>,
    pub actual_through: Vec<(usize, String, String)>,
    /// 实际穿组边中，被预测命中的条数
    pub group_hit: usize,
    /// 实际穿节点边中，被预测命中的条数
    pub through_hit: usize,
    /// 实际穿组且**已有**走廊链（说明问题不在缺链，而在未走廊/廊不可执行）
    pub group_with_chain: usize,
    /// 实际穿组且**无**走廊链
    pub group_no_chain: usize,
}

impl ChannelOccupancyReport {
    pub fn group_hit_rate(&self) -> Option<f64> {
        if self.actual_group_interior.is_empty() {
            None
        } else {
            Some(self.group_hit as f64 / self.actual_group_interior.len() as f64)
        }
    }
}

/// 计算通道占用只读报告（不改 `result`）。
pub fn compute_channel_occupancy_report(
    diagram: &Diagram,
    result: &LayoutResult,
) -> ChannelOccupancyReport {
    let model = compute_corridor_model(diagram, result);
    let severe = model.severe_overloaded_indices();

    let mut predicted: Vec<ChannelEdgeRisk> = Vec::new();
    for &edge_index in &model.no_chain_edges {
        let rel = &diagram.relations[edge_index];
        predicted.push(ChannelEdgeRisk {
            edge_index,
            from: rel.from.as_str().to_string(),
            to: rel.to.as_str().to_string(),
            from_leaf: None,
            to_leaf: None,
            kind: ChannelRiskKind::NoCorridorChain,
        });
    }
    for (edge_index, chain) in &model.edge_chains {
        if chain.iter().any(|c| severe.contains(c)) {
            let rel = &diagram.relations[*edge_index];
            predicted.push(ChannelEdgeRisk {
                edge_index: *edge_index,
                from: rel.from.as_str().to_string(),
                to: rel.to.as_str().to_string(),
                from_leaf: None,
                to_leaf: None,
                kind: ChannelRiskKind::OverloadedCorridor,
            });
        }
    }
    predicted.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| a.edge_index.cmp(&b.edge_index))
    });
    let predicted_idx: HashSet<usize> = predicted.iter().map(|r| r.edge_index).collect();

    let actual_group_interior = collect_actual_group_interior(diagram, result);
    let actual_through = collect_actual_through(diagram, result);
    let group_hit = actual_group_interior
        .iter()
        .filter(|(i, _, _)| predicted_idx.contains(i))
        .count();
    let through_hit = actual_through
        .iter()
        .filter(|(i, _, _)| predicted_idx.contains(i))
        .count();
    let mut group_with_chain = 0usize;
    let mut group_no_chain = 0usize;
    for (i, _, _) in &actual_group_interior {
        if model.edge_chains.contains_key(i) {
            group_with_chain += 1;
        } else {
            group_no_chain += 1;
        }
    }

    // 填 leaf 信息（报告可读性）
    let algo = crate::layout::group::routing_algo_for_diagram(diagram);
    let group_ctx = crate::layout::group::GroupRoutingContext::from_layout(diagram, result, algo);
    for r in &mut predicted {
        r.from_leaf = group_ctx
            .node_leaf_group(r.from.as_str())
            .map(str::to_string);
        r.to_leaf = group_ctx.node_leaf_group(r.to.as_str()).map(str::to_string);
    }

    ChannelOccupancyReport {
        corridors: model.corridors.len(),
        cross_scope_edges: model.cross_scope_edges,
        with_chain: model.with_chain,
        predicted,
        corridor_loads: model.demands,
        actual_group_interior,
        actual_through,
        group_hit,
        through_hit,
        group_with_chain,
        group_no_chain,
    }
}

/// Env 门控转储；默认零成本。
pub fn dump_channel_occupancy_if_enabled(diagram: &Diagram, result: &LayoutResult) {
    if std::env::var_os("PLOTGRAM_DUMP_CHANNEL_OCCUPANCY").is_none() {
        return;
    }
    let report = compute_channel_occupancy_report(diagram, result);
    log_channel_occupancy_report(&report);
}

fn log_channel_occupancy_report(report: &ChannelOccupancyReport) {
    let hit = report
        .group_hit_rate()
        .map(|r| format!("{:.0}%", r * 100.0))
        .unwrap_or_else(|| "n/a".into());
    crate::perf_log!(
        "[channel-occupancy] corridors={} cross_scope={} with_chain={} predicted={} \
         actual_group={} (hit {}/{}, {}; with_chain={} no_chain={}) actual_through={} (hit {}/{})",
        report.corridors,
        report.cross_scope_edges,
        report.with_chain,
        report.predicted.len(),
        report.actual_group_interior.len(),
        report.group_hit,
        report.actual_group_interior.len(),
        hit,
        report.group_with_chain,
        report.group_no_chain,
        report.actual_through.len(),
        report.through_hit,
        report.actual_through.len()
    );
    for r in report.predicted.iter().take(24) {
        crate::perf_log!(
            "  predict[{}] {:?} {}→{} leaf={:?}/{:?}",
            r.edge_index,
            r.kind,
            r.from,
            r.to,
            r.from_leaf,
            r.to_leaf
        );
    }
    for c in report
        .corridor_loads
        .iter()
        .filter(|c| c.load > 0)
        .take(12)
    {
        let flag = if c.capacity == 0 {
            " DEGEN"
        } else if c.load > c.capacity {
            " OVER"
        } else {
            ""
        };
        crate::perf_log!(
            "  corridor[{}] {:?} {}↔{} load={}/{} gap={:.0} span={:.0}{}",
            c.corridor_index,
            c.axis,
            c.group_a,
            c.group_b,
            c.load,
            c.capacity,
            c.gap,
            c.span,
            flag
        );
    }
    for (i, from, to) in report.actual_group_interior.iter().take(16) {
        crate::perf_log!("  actual_group[{}] {}→{}", i, from, to);
    }
}

fn collect_actual_group_interior(
    diagram: &Diagram,
    result: &LayoutResult,
) -> Vec<(usize, String, String)> {
    collect_lint_edges(diagram, result, crate::layout::lint::LintRuleId::EdgeCrossesGroupInterior)
}

fn collect_actual_through(
    diagram: &Diagram,
    result: &LayoutResult,
) -> Vec<(usize, String, String)> {
    collect_lint_edges(diagram, result, crate::layout::lint::LintRuleId::EdgeThroughNode)
}

fn collect_lint_edges(
    diagram: &Diagram,
    result: &LayoutResult,
    rule: crate::layout::lint::LintRuleId,
) -> Vec<(usize, String, String)> {
    let report = crate::layout::lint::lint_layout(diagram, result);
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for v in &report.violations {
        if v.rule != rule {
            continue;
        }
        let Some(edge_index) = v.edge_index else {
            continue;
        };
        if !seen.insert(edge_index) {
            continue;
        }
        if let Some(rel) = diagram.relations.get(edge_index) {
            out.push((
                edge_index,
                rel.from.as_str().to_string(),
                rel.to.as_str().to_string(),
            ));
        }
    }
    out.sort_by_key(|(i, _, _)| *i);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{GroupLayout, LayoutHints, NodeLayout};
    use std::collections::HashMap;

    #[test]
    fn empty_layout_report_is_zero() {
        let diagram = Diagram::default();
        let result = LayoutResult {
            nodes: HashMap::new(),
            groups: HashMap::new(),
            edges: vec![],
            total_width: 0.0,
            total_height: 0.0,
            hints: LayoutHints::default(),
        };
        let r = compute_channel_occupancy_report(&diagram, &result);
        assert_eq!(r.corridors, 0);
        assert_eq!(r.cross_scope_edges, 0);
        assert!(r.predicted.is_empty());
    }

    #[test]
    fn capacity_uses_gap_not_span() {
        let (cap, degen) = corridor_capacity_v1(80.0, 33.0);
        assert!(!degen);
        assert_eq!(cap, 4);
        let (cap_thin, degen_thin) = corridor_capacity_v1(10.0, 100.0);
        assert!(degen_thin);
        assert_eq!(cap_thin, 0);
        let (cap_short_span, degen_ss) = corridor_capacity_v1(80.0, 10.0);
        assert!(degen_ss);
        assert_eq!(cap_short_span, 0);
    }

    #[test]
    fn dump_noop_without_env() {
        let diagram = Diagram::default();
        let result = LayoutResult {
            nodes: HashMap::from([(
                "a".into(),
                NodeLayout {
                    x: 0.0,
                    y: 0.0,
                    width: 10.0,
                    height: 10.0,
                },
            )]),
            groups: HashMap::from([(
                "g".into(),
                GroupLayout {
                    x: 0.0,
                    y: 0.0,
                    width: 20.0,
                    height: 20.0,
                },
            )]),
            edges: vec![],
            total_width: 40.0,
            total_height: 40.0,
            hints: LayoutHints::default(),
        };
        dump_channel_occupancy_if_enabled(&diagram, &result);
    }
}
