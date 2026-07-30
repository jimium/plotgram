//! Independent orthogonal [`EdgeRouter`](plotgram_engine_api::EdgeRouter).
//!
//! Stub: same elbow geometry as built-in ink via [`crate::route::core`].
//! Extract to `plotgram-route-orthogonal` when this grows.

use std::collections::BTreeMap;

use plotgram_engine_api::{EdgeRouter, LayoutError, RouteInput};
use plotgram_model::port::PortRef;
use plotgram_model::result::{EdgePath, EdgePlacement};

use crate::route::core::{orthogonal_elbow, port_anchor};

/// Orthogonal edge router registry name: `"orthogonal"`.
#[derive(Debug, Default, Clone, Copy)]
pub struct OrthogonalEdgeRouter;

impl EdgeRouter for OrthogonalEdgeRouter {
    fn name(&self) -> &'static str {
        "orthogonal"
    }

    fn route(&self, input: RouteInput<'_>) -> Result<Vec<EdgePlacement>, LayoutError> {
        let frames: BTreeMap<&str, _> = input.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
        let mut out = Vec::with_capacity(input.edges.len());

        for stub in input.edges {
            let src = frames.get(stub.source.as_str()).ok_or_else(|| {
                LayoutError::message(format!(
                    "router: missing node `{}` for edge `{}`",
                    stub.source, stub.id
                ))
            })?;
            let tgt = frames.get(stub.target.as_str()).ok_or_else(|| {
                LayoutError::message(format!(
                    "router: missing node `{}` for edge `{}`",
                    stub.target, stub.id
                ))
            })?;

            let from = stub.from_port.ok_or_else(|| {
                LayoutError::message(format!(
                    "router: edge `{}` missing from_port (layout must resolve ports)",
                    stub.id
                ))
            })?;
            let to = stub.to_port.ok_or_else(|| {
                LayoutError::message(format!(
                    "router: edge `{}` missing to_port (layout must resolve ports)",
                    stub.id
                ))
            })?;

            out.push(EdgePlacement {
                id: stub.id.clone(),
                source: stub.source.clone(),
                target: stub.target.clone(),
                path: EdgePath {
                    points: orthogonal_elbow(
                        port_anchor(&src.frame, from),
                        port_anchor(&tgt.frame, to),
                    ),
                },
                from_port: Some(PortRef {
                    side: from.side,
                    slot: from.slot,
                }),
                to_port: Some(PortRef {
                    side: to.side,
                    slot: to.slot,
                }),
            });
        }
        Ok(out)
    }
}
