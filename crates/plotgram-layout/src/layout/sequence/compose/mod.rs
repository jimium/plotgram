//! Compose: lifeline order, message rows, kind, attach, route topo, activations,
//! combined fragments.
//!
//! Writer: LifelineOrderWriter + MessageWriter (+ Attach + Activation + Fragment).
//! No coordinates.

mod fragment;

use std::collections::BTreeMap;

use plotgram_algo::linear_arrange::{arrange, LinearArrangeMethod, LinearArrangement};
use plotgram_engine_api::LayoutError;
use plotgram_model::graph::{Arrow, Graph, NodeRole};

use super::params::{LifelineOrder, SelfLoopRowPolicy, SequenceParams};
use super::plan::{
    ActivationSpan, AttachSpec, FragmentPlan, LifelineSide, MessageAttach, MessageKind,
    MessagePlan, MessageRouteTopo, MessageTiming, SeqPlan,
};

pub struct ComposeOutput {
    pub plan: SeqPlan,
    pub warnings: Vec<String>,
}

/// Collect top-level entity participants in declaration order.
///
/// Does **not** recurse into `Graph::groups` (axes.md §1).
pub fn collect_lifelines(graph: &Graph) -> Vec<String> {
    graph
        .nodes
        .iter()
        .filter(|n| n.role == NodeRole::Entity)
        .map(|n| n.id.clone())
        .collect()
}

pub fn compose(graph: &Graph, params: &SequenceParams) -> Result<ComposeOutput, LayoutError> {
    let declared = collect_lifelines(graph);
    let (pins, befores, pin_map) = read_order_constraints(graph, &declared)?;
    let lifeline_order = order_lifelines(graph, &declared, params, &pins, &befores)?;

    let mut index: BTreeMap<&str, usize> = BTreeMap::new();
    for (i, id) in lifeline_order.iter().enumerate() {
        index.insert(id.as_str(), i);
    }

    let mut messages = Vec::new();
    let mut next_row: u32 = 0;

    for edge in graph.edges_in_declaration_order() {
        let from_idx = index.get(edge.source.as_str()).copied();
        let to_idx = index.get(edge.target.as_str()).copied();
        let (Some(fi), Some(ti)) = (from_idx, to_idx) else {
            let bad = if from_idx.is_none() {
                edge.source.as_str()
            } else {
                edge.target.as_str()
            };
            return Err(super::seq_err(format!(
                "sequence: invalid: message `{}` endpoint `{bad}` is not a \
                 top-level participant",
                edge.id
            )));
        };

        let is_self = edge.source == edge.target;
        let kind = if is_self {
            MessageKind::SelfCall
        } else if edge.arrow == Arrow::Response {
            MessageKind::Reply
        } else {
            MessageKind::Call
        };

        let (row, end_row) =
            if is_self && matches!(params.self_loop_row_policy, SelfLoopRowPolicy::DoubleRow) {
                let r = next_row;
                next_row += 2;
                (r, r + 1)
            } else {
                let r = next_row;
                next_row += 1;
                (r, r)
            };

        let (from_side, to_side, route) = if is_self {
            (
                LifelineSide::East,
                LifelineSide::East,
                MessageRouteTopo::SelfLoop {
                    side: LifelineSide::East,
                },
            )
        } else if ti > fi {
            (
                LifelineSide::East,
                LifelineSide::West,
                MessageRouteTopo::Horizontal,
            )
        } else if ti < fi {
            (
                LifelineSide::West,
                LifelineSide::East,
                MessageRouteTopo::Horizontal,
            )
        } else {
            return Err(super::seq_err(format!(
                "sequence: invariant: message `{}` has distinct endpoints but \
                 equal lifeline indices",
                edge.id
            )));
        };

        messages.push(MessagePlan {
            edge_id: edge.id.clone(),
            from: edge.source.clone(),
            to: edge.target.clone(),
            kind,
            timing: MessageTiming::SyncSameRow,
            row,
            end_row,
            attach: MessageAttach {
                from: AttachSpec {
                    side: from_side,
                    activation_depth: 0,
                },
                to: AttachSpec {
                    side: to_side,
                    activation_depth: 0,
                },
            },
            route,
        });
    }

    let fragments = fragment::collect(graph, &lifeline_order, &messages)?;
    let (activation_spans, warnings) =
        derive_activations(&lifeline_order, &mut messages, next_row, params, &fragments)?;

    Ok(ComposeOutput {
        plan: SeqPlan {
            lifeline_order,
            messages,
            row_count: next_row,
            activation_spans,
            lifeline_pins: pin_map,
            fragments,
        },
        warnings,
    })
}

