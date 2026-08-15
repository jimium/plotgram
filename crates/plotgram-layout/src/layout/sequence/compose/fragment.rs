//! Combined-fragment collection (architecture.md §10 M4 / 19 §4).
//!
//! Authoring: first-class `fragment` blocks (lower stamps the attrs below)
//! or the same edge attrs written by hand. Not `Graph::groups`.
//! - `fragment: id` membership; `a.b` / `"a/b"` / `"a,b"` = this message is in both
//! - `fragment_kind` / `fragment_operator` + `fragment_label` apply to the
//!   last id on that edge (first edge in declaration order wins)
//! - `fragment_path_kinds` / `fragment_path_operands` cover ancestor layers
//! - `fragment_operand` (0-based) marks the innermost alt/par partition
//!
//! Nesting is inferred from interval inclusion. Partial overlap (not nested)
//! is illegal UML and hard-fails.

use std::collections::BTreeMap;

use plotgram_engine_api::LayoutError;
use plotgram_model::graph::Graph;

use super::super::plan::{FragmentPlan, MessagePlan};

struct Acc {
    members: Vec<String>,
    operator: Option<String>,
    label: Option<String>,
    operands: BTreeMap<u32, Vec<String>>,
}

pub fn collect(
    graph: &Graph,
    lifeline_order: &[String],
    messages: &[MessagePlan],
) -> Result<Vec<FragmentPlan>, LayoutError> {
    let mut acc: BTreeMap<String, Acc> = BTreeMap::new();
    let mut msg_index: BTreeMap<&str, &MessagePlan> = BTreeMap::new();
    for m in messages {
        msg_index.insert(m.edge_id.as_str(), m);
    }

    for edge in graph.edges_in_declaration_order() {
        let Some(raw) = edge.attrs.get("fragment").and_then(|v| v.as_str()) else {
            continue;
        };
        if msg_index.get(edge.id.as_str()).is_none() {
            continue;
        }
        let ids = parse_fragment_ids(raw, &edge.id)?;
        let path_kinds: Vec<String> = edge
            .attrs
            .get("fragment_path_kinds")
            .and_then(|v| v.as_str())
            .map(|s| {
                s.split('.')
                    .map(|p| p.trim().to_string())
                    .filter(|p| !p.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        let innermost = ids.last().cloned().expect("parse_fragment_ids non-empty");
        let path_ops = edge
            .attrs
            .get("fragment_path_operands")
            .and_then(|v| v.as_str())
            .map(parse_path_operands);
        for (idx, id) in ids.iter().enumerate() {
            let slot = acc.entry(id.clone()).or_insert_with(|| Acc {
                members: Vec::new(),
                operator: None,
                label: None,
                operands: BTreeMap::new(),
            });
            if !slot.members.iter().any(|e| e == &edge.id) {
                slot.members.push(edge.id.clone());
            }
            if slot.operator.is_none() {
                if let Some(k) = path_kinds.get(idx) {
                    slot.operator = Some(k.clone());
                }
            }
            if let Some(op) = path_ops
                .as_ref()
                .and_then(|ops| ops.get(idx).copied().flatten())
            {
                slot.operands.entry(op).or_default().push(edge.id.clone());
            }
        }
        let inner = acc.get_mut(&innermost).expect("just inserted");
        if inner.operator.is_none() {
            if let Some(k) = edge
                .attrs
                .get("fragment_kind")
                .or_else(|| edge.attrs.get("fragment_operator"))
                .and_then(|v| v.as_str())
            {
                inner.operator = Some(k.to_string());
            }
        }
        if inner.label.is_none() {
            if let Some(l) = edge.attrs.get("fragment_label").and_then(|v| v.as_str()) {
                inner.label = Some(l.to_string());
            }
        }
        if path_ops.is_none() {
            if let Some(raw_op) = edge.attrs.get("fragment_operand") {
                let op = parse_operand(raw_op, &edge.id)?;
                inner.operands.entry(op).or_default().push(edge.id.clone());
            }
        }
    }

    if acc.is_empty() {
        return Ok(Vec::new());
    }

    let mut plans = Vec::new();
    for (id, a) in &acc {
        let span = span_of(&a.members, &msg_index, lifeline_order, id)?;
        let mut operands = a.operands.clone();
        if !operands.is_empty() {
            for eid in &a.members {
                let listed = operands.values().any(|v| v.iter().any(|x| x == eid));
                if !listed {
                    operands.entry(0).or_default().push(eid.clone());
                }
            }
        }
        let operand_splits = operand_splits(&operands, &msg_index);
        plans.push(FragmentPlan {
            id: id.clone(),
            decoration_id: format!("fragment:{id}"),
            operator: a.operator.clone().unwrap_or_else(|| "region".into()),
            label: a.label.clone(),
            edge_ids: a.members.clone(),
            start_row: span.0,
            end_row: span.1,
            lifeline_lo: span.2,
            lifeline_hi: span.3,
            depth: 0,
            parent: None,
            operand_splits,
        });
    }
    plans.sort_by(|a, b| a.id.cmp(&b.id));
    assign_nesting(&mut plans)?;
    Ok(plans)
}

fn parse_path_operands(raw: &str) -> Vec<Option<u32>> {
    raw.split('.')
        .map(|part| {
            let p = part.trim();
            if p.is_empty() || p == "-" {
                None
            } else {
                p.parse().ok()
            }
        })
        .collect()
}

fn parse_fragment_ids(raw: &str, edge_id: &str) -> Result<Vec<String>, LayoutError> {
    let sep = if raw.contains('/') {
        '/'
    } else if raw.contains(',') {
        ','
    } else if raw.contains('.') {
        '.'
    } else {
        '\0'
    };
    let mut ids = Vec::new();
    let parts: Vec<&str> = if sep == '\0' {
        vec![raw]
    } else {
        raw.split(sep).collect()
    };
    for part in parts {
        let id = part.trim();
        if id.is_empty() {
            continue;
        }
        if !ids.iter().any(|x| x == id) {
            ids.push(id.to_string());
        }
    }
    if ids.is_empty() {
        return Err(crate::layout::sequence::seq_err(format!(
            "sequence: invalid: message `{edge_id}` has empty `fragment`"
        )));
    }
    Ok(ids)
}

fn parse_operand(raw: &plotgram_model::attr::AttrValue, edge_id: &str) -> Result<u32, LayoutError> {
    let Some(x) = raw.as_f64() else {
        return Err(crate::layout::sequence::seq_err(format!(
            "sequence: invalid: message `{edge_id}` fragment_operand expected a number"
        )));
    };
    if !x.is_finite() || x < 0.0 || (x.fract() - 0.0).abs() > 1e-9 {
        return Err(crate::layout::sequence::seq_err(format!(
            "sequence: invalid: message `{edge_id}` fragment_operand {x} is not a whole index"
        )));
    }
    Ok(x as u32)
}

fn span_of(
    members: &[String],
    msgs: &BTreeMap<&str, &MessagePlan>,
    order: &[String],
    id: &str,
) -> Result<(u32, u32, u32, u32), LayoutError> {
    let mut start = u32::MAX;
    let mut end = 0u32;
    let mut lo = u32::MAX;
    let mut hi = 0u32;
    let index =
        |name: &str| -> Option<u32> { order.iter().position(|x| x == name).map(|i| i as u32) };
    for eid in members {
        let Some(m) = msgs.get(eid.as_str()) else {
            continue;
        };
        start = start.min(m.row);
        end = end.max(m.end_row);
        for name in [&m.from, &m.to] {
            let Some(i) = index(name) else {
                return Err(crate::layout::sequence::seq_err(format!(
                    "sequence: invariant: fragment `{id}` member `{eid}` endpoint `{name}` \
                     is not a participant"
                )));
            };
            lo = lo.min(i);
            hi = hi.max(i);
        }
    }
    if start == u32::MAX || lo == u32::MAX {
        return Err(crate::layout::sequence::seq_err(format!(
            "sequence: invalid: fragment `{id}` has no messages"
        )));
    }
    Ok((start, end, lo, hi))
}

fn operand_splits(
    operands: &BTreeMap<u32, Vec<String>>,
    msgs: &BTreeMap<&str, &MessagePlan>,
) -> Vec<u32> {
    if operands.len() < 2 {
        return Vec::new();
    }
    let mut keys: Vec<u32> = operands.keys().copied().collect();
    keys.sort();
    let mut splits = Vec::new();
    for w in keys.windows(2) {
        let mut last = 0u32;
        for eid in &operands[&w[0]] {
            if let Some(m) = msgs.get(eid.as_str()) {
                last = last.max(m.end_row);
            }
        }
        splits.push(last);
    }
    splits
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Relation {
    Equal,
    AContainsB,
    BContainsA,
    Disjoint,
    Overlap,
}

fn relation(a: &FragmentPlan, b: &FragmentPlan) -> Relation {
    if a.start_row == b.start_row
        && a.end_row == b.end_row
        && a.lifeline_lo == b.lifeline_lo
        && a.lifeline_hi == b.lifeline_hi
    {
        return Relation::Equal;
    }
    let a_in_b = a.start_row >= b.start_row
        && a.end_row <= b.end_row
        && a.lifeline_lo >= b.lifeline_lo
        && a.lifeline_hi <= b.lifeline_hi;
    let b_in_a = b.start_row >= a.start_row
        && b.end_row <= a.end_row
        && b.lifeline_lo >= a.lifeline_lo
        && b.lifeline_hi <= a.lifeline_hi;
    if a_in_b {
        return Relation::BContainsA;
    }
    if b_in_a {
        return Relation::AContainsB;
    }
    let rows = a.start_row <= b.end_row && b.start_row <= a.end_row;
    let cols = a.lifeline_lo <= b.lifeline_hi && b.lifeline_lo <= a.lifeline_hi;
    if rows && cols {
        Relation::Overlap
    } else {
        Relation::Disjoint
    }
}

fn assign_nesting(plans: &mut [FragmentPlan]) -> Result<(), LayoutError> {
    let n = plans.len();
    for i in 0..n {
        for j in (i + 1)..n {
            match relation(&plans[i], &plans[j]) {
                Relation::Equal => {
                    return Err(crate::layout::sequence::seq_err(format!(
                        "sequence: infeasible: fragments `{}` and `{}` cover the same interval",
                        plans[i].id, plans[j].id
                    )));
                }
                Relation::Overlap => {
                    return Err(crate::layout::sequence::seq_err(format!(
                        "sequence: infeasible: fragments `{}` and `{}` overlap (UML forbids \
                         partial overlap; nest or separate them)",
                        plans[i].id, plans[j].id
                    )));
                }
                Relation::AContainsB | Relation::BContainsA | Relation::Disjoint => {}
            }
        }
    }

    for i in 0..n {
        let mut parent: Option<usize> = None;
        let mut parent_area = u64::MAX;
        for j in 0..n {
            if i == j {
                continue;
            }
            if relation(&plans[j], &plans[i]) != Relation::AContainsB {
                continue;
            }
            let area = row_area(&plans[j]);
            let take = area < parent_area
                || (area == parent_area
                    && plans[j].id < parent.map(|p| plans[p].id.clone()).unwrap_or_default());
            if take {
                parent_area = area;
                parent = Some(j);
            }
        }
        if let Some(p) = parent {
            plans[i].parent = Some(plans[p].id.clone());
        }
    }

    let index: BTreeMap<String, usize> = plans
        .iter()
        .enumerate()
        .map(|(i, f)| (f.id.clone(), i))
        .collect();
    for i in 0..n {
        let mut d = 0u32;
        let mut cur = plans[i].parent.clone();
        let mut guard = 0u32;
        while let Some(p) = cur {
            d += 1;
            cur = index.get(&p).and_then(|&j| plans[j].parent.clone());
            guard += 1;
            if guard > n as u32 {
                return Err(crate::layout::sequence::seq_err(format!(
                    "sequence: infeasible: fragment `{}` nesting cycle",
                    plans[i].id
                )));
            }
        }
        plans[i].depth = d;
    }
    Ok(())
}

fn row_area(f: &FragmentPlan) -> u64 {
    u64::from(f.end_row.saturating_sub(f.start_row) + 1)
        * u64::from(f.lifeline_hi.saturating_sub(f.lifeline_lo) + 1)
}
