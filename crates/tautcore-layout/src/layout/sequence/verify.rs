//! Plan / Metric / Ink verifiers (architecture.md §9). Fail hard, never patch.

use tautcore_engine_api::LayoutError;
use tautcore_model::geometry::Rect;
use tautcore_model::result::EdgePlacement;
use std::collections::BTreeMap;

use super::demand::{SeqDemandBoard, SeqDemandKey};
use super::params::SequenceParams;
use super::plan::{terminal_dx, MessageKind, MessageRouteTopo, SeqMetric, SeqPlan};

const EPS: f64 = 1e-6;

pub fn verify_plan(plan: &SeqPlan) -> Result<(), LayoutError> {
    let mut seen = std::collections::BTreeSet::new();
    for id in &plan.lifeline_order {
        if !seen.insert(id.as_str()) {
            return Err(super::seq_err(format!(
                "sequence: invariant: duplicate lifeline `{id}`"
            )));
        }
    }
    for msg in &plan.messages {
        if plan.lifeline_index(&msg.from).is_none() || plan.lifeline_index(&msg.to).is_none() {
            return Err(super::seq_err(format!(
                "sequence: invariant: message `{}` endpoints not in lifeline_order",
                msg.edge_id
            )));
        }
        if msg.row >= plan.row_count || msg.end_row >= plan.row_count {
            return Err(super::seq_err(format!(
                "sequence: invariant: message `{}` row out of range",
                msg.edge_id
            )));
        }
        if msg.end_row < msg.row {
            return Err(super::seq_err(format!(
                "sequence: invariant: message `{}` end_row < row",
                msg.edge_id
            )));
        }
        match (msg.kind, msg.route) {
            (MessageKind::SelfCall, MessageRouteTopo::SelfLoop { .. }) => {}
            (MessageKind::SelfCall, _) => {
                return Err(super::seq_err(format!(
                    "sequence: invariant: SelfCall `{}` route is not SelfLoop",
                    msg.edge_id
                )));
            }
            (_, MessageRouteTopo::SelfLoop { .. }) => {
                return Err(super::seq_err(format!(
                    "sequence: invariant: non-self `{}` has SelfLoop topo",
                    msg.edge_id
                )));
            }
            _ => {}
        }
        if msg.from == msg.to && msg.kind != MessageKind::SelfCall {
            return Err(super::seq_err(format!(
                "sequence: invariant: same-endpoint `{}` is not SelfCall",
                msg.edge_id
            )));
        }
    }
    for span in &plan.activation_spans {
        if plan.lifeline_index(&span.lifeline).is_none() {
            return Err(super::seq_err(format!(
                "sequence: invariant: activation `{}` lifeline not in order",
                span.id
            )));
        }
        if span.start_row > span.end_row {
            return Err(super::seq_err(format!(
                "sequence: invariant: activation `{}` start_row > end_row",
                span.id
            )));
        }
        if span.end_row >= plan.row_count && plan.row_count > 0 {
            return Err(super::seq_err(format!(
                "sequence: invariant: activation `{}` row out of range",
                span.id
            )));
        }
    }
    for (id, slot) in &plan.lifeline_pins {
        match plan.lifeline_order.get(*slot as usize) {
            Some(got) if got == id => {}
            _ => {
                return Err(super::seq_err(format!(
                    "sequence: invariant: pin `{id}` @ {slot} not honoured"
                )));
            }
        }
    }
    verify_fragment_plan(plan)?;
    Ok(())
}

fn verify_fragment_plan(plan: &SeqPlan) -> Result<(), LayoutError> {
    let n_ll = plan.lifeline_order.len() as u32;
    let mut by_id: BTreeMap<&str, &super::plan::FragmentPlan> = BTreeMap::new();
    for f in &plan.fragments {
        if by_id.insert(f.id.as_str(), f).is_some() {
            return Err(super::seq_err(format!(
                "sequence: invariant: duplicate fragment `{}`",
                f.id
            )));
        }
        if f.start_row > f.end_row
            || f.end_row >= plan.row_count && plan.row_count > 0
            || f.lifeline_lo > f.lifeline_hi
            || f.lifeline_hi >= n_ll
        {
            return Err(super::seq_err(format!(
                "sequence: invariant: fragment `{}` span out of range",
                f.id
            )));
        }
        if let Some(p) = &f.parent {
            let Some(parent) = plan.fragments.iter().find(|x| x.id == *p) else {
                return Err(super::seq_err(format!(
                    "sequence: invariant: fragment `{}` parent `{p}` missing",
                    f.id
                )));
            };
            if !(parent.start_row <= f.start_row
                && parent.end_row >= f.end_row
                && parent.lifeline_lo <= f.lifeline_lo
                && parent.lifeline_hi >= f.lifeline_hi)
            {
                return Err(super::seq_err(format!(
                    "sequence: invariant: fragment `{}` is not nested inside `{p}`",
                    f.id
                )));
            }
        }
    }
    Ok(())
}

