//! Sequence DemandBoard (architecture.md §3.3).
//!
//! M1 publishes adjacent-lifeline gaps, multi-lifeline label spans, row
//! heights (including label / self-call floors), and activation-bar clearance.

use std::collections::BTreeMap;

use tautcore_model::graph::Graph;
use tautcore_model::sizes::NodeSizes;

use super::params::SequenceParams;
use super::plan::{MessageKind, SeqPlan, FRAGMENT_PAD, FRAGMENT_TITLE_H};

const ASCII_EM: f64 = 7.2;
const CJK_FACTOR: f64 = 1.65;
const LABEL_PAD_X: f64 = 12.0;
const LABEL_H: f64 = 14.0;
const ACTIVATION_CLEAR_PAD: f64 = 4.0;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum SeqDemandKey {
    /// Adjacent lifelines `i` and `i+1` (order indices). Gap between frames.
    LifelineGap(u32),
    /// Centre-to-centre lower bound across `lo..hi` (inclusive indices, `lo < hi`).
    LifelineSpan {
        lo: u32,
        hi: u32,
    },
    RowHeight(u32),
    /// Extra gap inserted *before* this row (fragment title bands).
    RowPadBefore(u32),
}

#[derive(Debug, Clone, Default)]
pub struct SeqDemandBoard {
    values: BTreeMap<SeqDemandKey, f64>,
    frozen: bool,
}

impl SeqDemandBoard {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn publish(&mut self, key: SeqDemandKey, lower_bound: f64) {
        assert!(
            !self.frozen,
            "sequence DemandBoard: publish after freeze (phase-order invariant)"
        );
        assert!(
            lower_bound.is_finite() && lower_bound >= 0.0,
            "sequence DemandBoard: InternalInvariant — publish requires finite \
             non-negative lower_bound, got {lower_bound}"
        );
        self.values
            .entry(key)
            .and_modify(|v| *v = (*v).max(lower_bound))
            .or_insert(lower_bound);
    }

    pub fn freeze(&mut self) {
        self.frozen = true;
    }

    pub fn get(&self, key: SeqDemandKey) -> f64 {
        self.values.get(&key).copied().unwrap_or(0.0)
    }

    pub fn spans(&self) -> impl Iterator<Item = (u32, u32, f64)> + '_ {
        self.values.iter().filter_map(|(k, v)| match k {
            SeqDemandKey::LifelineSpan { lo, hi } => Some((*lo, *hi, *v)),
            _ => None,
        })
    }
}

/// Approximate label width (layout must not depend on `tautcore-content`).
pub fn estimate_label_width(text: &str) -> f64 {
    let mut w = 0.0;
    for ch in text.chars() {
        w += if (ch as u32) > 0x7f {
            ASCII_EM * CJK_FACTOR
        } else {
            ASCII_EM
        };
    }
    w
}

/// Publish floors then freeze. Metric must not publish.
pub fn publish_floors(
    graph: &Graph,
    plan: &SeqPlan,
    params: &SequenceParams,
    sizes: &NodeSizes,
) -> SeqDemandBoard {
    let mut board = SeqDemandBoard::new();
    let n = plan.lifeline_order.len();

    if n > 1 {
        for i in 0..n - 1 {
            board.publish(SeqDemandKey::LifelineGap(i as u32), params.participant_gap);
        }
    }

    let mut max_depth: Vec<Option<u32>> = vec![None; n];
    for span in &plan.activation_spans {
        if let Some(i) = plan.lifeline_index(&span.lifeline) {
            max_depth[i] = Some(max_depth[i].unwrap_or(0).max(span.depth));
        }
    }
    if n > 1 {
        for i in 0..n - 1 {
            let east = bar_east(params, max_depth[i]);
            let west = bar_west(params, max_depth[i + 1]);
            if east > 0.0 || west > 0.0 {
                board.publish(
                    SeqDemandKey::LifelineGap(i as u32),
                    east + west + ACTIVATION_CLEAR_PAD,
                );
            }
        }
    }

    if params.label_to_gap {
        for msg in &plan.messages {
            let Some(edge) = graph.find_edge(&msg.edge_id) else {
                continue;
            };
            let Some(text) = edge.label.as_deref() else {
                continue;
            };
            if text.is_empty() {
                continue;
            }
            let Some(fi) = plan.lifeline_index(&msg.from) else {
                continue;
            };
            let Some(ti) = plan.lifeline_index(&msg.to) else {
                continue;
            };
            if fi == ti {
                continue;
            }
            let lo = fi.min(ti) as u32;
            let hi = fi.max(ti) as u32;
            let label_w = estimate_label_width(text) + LABEL_PAD_X;
            if hi == lo + 1 {
                let w_lo = sizes
                    .get(&plan.lifeline_order[lo as usize])
                    .map(|s| s.width)
                    .unwrap_or(0.0);
                let w_hi = sizes
                    .get(&plan.lifeline_order[hi as usize])
                    .map(|s| s.width)
                    .unwrap_or(0.0);
                let gap_need = (label_w - w_lo / 2.0 - w_hi / 2.0).max(0.0);
                board.publish(SeqDemandKey::LifelineGap(lo), gap_need);
            } else {
                board.publish(SeqDemandKey::LifelineSpan { lo, hi }, label_w);
            }
        }
    }

    for r in 0..plan.row_count {
        let mut h = params.message_gap;
        if matches!(
            params.self_loop_row_policy,
            super::params::SelfLoopRowPolicy::SingleTall
        ) {
            let uses_row = plan
                .messages
                .iter()
                .any(|m| m.kind == MessageKind::SelfCall && m.row == r && m.end_row == r);
            if uses_row {
                h = h.max(params.message_gap.max(16.0));
            }
        }
        let labeled = plan.messages.iter().any(|m| {
            m.row == r
                && graph
                    .find_edge(&m.edge_id)
                    .and_then(|e| e.label.as_ref())
                    .is_some_and(|t| !t.is_empty())
        });
        if labeled {
            h = h.max(LABEL_H + 4.0);
        }
        board.publish(SeqDemandKey::RowHeight(r), h);
    }
    let mut pad_before: BTreeMap<u32, f64> = BTreeMap::new();
    for frag in &plan.fragments {
        *pad_before.entry(frag.start_row).or_insert(0.0) += FRAGMENT_TITLE_H + FRAGMENT_PAD;
    }
    for (r, v) in pad_before {
        board.publish(SeqDemandKey::RowPadBefore(r), v);
    }
    board.freeze();
    board
}

fn bar_east(params: &SequenceParams, max_depth: Option<u32>) -> f64 {
    match max_depth {
        None => 0.0,
        Some(d) => {
            params.activation_width / 2.0
                + f64::from(d) * params.activation_inset
                + params.message_endpoint_inset
        }
    }
}

fn bar_west(params: &SequenceParams, max_depth: Option<u32>) -> f64 {
    match max_depth {
        None => 0.0,
        Some(_) => params.activation_width / 2.0 + params.message_endpoint_inset,
    }
}