fn order_lifelines(
    graph: &Graph,
    declared: &[String],
    params: &SequenceParams,
    pins: &[(usize, usize)],
    befores: &[(usize, usize)],
) -> Result<Vec<String>, LayoutError> {
    let n = declared.len();
    let mut index: BTreeMap<&str, usize> = BTreeMap::new();
    for (i, id) in declared.iter().enumerate() {
        index.insert(id.as_str(), i);
    }
    let mut edges = Vec::new();
    for edge in graph.edges_in_declaration_order() {
        let Some(&u) = index.get(edge.source.as_str()) else {
            continue;
        };
        let Some(&v) = index.get(edge.target.as_str()) else {
            continue;
        };
        if u != v {
            edges.push((u, v, 1.0));
        }
    }
    let method = match params.lifeline_order {
        LifelineOrder::Declaration => LinearArrangeMethod::Identity,
        LifelineOrder::Greedy => LinearArrangeMethod::Greedy,
        LifelineOrder::Local => LinearArrangeMethod::Local,
    };
    let perm = arrange(
        &LinearArrangement {
            n,
            edges,
            pins: pins.to_vec(),
            befores: befores.to_vec(),
        },
        method,
    )
    .map_err(|e| super::seq_err(format!("sequence: {e}")))?;
    Ok(perm.into_iter().map(|i| declared[i].clone()).collect())
}

fn read_order_constraints(
    graph: &Graph,
    ids: &[String],
) -> Result<
    (
        Vec<(usize, usize)>,
        Vec<(usize, usize)>,
        BTreeMap<String, u32>,
    ),
    LayoutError,
> {
    let n = ids.len();
    let mut index: BTreeMap<&str, usize> = BTreeMap::new();
    for (i, id) in ids.iter().enumerate() {
        index.insert(id.as_str(), i);
    }
    let mut pins = Vec::new();
    let mut pin_map = BTreeMap::new();
    let mut befores = Vec::new();
    let mut used_slot: BTreeMap<usize, usize> = BTreeMap::new();

    for (i, id) in ids.iter().enumerate() {
        let Some(node) = graph.find_node(id) else {
            continue;
        };
        if let Some(raw) = node.attrs.get("lifeline_pin") {
            let pos = parse_pin(raw, n, id)?;
            if let Some(&other) = used_slot.get(&pos) {
                return Err(super::seq_err(format!(
                    "sequence: infeasible: `{id}` and `{}` both pin slot {pos}",
                    ids[other]
                )));
            }
            used_slot.insert(pos, i);
            pins.push((i, pos));
            pin_map.insert(id.clone(), pos as u32);
        }
        if let Some(other) = node.attrs.get("lifeline_before").and_then(|v| v.as_str()) {
            let Some(&j) = index.get(other) else {
                return Err(super::seq_err(format!(
                    "sequence: invalid: `{id}` lifeline_before `{other}` is not a participant"
                )));
            };
            if i == j {
                return Err(super::seq_err(format!(
                    "sequence: infeasible: `{id}` lifeline_before itself"
                )));
            }
            befores.push((i, j));
        }
    }
    Ok((pins, befores, pin_map))
}

fn parse_pin(
    raw: &plotgram_model::attr::AttrValue,
    n: usize,
    id: &str,
) -> Result<usize, LayoutError> {
    if let Some(s) = raw.as_str() {
        let pos = match s {
            "left" | "start" | "first" => 0,
            "right" | "end" | "last" => n.saturating_sub(1),
            other => {
                return Err(super::seq_err(format!(
                    "sequence: invalid: `{id}` lifeline_pin `{other}` \
                     (expected left/right or a slot index)"
                )));
            }
        };
        return Ok(pos);
    }
    if let Some(x) = raw.as_f64() {
        if !x.is_finite() || x < 0.0 || (x.fract() - 0.0).abs() > 1e-9 {
            return Err(super::seq_err(format!(
                "sequence: invalid: `{id}` lifeline_pin {x} is not a whole slot index"
            )));
        }
        let pos = x as usize;
        if pos >= n {
            return Err(super::seq_err(format!(
                "sequence: infeasible: `{id}` lifeline_pin {pos} ≥ {n} participants"
            )));
        }
        return Ok(pos);
    }
    Err(super::seq_err(format!(
        "sequence: invalid: `{id}` lifeline_pin expected number or left/right"
    )))
}

struct OpenAct {
    edge_id: String,
    start_row: u32,
    depth: u32,
}