pub fn verify_metric(
    plan: &SeqPlan,
    metric: &SeqMetric,
    demand: &SeqDemandBoard,
    params: &SequenceParams,
) -> Result<(), LayoutError> {
    let mut prev_x = f64::NEG_INFINITY;
    for id in &plan.lifeline_order {
        let x = metric.lifeline_x.get(id).copied().ok_or_else(|| {
            super::seq_err(format!("sequence: invariant: missing lifeline_x `{id}`"))
        })?;
        if !x.is_finite() {
            return Err(super::seq_err(format!(
                "sequence: invariant: non-finite lifeline_x `{id}`"
            )));
        }
        if x + EPS < prev_x {
            return Err(super::seq_err(
                "sequence: invariant: lifeline x is not monotonic",
            ));
        }
        prev_x = x;
        if metric.participant_frames.get(id).is_none() {
            return Err(super::seq_err(format!(
                "sequence: invariant: missing participant frame `{id}`"
            )));
        }
    }
    let mut prev_y = f64::NEG_INFINITY;
    for (i, &y) in metric.row_y.iter().enumerate() {
        if !y.is_finite() {
            return Err(super::seq_err(format!(
                "sequence: invariant: non-finite row_y[{i}]"
            )));
        }
        if y + EPS < prev_y {
            return Err(super::seq_err(
                "sequence: invariant: row_y is not monotonic",
            ));
        }
        prev_y = y;
    }
    if !metric.header_bottom.is_finite() {
        return Err(super::seq_err(
            "sequence: invariant: non-finite header_bottom",
        ));
    }
    if metric.row_y.len() != plan.row_count as usize
        || metric.row_height.len() != plan.row_count as usize
    {
        return Err(super::seq_err(
            "sequence: invariant: row_y/row_height length != row_count",
        ));
    }
    for msg in &plan.messages {
        let terms = metric.message_terminals.get(&msg.edge_id).ok_or_else(|| {
            super::seq_err(format!(
                "sequence: invariant: missing terminals for `{}`",
                msg.edge_id
            ))
        })?;
        let axis_from = metric.lifeline_x[&msg.from];
        let axis_to = metric.lifeline_x[&msg.to];
        let expect_from = axis_from
            + terminal_dx(
                msg.attach.from.side,
                msg.attach.from.activation_depth,
                params.activation_width,
                params.activation_inset,
                params.message_endpoint_inset,
            );
        let expect_to = axis_to
            + terminal_dx(
                msg.attach.to.side,
                msg.attach.to.activation_depth,
                params.activation_width,
                params.activation_inset,
                params.message_endpoint_inset,
            );
        if (terms.from.x - expect_from).abs() > EPS || (terms.to.x - expect_to).abs() > EPS {
            return Err(super::seq_err(format!(
                "sequence: invariant: terminals of `{}` not on axis ± inset",
                msg.edge_id
            )));
        }
    }

    for i in 0..plan.lifeline_order.len().saturating_sub(1) {
        let lo = &plan.lifeline_order[i];
        let hi = &plan.lifeline_order[i + 1];
        let a = metric.participant_frames[lo];
        let b = metric.participant_frames[hi];
        let gap = b.x - a.right();
        let need = demand.get(SeqDemandKey::LifelineGap(i as u32));
        if gap + EPS < need {
            return Err(super::seq_err(format!(
                "sequence: invariant: LifelineGap({i}) {gap} < demand {need}"
            )));
        }
    }
    for (lo, hi, need) in demand.spans() {
        let x_lo = metric.lifeline_x[&plan.lifeline_order[lo as usize]];
        let x_hi = metric.lifeline_x[&plan.lifeline_order[hi as usize]];
        let dist = x_hi - x_lo;
        if dist + EPS < need {
            return Err(super::seq_err(format!(
                "sequence: invariant: LifelineSpan({lo},{hi}) {dist} < demand {need}"
            )));
        }
    }

    for span in &plan.activation_spans {
        if metric.activation_frames.get(&span.id).is_none() {
            return Err(super::seq_err(format!(
                "sequence: invariant: missing activation frame `{}`",
                span.id
            )));
        }
    }

    for msg in &plan.messages {
        if !matches!(msg.route, MessageRouteTopo::Horizontal) {
            continue;
        }
        let terms = &metric.message_terminals[&msg.edge_id];
        let x0 = terms.from.x.min(terms.to.x);
        let x1 = terms.from.x.max(terms.to.x);
        for id in &plan.lifeline_order {
            if id == &msg.from || id == &msg.to {
                continue;
            }
            let x = metric.lifeline_x[id];
            if x0 < x && x < x1 {
                let ys = metric
                    .lifeline_crossings
                    .get(id)
                    .map(|v| v.as_slice())
                    .unwrap_or(&[]);
                let recorded = ys.iter().any(|y| (y - terms.from.y).abs() <= EPS);
                if !recorded {
                    return Err(super::seq_err(format!(
                        "sequence: invariant: crossing of `{id}` by `{}` not recorded",
                        msg.edge_id
                    )));
                }
            }
        }
    }
    verify_fragment_metric(plan, metric)?;
    Ok(())
}

