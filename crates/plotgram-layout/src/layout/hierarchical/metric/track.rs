//! Publish main/cross coordinates for Channel TrackOrder lanes.

use std::collections::BTreeMap;

use plotgram_algo::orientation::Size;

use crate::layout::hierarchical::channel::{ChannelRoutePlan, TrackId, TrackOrient};
use crate::layout::hierarchical::compose::track_order::TrackOrderPlan;
use crate::layout::hierarchical::model::PlanGraph;

/// Per-substrate-track lane coordinates (main Y for Cross, cross X for Main).
#[derive(Debug, Default, Clone)]
pub struct TrackCoords {
    pub coords: BTreeMap<TrackId, Vec<f64>>,
}

impl TrackCoords {
    pub fn lane(&self, track: TrackId, index: u32) -> Option<f64> {
        self.coords
            .get(&track)
            .and_then(|v| v.get(index as usize))
            .copied()
    }
}

/// Place lanes inside each used substrate track.
pub fn assign_track_coords(
    plan: &PlanGraph,
    main: &[f64],
    cross_centers: &[f64],
    size_of: &dyn Fn(usize) -> Size,
    track_order: &TrackOrderPlan,
    route_plan: &ChannelRoutePlan,
    edge_gap: f64,
) -> TrackCoords {
    let mut out = TrackCoords::default();
    for (&tid, &count) in &track_order.track_counts {
        if count == 0 {
            continue;
        }
        let Some(t) = route_plan.substrate.track(tid) else {
            continue;
        };
        let ys = match t.orient {
            TrackOrient::Cross => {
                let line = t.line;
                if line == 0 || line > plan.layers.len() {
                    // Outside the stack — park at mid of adjacent layer if any.
                    let y = if line == 0 {
                        main.get(plan.layers[0][0]).copied().unwrap_or(0.0) - edge_gap
                    } else {
                        let last = plan.layers.len() - 1;
                        let e = plan.layers[last][0];
                        main[e] + size_of(e).height + edge_gap
                    };
                    vec![y; count]
                } else {
                    let r = line - 1;
                    let thickness = plan.layers[r]
                        .iter()
                        .map(|&e| size_of(e).height)
                        .fold(0.0_f64, f64::max);
                    let gap_top = main[plan.layers[r][0]] + thickness;
                    let gap_bot = main[plan.layers[r + 1][0]];
                    let usable = (gap_bot - gap_top).max(0.0);
                    let span = (count.saturating_sub(1) as f64) * edge_gap;
                    let start = gap_top + (usable - span) * 0.5;
                    (0..count)
                        .map(|i| start + i as f64 * edge_gap)
                        .collect()
                }
            }
            TrackOrient::Main => {
                // Vertical corridor X: center between order columns, then pitch.
                let og = t.line;
                let backbone = main_line_backbone_x(plan, cross_centers, size_of, og);
                let span = (count.saturating_sub(1) as f64) * edge_gap;
                let start = backbone - span * 0.5;
                (0..count)
                    .map(|i| start + i as f64 * edge_gap)
                    .collect()
            }
        };
        out.coords.insert(tid, ys);
    }
    out
}

fn main_line_backbone_x(
    plan: &PlanGraph,
    cross_centers: &[f64],
    size_of: &dyn Fn(usize) -> Size,
    order_gap: usize,
) -> f64 {
    let _ = size_of;
    // order_gap `og` sits between column og-1 and og (outside → inset).
    let mut xs = Vec::new();
    for layer in &plan.layers {
        if layer.is_empty() {
            continue;
        }
        if order_gap == 0 {
            xs.push(cross_centers[layer[0]]);
        } else if order_gap >= layer.len() {
            xs.push(cross_centers[*layer.last().unwrap()]);
        } else {
            xs.push(0.5 * (cross_centers[layer[order_gap - 1]] + cross_centers[layer[order_gap]]));
        }
    }
    if xs.is_empty() {
        0.0
    } else {
        xs.iter().sum::<f64>() / xs.len() as f64
    }
}