fn stack_attach_depth(stack: &[OpenAct]) -> u32 {
    match stack.last() {
        None => 0,
        Some(open) => open.depth + 1,
    }
}

/// Simplified stack (axes.md §4): Call pushes on target; Reply pops on source;
/// SelfCall is a closed interval and does not stay on the stack.
/// Attach depths are written here (M2): 0 = on-axis; d>0 = bar outer face.
fn derive_activations(
    lifeline_order: &[String],
    messages: &mut [MessagePlan],
    row_count: u32,
    params: &SequenceParams,
    fragments: &[FragmentPlan],
) -> Result<(Vec<ActivationSpan>, Vec<String>), LayoutError> {
    let mut stacks: BTreeMap<String, Vec<OpenAct>> = BTreeMap::new();
    let mut spans = Vec::new();
    let mut warnings = Vec::new();
    let mut ordinal: u32 = 0;

    for msg in messages.iter_mut() {
        match msg.kind {
            MessageKind::Call => {
                let from_d = stack_attach_depth(stacks.entry(msg.from.clone()).or_default());
                msg.attach.from.activation_depth = from_d;
                let stack = stacks.entry(msg.to.clone()).or_default();
                let depth = stack.len() as u32;
                stack.push(OpenAct {
                    edge_id: msg.edge_id.clone(),
                    start_row: msg.row,
                    depth,
                });
                msg.attach.to.activation_depth = stack_attach_depth(stack);
            }
            MessageKind::Reply => {
                let from_d = stack_attach_depth(stacks.entry(msg.from.clone()).or_default());
                msg.attach.from.activation_depth = from_d;
                let stack = stacks.entry(msg.from.clone()).or_default();
                match stack.pop() {
                    Some(open) => {
                        spans.push(ActivationSpan {
                            id: format!("activation:{}:{ordinal}", open.edge_id),
                            lifeline: msg.from.clone(),
                            start_row: open.start_row,
                            end_row: msg.row.max(open.start_row),
                            depth: open.depth,
                        });
                        ordinal += 1;
                    }
                    None => {
                        if is_alt_else_reply(&msg.edge_id, msg.row, fragments) {
                            // Mutually exclusive operand of alt/par: the call
                            // was already closed by a sibling operand.
                        } else {
                            let message = format!(
                                "sequence: unpaired reply `{}` on `{}`",
                                msg.edge_id, msg.from
                            );
                            if params.activation_strict {
                                return Err(super::seq_err(message));
                            }
                            warnings.push(message);
                        }
                    }
                }
                let to_d = stack_attach_depth(stacks.entry(msg.to.clone()).or_default());
                msg.attach.to.activation_depth = to_d;
            }
            MessageKind::SelfCall => {
                let stack = stacks.entry(msg.from.clone()).or_default();
                let depth = stack.len() as u32;
                let attach = depth + 1;
                msg.attach.from.activation_depth = attach;
                msg.attach.to.activation_depth = attach;
                spans.push(ActivationSpan {
                    id: format!("activation:{}:{ordinal}", msg.edge_id),
                    lifeline: msg.from.clone(),
                    start_row: msg.row,
                    end_row: msg.end_row,
                    depth,
                });
                ordinal += 1;
            }
        }
    }

    let last_row = row_count.saturating_sub(1);
    for id in lifeline_order {
        let Some(stack) = stacks.get_mut(id) else {
            continue;
        };
        while let Some(open) = stack.pop() {
            let message = format!(
                "sequence: unclosed activation on `{id}` from `{}`",
                open.edge_id
            );
            if params.activation_strict {
                return Err(super::seq_err(message));
            }
            warnings.push(message);
            spans.push(ActivationSpan {
                id: format!("activation:{}:{ordinal}", open.edge_id),
                lifeline: id.clone(),
                start_row: open.start_row,
                end_row: last_row.max(open.start_row),
                depth: open.depth,
            });
            ordinal += 1;
        }
    }

    Ok((spans, warnings))
}

fn is_alt_else_reply(edge_id: &str, row: u32, fragments: &[FragmentPlan]) -> bool {
    fragments.iter().any(|f| {
        matches!(f.operator.as_str(), "alt" | "par")
            && f.edge_ids.iter().any(|e| e == edge_id)
            && operand_index(f, row) > 0
    })
}

fn operand_index(frag: &FragmentPlan, row: u32) -> u32 {
    let mut op = 0u32;
    for &split in &frag.operand_splits {
        if row > split {
            op += 1;
        } else {
            break;
        }
    }
    op
}
