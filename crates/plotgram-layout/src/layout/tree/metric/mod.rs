//! Metric: layered centered subtree placement in canonical TB space.

use std::collections::BTreeMap;

use plotgram_engine_api::LayoutError;
use plotgram_model::geometry::{Rect, Size};
use plotgram_model::sizes::NodeSizes;

use super::params::TreeParams;
use super::plan::TreePlan;

pub fn assign(
    plan: &TreePlan,
    params: &TreeParams,
    sizes: &NodeSizes,
) -> Result<BTreeMap<String, Rect>, LayoutError> {
    let mut size_of: BTreeMap<&str, Size> = BTreeMap::new();
    for id in &plan.nodes {
        let s = sizes
            .get(id)
            .ok_or_else(|| plotgram_model::MissingNodeSize {
                node_id: id.clone(),
            })?;
        size_of.insert(id.as_str(), s);
    }

    let mut subtree_w: BTreeMap<String, f64> = BTreeMap::new();
    for id in &plan.nodes {
        let _ = subtree_width(id, plan, &size_of, params.node_gap, &mut subtree_w);
    }

    let mut layer_h: BTreeMap<u32, f64> = BTreeMap::new();
    for id in &plan.nodes {
        let d = *plan.depth.get(id).unwrap_or(&0);
        let h = size_of[id.as_str()].height;
        layer_h
            .entry(d)
            .and_modify(|v| *v = (*v).max(h))
            .or_insert(h);
    }
    let max_depth = plan.depth.values().copied().max().unwrap_or(0);
    let mut layer_y = BTreeMap::new();
    let mut y = 0.0;
    for d in 0..=max_depth {
        layer_y.insert(d, y);
        y += layer_h.get(&d).copied().unwrap_or(0.0) + params.layer_gap;
    }

    let mut frames = BTreeMap::new();
    let mut cursor = 0.0;
    for root in &plan.roots {
        let w = *subtree_w.get(root.as_str()).unwrap_or(&0.0);
        place(
            root,
            cursor,
            plan,
            params,
            &size_of,
            &subtree_w,
            &layer_y,
            &mut frames,
        );
        cursor += w + params.node_gap;
    }
    Ok(frames)
}

fn subtree_width(
    id: &str,
    plan: &TreePlan,
    size_of: &BTreeMap<&str, Size>,
    gap: f64,
    memo: &mut BTreeMap<String, f64>,
) -> f64 {
    if let Some(&w) = memo.get(id) {
        return w;
    }
    let own = size_of.get(id).map(|s| s.width).unwrap_or(0.0);
    let kids = plan.children_of(id);
    let w = if kids.is_empty() {
        own
    } else {
        let span: f64 = kids
            .iter()
            .map(|k| subtree_width(k, plan, size_of, gap, memo))
            .sum::<f64>()
            + gap * (kids.len().saturating_sub(1) as f64);
        own.max(span)
    };
    memo.insert(id.to_string(), w);
    w
}

#[allow(clippy::too_many_arguments)]
fn place(
    id: &str,
    x_left: f64,
    plan: &TreePlan,
    params: &TreeParams,
    size_of: &BTreeMap<&str, Size>,
    subtree_w: &BTreeMap<String, f64>,
    layer_y: &BTreeMap<u32, f64>,
    frames: &mut BTreeMap<String, Rect>,
) {
    let size = size_of[id];
    let w = *subtree_w.get(id).unwrap_or(&size.width);
    let d = *plan.depth.get(id).unwrap_or(&0);
    let y = *layer_y.get(&d).unwrap_or(&0.0);
    let cx = x_left + w / 2.0;
    frames.insert(
        id.to_string(),
        Rect::new(cx - size.width / 2.0, y, size.width, size.height),
    );

    let kids = plan.children_of(id);
    if kids.is_empty() {
        return;
    }
    let span: f64 = kids
        .iter()
        .map(|k| *subtree_w.get(k.as_str()).unwrap_or(&0.0))
        .sum::<f64>()
        + params.node_gap * (kids.len().saturating_sub(1) as f64);
    let mut x = x_left + (w - span).max(0.0) / 2.0;
    for k in kids {
        let kw = *subtree_w.get(k.as_str()).unwrap_or(&0.0);
        place(k, x, plan, params, size_of, subtree_w, layer_y, frames);
        x += kw + params.node_gap;
    }
}
