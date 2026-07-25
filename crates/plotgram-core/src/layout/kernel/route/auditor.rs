//! 对 `RouteProblem` + 已有几何求值，产出硬约束违规清单（Phase 1）。

use super::feasibility::{HardConstraintKind, check_edge_hard_constraints};
use super::model::RouteProblem;
use crate::ast::Diagram;
use crate::layout::group::GroupRoutingContext;
use crate::layout::types::LayoutResult;
use std::collections::BTreeMap;

/// 单条硬审计违规（对外稳定结构）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteHardViolation {
    pub sample: String,
    pub edge_index: usize,
    pub from: String,
    pub to: String,
    pub kind: HardConstraintKind,
    pub detail: String,
}

/// 硬审计报告。
#[derive(Debug, Clone, Default)]
pub struct RouteHardAuditReport {
    pub sample: String,
    pub violations: Vec<RouteHardViolation>,
    pub edges_checked: usize,
    pub problem_signature: u64,
}

impl RouteHardAuditReport {
    pub fn by_kind_counts(&self) -> BTreeMap<HardConstraintKind, usize> {
        let mut m = BTreeMap::new();
        for v in &self.violations {
            *m.entry(v.kind).or_insert(0) += 1;
        }
        m
    }
}

/// 用既有生产几何对照 `RouteProblem` 跑 H0–H3。
pub fn audit_problem_against_layout(
    sample_name: &str,
    problem: &RouteProblem,
    diagram: &Diagram,
    layout: &LayoutResult,
    group_ctx: &GroupRoutingContext,
) -> RouteHardAuditReport {
    let mut sorted_node_ids: Vec<String> = layout.nodes.keys().cloned().collect();
    sorted_node_ids.sort();
    let mut sorted_group_ids: Vec<String> = layout.groups.keys().cloned().collect();
    sorted_group_ids.sort();

    let mut violations = Vec::new();
    let n = diagram.relations.len().min(layout.edges.len());
    for i in 0..n {
        let rel = &diagram.relations[i];
        let edge = &layout.edges[i];
        for ev in check_edge_hard_constraints(
            i,
            edge,
            rel.from.as_str(),
            rel.to.as_str(),
            &layout.nodes,
            &layout.groups,
            group_ctx,
            &sorted_node_ids,
            &sorted_group_ids,
        ) {
            violations.push(RouteHardViolation {
                sample: sample_name.to_string(),
                edge_index: ev.edge_index,
                from: rel.from.as_str().to_string(),
                to: rel.to.as_str().to_string(),
                kind: ev.kind,
                detail: ev.detail,
            });
        }
    }

    RouteHardAuditReport {
        sample: sample_name.to_string(),
        violations,
        edges_checked: n,
        problem_signature: problem.signature(),
    }
}
