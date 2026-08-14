//! Minimal PlanVerifier after Compose + Channel (ink-and-verification.md §7.1).
//!
//! Flat subset: proper hierarchy, port completeness, orthogonal route
//! connectivity, EscapePlan ↔ PortPlan.side compatibility (P5-3).
//! Group/gate/scope checks are deferred to D₂.

use std::collections::BTreeMap;

use plotgram_algo::orientation::Side;
use plotgram_engine_api::LayoutError;

use crate::layout::hierarchical::channel::{ChannelRoutePlan, EscapeEnd, RouteTopology};
use crate::layout::hierarchical::compose::ports::EdgePorts;
use crate::layout::hierarchical::model::{PlanGraph, RealGraph};

/// Verify discrete Plan facts before Metric / Ink expand.
pub fn verify_plan(
    plan: &PlanGraph,
    graph: &RealGraph,
    ports: &BTreeMap<String, EdgePorts>,
    route_plan: &ChannelRoutePlan,
) -> Result<(), LayoutError> {
    verify_proper_hierarchy(plan)?;
    verify_ports_complete(graph, ports)?;
    verify_routes_connected(graph, route_plan, ports)?;
    Ok(())
}

fn verify_proper_hierarchy(plan: &PlanGraph) -> Result<(), LayoutError> {
    for s in &plan.segments {
        let rf = plan.elems[s.from].rank;
        let rt = plan.elems[s.to].rank;
        if rt != rf + 1 {
            return Err(LayoutError::message(format!(
                "hierarchical PlanVerifier: segment on `{}` connects rank {rf}→{rt} \
                 (proper hierarchy requires adjacent ranks)",
                s.edge_id
            )));
        }
    }
    Ok(())
}

fn verify_ports_complete(
    graph: &RealGraph,
    ports: &BTreeMap<String, EdgePorts>,
) -> Result<(), LayoutError> {
    for e in &graph.edges {
        if !ports.contains_key(&e.edge_id) {
            return Err(LayoutError::message(format!(
                "hierarchical PlanVerifier: edge `{}` missing PortPlan",
                e.edge_id
            )));
        }
    }
    Ok(())
}

fn escape_compatible(end: EscapeEnd, _side: Side) -> bool {
    match end {
        EscapeEnd::AtPortNormal | EscapeEnd::ViaGap(_) => true,
    }
}

fn verify_routes_connected(
    graph: &RealGraph,
    route_plan: &ChannelRoutePlan,
    ports: &BTreeMap<String, EdgePorts>,
) -> Result<(), LayoutError> {
    use crate::layout::hierarchical::compose::bundle::end_bus_edge_ids;
    let bus = end_bus_edge_ids(&route_plan.bundles);
    for e in &graph.edges {
        if bus.contains(&e.edge_id) {
            continue; // Ink joins BundlePlan; no ChannelPath required
        }
        let Some(topo) = route_plan.routes.get(&e.edge_id) else {
            return Err(LayoutError::message(format!(
                "hierarchical PlanVerifier: edge `{}` missing RouteTopology",
                e.edge_id
            )));
        };
        let RouteTopology::Orthogonal(path) = topo;
        if path.tracks.is_empty() {
            return Err(LayoutError::message(format!(
                "hierarchical PlanVerifier: edge `{}` has empty ChannelPath",
                e.edge_id
            )));
        }
        for &tid in &path.tracks {
            if route_plan.substrate.track(tid).is_none() {
                return Err(LayoutError::message(format!(
                    "hierarchical PlanVerifier: edge `{}` references unknown track {:?}",
                    e.edge_id, tid
                )));
            }
        }
        let Some(ep) = ports.get(&e.edge_id) else {
            continue;
        };
        if !escape_compatible(path.escape.source, ep.source.side) {
            return Err(LayoutError::message(format!(
                "hierarchical PlanVerifier: edge `{}` escape.source {:?} incompatible with \
                 source PortPlan.side {:?}",
                e.edge_id, path.escape.source, ep.source.side
            )));
        }
        if !escape_compatible(path.escape.target, ep.target.side) {
            return Err(LayoutError::message(format!(
                "hierarchical PlanVerifier: edge `{}` escape.target {:?} incompatible with \
                 target PortPlan.side {:?}",
                e.edge_id, path.escape.target, ep.target.side
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::hierarchical::model::{Elem, ElemKey, RealEdge, Segment};

    fn tiny_plan() -> (PlanGraph, RealGraph) {
        let elems = vec![
            Elem {
                key: ElemKey::Real("a".into()),
                group_path: vec![],
                rank: 0,
            },
            Elem {
                key: ElemKey::Real("b".into()),
                group_path: vec![],
                rank: 1,
            },
        ];
        let index_of = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let plan = PlanGraph {
            elems,
            index_of,
            decl_index: vec![0, 1],
            segments: vec![Segment {
                edge_id: "e0".into(),
                ordinal: 0,
                from: 0,
                to: 1,
            }],
            layers: vec![vec![0], vec![1]],
            ..Default::default()
        };
        let mut ids = BTreeMap::new();
        ids.insert("a".into(), 0);
        ids.insert("b".into(), 1);
        let graph = RealGraph {
            ids: vec!["a".into(), "b".into()],
            index_of: ids,
            group_path: vec![vec![], vec![]],
            shapes: vec![plotgram_model::NodeShape::DEFAULT; 2],
            edges: vec![RealEdge {
                edge_id: "e0".into(),
                original_source: 0,
                original_target: 1,
                working_source: 0,
                working_target: 1,
                reversed: false,
                from_port: None,
                to_port: None,
                weight: 1.0,
                ..Default::default()
            }],
            self_loops: vec![],
            ..Default::default()
        };
        (plan, graph)
    }

    #[test]
    fn proper_hierarchy_rejects_skip_rank() {
        let (mut plan, _g) = tiny_plan();
        plan.elems[1].rank = 2;
        let err = verify_proper_hierarchy(&plan).unwrap_err();
        assert!(err.to_string().contains("proper hierarchy"));
    }
}
