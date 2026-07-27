//! Phase 5 / D4：Main 轴（rank / y）二次求解。
//!
//! 每层一个变量（层顶 y），相邻层 `MinSeparation` = 上层高 + rank 缝。
//! Stage 3：Cross track 作为 `VarKind::Track` 插入层缝，与两侧层顶分离。
//! 不与 Cross 同矩阵联合；固定顺序 Cross → Main。

use std::collections::{BTreeMap, HashMap};

use crate::layout::atlas::channel::{Occupancy, Substrate, TrackOrient};
use crate::layout::atlas::channel_metric::{
    cross_track_band_need, labeled_edge_counts_by_cross_track,
};
use crate::layout::atlas::plan::Plan;
use crate::ast::Diagram;
use crate::layout::kernel::coordinate::model::{
    ConstraintSource, ConstraintSourceKind, CoordinateProblem, CoordinateSolverConfig,
    HardConstraint, InitialCoordinates, LayerConstraintSet, NodeVariable, SolveAxis, SolverStatus,
    VarKind,
};
use crate::layout::kernel::coordinator::CoordinateKernel;

/// Main 轴 + Cross track 求解结果。
#[derive(Debug, Clone)]
pub struct MainAxisSolveResult {
    /// 各层顶 y（长度 = rank_count）。
    pub layer_tops: Vec<f64>,
    /// track 中心线坐标：`TrackId.0 → y`。
    pub track_coords: BTreeMap<u32, f64>,
    pub problem: CoordinateProblem,
    pub status: SolverStatus,
    pub audit_passed: bool,
}

/// 求解各层顶边 y，返回 `layer_y_offsets`（与旧启发式同语义）。
pub fn solve_main_axis_layer_tops(
    layer_heights: &[f64],
    per_layer_gaps: &[f64],
    first_top: f64,
    default_gap: f64,
) -> Vec<f64> {
    let n = layer_heights.len();
    if n == 0 {
        return Vec::new();
    }

    let mut vars = Vec::with_capacity(n);
    let mut initial = Vec::with_capacity(n);
    let mut cursor = first_top;
    for (i, &h) in layer_heights.iter().enumerate() {
        vars.push(NodeVariable {
            var_id: i,
            stable_id: format!("rank#{i}"),
            kind: VarKind::Axis,
            rank: i,
            order: 0,
            axis_size: h,
            movable: i > 0,
        });
        initial.push(cursor);
        if i + 1 < n {
            let gap = per_layer_gaps.get(i).copied().unwrap_or(default_gap);
            cursor += h + gap;
        }
    }

    let mut hard = Vec::new();
    hard.push(HardConstraint::Fixed {
        var: 0,
        value: first_top,
        source: ConstraintSource {
            kind: ConstraintSourceKind::UserConstraint,
            nodes: vec![],
            note: "main-axis first rank top",
        },
    });
    for i in 0..n.saturating_sub(1) {
        let gap = per_layer_gaps.get(i).copied().unwrap_or(default_gap);
        let distance = layer_heights[i] + gap;
        hard.push(HardConstraint::MinSeparation {
            left: i,
            right: i + 1,
            distance,
            source: ConstraintSource {
                kind: ConstraintSourceKind::SpaceBudget,
                nodes: vec![],
                note: "vertical rank gap (main axis)",
            },
        });
    }

    let mut problem = CoordinateProblem::build(
        vars,
        vec![LayerConstraintSet {
            rank: 0,
            vars: (0..n).collect(),
            separations: (0..n.saturating_sub(1))
                .map(|i| {
                    let gap = per_layer_gaps.get(i).copied().unwrap_or(default_gap);
                    layer_heights[i] + gap
                })
                .collect(),
        }],
        hard,
        vec![],
        InitialCoordinates {
            values: initial.clone(),
        },
        SolveAxis::Main,
    );
    problem.config = CoordinateSolverConfig {
        max_iter_p1: 20,
        max_iter_p2: 10,
        max_iter_p3: 10,
        ..CoordinateSolverConfig::default()
    };

    let result = CoordinateKernel::solve("architecture-main-axis", &problem);
    if result.coordinates.len() >= n {
        result.coordinates[..n].to_vec()
    } else {
        initial
    }
}

