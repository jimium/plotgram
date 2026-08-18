//! Ink: expand [`MessageRouteTopo`] into polyline points. Zero new decisions.

use tautcore_engine_api::LayoutError;
use tautcore_model::geometry::Point;
use tautcore_model::graph::Graph;
use tautcore_model::port::{AlongSpec, PortRef, Side};
use tautcore_model::result::{EdgePath, EdgePlacement};

use super::params::SequenceParams;
use super::plan::{LifelineSide, MessageRouteTopo, SeqMetric, SeqPlan};

pub fn expand(
    graph: &Graph,
    plan: &SeqPlan,
    metric: &SeqMetric,
    params: &SequenceParams,
) -> Result<Vec<EdgePlacement>, LayoutError> {
    let mut edges = Vec::with_capacity(plan.messages.len());
    for msg in &plan.messages {
        let terms = metric.message_terminals.get(&msg.edge_id).ok_or_else(|| {
            super::seq_err(format!(
                "sequence: invariant: missing terminals for `{}`",
                msg.edge_id
            ))
        })?;
        let points = match msg.route {
            MessageRouteTopo::Horizontal => vec![terms.from, terms.to],
            MessageRouteTopo::SelfLoop { side } => {
                let x0 = terms.from.x;
                let x1 = match side {
                    LifelineSide::East => x0 + params.self_loop_width,
                    LifelineSide::West => x0 - params.self_loop_width,
                    LifelineSide::Center => {
                        return Err(super::seq_err(format!(
                            "sequence: invariant: SelfLoop Center is not a valid topo \
                             for `{}`",
                            msg.edge_id
                        )));
                    }
                };
                vec![
                    terms.from,
                    Point {
                        x: x1,
                        y: terms.from.y,
                    },
                    Point {
                        x: x1,
                        y: terms.to.y,
                    },
                    terms.to,
                ]
            }
        };
        let edge = graph.find_edge(&msg.edge_id).ok_or_else(|| {
            super::seq_err(format!(
                "sequence: invariant: edge `{}` missing from graph",
                msg.edge_id
            ))
        })?;
        edges.push(EdgePlacement {
            id: msg.edge_id.clone(),
            source: edge.source.clone(),
            target: edge.target.clone(),
            path: EdgePath::polyline(points),
            from_port: port_ref(msg.attach.from.side, msg.attach.from.activation_depth),
            to_port: port_ref(msg.attach.to.side, msg.attach.to.activation_depth),
        });
    }
    Ok(edges)
}

fn port_ref(side: LifelineSide, depth: u32) -> Option<PortRef> {
    let model_side = match side {
        LifelineSide::East => Side::East,
        LifelineSide::West => Side::West,
        LifelineSide::Center => return None,
    };
    Some(PortRef {
        side: model_side,
        along: AlongSpec::Ordered {
            order: depth,
            count: depth + 1,
        },
    })
}
