//! Metric: lifeline x, message row y, terminals, activation frames.
//!
//! Writer: CoordWriter. Does not change route topo or row assignment.

use std::collections::BTreeMap;

use tautcore_engine_api::LayoutError;
use tautcore_model::geometry::{Point, Rect};
use tautcore_model::sizes::NodeSizes;

use super::demand::{SeqDemandBoard, SeqDemandKey};
use super::params::{SelfLoopRowPolicy, SequenceParams};
use super::plan::{
    terminal_dx, FragmentPlan, MessageRouteTopo, MessageTerminals, SeqMetric, SeqPlan,
    FRAGMENT_PAD, FRAGMENT_PAD_STEP, FRAGMENT_TITLE_H,
};

const LIFELINE_BOTTOM_PAD: f64 = 24.0;
const EPS: f64 = 1e-9;

pub fn assign(
    plan: &SeqPlan,
    params: &SequenceParams,
    sizes: &NodeSizes,
    demand: &SeqDemandBoard,
) -> Result<SeqMetric, LayoutError> {
    let n = plan.lifeline_order.len();
    let mut widths = Vec::with_capacity(n);
    let mut heights = Vec::with_capacity(n);
    let mut header_bottom: f64 = 0.0;

    for id in &plan.lifeline_order {
        let size = sizes
            .get(id)
            .ok_or_else(|| tautcore_model::MissingNodeSize {
                node_id: id.clone(),
            })?;
        widths.push(size.width);
        heights.push(size.height);
        header_bottom = header_bottom.max(size.height);
    }

    let mut gaps = vec![0.0; n.saturating_sub(1)];
    for (i, gap) in gaps.iter_mut().enumerate() {
        *gap = demand.get(SeqDemandKey::LifelineGap(i as u32));
    }

    let mut x_left = vec![0.0; n];
    let mut cursor = 0.0;
    for i in 0..n {
        x_left[i] = cursor;
        if i + 1 < n {
            cursor += widths[i] + gaps[i];
        }
    }
    let mut centers: Vec<f64> = (0..n).map(|i| x_left[i] + widths[i] / 2.0).collect();

    let mut span_list: Vec<(u32, u32, f64)> = demand.spans().collect();
    span_list.sort_by_key(|(lo, hi, _)| (*hi, *lo));
    for (lo, hi, need) in span_list {
        if (hi as usize) >= n || (lo as usize) >= n || hi <= lo {
            continue;
        }
        let dist = centers[hi as usize] - centers[lo as usize];
        if dist + EPS < need {
            let deficit = need - dist;
            for k in hi as usize..n {
                x_left[k] += deficit;
                centers[k] += deficit;
            }
        }
    }

    let mut participant_frames = BTreeMap::new();
    let mut lifeline_x = BTreeMap::new();
    for (i, id) in plan.lifeline_order.iter().enumerate() {
        participant_frames.insert(id.clone(), Rect::new(x_left[i], 0.0, widths[i], heights[i]));
        lifeline_x.insert(id.clone(), centers[i]);
    }

    let n_rows = plan.row_count as usize;
    let mut row_height = vec![0.0; n_rows];
    let mut extra_before = vec![0.0; n_rows];
    for r in 0..n_rows {
        row_height[r] = demand.get(SeqDemandKey::RowHeight(r as u32));
        extra_before[r] = demand.get(SeqDemandKey::RowPadBefore(r as u32));
    }

    let mut row_y = vec![0.0; n_rows];
    if n_rows > 0 {
        row_y[0] =
            header_bottom + params.first_message_offset + extra_before[0] + row_height[0] / 2.0;
        for r in 1..n_rows {
            row_y[r] =
                row_y[r - 1] + row_height[r - 1] / 2.0 + extra_before[r] + row_height[r] / 2.0;
        }
    }

    let lifeline_y0 = header_bottom;
    let lifeline_y1 = if n_rows == 0 {
        header_bottom + params.first_message_offset
    } else {
        row_y[n_rows - 1] + row_height[n_rows - 1] / 2.0 + LIFELINE_BOTTOM_PAD
    };

    let mut message_terminals = BTreeMap::new();
    for msg in &plan.messages {
        let x_from = *lifeline_x.get(&msg.from).ok_or_else(|| {
            super::seq_err(format!(
                "sequence: invariant: missing lifeline_x for `{}`",
                msg.from
            ))
        })?;
        let x_to = *lifeline_x.get(&msg.to).ok_or_else(|| {
            super::seq_err(format!(
                "sequence: invariant: missing lifeline_x for `{}`",
                msg.to
            ))
        })?;
        let (y_from, y_to) = match msg.route {
            MessageRouteTopo::Horizontal => {
                let y = row_centre(&row_y, msg.row)?;
                (y, y)
            }
            MessageRouteTopo::SelfLoop { .. }
                if matches!(params.self_loop_row_policy, SelfLoopRowPolicy::SingleTall)
                    && msg.end_row == msg.row =>
            {
                let y = row_centre(&row_y, msg.row)?;
                let half = (row_height[msg.row as usize] * 0.25).max(6.0);
                (y - half, y + half)
            }
            MessageRouteTopo::SelfLoop { .. } => (
                row_centre(&row_y, msg.row)?,
                row_centre(&row_y, msg.end_row)?,
            ),
        };

        let from_pt = Point {
            x: x_from
                + terminal_dx(
                    msg.attach.from.side,
                    msg.attach.from.activation_depth,
                    params.activation_width,
                    params.activation_inset,
                    params.message_endpoint_inset,
                ),
            y: y_from,
        };
        let to_pt = Point {
            x: x_to
                + terminal_dx(
                    msg.attach.to.side,
                    msg.attach.to.activation_depth,
                    params.activation_width,
                    params.activation_inset,
                    params.message_endpoint_inset,
                ),
            y: y_to,
        };
        message_terminals.insert(
            msg.edge_id.clone(),
            MessageTerminals {
                from: from_pt,
                to: to_pt,
            },
        );
    }

    let mut activation_frames = BTreeMap::new();
    for span in &plan.activation_spans {
        let cx = *lifeline_x.get(&span.lifeline).ok_or_else(|| {
            super::seq_err(format!(
                "sequence: invariant: missing lifeline_x for activation `{}`",
                span.id
            ))
        })?;
        let y_start = row_band_top(&row_y, &row_height, span.start_row)?;
        let y_end = row_band_bottom(&row_y, &row_height, span.end_row)?;
        let x =
            cx - params.activation_width / 2.0 + f64::from(span.depth) * params.activation_inset;
        activation_frames.insert(
            span.id.clone(),
            Rect::new(
                x,
                y_start,
                params.activation_width,
                (y_end - y_start).max(1.0),
            ),
        );
    }

    let mut lifeline_crossings: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for msg in &plan.messages {
        if !matches!(msg.route, MessageRouteTopo::Horizontal) {
            continue;
        }
        let Some(terms) = message_terminals.get(&msg.edge_id) else {
            continue;
        };
        let x0 = terms.from.x.min(terms.to.x);
        let x1 = terms.from.x.max(terms.to.x);
        let y = terms.from.y;
        for id in &plan.lifeline_order {
            if id == &msg.from || id == &msg.to {
                continue;
            }
            let x = lifeline_x[id];
            if x0 < x && x < x1 {
                lifeline_crossings.entry(id.clone()).or_default().push(y);
            }
        }
    }
    for ys in lifeline_crossings.values_mut() {
        ys.sort_by(|a, b| a.total_cmp(b));
        ys.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    }

    let (fragment_frames, fragment_operand_ys) =
        assign_fragment_frames(plan, &participant_frames, &row_y, &row_height)?;

    Ok(SeqMetric {
        participant_frames,
        lifeline_x,
        row_y,
        row_height,
        message_terminals,
        header_bottom,
        lifeline_y0,
        lifeline_y1,
        activation_frames,
        lifeline_crossings,
        fragment_frames,
        fragment_operand_ys,
    })
}