/// Stage 3：层顶 + Cross track 联合求解。
///
/// Cross `line = k`（`1 <= k < rank_count`）插在 rank `k-1` 与 `k` 之间：
/// - `MinSeparation(rank#(k-1), track, h[k-1] + W/2)`
/// - `MinSeparation(track, rank#k, W/2)`
/// 另保留相邻层地板缝 `h[i] + base_gap[i]`。
pub fn solve_main_axis_with_cross_tracks(
    layer_heights: &[f64],
    base_gaps: &[f64],
    first_top: f64,
    default_gap: f64,
    substrate: &Substrate,
    occupancy: &Occupancy,
    band_scale: f64,
    diagram: &Diagram,
    plan: &Plan,
) -> MainAxisSolveResult {
    let n = layer_heights.len();
    let empty = MainAxisSolveResult {
        layer_tops: Vec::new(),
        track_coords: BTreeMap::new(),
        problem: CoordinateProblem::build(
            vec![],
            vec![],
            vec![],
            vec![],
            InitialCoordinates { values: vec![] },
            SolveAxis::Main,
        ),
        status: SolverStatus::Converged,
        audit_passed: true,
    };
    if n == 0 {
        return empty;
    }

    let scale = band_scale.clamp(0.5, 1.0);
    let mut vars = Vec::with_capacity(n + 8);
    let mut initial = Vec::with_capacity(n + 8);
    let mut cursor = first_top;

    let labeled = labeled_edge_counts_by_cross_track(diagram, plan, substrate);

    // 预收集 Cross track（按 TrackId 排序，确定性）
    let mut tracks: Vec<(u32, usize, f64)> = Vec::new(); // (id, line, band)
    for t in substrate.tracks() {
        if t.orient != TrackOrient::Cross {
            continue;
        }
        let lanes = occupancy.lane_demand(t.id);
        if lanes == 0 {
            continue;
        }
        let line = t.line;
        if line == 0 || line >= n {
            continue;
        }
        let labeled_n = labeled.get(&t.id).copied().unwrap_or(0);
        let band = cross_track_band_need(lanes, labeled_n) * scale;
        tracks.push((t.id.0, line, band));
    }
    tracks.sort_by_key(|(id, _, _)| *id);

    // gap 有效值 = max(base, max band on that gap)
    let mut effective_gaps: Vec<f64> = (0..n.saturating_sub(1))
        .map(|i| base_gaps.get(i).copied().unwrap_or(default_gap))
        .collect();
    for &(_, line, band) in &tracks {
        let gap_idx = line - 1;
        if gap_idx < effective_gaps.len() {
            effective_gaps[gap_idx] = effective_gaps[gap_idx].max(band);
        }
    }

    for (i, &h) in layer_heights.iter().enumerate() {
        vars.push(NodeVariable {
            var_id: i,
            stable_id: format!("rank#{i}"),
            kind: VarKind::Axis,
            rank: i,
            order: 0,
            axis_size: h,
            movable: i > 0,
        });
        initial.push(cursor);
        if i + 1 < n {
            cursor += h + effective_gaps[i];
        }
    }

    // Track 变量：初值 = 缝中心
    let mut track_var_ids: Vec<(u32, usize, f64)> = Vec::new(); // id, var_id, band
    for &(tid, line, band) in &tracks {
        let left_rank = line - 1;
        let left_top = initial[left_rank];
        let left_h = layer_heights[left_rank];
        let right_top = initial.get(line).copied().unwrap_or(left_top + left_h + band);
        let seam_lo = left_top + left_h;
        let track_y = (seam_lo + right_top) * 0.5;
        let vid = vars.len();
        vars.push(NodeVariable {
            var_id: vid,
            stable_id: format!("track#{tid}"),
            kind: VarKind::Track,
            rank: left_rank,
            order: usize::MAX,
            axis_size: band,
            movable: true,
        });
        initial.push(track_y);
        track_var_ids.push((tid, vid, band));
    }

    let mut hard = Vec::new();
    hard.push(HardConstraint::Fixed {
        var: 0,
        value: first_top,
        source: ConstraintSource {
            kind: ConstraintSourceKind::UserConstraint,
            nodes: vec![],
            note: "main-axis first rank top",
        },
    });

    // 层间地板
    for i in 0..n.saturating_sub(1) {
        let gap = effective_gaps[i];
        hard.push(HardConstraint::MinSeparation {
            left: i,
            right: i + 1,
            distance: layer_heights[i] + gap,
            source: ConstraintSource {
                kind: ConstraintSourceKind::SpaceBudget,
                nodes: vec![],
                note: "vertical rank gap floor",
            },
        });
    }

    // Track 与两侧层顶
    for &(tid, line, band) in &tracks {
        let left = line - 1;
        let right = line;
        let Some(&(_, track_vid, _)) = track_var_ids.iter().find(|(id, _, _)| *id == tid) else {
            continue;
        };
        let half = band * 0.5;
        hard.push(HardConstraint::MinSeparation {
            left,
            right: track_vid,
            distance: layer_heights[left] + half,
            source: ConstraintSource {
                kind: ConstraintSourceKind::RouteDemand,
                nodes: vec![format!("track#{tid}")],
                note: "channel cross-track below previous rank",
            },
        });
        hard.push(HardConstraint::MinSeparation {
            left: track_vid,
            right,
            distance: half,
            source: ConstraintSource {
                kind: ConstraintSourceKind::RouteDemand,
                nodes: vec![format!("track#{tid}")],
                note: "channel cross-track above next rank",
            },
        });
    }

    let mut problem = CoordinateProblem::build(
        vars,
        vec![LayerConstraintSet {
            rank: 0,
            vars: (0..n).collect(),
            separations: effective_gaps
                .iter()
                .enumerate()
                .map(|(i, &g)| layer_heights[i] + g)
                .collect(),
        }],
        hard,
        vec![],
        InitialCoordinates {
            values: initial.clone(),
        },
        SolveAxis::Main,
    );
    problem.config = CoordinateSolverConfig {
        max_iter_p1: 40,
        max_iter_p2: 20,
        max_iter_p3: 10,
        ..CoordinateSolverConfig::default()
    };

    let result = CoordinateKernel::solve("atlas-main-axis-tracks", &problem);
    let coords = if result.coordinates.len() >= n {
        result.coordinates
    } else {
        initial
    };

    let layer_tops = coords[..n].to_vec();
    let mut track_coords = BTreeMap::new();
    for &(tid, vid, _) in &track_var_ids {
        if let Some(&y) = coords.get(vid) {
            track_coords.insert(tid, y);
        }
    }

    MainAxisSolveResult {
        layer_tops,
        track_coords,
        problem,
        status: result.status,
        audit_passed: result.audit_passed,
    }
}

