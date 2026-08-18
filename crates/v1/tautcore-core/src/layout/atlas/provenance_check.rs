//! Plan 边级 Provenance 覆盖率（Stage 7 / 23 号文 §9）。
//!
//! Hierarchical Ink 路径：`plan.channels` 的每条成功边必须有配套
//! `provenance`（及 `gates`）。缺口不得静默——调用方应 `?` 或显式
//! Degraded，禁止在缺溯源时落笔。

use super::channel::EdgeId;
use super::plan::{Plan, PlanError};

/// 断言 channel 边的 provenance 覆盖率 = 100%，并跑 [`Plan::validate`]。
///
/// Ink [`super::ink::materialize_edges`] 之前调用。
pub fn assert_channel_provenance_coverage(plan: &Plan) -> Result<(), PlanError> {
    let missing = missing_channel_provenance(plan);
    if let Some(&edge) = missing.first() {
        return Err(PlanError::MissingEdgeRecord(edge, "provenance"));
    }
    plan.validate()
}

/// 返回 `channels` 有边但缺 `provenance` 的 EdgeId（升序，确定性）。
pub fn missing_channel_provenance(plan: &Plan) -> Vec<EdgeId> {
    plan.channels
        .keys()
        .copied()
        .filter(|e| !plan.provenance.contains_key(e))
        .collect()
}

