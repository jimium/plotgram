//! Publish main/cross coordinates for Channel TrackOrder lanes.

use std::collections::BTreeMap;

use plotgram_algo::orientation::Size;
use plotgram_model::diagnostics::Relaxation;

use crate::layout::hierarchical::channel::{ChannelRoutePlan, TrackId, TrackOrient};
use crate::layout::hierarchical::compose::track_order::TrackOrderPlan;
use crate::layout::hierarchical::group_frame::GroupShellBands;
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
///
/// Cross lanes sample a **per-corridor-line shared pitch grid**: group cuts
/// split one rank gap into several scope-cut tracks, and every track on the
/// line must map the same lane index to the same Y so gate crossings stay
/// collinear (channel-d1 §5.1). Returns soft relaxations for shell-band
/// fallbacks (lanes could not stay inside the band-free sub-interval).
///
/// `group_obstacles`: group envelope frames (finalize pad contract,
/// canonical space). Only the **outer** Main lanes clear them — outer lanes
/// are the long-haul return corridors, and a full-height rail inside a
/// foreign group frame is a penetration (strong-macro.md §6 SM-4). Inner
/// lanes keep node-only clearance. Both policies pass the Metric group
/// envelopes (D₂.0 frame true source; Weak previously passed nothing).
pub fn assign_track_coords(
    plan: &PlanGraph,
    main: &[f64],
    cross_centers: &[f64],
    size_of: &dyn Fn(usize) -> Size,
    track_order: &TrackOrderPlan,
    route_plan: &ChannelRoutePlan,
    shell: &GroupShellBands,
    edge_gap: f64,
    group_obstacles: &[(f64, f64, f64, f64)],
) -> (TrackCoords, Vec<Relaxation>) {
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
    let mut relaxations = Vec::new();

    // One shared lane grid per Cross corridor line.
    let mut cross_grids: BTreeMap<usize, Vec<f64>> = BTreeMap::new();
    let mut line_max: BTreeMap<usize, usize> = BTreeMap::new();
    for (&tid, &count) in &track_order.track_counts {
        let Some(t) = route_plan.substrate.track(tid) else {
            continue;
        };
        if matches!(t.orient, TrackOrient::Cross) {
            let e = line_max.entry(t.line).or_insert(0);
            *e = (*e).max(count);
        }
    }
    for (&line, &track_max) in &line_max {
        let count = track_order
            .cross_line_lane_counts
            .get(&line)
            .copied()
            .unwrap_or(track_max);
        if count == 0 {
            continue;
        }
        let ys = if line == 0 || line >= plan.layers.len() {
            // Outside the stack — park near adjacent layer; still separate
            // lanes (P5-5). Keep one `edge_gap` clear of the group shell
            // band that extends past the outer rank.
            let below = line > 0;
            let base = if line == 0 {
                let top = main.get(plan.layers[0][0]).copied().unwrap_or(0.0);
                top - edge_gap.max(shell.outer_top + edge_gap)
            } else {
                let last = plan.layers.len() - 1;
                let e = plan.layers[last][0];
                main[e] + size_of(e).height + edge_gap.max(shell.outer_bottom + edge_gap)
            };
            let mid = (count.saturating_sub(1) as f64) * 0.5;
            (0..count)
                .map(|i| {
                    // A row taller than edge_gap puts the naive parked lane
                    // inside the adjacent row's Y-projection — push further
                    // out until clear (mirror of clear_main_x; gates-run e21
                    // penetrated a last-rank body through such a lane).
                    clear_parked_y(
                        base + (i as f64 - mid) * edge_gap,
                        &obstacles,
                        edge_gap,
                        below,
                    )
                })
                .collect()
        } else {
            let r = line - 1;
            let thickness = plan.layers[r]
                .iter()
                .map(|&e| size_of(e).height)
                .fold(0.0_f64, f64::max);
            let gap_top = main[plan.layers[r][0]] + thickness;
            let gap_bot = main[plan.layers[r + 1][0]];
            let (below_band, above_band) = shell.gap.get(r).copied().unwrap_or((0.0, 0.0));
            let (start, fallback) = cross_lane_start(
                gap_top, gap_bot, below_band, above_band, count, edge_gap,
            );
            if fallback {
                relaxations.push(Relaxation {
                    rule: "channel-shell-band-fallback".into(),
                    detail: format!(
                        "Cross line {line}: {count}-lane fan wider than the \
                         shell-band-free gap; lanes fall back to the full rank gap"
                    ),
                });
            }
            (0..count)
                .map(|i| start + i as f64 * edge_gap)
                .collect()
        };
        cross_grids.insert(line, ys);
    }

    for (&tid, &count) in &track_order.track_counts {
        if count == 0 {
            continue;
        }
        let Some(t) = route_plan.substrate.track(tid) else {
            continue;
        };
        let ys = match t.orient {
            TrackOrient::Cross => {
                // Lane index is corridor-wide (TrackOrder); sample the line
                // grid so scope-cut tracks stay collinear across gates.
                let grid = &cross_grids[&t.line];
                grid.iter().take(count).copied().collect()
            }
            TrackOrient::Main => {
                // Vertical corridor X: sit in order *gaps* (or outside the
                // outer columns), never on a node center — otherwise a
                // full-height Main rail penetrates intermediate nodes.
                let og = t.line;
                let max_cols = plan.layers.iter().map(|l| l.len()).max().unwrap_or(0);
                let mut backbone =
                    main_line_backbone_x(plan, cross_centers, size_of, og, edge_gap);
                backbone = clear_main_x(backbone, &obstacles, edge_gap);
                // Outer rails additionally clear group envelopes (SM-4).
                let outer = og == 0 || og >= max_cols;
                if outer {
                    backbone = clear_outside(backbone, &obstacles, group_obstacles, edge_gap);
                }
                let ys: Vec<f64> = if og == 0 {
                    // West outer: pack further left so parallel returns stay outside.
                    (0..count)
                        .map(|i| {
                            clear_outside(
                                backbone - i as f64 * edge_gap,
                                &obstacles,
                                group_obstacles,
                                edge_gap,
                            )
                        })
                        .collect()
                } else if og >= max_cols {
                    // East outer: pack further right.
                    (0..count)
                        .map(|i| {
                            clear_outside(
                                backbone + i as f64 * edge_gap,
                                &obstacles,
                                group_obstacles,
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
    (out, relaxations)
}

/// First lane Y for an interior Cross corridor: center the lane fan in the
/// group-shell-free sub-interval of the rank gap (demand reserves room for
/// it); fall back to centering in the full gap when the free interval is too
/// narrow (degraded group chains / tight params) — flagged in the second
/// return value so the caller can surface a relaxation.
fn cross_lane_start(
    gap_top: f64,
    gap_bot: f64,
    below_band: f64,
    above_band: f64,
    count: usize,
    edge_gap: f64,
) -> (f64, bool) {
    let span = (count.saturating_sub(1) as f64) * edge_gap;
    let free_top = gap_top + below_band;
    let free_bot = gap_bot - above_band;
    let fits_free = free_bot - free_top >= span;
    let (lo, hi) = if fits_free {
        (free_top, free_bot)
    } else {
        (gap_top, gap_bot)
    };
    let usable = (hi - lo).max(0.0);
    let fallback = !fits_free && (below_band > 0.0 || above_band > 0.0);
    (lo + (usable - span) * 0.5, fallback)
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
            // West outer: fold over every element's west face so the lane
            // clears all bodies of every rank (first/last can be a narrow
            // dummy, and clearance must not push the rim lane inward — the
            // Channel solver relies on rim lanes sitting on port normals).
            let face = layer
                .iter()
                .map(|&e| cross_centers[e] - size_of(e).width / 2.0)
                .fold(f64::INFINITY, f64::min);
            xs.push(face - margin);
        } else if order_gap >= layer.len() {
            // East outer: symmetric fold over every element's east face.
            let face = layer
                .iter()
                .map(|&e| cross_centers[e] + size_of(e).width / 2.0)
                .fold(f64::NEG_INFINITY, f64::max);
            xs.push(face + margin);
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

/// Outer-rail clearance: [`clear_main_x`] against node bodies **and** group
/// envelopes, iterated to a fixpoint (pushing out of a group can land
/// inside a node projection further out, and vice versa).
fn clear_outside(
    mut x: f64,
    obstacles: &[(f64, f64, f64, f64)],
    group_obstacles: &[(f64, f64, f64, f64)],
    edge_gap: f64,
) -> f64 {
    for _ in 0..16 {
        let next = clear_main_x(clear_main_x(x, obstacles, edge_gap), group_obstacles, edge_gap);
        if next == x {
            return x;
        }
        x = next;
    }
    x
}

/// Clear a parked Cross lane Y out of every node's Y-projection, always
/// pushing away from the stack (`below` = parked under the last rank).
/// Repeats until clear — the pushed position can land inside a taller row's
/// projection further out.
fn clear_parked_y(y: f64, obstacles: &[(f64, f64, f64, f64)], edge_gap: f64, below: bool) -> f64 {
    const EPS: f64 = 1e-6;
    let margin = edge_gap.max(1.0) * 0.5;
    let mut y = y;
    loop {
        let hit = obstacles
            .iter()
            .find(|&&(_, t, _, b)| y > t + EPS && y < b - EPS)
            .map(|&(_, t, _, b)| (t, b));
        match hit {
            Some((t, b)) => {
                y = if below { b + margin } else { t - margin };
            }
            None => return y,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::clear_main_x;
    use super::clear_parked_y;
    use super::cross_lane_start;
    use super::*;

    use crate::layout::hierarchical::channel::substrate::{BlueprintIndex, Substrate};
    use crate::layout::hierarchical::compose::track_order::TrackOrderPlan;
    use crate::layout::hierarchical::model::{Elem, ElemKey, PlanGraph};

    /// Two scope-cut tracks on one interior Cross line sample one shared
    /// pitch grid: the 1-lane track takes its grid slot instead of
    /// re-centering its own fan.
    #[test]
    fn cross_tracks_on_one_line_share_a_pitch_grid() {
        // Table: (below_band, above_band, wide_count, expected ys, fallback).
        // Ranks at y=0 / y=100, node height 10 → interior gap [10, 100].
        let cases: &[(f64, f64, usize, &[f64], bool)] = &[
            // No bands: 3-lane fan centered in the gap.
            (0.0, 0.0, 3, &[39.0, 55.0, 71.0], false),
            // Bands squeeze the free interval below the fan span → full-gap
            // fallback, surfaced as a relaxation.
            (45.0, 45.0, 2, &[47.0, 63.0], true),
        ];
        for &(below, above, count, expected, fallback) in cases {
            let elems = vec![
                Elem {
                    key: ElemKey::Real("a".into()),
                    group_path: vec![],
                    rank: 0,
                },
                Elem {
                    key: ElemKey::Real("b".into()),
                    group_path: vec![],
                    rank: 1,
                },
            ];
            let index_of = elems
                .iter()
                .enumerate()
                .map(|(i, e)| (e.key.clone(), i))
                .collect();
            let plan = PlanGraph {
                elems,
                index_of,
                decl_index: (0..2).collect(),
                segments: vec![],
                layers: vec![vec![0], vec![1]],
            };
            // One corridor line cut into a wide + narrow scope-cut track.
            let mut sub = Substrate::new();
            let t_wide = sub.alloc_track_id();
            sub.add_track(t_wide, TrackOrient::Cross, None, 1.0, 1, (0, 1))
                .unwrap();
            let t_narrow = sub.alloc_track_id();
            sub.add_track(t_narrow, TrackOrient::Cross, None, 1.0, 1, (2, 3))
                .unwrap();
            let route_plan = ChannelRoutePlan {
                substrate: sub,
                index: BlueprintIndex::default(),
                routes: BTreeMap::new(),
                bundles: vec![],
                relaxations: vec![],
                ripup_rounds: 0,
                used_gates: false,
                route_order: vec![],
            };
            let track_order = TrackOrderPlan {
                assignments: BTreeMap::new(),
                track_counts: [(t_wide, count), (t_narrow, 1)].into_iter().collect(),
                rank_gap_track_counts: BTreeMap::new(),
                cross_line_lane_counts: [(1usize, count)].into_iter().collect(),
            };
            let shell = GroupShellBands {
                gap: vec![(below, above)],
                outer_top: 0.0,
                outer_bottom: 0.0,
            };
            let (coords, relaxations) = assign_track_coords(
                &plan,
                &[0.0, 100.0],
                &[0.0, 0.0],
                &|_| Size::new(20.0, 10.0),
                &track_order,
                &route_plan,
                &shell,
                16.0,
                &[],
            );
            assert_eq!(
                &coords.coords[&t_wide],
                expected,
                "below={below} above={above} count={count}"
            );
            // The single-lane track takes lane 0 of the same grid — never an
            // independently centered fan.
            assert_eq!(
                coords.coords[&t_narrow],
                vec![expected[0]],
                "narrow track must sample the corridor grid"
            );
            assert_eq!(
                relaxations.len(),
                if fallback { 1 } else { 0 },
                "below={below} above={above} count={count}"
            );
            if fallback {
                assert_eq!(relaxations[0].rule, "channel-shell-band-fallback");
            }
        }
    }

    #[test]
    fn cross_lane_start_avoids_shell_bands() {
        // Table: (below_band, above_band, count, expected start).
        // Gap [100, 180], edge_gap 16.
        let cases: &[(f64, f64, usize, f64)] = &[
            // No bands: center in the full gap (single lane).
            (0.0, 0.0, 1, 140.0),
            // Bands [100,116] / [156,180]: center in free [116,156].
            (16.0, 24.0, 1, 136.0),
            // Two lanes span 16 in free [116,156] → start 128.
            (16.0, 24.0, 2, 128.0),
            // Free interval narrower than the fan → fall back to full gap.
            (40.0, 40.0, 2, 100.0 + (80.0 - 16.0) * 0.5),
        ];
        for &(below, above, count, expected) in cases {
            let (start, fallback) = cross_lane_start(100.0, 180.0, below, above, count, 16.0);
            assert!(
                (start - expected).abs() < 1e-9,
                "below={below} above={above} count={count}: {start} != {expected}"
            );
            if below + above < 80.0 {
                let last = start + (count.saturating_sub(1) as f64) * 16.0;
                assert!(start >= 100.0 + below - 1e-9, "start pierces below band");
                assert!(last <= 180.0 - above + 1e-9, "last lane pierces above band");
                assert!(!fallback, "free interval fits — no fallback expected");
            } else {
                assert!(fallback, "squeezed fan must flag the shell-band fallback");
            }
        }
    }

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
    fn clear_parked_y_pushes_out_of_tall_row() {
        // Row at y 100..140 (taller than edge_gap): parked lanes inside the
        // body must move past it, away from the stack.
        let obstacles = vec![(0.0, 100.0, 50.0, 140.0)];
        let below = clear_parked_y(116.0, &obstacles, 16.0, true);
        assert!(below >= 140.0 + 8.0 - 1e-9, "below={below}");
        let above = clear_parked_y(116.0, &obstacles, 16.0, false);
        assert!(above <= 100.0 - 8.0 + 1e-9, "above={above}");
        // Already-clear lanes stay put.
        let free = clear_parked_y(160.0, &obstacles, 16.0, true);
        assert!((free - 160.0).abs() < 1e-9);
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

    /// SM-4: an East-outer Main rail whose node-only clearance parks it in
    /// a group's pad band must move outside the group envelope.
    #[test]
    fn outer_main_rail_clears_group_envelope() {
        let elems = vec![
            Elem {
                key: ElemKey::Real("a".into()),
                group_path: vec![],
                rank: 0,
            },
            Elem {
                key: ElemKey::Real("b".into()),
                group_path: vec![],
                rank: 1,
            },
        ];
        let index_of = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let plan = PlanGraph {
            elems,
            index_of,
            decl_index: (0..2).collect(),
            segments: vec![],
            layers: vec![vec![0], vec![1]],
        };
        // East-outer Main track (order gap 1 >= max_cols 1).
        let mut sub = Substrate::new();
        let t_main = sub.alloc_track_id();
        sub.add_track(t_main, TrackOrient::Main, None, 1.0, 1, (0, 1))
            .unwrap();
        let route_plan = ChannelRoutePlan {
            substrate: sub,
            index: BlueprintIndex::default(),
            routes: BTreeMap::new(),
            bundles: vec![],
            relaxations: vec![],
            ripup_rounds: 0,
            used_gates: false,
            route_order: vec![],
        };
        let track_order = TrackOrderPlan {
            assignments: BTreeMap::new(),
            track_counts: [(t_main, 1)].into_iter().collect(),
            rank_gap_track_counts: BTreeMap::new(),
            cross_line_lane_counts: BTreeMap::new(),
        };
        let shell = GroupShellBands {
            gap: vec![(0.0, 0.0)],
            outer_top: 0.0,
            outer_bottom: 0.0,
        };
        // Node bodies [0,20]×two rows; group envelope extends to x=36.
        let group_env = vec![(0.0, -24.0, 36.0, 130.0)];
        let (with_group, _) = assign_track_coords(
            &plan,
            &[0.0, 100.0],
            &[10.0, 10.0],
            &|_| Size::new(20.0, 10.0),
            &track_order,
            &route_plan,
            &shell,
            16.0,
            &group_env,
        );
        // Node-only backbone is 28 (east face 20 + margin 8) — inside the
        // envelope; clearance pushes past 36 + margin.
        assert_eq!(with_group.coords[&t_main], vec![44.0]);
        // Without group obstacles the rail stays at the node-only backbone.
        let (no_group, _) = assign_track_coords(
            &plan,
            &[0.0, 100.0],
            &[10.0, 10.0],
            &|_| Size::new(20.0, 10.0),
            &track_order,
            &route_plan,
            &shell,
            16.0,
            &[],
        );
        assert_eq!(no_group.coords[&t_main], vec![28.0]);
    }
}