/// 由层顶 y + 层高得到节点中心 y 映射（按 layers）。
pub fn layer_center_ys_from_tops(
    layers: &[Vec<String>],
    layer_tops: &[f64],
    layer_heights: &[f64],
) -> HashMap<String, f64> {
    let mut out = HashMap::new();
    for (i, layer) in layers.iter().enumerate() {
        let top = layer_tops.get(i).copied().unwrap_or(0.0);
        let h = layer_heights.get(i).copied().unwrap_or(0.0);
        let cy = top + h * 0.5;
        for id in layer {
            out.insert(id.clone(), cy);
        }
    }
    out
}

/// Stage 3.3：lane 中心 = track 中心 + (i - (n-1)/2) × pitch。
pub fn lane_centers(track_coord: f64, lanes: u32, pitch: f64) -> Vec<f64> {
    if lanes == 0 {
        return Vec::new();
    }
    let n = lanes as f64;
    let mid = (n - 1.0) * 0.5;
    (0..lanes)
        .map(|i| track_coord + (i as f64 - mid) * pitch)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lane_centers_symmetric_around_track() {
        let cs = lane_centers(100.0, 3, 18.0);
        assert_eq!(cs.len(), 3);
        assert!((cs[1] - 100.0).abs() < 1e-9);
        assert!((cs[0] - (100.0 - 18.0)).abs() < 1e-9);
        assert!((cs[2] - (100.0 + 18.0)).abs() < 1e-9);
    }

    #[test]
    fn main_axis_without_tracks_matches_heuristic_stack() {
        let heights = vec![40.0, 40.0, 40.0];
        let gaps = vec![50.0, 50.0];
        let tops = solve_main_axis_layer_tops(&heights, &gaps, 20.0, 50.0);
        assert!((tops[0] - 20.0).abs() < 1e-6);
        assert!((tops[1] - (20.0 + 40.0 + 50.0)).abs() < 1e-6);
        assert!((tops[2] - (20.0 + 80.0 + 100.0)).abs() < 1e-6);
    }
}