/// 覆盖率：有 provenance 的 channel 边数 / channel 边总数。
pub fn channel_provenance_coverage_ratio(plan: &Plan) -> f64 {
    let n = plan.channels.len();
    if n == 0 {
        return 1.0;
    }
    let covered = plan
        .channels
        .keys()
        .filter(|e| plan.provenance.contains_key(e))
        .count();
    covered as f64 / n as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::algorithm_config::SugiyamaLayoutConfig;
    use crate::layout::atlas::channel::verify_route_scope;
    use crate::layout::atlas::channel::TrackId;
    use crate::layout::atlas::plan::Provenance;
    use crate::layout::atlas::solve::solve_atlas_with_config;
    use crate::prepare::StyleRequest;

    fn parse_diagram(source: &str) -> crate::ast::PreparedDiagram {
        let output = crate::pipeline::parse_prepare_validate(source, &StyleRequest::default());
        assert!(output.is_valid(), "{:?}", output.errors);
        output.diagram.expect("diagram")
    }

    fn endpoint_scope(
        substrate: &crate::layout::atlas::channel::Substrate,
        plan: &Plan,
        edge: EdgeId,
        from: bool,
    ) -> Option<crate::layout::atlas::channel::GroupId> {
        let ep = plan.ports.get(&edge)?;
        let slot_id = if from {
            ep.from.slot_id
        } else {
            ep.to.slot_id
        }?;
        let port = substrate.port(slot_id)?;
        substrate.track(port.track).and_then(|t| t.scope)
    }

    #[test]
    fn empty_plan_coverage_is_full() {
        let plan = Plan::default();
        assert_eq!(channel_provenance_coverage_ratio(&plan), 1.0);
        assert!(assert_channel_provenance_coverage(&plan).is_ok());
        assert!(missing_channel_provenance(&plan).is_empty());
    }

    #[test]
    fn missing_provenance_is_detected() {
        let mut plan = Plan::default();
        plan.channels.insert(3, vec![TrackId(0)]);
        plan.gates.insert(3, vec![]);
        assert_eq!(missing_channel_provenance(&plan), vec![3]);
        assert!(assert_channel_provenance_coverage(&plan).is_err());
        plan.provenance.insert(3, Provenance::ChannelRoute);
        assert!(assert_channel_provenance_coverage(&plan).is_ok());
        assert_eq!(channel_provenance_coverage_ratio(&plan), 1.0);
    }

    /// Hierarchical Ink：product flowchart 的 atlas_plan 覆盖率 100%。
    #[test]
    fn product_flowchart_channel_provenance_full() {
        let cases = [
            include_str!("../../../../../showcase/flowchart/product.linear-chain.taut"),
            include_str!("../../../../../showcase/flowchart/product.user-auth.taut"),
            include_str!("../../../../../showcase/flowchart/product.password-reset.taut"),
            include_str!("../../../../../showcase/flowchart/product.symmetric-fanout.taut"),
            include_str!("../../../../../showcase/flowchart/product.self-loop-retry.taut"),
        ];
        for source in cases {
            let prepared = parse_diagram(source);
            let layout = crate::layout::compute_layout(prepared.inner()).expect("layout");
            let plan = layout
                .hints
                .atlas_plan
                .as_ref()
                .expect("hierarchical atlas_plan");
            assert_channel_provenance_coverage(plan)
                .unwrap_or_else(|e| panic!("provenance gap: {e:?}"));
            assert_eq!(channel_provenance_coverage_ratio(plan), 1.0);
        }
    }

    /// 穿组由构造保证：L6 + `verify_route_scope` 对 product flowchart 硬断言。
    #[test]
    fn product_flowchart_no_group_penetration() {
        let cases = [
            include_str!("../../../../../showcase/flowchart/product.linear-chain.taut"),
            include_str!("../../../../../showcase/flowchart/product.user-auth.taut"),
            include_str!("../../../../../showcase/flowchart/product.refund-process.taut"),
            include_str!("../../../../../showcase/flowchart/product.swimlane-order-process.taut"),
            include_str!("../../../../../showcase/flowchart/product.leave-approval-process.taut"),
        ];
        let config = SugiyamaLayoutConfig::default();
        for source in cases {
            let prepared = parse_diagram(source);
            let name = prepared.inner().title().unwrap_or("<untitled>");
            let output = solve_atlas_with_config(prepared.inner(), &config);
            let Some(metric) = output.channel.as_ref() else {
                continue;
            };
            assert_channel_provenance_coverage(&metric.plan)
                .unwrap_or_else(|e| panic!("{name}: provenance {e:?}"));
            let penetrations = metric.substrate.verify_no_group_penetration();
            assert!(
                penetrations.is_empty(),
                "{name}: L6 penetrations={penetrations:?}"
            );
            for (&eid, tracks) in &metric.plan.channels {
                let gates = metric.plan.gates.get(&eid).cloned().unwrap_or_default();
                let u = endpoint_scope(&metric.substrate, &metric.plan, eid, true);
                let v = endpoint_scope(&metric.substrate, &metric.plan, eid, false);
                let scope_v = verify_route_scope(&metric.substrate, tracks, &gates, u, v);
                assert!(
                    scope_v.is_empty(),
                    "{name}: edge {eid} verify_route_scope={scope_v:?}"
                );
            }
        }
    }

    /// Post-Ink 证明链：完整布局后 product 图 `EdgeCrossesGroupInterior` = 0。
    #[test]
    fn product_post_ink_no_group_penetration_lint() {
        use crate::layout::quality::lint::{lint_layout, LintRuleId};
        let cases = [
            include_str!("../../../../../showcase/flowchart/product.linear-chain.taut"),
            include_str!("../../../../../showcase/flowchart/product.user-auth.taut"),
            include_str!("../../../../../showcase/flowchart/product.swimlane-order-process.taut"),
            include_str!("../../../../../showcase/architecture/product.ecommerce-platform.taut"),
            include_str!("../../../../../showcase/architecture/product.cloud-native.taut"),
        ];
        for source in cases {
            let prepared = parse_diagram(source);
            let name = prepared.inner().title().unwrap_or("<untitled>");
            let layout = crate::layout::compute_layout(prepared.inner())
                .unwrap_or_else(|e| panic!("{name}: layout {e:?}"));
            let report = lint_layout(prepared.inner(), &layout);
            let crosses = report.by_rule(LintRuleId::EdgeCrossesGroupInterior).count();
            assert_eq!(
                crosses, 0,
                "{name}: post-ink edge_crosses_group_interior={crosses}"
            );
        }
    }
}