fn verify_fragment_metric(plan: &SeqPlan, metric: &SeqMetric) -> Result<(), LayoutError> {
    for f in &plan.fragments {
        let Some(frame) = metric.fragment_frames.get(&f.id).copied() else {
            return Err(super::seq_err(format!(
                "sequence: invariant: missing fragment frame `{}`",
                f.id
            )));
        };
        if !frame_finite(frame) {
            return Err(super::seq_err(format!(
                "sequence: invariant: non-finite fragment frame `{}`",
                f.id
            )));
        }
        if let Some(p) = &f.parent {
            let Some(pf) = metric.fragment_frames.get(p).copied() else {
                return Err(super::seq_err(format!(
                    "sequence: invariant: missing parent fragment frame `{p}`"
                )));
            };
            if !contains_rect(pf, frame, EPS) {
                return Err(super::seq_err(format!(
                    "sequence: invariant: fragment `{}` frame is not nested inside `{p}`",
                    f.id
                )));
            }
        }
    }
    for i in 0..plan.fragments.len() {
        for j in (i + 1)..plan.fragments.len() {
            let a = &plan.fragments[i];
            let b = &plan.fragments[j];
            if a.parent.as_deref() == Some(b.id.as_str())
                || b.parent.as_deref() == Some(a.id.as_str())
            {
                continue;
            }
            if ancestor_of(plan, &a.id, &b.id) || ancestor_of(plan, &b.id, &a.id) {
                continue;
            }
            let fa = metric.fragment_frames[&a.id];
            let fb = metric.fragment_frames[&b.id];
            if rects_overlap(fa, fb, EPS) {
                return Err(super::seq_err(format!(
                    "sequence: invariant: fragment frames `{}` and `{}` overlap",
                    a.id, b.id
                )));
            }
        }
    }
    Ok(())
}

fn ancestor_of(plan: &SeqPlan, ancestor: &str, child: &str) -> bool {
    let mut cur = plan
        .fragments
        .iter()
        .find(|f| f.id == child)
        .and_then(|f| f.parent.clone());
    while let Some(p) = cur {
        if p == ancestor {
            return true;
        }
        cur = plan
            .fragments
            .iter()
            .find(|f| f.id == p)
            .and_then(|f| f.parent.clone());
    }
    false
}

fn frame_finite(r: Rect) -> bool {
    r.x.is_finite() && r.y.is_finite() && r.width.is_finite() && r.height.is_finite()
}

fn contains_rect(outer: Rect, inner: Rect, eps: f64) -> bool {
    outer.x <= inner.x + eps
        && outer.y <= inner.y + eps
        && outer.right() + eps >= inner.right()
        && outer.bottom() + eps >= inner.bottom()
}

fn rects_overlap(a: Rect, b: Rect, eps: f64) -> bool {
    a.x < b.right() - eps
        && b.x < a.right() - eps
        && a.y < b.bottom() - eps
        && b.y < a.bottom() - eps
}

pub fn verify_ink(
    plan: &SeqPlan,
    metric: &SeqMetric,
    edges: &[EdgePlacement],
    _params: &SequenceParams,
) -> Result<(), LayoutError> {
    if edges.len() != plan.messages.len() {
        return Err(super::seq_err(
            "sequence: invariant: ink edge count != message count",
        ));
    }
    for (msg, edge) in plan.messages.iter().zip(edges.iter()) {
        if edge.id != msg.edge_id {
            return Err(super::seq_err(format!(
                "sequence: invariant: ink order drifted at `{}`",
                msg.edge_id
            )));
        }
        let terms = &metric.message_terminals[&msg.edge_id];
        let pts = edge.path.polyline_points().ok_or_else(|| {
            super::seq_err(format!(
                "sequence: invariant: message `{}` path is not a polyline",
                msg.edge_id
            ))
        })?;
        match msg.route {
            MessageRouteTopo::Horizontal => {
                if pts.len() < 2 {
                    return Err(super::seq_err(format!(
                        "sequence: invariant: Horizontal `{}` needs ≥2 points",
                        msg.edge_id
                    )));
                }
                if (pts[0].y - pts[pts.len() - 1].y).abs() > EPS {
                    return Err(super::seq_err(format!(
                        "sequence: invariant: Horizontal `{}` y is not collinear",
                        msg.edge_id
                    )));
                }
            }
            MessageRouteTopo::SelfLoop { .. } => {
                if pts.len() < 4 {
                    return Err(super::seq_err(format!(
                        "sequence: invariant: SelfLoop `{}` needs ≥4 points",
                        msg.edge_id
                    )));
                }
            }
        }
        if !near(pts[0], terms.from) || !near(*pts.last().unwrap(), terms.to) {
            return Err(super::seq_err(format!(
                "sequence: invariant: path ends of `{}` != Metric terminals",
                msg.edge_id
            )));
        }
    }
    Ok(())
}

fn near(a: tautcore_model::geometry::Point, b: tautcore_model::geometry::Point) -> bool {
    (a.x - b.x).abs() <= EPS && (a.y - b.y).abs() <= EPS
}
