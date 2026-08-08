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
    // Real-node frames for Main-lane clearance (InkVerifier node-penetration).
    let mut obstacles: Vec<(f64, f64, f64, f64)> = Vec::new(); // l,t,r,b
    for layer in &plan.layers {
        for &e in layer {
            if plan.elems[e].key.is_zero_width() {
                continue;
            }
            let s = size_of(e);
            let cx = cross_centers[e];
            let y = main[e];
            obstacles.push((
                cx - s.width / 2.0,
                y,
                cx + s.width / 2.0,
                y + s.height,
            ));
        }
    }

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
                if line == 0 || line >= plan.layers.len() {
                    // Outside the stack — park near adjacent layer; still
                    // separate lanes (P5-5).
                    let base = if line == 0 {
                        main.get(plan.layers[0][0]).copied().unwrap_or(0.0) - edge_gap
                    } else {
                        let last = plan.layers.len() - 1;
                        let e = plan.layers[last][0];
                        main[e] + size_of(e).height + edge_gap
                    };
                    let mid = (count.saturating_sub(1) as f64) * 0.5;
                    (0..count)
                        .map(|i| base + (i as f64 - mid) * edge_gap)
                        .collect()
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
                // Vertical corridor X: sit in order *gaps* (or outside the
                // outer columns), never on a node center — otherwise a
                // full-height Main rail penetrates intermediate nodes.
                let og = t.line;
                let mut backbone =
                    main_line_backbone_x(plan, cross_centers, size_of, og, edge_gap);
                backbone = clear_main_x(backbone, &obstacles, edge_gap);
                let max_cols = plan.layers.iter().map(|l| l.len()).max().unwrap_or(0);
                let ys: Vec<f64> = if og == 0 {
                    // West outer: pack further left so parallel returns stay outside.
                    (0..count)
                        .map(|i| {
                            clear_main_x(
                                backbone - i as f64 * edge_gap,
                                &obstacles,
                                edge_gap,
                            )
                        })
                        .collect()
                } else if og >= max_cols {
                    // East outer: pack further right.
                    (0..count)
                        .map(|i| {
                            clear_main_x(
                                backbone + i as f64 * edge_gap,
                                &obstacles,
                                edge_gap,
                            )
                        })
                        .collect()
                } else {
                    let span = (count.saturating_sub(1) as f64) * edge_gap;
                    let start = backbone - span * 0.5;
                    (0..count)
                        .map(|i| {
                            clear_main_x(start + i as f64 * edge_gap, &obstacles, edge_gap)
                        })
                        .collect()
                };
                ys
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
    edge_gap: f64,
) -> f64 {
    let margin = edge_gap.max(1.0) * 0.5;
    let max_cols = plan.layers.iter().map(|l| l.len()).max().unwrap_or(0);
    let mut xs = Vec::new();
    for layer in &plan.layers {
        if layer.is_empty() {
            continue;
        }
        if order_gap == 0 {
            let e = layer[0];
            xs.push(cross_centers[e] - size_of(e).width / 2.0 - margin);
        } else if order_gap >= layer.len() {
            let e = *layer.last().unwrap();
            xs.push(cross_centers[e] + size_of(e).width / 2.0 + margin);
        } else {
            let l = layer[order_gap - 1];
            let r = layer[order_gap];
            let left_face = cross_centers[l] + size_of(l).width / 2.0;
            let right_face = cross_centers[r] - size_of(r).width / 2.0;
            xs.push(0.5 * (left_face + right_face));
        }
    }
    if xs.is_empty() {
        0.0
    } else if order_gap == 0 {
        xs.iter().cloned().fold(f64::INFINITY, f64::min)
    } else if order_gap >= max_cols {
        xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
    } else {
        xs.iter().sum::<f64>() / xs.len() as f64
    }
}

/// Nudge a candidate Main-lane X until it does not pierce any node interior.
///
/// P5-2 wanted this deleted after substrate node occupancy; P5-1 could not
/// punch Main odd cells without breaking Channel search, so clearance stays
/// in the Metric Main-X writer (sole writer — not Ink).
///
/// Overlapping X-projections (same column / tight neighbors) are merged so we
/// cannot oscillate between faces of two overlapping intervals.
fn clear_main_x(x: f64, obstacles: &[(f64, f64, f64, f64)], edge_gap: f64) -> f64 {
    const EPS: f64 = 1e-6;
    let margin = edge_gap.max(1.0) * 0.5;
    let mut intervals: Vec<(f64, f64)> = obstacles.iter().map(|&(l, _, r, _)| (l, r)).collect();
    intervals.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap().then(a.1.partial_cmp(&b.1).unwrap()));
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for (l, r) in intervals {
        if let Some(last) = merged.last_mut() {
            // Merge if overlapping or closer than 2*margin (no room for a lane).
            if l <= last.1 + 2.0 * margin {
                last.1 = last.1.max(r);
            } else {
                merged.push((l, r));
            }
        } else {
            merged.push((l, r));
        }
    }
    for &(l, r) in &merged {
        if x > l + EPS && x < r - EPS {
            return if (x - l) <= (r - x) {
                l - margin
            } else {
                r + margin
            };
        }
    }
    x
}

#[cfg(test)]
mod tests {
    use super::clear_main_x;

    #[test]
    fn clear_main_x_pushes_off_node_body() {
        let obstacles = vec![(10.0, 0.0, 30.0, 20.0)];
        let x = clear_main_x(20.0, &obstacles, 8.0);
        assert!(x <= 10.0 - 4.0 + 1e-9 || x >= 30.0 + 4.0 - 1e-9, "x={x}");
    }

    #[test]
    fn clear_main_x_leaves_gap_alone() {
        let obstacles = vec![(10.0, 0.0, 30.0, 20.0), (50.0, 0.0, 70.0, 20.0)];
        let x = clear_main_x(40.0, &obstacles, 8.0);
        assert!((x - 40.0).abs() < 1e-9);
    }

    #[test]
    fn clear_main_x_handles_overlapping_projections() {
        // Two columns whose X-ranges overlap — naive push oscillates.
        let obstacles = vec![(0.0, 0.0, 100.0, 10.0), (90.0, 20.0, 190.0, 30.0)];
        let x = clear_main_x(95.0, &obstacles, 8.0);
        assert!(
            x <= 0.0 - 4.0 + 1e-9 || x >= 190.0 + 4.0 - 1e-9,
            "must escape merged block, got {x}"
        );
    }
}
