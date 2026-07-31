//! Port decision for hierarchical stub (composition-phase writer).

use std::collections::BTreeMap;

use plotgram_engine_api::LayoutError;
use plotgram_model::graph::Graph;
use plotgram_model::port::{PortRef, Side};
use plotgram_model::result::{EdgePath, EdgePlacement, NodePlacement};

use super::params::HierarchicalParams;

/// Infer ports from flow direction when author constraints are absent.
pub fn build_edge_stubs(
    graph: &Graph,
    nodes: &[NodePlacement],
    params: &HierarchicalParams,
) -> Result<Vec<EdgePlacement>, LayoutError> {
    let frames: BTreeMap<&str, &NodePlacement> =
        nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let vertical = params.orientation.is_vertical();

    let mut out = Vec::new();
    for e in graph.edges_in_declaration_order() {
        if !frames.contains_key(e.source.as_str()) || !frames.contains_key(e.target.as_str()) {
            return Err(LayoutError::message(format!(
                "edge `{}` endpoint missing placement",
                e.id
            )));
        }

        let from_port = e.from_port.map(|c| PortRef {
            side: c.side,
            slot: c.slot.unwrap_or(0),
        }).unwrap_or_else(|| PortRef {
            side: if vertical { Side::South } else { Side::East },
            slot: 0,
        });
        let to_port = e.to_port.map(|c| PortRef {
            side: c.side,
            slot: c.slot.unwrap_or(0),
        }).unwrap_or_else(|| PortRef {
            side: if vertical { Side::North } else { Side::West },
            slot: 0,
        });

        out.push(EdgePlacement {
            id: e.id.clone(),
            source: e.source.clone(),
            target: e.target.clone(),
            path: EdgePath { points: vec![] },
            from_port: Some(from_port),
            to_port: Some(to_port),
        });
    }
    Ok(out)
}
