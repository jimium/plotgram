//! Built-in orthogonal ink: expand resolved ports via route-core.

use std::collections::BTreeMap;

use plotgram_engine_api::LayoutError;
use plotgram_model::port::PortRef;
use plotgram_model::result::{EdgePath, EdgePlacement, NodePlacement};
use crate::route::core::{orthogonal_elbow, port_anchor};

pub fn route_builtin(
    nodes: &[NodePlacement],
    edges: Vec<EdgePlacement>,
) -> Result<Vec<EdgePlacement>, LayoutError> {
    let frames: BTreeMap<&str, &NodePlacement> =
        nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    let mut out = Vec::with_capacity(edges.len());
    for mut e in edges {
        let src = frames.get(e.source.as_str()).ok_or_else(|| {
            LayoutError::message(format!("missing node `{}` for edge `{}`", e.source, e.id))
        })?;
        let tgt = frames.get(e.target.as_str()).ok_or_else(|| {
            LayoutError::message(format!("missing node `{}` for edge `{}`", e.target, e.id))
        })?;

        let from = e.from_port.unwrap_or(PortRef {
            side: plotgram_model::port::Side::South,
            slot: 0,
        });
        let to = e.to_port.unwrap_or(PortRef {
            side: plotgram_model::port::Side::North,
            slot: 0,
        });
        e.from_port = Some(from);
        e.to_port = Some(to);

        let a = port_anchor(&src.frame, from);
        let b = port_anchor(&tgt.frame, to);
        e.path = EdgePath {
            points: orthogonal_elbow(a, b),
        };
        out.push(e);
    }
    Ok(out)
}