fn row_centre(row_y: &[f64], row: u32) -> Result<f64, LayoutError> {
    row_y
        .get(row as usize)
        .copied()
        .ok_or_else(|| super::seq_err(format!("sequence: invariant: row {row} out of range")))
}

fn row_band_top(row_y: &[f64], row_height: &[f64], row: u32) -> Result<f64, LayoutError> {
    let i = row as usize;
    let y = row_centre(row_y, row)?;
    let h = *row_height
        .get(i)
        .ok_or_else(|| super::seq_err(format!("sequence: invariant: row {row} height missing")))?;
    Ok(y - h * 0.35)
}

fn row_band_bottom(row_y: &[f64], row_height: &[f64], row: u32) -> Result<f64, LayoutError> {
    let i = row as usize;
    let y = row_centre(row_y, row)?;
    let h = *row_height
        .get(i)
        .ok_or_else(|| super::seq_err(format!("sequence: invariant: row {row} height missing")))?;
    Ok(y + h * 0.35)
}

fn assign_fragment_frames(
    plan: &SeqPlan,
    participant_frames: &BTreeMap<String, Rect>,
    row_y: &[f64],
    row_height: &[f64],
) -> Result<(BTreeMap<String, Rect>, BTreeMap<String, Vec<f64>>), LayoutError> {
    let mut frames: BTreeMap<String, Rect> = BTreeMap::new();
    let mut operand_ys: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    if plan.fragments.is_empty() {
        return Ok((frames, operand_ys));
    }

    let mut order: Vec<usize> = (0..plan.fragments.len()).collect();
    order.sort_by(|a, b| {
        plan.fragments[*b]
            .depth
            .cmp(&plan.fragments[*a].depth)
            .then_with(|| plan.fragments[*a].id.cmp(&plan.fragments[*b].id))
    });

    for &idx in &order {
        let frag = &plan.fragments[idx];
        let content = fragment_content_rect(plan, frag, participant_frames, row_y, row_height)?;
        let mut r = content;
        for child in &plan.fragments {
            if child.parent.as_deref() == Some(frag.id.as_str()) {
                if let Some(cf) = frames.get(&child.id) {
                    r = union_rect(r, *cf);
                }
            }
        }
        let pad = if plan
            .fragments
            .iter()
            .any(|c| c.parent.as_deref() == Some(frag.id.as_str()))
        {
            FRAGMENT_PAD_STEP
        } else {
            FRAGMENT_PAD
        };
        r = inflate_rect(r, pad);
        frames.insert(frag.id.clone(), r);

        let mut ys = Vec::new();
        for &split_row in &frag.operand_splits {
            let y = if (split_row as usize) + 1 < row_y.len() {
                (row_centre(row_y, split_row)? + row_centre(row_y, split_row + 1)?) / 2.0
            } else {
                row_band_bottom(row_y, row_height, split_row)?
            };
            let y = y.clamp(r.y + FRAGMENT_TITLE_H, r.bottom() - 1.0);
            ys.push(y);
        }
        operand_ys.insert(frag.id.clone(), ys);
    }

    Ok((frames, operand_ys))
}

fn fragment_content_rect(
    plan: &SeqPlan,
    frag: &FragmentPlan,
    participant_frames: &BTreeMap<String, Rect>,
    row_y: &[f64],
    row_height: &[f64],
) -> Result<Rect, LayoutError> {
    let lo_id = &plan.lifeline_order[frag.lifeline_lo as usize];
    let hi_id = &plan.lifeline_order[frag.lifeline_hi as usize];
    let left = participant_frames.get(lo_id).ok_or_else(|| {
        super::seq_err(format!(
            "sequence: invariant: fragment `{}` missing frame `{lo_id}`",
            frag.id
        ))
    })?;
    let right = participant_frames.get(hi_id).ok_or_else(|| {
        super::seq_err(format!(
            "sequence: invariant: fragment `{}` missing frame `{hi_id}`",
            frag.id
        ))
    })?;
    let ancestors_same_start = {
        let mut n = 0u32;
        let mut cur = frag.parent.clone();
        while let Some(pid) = cur {
            let Some(p) = plan.fragments.iter().find(|f| f.id == pid) else {
                break;
            };
            if p.start_row == frag.start_row {
                n += 1;
            }
            cur = p.parent.clone();
        }
        n
    };
    let y0 = row_band_top(row_y, row_height, frag.start_row)?
        - FRAGMENT_TITLE_H * f64::from(ancestors_same_start + 1);
    let y1 = row_band_bottom(row_y, row_height, frag.end_row)?;
    Ok(Rect::new(
        left.x,
        y0,
        (right.right() - left.x).max(1.0),
        (y1 - y0).max(1.0),
    ))
}

fn union_rect(a: Rect, b: Rect) -> Rect {
    let x0 = a.x.min(b.x);
    let y0 = a.y.min(b.y);
    let x1 = a.right().max(b.right());
    let y1 = a.bottom().max(b.bottom());
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}

fn inflate_rect(r: Rect, pad: f64) -> Rect {
    Rect::new(
        r.x - pad,
        r.y - pad,
        r.width + 2.0 * pad,
        r.height + 2.0 * pad,
    )
}
