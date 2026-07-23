//! Path building for orthogonal edge routing

use super::*;
use crate::layout::geometry::{Axis, Point, Rect, EPS};
use crate::layout::group::{prefer_corridor_coord, CorridorAxis, GroupCorridor};
use crate::layout::{GroupLayout, Port};
use std::collections::{HashMap, HashSet};

/// 单条边的路由走廊 bbox，用于裁剪折点/通道候选（P0-B）。
#[derive(Clone, Copy)]
struct EdgeCorridor {
    x_lo: f64,
    y_lo: f64,
    x_hi: f64,
    y_hi: f64,
}

impl EdgeCorridor {
    fn from_endpoints(sx: f64, sy: f64, ex: f64, ey: f64, margin: f64) -> Self {
        let pad = NODE_OBSTACLE_PAD + margin + PORT_CLEARANCE;
        Self {
            x_lo: sx.min(ex) - pad,
            y_lo: sy.min(ey) - pad,
            x_hi: sx.max(ex) + pad,
            y_hi: sy.max(ey) + pad,
        }
    }

    fn overlaps_travel_band(self, r: &Rect, fold_axis: Axis, pad: f64) -> bool {
        let travel = fold_axis.other();
        let (band_lo, band_hi) = self.main_range(travel);
        let (o_lo, o_hi) = r.range_on_axis(travel);
        o_hi + pad >= band_lo && o_lo - pad <= band_hi
    }

    fn main_range(self, axis: Axis) -> (f64, f64) {
        match axis {
            Axis::Horizontal => (self.x_lo, self.x_hi),
            Axis::Vertical => (self.y_lo, self.y_hi),
        }
    }

    fn contains_cross_coord(self, axis: Axis, coord: f64) -> bool {
        let (lo, hi) = match axis {
            Axis::Horizontal => (self.y_lo, self.y_hi),
            Axis::Vertical => (self.x_lo, self.x_hi),
        };
        coord >= lo && coord <= hi
    }
}

/// 计算分组间垂直于指定段方向的通道间隙中点坐标。
/// - axis=Vertical：段沿垂直方向延伸（y 轴为主轴），找 x 方向间隙中点（垂直通道 x 坐标）；
/// - axis=Horizontal：段沿水平方向延伸（x 轴为主轴），找 y 方向间隙中点（水平通道 y 坐标）。
/// 只考虑主轴范围重叠的分组对（同一行/列），避免不同行/列分组的干扰。
/// 优先使用 GroupCorridors 中预定义的走廊坐标。
fn group_gap_midpoints_on_axis(
    groups: &HashMap<String, GroupLayout>,
    sorted_group_ids: &[String],
    corridors: &[GroupCorridor],
    axis: Axis,
    corridor: EdgeCorridor,
) -> Vec<f64> {
    let corridor_axis = match axis {
        Axis::Vertical => CorridorAxis::Vertical,
        Axis::Horizontal => CorridorAxis::Horizontal,
    };
    let (main_lo, main_hi) = corridor.main_range(axis);
    let mut mids = Vec::new();

    for c in corridors {
        if c.axis != corridor_axis {
            continue;
        }
        if !corridor.contains_cross_coord(axis, c.coord) {
            continue;
        }
        let (c_lo, c_hi) = (c.span_min, c.span_max);
        if c_hi <= main_lo + EPS || c_lo >= main_hi - EPS {
            continue;
        }
        mids.push(c.coord);
    }

    let mut ranges: Vec<(f64, f64, f64, f64, &str)> = sorted_group_ids
        .iter()
        .filter_map(|gid| {
            let g = groups.get(gid)?;
            let r = Rect::from(g);
            if !corridor.overlaps_travel_band(&r, axis, GROUP_OBSTACLE_PAD) {
                return None;
            }
            let (cross_lo, cross_hi) = r.cross_range_on_axis(axis);
            let (m_lo, m_hi) = r.range_on_axis(axis);
            Some((cross_lo, cross_hi, m_lo, m_hi, gid.as_str()))
        })
        .collect();
    ranges.sort_by(|a, b| {
        a.0
            .partial_cmp(&b.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.4.cmp(b.4))
    });

    for i in 0..ranges.len() {
        for j in i + 1..ranges.len() {
            let (_ac_lo, ac_hi, am_lo, am_hi, _) = ranges[i];
            let (bc_lo, _bc_hi, bm_lo, bm_hi, _) = ranges[j];
            if am_hi <= bm_lo || bm_hi <= am_lo {
                continue;
            }
            if am_hi <= main_lo || main_hi <= am_lo {
                continue;
            }
            if ac_hi < bc_lo {
                let mid = (ac_hi + bc_lo) / 2.0;
                if corridor.contains_cross_coord(axis, mid) {
                    mids.push(mid);
                }
            }
        }
    }
    mids.sort_by(|a, b| {
        a.partial_cmp(b)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    // 相等坐标按首次出现序稳定化（先排序再 dedup 保留左侧）
    let mut indexed: Vec<(usize, f64)> = mids.into_iter().enumerate().collect();
    indexed.sort_by(|a, b| {
        a.1
            .partial_cmp(&b.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(&b.0))
    });
    let mut mids: Vec<f64> = indexed.into_iter().map(|(_, m)| m).collect();
    mids.dedup_by(|a, b| (*a - *b).abs() < 1.0);
    mids
}

/// 收集节点和分组在指定段方向的 cross 轴上的边界坐标（用于折点候选）。
/// - axis=Vertical（垂直段）：收集 x 坐标边界
/// - axis=Horizontal（水平段）：收集 y 坐标边界
fn collect_obstacle_boundaries_on_axis(
    axis: Axis,
    nodes: &HashMap<String, NodeLayout>,
    groups: &HashMap<String, GroupLayout>,
    endpoint_groups: &HashSet<&str>,
    from_id: &str,
    to_id: &str,
    node_pad: f64,
    group_pad: f64,
    margin: f64,
    sorted_node_ids: &[String],
    sorted_group_ids: &[String],
    corridor: EdgeCorridor,
) -> Vec<f64> {
    let mut coords = Vec::new();

    for nid in sorted_node_ids {
        if nid.as_str() == from_id || nid.as_str() == to_id {
            continue;
        }
        let r = Rect::from(&nodes[nid]);
        if !corridor.overlaps_travel_band(&r, axis, node_pad + margin) {
            continue;
        }
        let (lo, hi) = r.cross_range_on_axis(axis);
        coords.push((lo - node_pad) - margin);
        coords.push((hi + node_pad) + margin);
    }

    for gid in sorted_group_ids {
        if endpoint_groups.contains(gid.as_str()) {
            continue;
        }
        if let Some(gl) = groups.get(gid) {
            let r = Rect::from(gl);
            if !corridor.overlaps_travel_band(&r, axis, group_pad + margin) {
                continue;
            }
            let (lo, hi) = r.cross_range_on_axis(axis);
            coords.push((lo - group_pad) - margin);
            coords.push((hi + group_pad) + margin);
        }
    }

    coords
}

/// A routed segment recorded for overlap detection
#[derive(Clone, Copy)]
pub struct RoutedSegment {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    pub edge_index: usize,
}

/// Select the best-scoring path using a custom scorer.
///
/// P0-1: 硬过滤——穿障候选直接丢弃。若全部被过滤，退化为最低惩罚候选
/// （保证边不断线，剩余穿障由 refine 循环处理）。
/// P2-1: 路径选择统计（可选，用于 debug 导出）
#[derive(Default)]
pub struct PathSelectStats {
    /// 生成的候选路径总数
    pub candidate_count: usize,
    /// 硬过滤拒绝的候选数（穿障候选被丢弃）
    pub hard_filter_reject_count: usize,
    /// 是否退化（所有干净候选均被拒绝，使用脏候选）
    pub degraded: bool,
}

/// 额外 channel_margin 档位（P1-B：base 档无 strict 干净候选时再逐档尝试）。
const EXTRA_CHANNEL_MARGINS: [f64; 2] = [28.0, 40.0];
/// R2：主路径仍无 clean 时再升档的 channel margin。
const FORCE_UPGRADE_MARGINS: [f64; 3] = [56.0, 72.0, 96.0];
/// Iteration 3：每边候选评估上限（超出则截断，优先保留已生成的前缀）。
const MAX_CANDIDATES: usize = 48;

struct PathEvalState {
    best_strict: Option<(f64, Vec<Point>)>,
    best_nodes_only: Option<(f64, Vec<Point>)>,
    best_dirty: Option<(f64, Vec<Point>)>,
    strict_count: usize,
    nodes_only_count: usize,
    candidate_count: usize,
}

fn lex_path_cmp(a: &[Point], b: &[Point]) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let n = a.len().min(b.len());
    for i in 0..n {
        let c = a[i]
            .x
            .partial_cmp(&b[i].x)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a[i].y.partial_cmp(&b[i].y).unwrap_or(Ordering::Equal));
        if c != Ordering::Equal {
            return c;
        }
    }
    a.len().cmp(&b.len())
}

/// 同分时按路径点字典序选取，保证候选选择确定性。
fn candidate_better(score: f64, path: &[Point], best: &Option<(f64, Vec<Point>)>) -> bool {
    use std::cmp::Ordering;
    match best {
        None => true,
        Some((best_score, best_path)) => match score.partial_cmp(best_score) {
            Some(Ordering::Less) => true,
            Some(Ordering::Greater) => false,
            _ => lex_path_cmp(path, best_path) == Ordering::Less,
        },
    }
}

/// OVG 单条路径评估（绕过 MAX_CANDIDATES 预算限制）。
/// OVG 产出的是全局最短路径，质量通常优于启发式候选，不应被预算截断。
fn evaluate_single_ovg_path(
    path: Vec<Point>,
    ctx: &OrthoRoutingContext,
    pair: &EndpointPair,
    scorer: &dyn CandidateScorer,
    from_id: &str,
    to_id: &str,
    state: &mut PathEvalState,
) {
    state.candidate_count += 1;
    if path_is_clean(
        &path, from_id, to_id, ctx.nodes, ctx.group_ctx,
        &ctx.obstacles.sorted_node_ids,
    ) {
        let lower_bound =
            path_length(&path) + path.len().saturating_sub(2) as f64 * BEND_PENALTY;
        if path_avoids_group_interiors(
            &path, from_id, to_id, ctx.group_ctx,
            &ctx.obstacles.sorted_group_ids,
        ) {
            state.strict_count += 1;
            if state.best_strict.as_ref().is_none_or(|(bs, _)| lower_bound < *bs) {
                let score = scorer.score(&path, ctx, pair);
                if candidate_better(score, &path, &state.best_strict) {
                    state.best_strict = Some((score, path));
                }
            }
        } else {
            state.nodes_only_count += 1;
            if state.best_nodes_only.as_ref().is_none_or(|(bs, _)| lower_bound < *bs) {
                let score = scorer.score(&path, ctx, pair);
                if candidate_better(score, &path, &state.best_nodes_only) {
                    state.best_nodes_only = Some((score, path));
                }
            }
        }
    } else {
        let score = path_length(&path) + path.len().saturating_sub(2) as f64 * BEND_PENALTY;
        if candidate_better(score, &path, &state.best_dirty) {
            state.best_dirty = Some((score, path));
        }
    }
}

fn evaluate_path_batch(
    mut paths: Vec<Vec<Point>>,
    ctx: &OrthoRoutingContext,
    pair: &EndpointPair,
    scorer: &dyn CandidateScorer,
    from_id: &str,
    to_id: &str,
    state: &mut PathEvalState,
) {
    let remaining = MAX_CANDIDATES.saturating_sub(state.candidate_count);
    if remaining == 0 {
        return;
    }
    if paths.len() > remaining {
        paths.truncate(remaining);
    }
    state.candidate_count += paths.len();
    for path in paths {
        // S4.x：不再把「外环但仍边交叉」硬降为 dirty（否则与穿模脏路径同池，短穿模胜出）。
        // 边交叉改由 DefaultScorer 在 prefer_outer 下 overlap×3 软惩罚。
        if path_is_clean(
            &path,
            from_id,
            to_id,
            ctx.nodes,
            ctx.group_ctx,
            &ctx.obstacles.sorted_node_ids,
        ) {
            let lower_bound =
                path_length(&path) + path.len().saturating_sub(2) as f64 * BEND_PENALTY;
            if path_avoids_group_interiors(
                &path,
                from_id,
                to_id,
                ctx.group_ctx,
                &ctx.obstacles.sorted_group_ids,
            ) {
                state.strict_count += 1;
                if state.best_strict.as_ref().is_none_or(|(bs, _)| lower_bound < *bs) {
                    let score = scorer.score(&path, ctx, pair);
                    if candidate_better(score, &path, &state.best_strict) {
                        state.best_strict = Some((score, path));
                    }
                }
            } else {
                state.nodes_only_count += 1;
                if state.best_nodes_only.as_ref().is_none_or(|(bs, _)| lower_bound < *bs) {
                    let score = scorer.score(&path, ctx, pair);
                    if candidate_better(score, &path, &state.best_nodes_only) {
                        state.best_nodes_only = Some((score, path));
                    }
                }
            }
        } else {
            let score = path_length(&path) + path.len().saturating_sub(2) as f64 * BEND_PENALTY;
            if candidate_better(score, &path, &state.best_dirty) {
                state.best_dirty = Some((score, path));
            }
        }
    }
}

/// P2-1: 带 debug 统计的路径选择
///
/// `phase1_only`: 为 true 时跳过阶梯候选，供 `fix_slot_inversions` 轻量重路由使用。
///
/// P1-A 渐进式候选：L 形 → channel（单档 margin）→ channel（加档 margin）→ z-fold → 阶梯。
pub fn select_best_path_with_scorer_stats(
    ctx: &OrthoRoutingContext,
    pair: &EndpointPair,
    scorer: &dyn CandidateScorer,
    mut stats: Option<&mut PathSelectStats>,
    phase1_only: bool,
) -> Vec<Point> {
    let start = pair.from_anchor();
    let end = pair.to_anchor();
    let sx = start.x;
    let sy = start.y;
    let ex = end.x;
    let ey = end.y;
    let from_side = pair.from.side;
    let to_side = pair.to.side;

    let from_id = pair.from_id();
    let to_id = pair.to_id();
    let corridor = EdgeCorridor::from_endpoints(sx, sy, ex, ey, ctx.cfg.channel_margin);

    let mut state = PathEvalState {
        best_strict: None,
        best_nodes_only: None,
        best_dirty: None,
        strict_count: 0,
        nodes_only_count: 0,
        candidate_count: 0,
    };

    // S4：feedback / 监控枢纽 —— 先评估外环（软偏好；不挂 corridor_boost，避免有组大图行为漂移）
    if ctx.prefer_outer_ring {
        let pad = if ctx.corridor_boost { 56.0 } else { 40.0 };
        evaluate_path_batch(
            build_outer_ring_candidates(sx, sy, ex, ey, from_side, to_side, ctx, pad),
            ctx,
            pair,
            scorer,
            from_id,
            to_id,
            &mut state,
        );
    }

    // Level 0: 基础 L 形 + 混合端口扩展
    evaluate_path_batch(
        build_candidate_paths(sx, sy, from_side, ex, ey, to_side, PORT_CLEARANCE, PORT_CLEARANCE),
        ctx,
        pair,
        scorer,
        from_id,
        to_id,
        &mut state,
    );

    // A-3（契约①/Stub）：跨组边追加「出组 stub」候选——首段延伸到源组外边界再转弯，
    // 使评分器能在「组内提前转弯」与「出组后转弯」之间选择后者（消灭 ISS-001）。
    if let Some(exit_stub) = source_group_exit_stub_len(ctx, from_id, to_id, from_side, sx, sy) {
        if exit_stub > PORT_CLEARANCE + EPS {
            evaluate_path_batch(
                build_candidate_paths(sx, sy, from_side, ex, ey, to_side, exit_stub, PORT_CLEARANCE),
                ctx,
                pair,
                scorer,
                from_id,
                to_id,
                &mut state,
            );
        }
    }

    // P1-1 trunk+fork：flowchart profile 额外评估 fork 候选（单侧 stub_len=0）
    if ctx.profile.prefer_trunk_fork {
        evaluate_path_batch(
            build_candidate_paths(sx, sy, from_side, ex, ey, to_side, 0.0, PORT_CLEARANCE),
            ctx,
            pair,
            scorer,
            from_id,
            to_id,
            &mut state,
        );
        evaluate_path_batch(
            build_candidate_paths(sx, sy, from_side, ex, ey, to_side, PORT_CLEARANCE, 0.0),
            ctx,
            pair,
            scorer,
            from_id,
            to_id,
            &mut state,
        );
    }

    // Level 1 + P1-B: channel detour——先 base margin，无 strict 再加档
    if state.best_strict.is_none() {
        let base_margin = ctx.cfg.channel_margin;
        evaluate_path_batch(
            build_channel_detours(
                sx, sy, from_side, ex, ey, to_side, pair, ctx, corridor, &[base_margin],
            ),
            ctx,
            pair,
            scorer,
            from_id,
            to_id,
            &mut state,
        );
        for &extra in &EXTRA_CHANNEL_MARGINS {
            if state.best_strict.is_some() {
                break;
            }
            if extra <= base_margin + EPS {
                continue;
            }
            evaluate_path_batch(
                build_channel_detours(
                    sx, sy, from_side, ex, ey, to_side, pair, ctx, corridor, &[extra],
                ),
                ctx,
                pair,
                scorer,
                from_id,
                to_id,
                &mut state,
            );
        }
    }

    // Level 2: 障碍物感知 z-fold
    if state.best_strict.is_none() {
        evaluate_path_batch(
            build_obstacle_aware_z_folds(sx, sy, from_side, ex, ey, to_side, pair, ctx, corridor),
            ctx,
            pair,
            scorer,
            from_id,
            to_id,
            &mut state,
        );
    }

    // Level 3: 阶梯候选（开销最高）
    if !phase1_only && state.best_strict.is_none() {
        let mut phase2 = build_staircase_candidates(
            sx, sy, from_side, ex, ey, to_side, pair, ctx, FoldOrder::VerticalFirst, corridor,
        );
        phase2.extend(build_staircase_candidates(
            sx, sy, from_side, ex, ey, to_side, pair, ctx, FoldOrder::HorizontalFirst, corridor,
        ));
        evaluate_path_batch(phase2, ctx, pair, scorer, from_id, to_id, &mut state);
    }

    // Phase B Level 4: OVG 路径搜索（组感知 Dijkstra，仅当无 strict 候选时作为 fallback）
    if state.best_strict.is_none() && !phase1_only {
        if let Some(ovg) = ctx.ovg {
            if !ovg.is_empty() {
                // 找到 from/to 节点在障碍物列表中的索引
                let from_idx = ctx.obstacles.sorted_node_ids.iter().position(|id| id == from_id);
                let to_idx = ctx.obstacles.sorted_node_ids.iter().position(|id| id == to_id);
                // 收集搜索范围内的已路由段（用于重叠惩罚）
                let margin = ovg.search_margin();
                let occ_x_lo = start.x.min(end.x) - margin;
                let occ_x_hi = start.x.max(end.x) + margin;
                let occ_y_lo = start.y.min(end.y) - margin;
                let occ_y_hi = start.y.max(end.y) + margin;
                let occupied: Vec<(f64, f64, f64, f64)> = ctx.grid
                    .query_bbox(occ_x_lo, occ_y_lo, occ_x_hi, occ_y_hi)
                    .iter()
                    .map(|s| (s.x1, s.y1, s.x2, s.y2))
                    .collect();
                let ovg_result = ovg.shortest_path_excluding(
                    start,
                    end,
                    from_side,
                    to_side,
                    BEND_PENALTY,
                    from_idx,
                    to_idx,
                    &occupied,
                );
                if let Some(ovg_path) = ovg_result {
                    // OVG 路径绕过 MAX_CANDIDATES 预算（单条高质量候选）
                    evaluate_single_ovg_path(
                        ovg_path, ctx, pair, scorer, from_id, to_id, &mut state,
                    );
                }
            }
        }
    }

    // R2：dirty 前强制升档——无 clean（strict / nodes_only）时再试更大 channel margin。
    if state.best_strict.is_none() && state.best_nodes_only.is_none() {
        for &extra in &FORCE_UPGRADE_MARGINS {
            if state.best_strict.is_some() || state.best_nodes_only.is_some() {
                break;
            }
            evaluate_path_batch(
                build_channel_detours(
                    sx, sy, from_side, ex, ey, to_side, pair, ctx, corridor, &[extra],
                ),
                ctx,
                pair,
                scorer,
                from_id,
                to_id,
                &mut state,
            );
        }
    }

    if let Some(s) = stats.as_mut() {
        s.candidate_count = state.candidate_count;
        s.hard_filter_reject_count = state
            .candidate_count
            .saturating_sub(state.strict_count + state.nodes_only_count);
        s.degraded = state.best_strict.is_none() && state.best_nodes_only.is_none();
    }

    // Iteration 2 / P1：strict 时拒绝 nodes-only（穿组）；避组脏路径可胜出。
    // R2：主选择路径禁止「穿组」；无 clean/避组时走 orthogonal_degraded_fallback。
    let chosen = if ctx.strict_group_transit {
        state.best_strict.or_else(|| {
            state.best_dirty.filter(|(_, path)| {
                path_avoids_group_interiors(
                    path,
                    from_id,
                    to_id,
                    ctx.group_ctx,
                    &ctx.obstacles.sorted_group_ids,
                )
            })
        })
    } else {
        state.best_strict.or(state.best_nodes_only)
        // 不再 .or(best_dirty)：node 穿模只能由 degraded fallback 显式接受
    };
    chosen
        .map(|(_, p)| p)
        // A2：禁止斜线逃生；候选全失败时仍输出正交路径（优先绕外框，永不对角线）。
        .unwrap_or_else(|| {
            orthogonal_degraded_fallback(ctx, start, end, from_side, to_side, from_id, to_id)
        })
}

/// 路由硬失败时的正交兜底：同轴可直线；否则在 L/Z 与外框绕行中选穿组更少者。
/// 所有候选在出口/入口强制外向 stub，避免退化路径首段反向伸入节点。
///
/// R2：先升档外框垫寻找 **clean** 路径；仅当各档均无 clean 时才允许 dirty（显式 degraded）。
fn orthogonal_degraded_fallback(
    ctx: &OrthoRoutingContext<'_>,
    start: Point,
    end: Point,
    from_side: Port,
    to_side: Port,
    from_id: &str,
    to_id: &str,
) -> Vec<Point> {
    let sx = start.x;
    let sy = start.y;
    let ex = end.x;
    let ey = end.y;

    // 升档垫：先小后大；corridor_boost / prefer_outer 时用更大外环垫，降低 degraded 穿模
    let pad_tiers: &[f64] = if ctx.prefer_outer_ring {
        if ctx.corridor_boost {
            &[84.0, 120.0, 160.0, 200.0]
        } else {
            &[56.0, 84.0, 120.0, 160.0]
        }
    } else if ctx.corridor_boost {
        &[56.0, 84.0, 120.0]
    } else {
        &[28.0, 56.0, 84.0]
    };

    let mut best_clean: Option<(u32, f64, Vec<Point>)> = None;
    let mut best_dirty: Option<(u32, f64, Vec<Point>)> = None;

    for &outer_pad in pad_tiers {
        let mut candidates: Vec<Vec<Point>> = Vec::new();
        if (sx - ex).abs() < EPS || (sy - ey).abs() < EPS {
            let straight = ensure_port_stubs(vec![start, end], from_side, to_side);
            let plen = path_length(&straight);
            let spans = plen > PORT_CLEARANCE + 1.0;
            if spans
                && path_is_clean(
                    &straight,
                    from_id,
                    to_id,
                    ctx.nodes,
                    ctx.group_ctx,
                    &ctx.obstacles.sorted_node_ids,
                )
            {
                return straight;
            }
        } else {
            candidates.extend(compute_orthogonal_path_variants(
                sx, sy, from_side, ex, ey, to_side,
            ));
            candidates.push(simplify_path(vec![
                Point::new(sx, sy),
                Point::new(ex, sy),
                Point::new(ex, ey),
            ], false));
            candidates.push(simplify_path(vec![
                Point::new(sx, sy),
                Point::new(sx, ey),
                Point::new(ex, ey),
            ], false));
        }

        if let Some((x_lo, y_lo, x_hi, y_hi)) = routing_outer_bounds(ctx, false) {
            let left = x_lo - outer_pad;
            let right = x_hi + outer_pad;
            let top = y_lo - outer_pad;
            let bottom = y_hi + outer_pad;
            for x in [left, right] {
                candidates.push(simplify_path(vec![
                    Point::new(sx, sy),
                    Point::new(x, sy),
                    Point::new(x, ey),
                    Point::new(ex, ey),
                ], false));
            }
            for y in [top, bottom] {
                candidates.push(simplify_path(vec![
                    Point::new(sx, sy),
                    Point::new(sx, y),
                    Point::new(ex, y),
                    Point::new(ex, ey),
                ], false));
            }
            // S4.x：与 build_outer_ring_candidates 一致的离排再绕行
            if ctx.prefer_outer_ring {
                let (fx, fy) = port_outward(from_side);
                let stub = Point::new(sx + fx * PORT_CLEARANCE, sy + fy * PORT_CLEARANCE);
                for &jog_y in &same_row_clearance_ys(sy, ctx) {
                    if (jog_y - sy).abs() < 8.0 {
                        continue;
                    }
                    for x in [left, right] {
                        candidates.push(simplify_path(
                            vec![
                                Point::new(sx, sy),
                                stub,
                                Point::new(stub.x, jog_y),
                                Point::new(x, jog_y),
                                Point::new(x, ey),
                                Point::new(ex, ey),
                            ],
                            false,
                        ));
                    }
                }
            }
        }

        for path in candidates {
            let path = ensure_port_stubs(path, from_side, to_side);
            if path.len() < 2 || !path_is_orthogonal(&path) {
                continue;
            }
            let group_hits = count_unrelated_group_hits_binary(
                &path,
                from_id,
                to_id,
                ctx.group_ctx,
                &ctx.obstacles.sorted_group_ids,
            );
            let clean = path_is_clean(
                &path,
                from_id,
                to_id,
                ctx.nodes,
                ctx.group_ctx,
                &ctx.obstacles.sorted_node_ids,
            );
            let len = path_length(&path);
            if clean {
                let key = (group_hits, len);
                if best_clean
                    .as_ref()
                    .is_none_or(|(g, l, _)| key < (*g, *l))
                {
                    best_clean = Some((group_hits, len, path));
                }
            } else {
                let key = (group_hits, len);
                if best_dirty
                    .as_ref()
                    .is_none_or(|(g, l, _)| key < (*g, *l))
                {
                    best_dirty = Some((group_hits, len, path));
                }
            }
        }
        // 本档已有 clean → 不再升档（控制性能）
        if best_clean.is_some() {
            break;
        }
    }

    // P1：strict 时优先少穿组（避组脏路径可胜于穿组净路径）；非 strict 仍先 clean。
    if ctx.strict_group_transit {
        match (best_clean, best_dirty) {
            (Some(c), Some(d)) => {
                if d.0 < c.0 {
                    d.2
                } else {
                    c.2
                }
            }
            (Some(c), None) => c.2,
            (None, Some(d)) => d.2,
            (None, None) => ensure_port_stubs(
                simplify_path(
                    vec![
                        Point::new(sx, sy),
                        Point::new(ex, sy),
                        Point::new(ex, ey),
                    ],
                    false,
                ),
                from_side,
                to_side,
            ),
        }
    } else {
        best_clean
            .map(|(_, _, p)| p)
            .or_else(|| best_dirty.map(|(_, _, p)| p))
            .unwrap_or_else(|| {
                ensure_port_stubs(
                    simplify_path(
                        vec![
                            Point::new(sx, sy),
                            Point::new(ex, sy),
                            Point::new(ex, ey),
                        ],
                        false,
                    ),
                    from_side,
                    to_side,
                )
            })
    }
}

/// 保证路径首/末段沿端口外向离开/进入（退化兜底专用）。
fn ensure_port_stubs(mut path: Vec<Point>, from_side: Port, to_side: Port) -> Vec<Point> {
    if path.len() < 2 {
        return path;
    }
    let (fox, foy) = port_outward(from_side);
    let (tox, toy) = port_outward(to_side);
    let start = path[0];
    let end = *path.last().unwrap();
    let from_stub = Point::new(start.x + fox * PORT_CLEARANCE, start.y + foy * PORT_CLEARANCE);
    let to_stub = Point::new(end.x + tox * PORT_CLEARANCE, end.y + toy * PORT_CLEARANCE);

    // 去掉旧首段若已反向或过短，再插入标准 stub
    let mut mid: Vec<Point> = path.drain(1..path.len().saturating_sub(1)).collect();
    // 若 mid 首点在 from 背后，丢掉
    while let Some(&p) = mid.first() {
        let fp = (p.x - start.x) * fox + (p.y - start.y) * foy;
        if fp >= PORT_CLEARANCE * 0.5 {
            break;
        }
        mid.remove(0);
    }
    while let Some(&p) = mid.last() {
        let fp = (p.x - end.x) * tox + (p.y - end.y) * toy;
        if fp >= PORT_CLEARANCE * 0.5 {
            break;
        }
        mid.pop();
    }

    let mut out = vec![start, from_stub];
    if let Some(&first_mid) = mid.first() {
        if (from_stub.x - first_mid.x).abs() > EPS && (from_stub.y - first_mid.y).abs() > EPS {
            // 禁止 (target.x, stub.y) 这类沿端口内向折回的肘点（会穿源节点）
            let elbow = port_aware_elbow(from_stub, first_mid, from_side);
            if (elbow.x - from_stub.x).abs() > EPS || (elbow.y - from_stub.y).abs() > EPS {
                out.push(elbow);
            }
        }
        out.extend(mid);
    }
    if let Some(&last) = out.last() {
        if (last.x - to_stub.x).abs() > EPS && (last.y - to_stub.y).abs() > EPS {
            let elbow = port_aware_elbow(to_stub, last, to_side);
            // 从 mid 走向 to_stub：肘点在 stub 侧，路径为 last → elbow → to_stub
            if (elbow.x - last.x).abs() > EPS || (elbow.y - last.y).abs() > EPS {
                // port_aware_elbow 以 stub 为原点；转换为 last→elbow→stub
                out.push(elbow);
            }
        }
    }
    if out.last().is_none_or(|p| (p.x - to_stub.x).abs() > EPS || (p.y - to_stub.y).abs() > EPS) {
        out.push(to_stub);
    }
    out.push(end);
    simplify_path(out, true)
}

/// A-3（契约①/Stub）：出组 stub 需越过源组外边界的额外余量（px）。
///
/// stub 终点（首个转弯点）须明确落在源组 bbox 之外，余量保证转弯点不贴在边界上。
const GROUP_EXIT_STUB_MARGIN: f64 = 8.0;

/// A-3（契约①/Stub）：跨组边源端 stub 需延伸到源组外边界的最小长度。
///
/// 跨组边（源在某叶子组内、目标不在同叶子组）从源锚点沿端口外向延伸，首段终点
/// 必须越过源组外边界才允许首个转弯。返回 `Some(len)`（≥ PORT_CLEARANCE）表示
/// 该边需要这么长的出组 stub；返回 `None` 表示非跨组边 / 无组，沿用 PORT_CLEARANCE。
///
/// 确定性（AGENTS.md §2）：仅做 HashMap 查询，不参与迭代驱动。
fn source_group_exit_stub_len(
    ctx: &OrthoRoutingContext<'_>,
    from_id: &str,
    to_id: &str,
    from_side: Port,
    sx: f64,
    sy: f64,
) -> Option<f64> {
    let from_leaf = ctx.group_ctx.node_leaf_group.get(from_id)?;
    if ctx.group_ctx.node_leaf_group.get(to_id) == Some(from_leaf) {
        return None; // 同叶子组内部边，无需出组
    }
    let group = ctx.group_ctx.groups.get(from_leaf)?;
    let (ox, oy) = port_outward(from_side);
    // 沿端口外向方向，从源锚点到源组外边界的距离。
    let dist = if ox > 0.0 {
        (group.x + group.width) - sx // Right
    } else if ox < 0.0 {
        sx - group.x // Left
    } else if oy > 0.0 {
        (group.y + group.height) - sy // Bottom
    } else {
        sy - group.y // Top
    };
    let len = (dist + GROUP_EXIT_STUB_MARGIN).max(PORT_CLEARANCE);
    Some(len)
}

fn groups_outer_bounds(ctx: &OrthoRoutingContext<'_>) -> Option<(f64, f64, f64, f64)> {
    let mut iter = ctx.group_ctx.groups.values();
    let first = iter.next()?;
    let mut x_lo = first.x;
    let mut y_lo = first.y;
    let mut x_hi = first.x + first.width;
    let mut y_hi = first.y + first.height;
    for g in iter {
        x_lo = x_lo.min(g.x);
        y_lo = y_lo.min(g.y);
        x_hi = x_hi.max(g.x + g.width);
        y_hi = y_hi.max(g.y + g.height);
    }
    Some((x_lo, y_lo, x_hi, y_hi))
}

fn nodes_outer_bounds(nodes: &HashMap<String, crate::layout::NodeLayout>) -> Option<(f64, f64, f64, f64)> {
    if nodes.is_empty() {
        return None;
    }
    let mut iter = nodes.values();
    let first = iter.next()?;
    let mut x_lo = first.x;
    let mut y_lo = first.y;
    let mut x_hi = first.x + first.width;
    let mut y_hi = first.y + first.height;
    for n in iter {
        x_lo = x_lo.min(n.x);
        y_lo = y_lo.min(n.y);
        x_hi = x_hi.max(n.x + n.width);
        y_hi = y_hi.max(n.y + n.height);
    }
    Some((x_lo, y_lo, x_hi, y_hi))
}

/// S4：有组用组外框；`prefer_nodes` 时（无组监控外环）回退节点 bbox。
fn routing_outer_bounds(
    ctx: &OrthoRoutingContext<'_>,
    prefer_nodes: bool,
) -> Option<(f64, f64, f64, f64)> {
    groups_outer_bounds(ctx).or_else(|| {
        if prefer_nodes {
            nodes_outer_bounds(ctx.nodes)
        } else {
            None
        }
    })
}

/// S4.x：以目标端口外向 stub 收束。
///
/// 若路径以「沿端口边横走」结束（Bottom 时 y=ey），`ensure_outward_stub` 会弹出
/// 端口平面折点并用 stub.x 肘点回接，随后 simplify 与上游横段共线，吞掉外环 U 形。
/// 横移须在 `PORT_CLEARANCE + NODE_OBSTACLE_PAD` 之外，否则 path_is_clean 会因擦边判脏。
fn push_target_approach(pts: &mut Vec<Point>, ex: f64, ey: f64, to_side: Port) {
    let (tx, ty) = port_outward(to_side);
    let deep_dist = PORT_CLEARANCE + NODE_OBSTACLE_PAD + 8.0;
    let deep = Point::new(ex + tx * deep_dist, ey + ty * deep_dist);
    let stub = Point::new(ex + tx * PORT_CLEARANCE, ey + ty * PORT_CLEARANCE);
    let end = Point::new(ex, ey);
    let Some(&last) = pts.last() else {
        pts.push(deep);
        pts.push(stub);
        pts.push(end);
        return;
    };
    if matches!(to_side, Port::Top | Port::Bottom) {
        if (last.y - deep.y).abs() > EPS {
            pts.push(Point::new(last.x, deep.y));
        }
        if pts
            .last()
            .is_none_or(|p| (p.x - deep.x).abs() > EPS || (p.y - deep.y).abs() > EPS)
        {
            pts.push(deep);
        }
    } else {
        if (last.x - deep.x).abs() > EPS {
            pts.push(Point::new(deep.x, last.y));
        }
        if pts
            .last()
            .is_none_or(|p| (p.x - deep.x).abs() > EPS || (p.y - deep.y).abs() > EPS)
        {
            pts.push(deep);
        }
    }
    if pts
        .last()
        .is_none_or(|p| (p.x - stub.x).abs() > EPS || (p.y - stub.y).abs() > EPS)
    {
        pts.push(stub);
    }
    pts.push(end);
}

/// S4.x：为仍穿模的监控边强制生成「离排 → 左右外廊」路径；若干净则返回 Some。
pub(super) fn force_outer_escape_path(
    start: Point,
    end: Point,
    from_side: Port,
    to_side: Port,
    from_id: &str,
    to_id: &str,
    nodes: &HashMap<String, crate::layout::NodeLayout>,
    group_ctx: &crate::layout::group::GroupRoutingContext,
    obstacles: &PreparedObstacles,
) -> Option<Vec<Point>> {
    let (x_lo, _y_lo, x_hi, _y_hi) = nodes_outer_bounds(nodes)?;
    let outer_pad = 56.0;
    let left = x_lo - outer_pad;
    let right = x_hi + outer_pad;
    let sx = start.x;
    let sy = start.y;
    let ex = end.x;
    let ey = end.y;
    let (fx, fy) = port_outward(from_side);
    let jog_ys = same_row_clearance_ys_nodes(sy, nodes);
    let mut best: Option<(f64, Vec<Point>)> = None;
    for &jog_y in &jog_ys {
        if (jog_y - sy).abs() < 8.0 {
            continue;
        }
        let toward_target =
            (jog_y - sy).signum() == (ey - sy).signum() || (ey - sy).abs() < 1.0;
        if !toward_target {
            continue;
        }
        for x in [left, right] {
            let mut pts = vec![Point::new(sx, sy)];
            if matches!(from_side, Port::Left | Port::Right) {
                pts.push(Point::new(sx + fx * PORT_CLEARANCE, sy + fy * PORT_CLEARANCE));
                pts.push(Point::new(sx + fx * PORT_CLEARANCE, jog_y));
            } else {
                pts.push(Point::new(sx, jog_y));
            }
            pts.push(Point::new(x, jog_y));
            push_target_approach(&mut pts, ex, ey, to_side);
            let path = simplify_path(pts, false);
            if path.len() < 2 || !path_is_orthogonal(&path) {
                continue;
            }
            if !path_is_clean(
                &path,
                from_id,
                to_id,
                nodes,
                group_ctx,
                &obstacles.sorted_node_ids,
            ) {
                continue;
            }
            let len = path_length(&path);
            if best.as_ref().is_none_or(|(bl, _)| len < *bl) {
                best = Some((len, path));
            }
        }
    }
    best.map(|(_, p)| p)
}

/// 外环绕行候选（左右竖廊 / 上下横廊 / U 形外框），供 feedback 主选与 degraded 共用。
///
/// 按端口朝向过滤：L/R 侧通道优先左右竖廊；T/B 侧通道用 U 形（先出侧廊再绕顶/底），
/// 避免「沿源 x 竖穿全图再横顶」横切业务层。
fn build_outer_ring_candidates(
    sx: f64,
    sy: f64,
    ex: f64,
    ey: f64,
    from_side: Port,
    to_side: Port,
    ctx: &OrthoRoutingContext<'_>,
    outer_pad: f64,
) -> Vec<Vec<Point>> {
    let Some((x_lo, y_lo, x_hi, y_hi)) = routing_outer_bounds(ctx, true) else {
        return Vec::new();
    };
    let left = x_lo - outer_pad;
    let right = x_hi + outer_pad;
    let top = y_lo - outer_pad;
    let bottom = y_hi + outer_pad;
    let prefer_vertical_ring = matches!(from_side, Port::Left | Port::Right)
        || matches!(to_side, Port::Left | Port::Right);
    let prefer_horizontal_ring = matches!(from_side, Port::Top | Port::Bottom)
        || matches!(to_side, Port::Top | Port::Bottom);
    let mut candidates = Vec::with_capacity(16);

    let push = |cands: &mut Vec<Vec<Point>>, pts: Vec<Point>| {
        cands.push(ensure_port_stubs(
            simplify_path(pts, false),
            from_side,
            to_side,
        ));
    };
    // S4.x 离排候选不加二次 stub（路径已含 PORT_CLEARANCE stub），避免 simplify 折坏
    let push_raw = |cands: &mut Vec<Vec<Point>>, pts: Vec<Point>| {
        cands.push(simplify_path(pts, false));
    };

    // S4.x：优先评估「离排再绕廊」候选（放在列表前部，避免 MAX_CANDIDATES 截断）
    // 首段必须是单段出端口→jog（可合并共线点），以便 path_is_clean 允许源节点仅在 segment0。
    if ctx.prefer_outer_ring {
        let (fx, fy) = port_outward(from_side);
        for &jog_y in &same_row_clearance_ys(sy, ctx) {
            if (jog_y - sy).abs() < 8.0 {
                continue;
            }
            let toward_target =
                (jog_y - sy).signum() == (ey - sy).signum() || (ey - sy).abs() < 1.0;
            if !toward_target {
                continue;
            }
            // 沿端口外向先走出清除距，再折向 jog（L/R 时 stub.x≠sx；T/B 时直达 jog_y）
            let mid = if matches!(from_side, Port::Left | Port::Right) {
                let stub = Point::new(sx + fx * PORT_CLEARANCE, sy + fy * PORT_CLEARANCE);
                Point::new(stub.x, jog_y)
            } else {
                Point::new(sx, jog_y)
            };
            for x in [left, right] {
                let mut pts = vec![Point::new(sx, sy)];
                if matches!(from_side, Port::Left | Port::Right) {
                    pts.push(Point::new(sx + fx * PORT_CLEARANCE, sy + fy * PORT_CLEARANCE));
                }
                pts.push(mid);
                pts.push(Point::new(x, mid.y));
                push_target_approach(&mut pts, ex, ey, to_side);
                push_raw(&mut candidates, pts);
            }
        }
    }

    if prefer_vertical_ring || !prefer_horizontal_ring {
        for x in [left, right] {
            if ctx.prefer_outer_ring {
                let mut pts = vec![Point::new(sx, sy), Point::new(x, sy)];
                push_target_approach(&mut pts, ex, ey, to_side);
                push(&mut candidates, pts);
            } else {
                push(
                    &mut candidates,
                    vec![
                        Point::new(sx, sy),
                        Point::new(x, sy),
                        Point::new(x, ey),
                        Point::new(ex, ey),
                    ],
                );
            }
        }
    }
    if prefer_horizontal_ring {
        // U 形：侧廊 → 顶/底横廊 → 回落（不沿 sx 竖穿）
        for x in [left, right] {
            for y in [top, bottom] {
                if ctx.prefer_outer_ring {
                    let mut pts = vec![
                        Point::new(sx, sy),
                        Point::new(x, sy),
                        Point::new(x, y),
                        Point::new(ex, y),
                    ];
                    push_target_approach(&mut pts, ex, ey, to_side);
                    push(&mut candidates, pts);
                } else {
                    push(
                        &mut candidates,
                        vec![
                            Point::new(sx, sy),
                            Point::new(x, sy),
                            Point::new(x, y),
                            Point::new(ex, y),
                            Point::new(ex, ey),
                        ],
                    );
                }
            }
        }
    } else if !prefer_vertical_ring {
        for y in [top, bottom] {
            if ctx.prefer_outer_ring {
                let mut pts = vec![
                    Point::new(sx, sy),
                    Point::new(sx, y),
                    Point::new(ex, y),
                ];
                push_target_approach(&mut pts, ex, ey, to_side);
                push(&mut candidates, pts);
            } else {
                push(
                    &mut candidates,
                    vec![
                        Point::new(sx, sy),
                        Point::new(sx, y),
                        Point::new(ex, y),
                        Point::new(ex, ey),
                    ],
                );
            }
        }
    }
    candidates
}

/// S4.x：与 `sy` 相交的同排节点包络之外的清除 y（上/下各一），供外环先离排再绕行。
fn same_row_clearance_ys(sy: f64, ctx: &OrthoRoutingContext<'_>) -> Vec<f64> {
    same_row_clearance_ys_nodes(sy, ctx.nodes)
}

fn same_row_clearance_ys_nodes(
    sy: f64,
    nodes: &HashMap<String, crate::layout::NodeLayout>,
) -> Vec<f64> {
    const CLEAR: f64 = 40.0;
    let mut row_top = f64::INFINITY;
    let mut row_bot = f64::NEG_INFINITY;
    let mut found = false;
    for n in nodes.values() {
        if n.y - 1.0 > sy || n.y + n.height + 1.0 < sy {
            continue;
        }
        found = true;
        row_top = row_top.min(n.y);
        row_bot = row_bot.max(n.y + n.height);
    }
    if !found {
        return vec![sy - CLEAR, sy + CLEAR];
    }
    let mut ys = vec![row_top - CLEAR, row_bot + CLEAR];
    ys.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    ys.dedup_by(|a, b| (*a - *b).abs() < 1.0);
    ys
}

fn path_is_orthogonal(path: &[Point]) -> bool {
    path.windows(2).all(|w| {
        (w[0].x - w[1].x).abs() < EPS || (w[0].y - w[1].y).abs() < EPS
    })
}

fn count_unrelated_group_hits_binary(
    path: &[Point],
    from_id: &str,
    to_id: &str,
    group_ctx: &crate::layout::group::GroupRoutingContext,
    sorted_group_ids: &[String],
) -> u32 {
    if path_avoids_group_interiors(path, from_id, to_id, group_ctx, sorted_group_ids) {
        0
    } else {
        1
    }
}

fn build_candidate_paths(
    sx: f64,
    sy: f64,
    from_side: Port,
    ex: f64,
    ey: f64,
    to_side: Port,
    from_stub_len: f64,
    to_stub_len: f64,
) -> Vec<Vec<Point>> {
    if can_go_straight(from_side, to_side, sx, sy, ex, ey) {
        return vec![vec![Point::new(sx, sy), Point::new(ex, ey)]];
    }

    let (fx, fy) = port_outward(from_side);
    let (tx, ty) = port_outward(to_side);
    let start_stub = Point::new(sx + fx * from_stub_len, sy + fy * from_stub_len);
    let end_stub = Point::new(ex + tx * to_stub_len, ey + ty * to_stub_len);

    let middles = compute_orthogonal_path_variants(
        start_stub.x,
        start_stub.y,
        from_side,
        end_stub.x,
        end_stub.y,
        to_side,
    );

    let mut candidates: Vec<Vec<Point>> = middles
        .into_iter()
        .map(|middle| {
            let mut path = Vec::with_capacity(middle.len() + 2);
            path.push(Point::new(sx, sy));
            path.push(start_stub);
            path.extend(middle.into_iter().skip(1));
            path.push(Point::new(ex, ey));
            simplify_path(path, true)
        })
        .collect();

    // P0-1: 混合端口（L 形组合）扩展候选——沿端口方向延伸 stub 再转向，
    // 为硬过滤提供更多绕行选项（修复 G1 混合端口盲区）。
    // P1-1: fork 模式（stub_len=0）下跳过此扩展。
    if from_stub_len > 0.0
        && to_stub_len > 0.0
        && is_vertical_port(from_side) != is_vertical_port(to_side)
    {
        for &ext in &[2.5, 4.0, 6.0] {
            let ext_start = Point::new(
                sx + fx * from_stub_len * ext,
                sy + fy * from_stub_len * ext,
            );
            let ext_end = Point::new(ex + tx * to_stub_len * ext, ey + ty * to_stub_len * ext);
            let ext_middles = compute_orthogonal_path_variants(
                ext_start.x,
                ext_start.y,
                from_side,
                ext_end.x,
                ext_end.y,
                to_side,
            );
            for middle in ext_middles {
                let mut path = Vec::with_capacity(middle.len() + 3);
                path.push(Point::new(sx, sy));
                path.push(ext_start);
                path.extend(middle.into_iter().skip(1));
                path.push(ext_end);
                path.push(Point::new(ex, ey));
                candidates.push(simplify_path(path, true));
            }
        }
    }

    candidates
}

/// 为指定方向的中间折叠段生成 Z-shape 折点候选路径。
/// axis 为中间段的延伸方向（Horizontal=水平段 y=fold，Vertical=垂直段 x=fold）。
fn generate_axis_folds(
    axis: Axis,
    s1: Point,
    e1: Point,
    sx: f64, sy: f64, ex: f64, ey: f64,
    pair: &EndpointPair,
    ctx: &OrthoRoutingContext,
    endpoint_groups: &HashSet<&str>,
    margin: f64,
    corridor: EdgeCorridor,
) -> Vec<Vec<Point>> {
    let nodes = ctx.nodes;
    let groups = &ctx.group_ctx.groups;
    let from_id = pair.from_id();
    let to_id = pair.to_id();

    let mut folds = collect_obstacle_boundaries_on_axis(
        axis, nodes, groups, endpoint_groups, from_id, to_id,
        NODE_OBSTACLE_PAD, GROUP_OBSTACLE_PAD, margin,
        &ctx.obstacles.sorted_node_ids, &ctx.obstacles.sorted_group_ids,
        corridor,
    );
    folds.extend(group_gap_midpoints_on_axis(
        groups,
        &ctx.obstacles.sorted_group_ids,
        &ctx.group_ctx.corridors,
        axis,
        corridor,
    ));

    let mut indexed: Vec<(usize, f64)> = folds.into_iter().enumerate().collect();
    indexed.sort_by(|a, b| {
        a.1
            .partial_cmp(&b.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(&b.0))
    });
    let mut folds: Vec<f64> = indexed.into_iter().map(|(_, v)| v).collect();
    folds.dedup_by(|a, b| (*a - *b).abs() < 1.0);

    let s_main = axis.main_coord(s1);
    let e_main = axis.main_coord(e1);

    folds.into_iter().map(|fold| {
        let p1 = axis.point(s_main, fold);
        let p2 = axis.point(e_main, fold);
        simplify_path(vec![
            Point::new(sx, sy), s1, p1, p2, e1, Point::new(ex, ey),
        ], true)
    }).collect()
}

/// 障碍物感知的 Z-shape 折点候选：在障碍物边界处生成折点，
/// 使 Z-shape 能在障碍物上方/下方（或左/右）折叠，避免穿障。
///
/// 对所有端口组合生效（同轴反向、同向、混合端口）。
/// 固定比例折点可能恰好落在障碍物内部，此函数补充障碍物边界处的折点候选。
/// 混合端口（如 Top→Right）的边在固定 L-shape 全部穿障时，需要障碍物边界处的
/// 折点候选来绕行。路径结构 (sx,sy)→s1→(s1.0,fy)→(e1.0,fy)→e1→(ex,ey) 对所有
/// 端口组合都是正交的。
fn build_obstacle_aware_z_folds(
    sx: f64,
    sy: f64,
    from_side: Port,
    ex: f64,
    ey: f64,
    to_side: Port,
    pair: &EndpointPair,
    ctx: &OrthoRoutingContext,
    corridor: EdgeCorridor,
) -> Vec<Vec<Point>> {
    let from_vertical = is_vertical_port(from_side);
    let to_vertical = is_vertical_port(to_side);
    let mixed = from_vertical != to_vertical;

    let endpoint_groups = ctx.group_ctx.endpoint_group_set(pair.from_id(), pair.to_id());

    let (out_fx, out_fy) = port_outward(from_side);
    let (out_tx, out_ty) = port_outward(to_side);
    let s1 = Point::new(sx + out_fx * PORT_CLEARANCE, sy + out_fy * PORT_CLEARANCE);
    let e1 = Point::new(ex + out_tx * PORT_CLEARANCE, ey + out_ty * PORT_CLEARANCE);

    let margin = ctx.cfg.channel_margin;
    let mut candidates = Vec::new();

    if from_vertical || mixed {
        candidates.extend(generate_axis_folds(
            Axis::Horizontal, s1, e1, sx, sy, ex, ey, pair, ctx, &endpoint_groups, margin, corridor,
        ));
    }
    if !from_vertical || mixed {
        candidates.extend(generate_axis_folds(
            Axis::Vertical, s1, e1, sx, sy, ex, ey, pair, ctx, &endpoint_groups, margin, corridor,
        ));
    }

    candidates
}

/// 阶梯路径的折叠顺序：先沿主轴折叠（VerticalFirst）或先沿交叉轴折叠（HorizontalFirst）。
#[derive(Clone, Copy)]
enum FoldOrder { VerticalFirst, HorizontalFirst }

/// 对坐标列表排序、去重、下采样。
fn prepare_coords(coords: &mut Vec<f64>) {
    coords.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    coords.dedup_by(|a, b| (*a - *b).abs() < 1.0);
    const MAX_PER_AXIS: usize = 6;
    if coords.len() > MAX_PER_AXIS {
        let step = coords.len() as f64 / MAX_PER_AXIS as f64;
        *coords = (0..MAX_PER_AXIS)
            .map(|i| coords[(i as f64 * step) as usize])
            .collect();
    }
}

/// 阶梯候选路径（2-fold）：结合主轴折点和交叉轴通道，
/// 生成能同时绕开两个方向障碍物的路径。
///
/// 当单折路径（Z-fold / channel detour）全部穿障时，阶梯路径提供更多绕行选项。
/// fold_order 控制折叠顺序：VerticalFirst 先沿端口方向折叠，HorizontalFirst 先沿垂直端口方向折叠。
fn build_staircase_candidates(
    sx: f64,
    sy: f64,
    from_side: Port,
    ex: f64,
    ey: f64,
    to_side: Port,
    pair: &EndpointPair,
    ctx: &OrthoRoutingContext,
    fold_order: FoldOrder,
    corridor: EdgeCorridor,
) -> Vec<Vec<Point>> {
    let from_id = pair.from_id();
    let to_id = pair.to_id();
    let nodes = ctx.nodes;
    let groups = &ctx.group_ctx.groups;
    let margin = ctx.cfg.channel_margin;

    let from_vertical = is_vertical_port(from_side);
    let _to_vertical = is_vertical_port(to_side);

    let endpoint_groups = ctx.group_ctx.endpoint_group_set(from_id, to_id);

    let (out_fx, out_fy) = port_outward(from_side);
    let (out_tx, out_ty) = port_outward(to_side);
    let s1 = Point::new(sx + out_fx * PORT_CLEARANCE, sy + out_fy * PORT_CLEARANCE);
    let e1 = Point::new(ex + out_tx * PORT_CLEARANCE, ey + out_ty * PORT_CLEARANCE);

    let axis = if from_vertical { Axis::Vertical } else { Axis::Horizontal };

    let mut fold_coords = collect_obstacle_boundaries_on_axis(
        axis.other(), nodes, groups, &endpoint_groups, from_id, to_id,
        NODE_OBSTACLE_PAD, GROUP_OBSTACLE_PAD, margin,
        &ctx.obstacles.sorted_node_ids, &ctx.obstacles.sorted_group_ids,
        corridor,
    );
    let mut channel_coords = collect_obstacle_boundaries_on_axis(
        axis, nodes, groups, &endpoint_groups, from_id, to_id,
        NODE_OBSTACLE_PAD, GROUP_OBSTACLE_PAD, margin,
        &ctx.obstacles.sorted_node_ids, &ctx.obstacles.sorted_group_ids,
        corridor,
    );

    fold_coords.extend(group_gap_midpoints_on_axis(
        groups,
        &ctx.obstacles.sorted_group_ids,
        &ctx.group_ctx.corridors,
        axis.other(),
        corridor,
    ));
    channel_coords.extend(group_gap_midpoints_on_axis(
        groups,
        &ctx.obstacles.sorted_group_ids,
        &ctx.group_ctx.corridors,
        axis,
        corridor,
    ));

    prepare_coords(&mut fold_coords);
    prepare_coords(&mut channel_coords);

    let s_main = axis.main_coord(s1);
    let s_cross = axis.cross_coord(s1);
    let e_main = axis.main_coord(e1);
    let e_cross = axis.cross_coord(e1);

    let mut candidates = Vec::new();

    match fold_order {
        FoldOrder::VerticalFirst => {
            for &fold in &fold_coords {
                for &channel in &channel_coords {
                    let p1 = axis.point(fold, s_cross);
                    let p2 = axis.point(fold, channel);
                    let p3 = axis.point(e_main, channel);
                    candidates.push(simplify_path(vec![
                        Point::new(sx, sy), s1, p1, p2, p3, e1, Point::new(ex, ey),
                    ], true));
                }
            }
        }
        FoldOrder::HorizontalFirst => {
            for &channel in &channel_coords {
                for &fold in &fold_coords {
                    let p1 = axis.point(s_main, channel);
                    let p2 = axis.point(fold, channel);
                    let p3 = axis.point(fold, e_cross);
                    candidates.push(simplify_path(vec![
                        Point::new(sx, sy), s1, p1, p2, p3, e1, Point::new(ex, ey),
                    ], true));
                }
            }
        }
    }

    candidates
}

/// 为指定轴方向生成侧通道绕行候选。
fn build_channel_detours_on_axis(
    axis: Axis,
    sx: f64,
    sy: f64,
    from_side: Port,
    ex: f64,
    ey: f64,
    to_side: Port,
    pair: &EndpointPair,
    ctx: &OrthoRoutingContext,
    endpoint_groups: &HashSet<&str>,
    margins: &[f64],
    base_margin: f64,
    _corridor: EdgeCorridor,
) -> Vec<Vec<Point>> {
    let from_id = pair.from_id();
    let to_id = pair.to_id();
    let nodes = ctx.nodes;
    let groups = &ctx.group_ctx.groups;

    let start = Point::new(sx, sy);
    let end = Point::new(ex, ey);
    let band_lo = axis.main_coord(start).min(axis.main_coord(end));
    let band_hi = axis.main_coord(start).max(axis.main_coord(end));
    let corridor_lo = axis.cross_coord(start).min(axis.cross_coord(end));
    let corridor_hi = axis.cross_coord(start).max(axis.cross_coord(end));

    let mut blocking_in_corridor = false;
    let mut all_bounds: Vec<(f64, f64)> = Vec::new();
    let mut min_cross = f64::MAX;
    let mut max_cross = f64::MIN;

    // 使用预排序的 node_ids / group_ids（方案 2，确定性 AGENTS.md §2）
    for nid in &ctx.obstacles.sorted_node_ids {
        if nid.as_str() == from_id || nid.as_str() == to_id {
            continue;
        }
        let r = Rect::from(&nodes[nid]);
        let pad = NODE_OBSTACLE_PAD;
        let (m_lo, m_hi) = r.range_on_axis(axis);
        let (c_lo, c_hi) = r.cross_range_on_axis(axis);
        let m_lo_pad = m_lo - pad;
        let m_hi_pad = m_hi + pad;
        let c_lo_pad = c_lo - pad;
        let c_hi_pad = c_hi + pad;
        if m_hi_pad > band_lo + EPS && m_lo_pad < band_hi - EPS {
            all_bounds.push((c_lo_pad, c_hi_pad));
            if c_hi_pad > corridor_lo - EPS && c_lo_pad < corridor_hi + EPS {
                blocking_in_corridor = true;
                min_cross = min_cross.min(c_lo_pad);
                max_cross = max_cross.max(c_hi_pad);
            }
        }
    }

    for gid in &ctx.obstacles.sorted_group_ids {
        if endpoint_groups.contains(gid.as_str()) {
            continue;
        }
        if let Some(gl) = groups.get(gid) {
            let r = Rect::from(gl);
            let pad = GROUP_OBSTACLE_PAD;
            let (m_lo, m_hi) = r.range_on_axis(axis);
            let (c_lo, c_hi) = r.cross_range_on_axis(axis);
            let m_lo_pad = m_lo - pad;
            let m_hi_pad = m_hi + pad;
            let c_lo_pad = c_lo - pad;
            let c_hi_pad = c_hi + pad;
            if m_hi_pad > band_lo + EPS && m_lo_pad < band_hi - EPS {
                all_bounds.push((c_lo_pad, c_hi_pad));
                if c_hi_pad > corridor_lo - EPS && c_lo_pad < corridor_hi + EPS {
                    blocking_in_corridor = true;
                    min_cross = min_cross.min(c_lo_pad);
                    max_cross = max_cross.max(c_hi_pad);
                }
            }
        }
    }

    if !blocking_in_corridor {
        return Vec::new();
    }

    let (out_fx, out_fy) = port_outward(from_side);
    let (out_tx, out_ty) = port_outward(to_side);
    let s1 = match axis {
        Axis::Vertical => Point::new(sx, sy + out_fy * PORT_CLEARANCE),
        Axis::Horizontal => Point::new(sx + out_fx * PORT_CLEARANCE, sy),
    };
    let e1 = match axis {
        Axis::Vertical => Point::new(ex, ey + out_ty * PORT_CLEARANCE),
        Axis::Horizontal => Point::new(ex + out_tx * PORT_CLEARANCE, ey),
    };

    let mut channel_coords: Vec<f64> = Vec::new();

    // Phase B3: 注入全局通道规划分配的坐标（优先候选）
    if let Some(planned) = ctx.planned_channel {
        channel_coords.push(planned);
    }

    for &margin in margins {
        channel_coords.push(channel_coord_on_axis(
            max_cross,
            true,
            groups,
            &ctx.obstacles.sorted_group_ids,
            &ctx.group_ctx.corridors,
            axis,
            margin,
            band_lo,
            band_hi,
        ));
        channel_coords.push(channel_coord_on_axis(
            min_cross,
            false,
            groups,
            &ctx.obstacles.sorted_group_ids,
            &ctx.group_ctx.corridors,
            axis,
            margin,
            band_lo,
            band_hi,
        ));
    }

    all_bounds.sort_by(|a, b| {
        a.0
            .partial_cmp(&b.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    });
    for &(lo, hi) in &all_bounds {
        for &margin in margins {
            channel_coords.push(lo - margin);
            channel_coords.push(hi + margin);
        }
    }

    for w in all_bounds.windows(2) {
        let gap_lo = w[0].1;
        let gap_hi = w[1].0;
        if gap_hi > gap_lo + 2.0 * base_margin {
            let mid = (gap_lo + gap_hi) / 2.0;
            channel_coords.push(mid);
        }
    }

    channel_coords.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    channel_coords.dedup_by(|a, b| (*a - *b).abs() < 1.0);

    const MAX_CHANNEL_CANDIDATES: usize = 30;
    if channel_coords.len() > MAX_CHANNEL_CANDIDATES {
        let step = channel_coords.len() as f64 / MAX_CHANNEL_CANDIDATES as f64;
        channel_coords = (0..MAX_CHANNEL_CANDIDATES)
            .map(|i| channel_coords[(i as f64 * step) as usize])
            .collect();
    }

    let mut candidates = Vec::new();
    for &channel in &channel_coords {
        let p_mid1 = axis.point(axis.main_coord(s1), channel);
        let p_mid2 = axis.point(axis.main_coord(e1), channel);
        candidates.push(simplify_path(vec![
            Point::new(sx, sy),
            s1,
            p_mid1,
            p_mid2,
            e1,
            Point::new(ex, ey),
        ], true));
    }
    candidates
}

/// P0-1: 多档 channel_margin；P1-B 由调用方按档懒加载传入 `margins`。
fn build_channel_detours(
    sx: f64,
    sy: f64,
    from_side: Port,
    ex: f64,
    ey: f64,
    to_side: Port,
    pair: &EndpointPair,
    ctx: &OrthoRoutingContext,
    corridor: EdgeCorridor,
    margins: &[f64],
) -> Vec<Vec<Point>> {
    if margins.is_empty() {
        return Vec::new();
    }

    let from_id = pair.from_id();
    let to_id = pair.to_id();

    let endpoint_groups = ctx.group_ctx.endpoint_group_set(from_id, to_id);

    let from_vertical = is_vertical_port(from_side);
    let to_vertical = is_vertical_port(to_side);
    if from_vertical != to_vertical {
        return Vec::new();
    }

    let axis = if from_vertical { Axis::Vertical } else { Axis::Horizontal };
    let base_margin = margins[0];

    build_channel_detours_on_axis(
        axis, sx, sy, from_side, ex, ey, to_side, pair, ctx,
        &endpoint_groups, margins, base_margin, corridor,
    )
}

/// 计算侧通道坐标（垂直通道 x 或水平通道 y）。
///
/// `edge_main` 是障碍节点在通道位置轴上的外边界（右通道: max_right, 左通道: min_left, ...）。
/// `outward_positive=true` 表示向正方向偏移（右/下），`false` 表示向负方向偏移（左/上）。
/// 默认偏移 `margin`；若附近有分组边框，将通道放在"节点列边缘"和"分组边框"中点，
/// 避免贴着边框走。通道始终距分组边框至少 `margin`。
fn channel_coord_on_axis(
    edge_main: f64,
    outward_positive: bool,
    groups: &HashMap<String, GroupLayout>,
    sorted_group_ids: &[String],
    corridors: &[GroupCorridor],
    axis: Axis,
    margin: f64,
    span_min: f64,
    span_max: f64,
) -> f64 {
    let corridor_axis = match axis {
        Axis::Vertical => CorridorAxis::Vertical,
        Axis::Horizontal => CorridorAxis::Horizontal,
    };

    let ch = if outward_positive {
        let mut ch = edge_main + margin;
        for gid in sorted_group_ids {
            let Some(g) = groups.get(gid) else { continue };
            let r = Rect::from(g);
            let (_, wall) = r.cross_range_on_axis(axis);
            if wall > edge_main + EPS && wall < edge_main + 3.0 * margin {
                let centered = ((edge_main + wall) / 2.0).min(wall - margin);
                ch = ch.min(centered);
            }
        }
        ch.max(edge_main + MIN_CHANNEL_CLEARANCE)
    } else {
        let mut ch = edge_main - margin;
        for gid in sorted_group_ids {
            let Some(g) = groups.get(gid) else { continue };
            let r = Rect::from(g);
            let (wall, _) = r.cross_range_on_axis(axis);
            if wall < edge_main - EPS && wall > edge_main - 3.0 * margin {
                let centered = ((edge_main + wall) / 2.0).max(wall + margin);
                ch = ch.max(centered);
            }
        }
        ch.min(edge_main - MIN_CHANNEL_CLEARANCE)
    };
    prefer_corridor_coord(
        corridor_axis,
        ch,
        span_min,
        span_max,
        corridors,
        3.0 * margin,
    )
}

fn compute_orthogonal_path_variants(
    sx: f64,
    sy: f64,
    from_side: Port,
    ex: f64,
    ey: f64,
    to_side: Port,
) -> Vec<Vec<Point>> {
    if can_go_straight(from_side, to_side, sx, sy, ex, ey) {
        return vec![vec![Point::new(sx, sy), Point::new(ex, ey)]];
    }

    let from_vertical = is_vertical_port(from_side);
    let to_vertical = is_vertical_port(to_side);

    if from_vertical != to_vertical {
        let l1 = if from_vertical {
            vec![Point::new(sx, sy), Point::new(sx, ey), Point::new(ex, ey)]
        } else {
            vec![Point::new(sx, sy), Point::new(ex, sy), Point::new(ex, ey)]
        };
        let mut variants = vec![simplify_path(l1, false)];
        if !from_vertical && (ey - sy).abs() > EPS {
            variants.push(simplify_path(vec![Point::new(sx, sy), Point::new(sx, ey), Point::new(ex, ey)], false));
        }
        if from_vertical && (ex - sx).abs() > EPS {
            variants.push(simplify_path(vec![Point::new(sx, sy), Point::new(ex, sy), Point::new(ex, ey)], false));
        }
        return variants;
    }

    if is_opposite_ports(from_side, to_side) {
        let ratios = [0.25, 0.18, 0.32, 0.12, 0.4, 0.5, 0.6, 0.75];
        if from_vertical {
            ratios
                .iter()
                .map(|r| {
                    let yj = sy + (ey - sy) * r;
                    simplify_path(vec![Point::new(sx, sy), Point::new(sx, yj), Point::new(ex, yj), Point::new(ex, ey)], false)
                })
                .collect()
        } else {
            ratios
                .iter()
                .map(|r| {
                    let xj = sx + (ex - sx) * r;
                    simplify_path(vec![Point::new(sx, sy), Point::new(xj, sy), Point::new(xj, ey), Point::new(ex, ey)], false)
                })
                .collect()
        }
    } else {
        vec![simplify_path(same_side_path(sx, sy, from_side, ex, ey), false)]
    }
}

pub(super) fn port_outward(side: Port) -> (f64, f64) {
    match side {
        Port::Top => (0.0, -1.0),
        Port::Bottom => (0.0, 1.0),
        Port::Left => (-1.0, 0.0),
        Port::Right => (1.0, 0.0),
    }
}

/// 从端口 stub 拐向 `target` 时选择肘点，使 **stub→肘点** 不沿端口内向折回。
///
/// 错误肘点 `(target.x, stub.y)`（Left/Right）会在出 stub 后立刻反向穿源节点；
/// 应优先在 stub 外向坐标上转弯：Left/Right 用 `(stub.x, target.y)`，Top/Bottom 用 `(target.x, stub.y)`。
pub(super) fn port_aware_elbow(stub: Point, target: Point, side: Port) -> Point {
    let (ox, oy) = port_outward(side);
    let cand_h = Point::new(target.x, stub.y);
    let cand_v = Point::new(stub.x, target.y);
    let score = |elbow: Point| -> f64 {
        let dx = elbow.x - stub.x;
        let dy = elbow.y - stub.y;
        if dx.abs() < EPS && dy.abs() < EPS {
            return f64::NEG_INFINITY;
        }
        dx * ox + dy * oy
    };
    let sh = score(cand_h);
    let sv = score(cand_v);
    if sh > sv + EPS {
        cand_h
    } else if sv > sh + EPS {
        cand_v
    } else {
        // 平局（常见：两肘外向投影均为 0）：Left/Right 保 stub.x，Top/Bottom 保 stub.y
        match side {
            Port::Left | Port::Right => cand_v,
            Port::Top | Port::Bottom => cand_h,
        }
    }
}

fn is_opposite_ports(a: Port, b: Port) -> bool {
    matches!(
        (a, b),
        (Port::Top, Port::Bottom)
            | (Port::Bottom, Port::Top)
            | (Port::Left, Port::Right)
            | (Port::Right, Port::Left)
    )
}

/// Can go straight when opposite ports are co-axially aligned
fn can_go_straight(from_side: Port, to_side: Port, sx: f64, sy: f64, ex: f64, ey: f64) -> bool {
    match (from_side, to_side) {
        (Port::Bottom, Port::Top) => (sx - ex).abs() < EPS && sy < ey,
        (Port::Top, Port::Bottom) => (sx - ex).abs() < EPS && sy > ey,
        (Port::Left, Port::Right) => (sy - ey).abs() < EPS && sx > ex,
        (Port::Right, Port::Left) => (sy - ey).abs() < EPS && sx < ex,
        _ => false,
    }
}

const SAME_SIDE_PADDING: f64 = 24.0;

fn same_side_path(sx: f64, sy: f64, from_side: Port, ex: f64, ey: f64) -> Vec<Point> {
    match from_side {
        Port::Bottom => {
            let y_out = sy.max(ey) + SAME_SIDE_PADDING;
            vec![Point::new(sx, sy), Point::new(sx, y_out), Point::new(ex, y_out), Point::new(ex, ey)]
        }
        Port::Top => {
            let y_out = sy.min(ey) - SAME_SIDE_PADDING;
            vec![Point::new(sx, sy), Point::new(sx, y_out), Point::new(ex, y_out), Point::new(ex, ey)]
        }
        Port::Left => {
            let x_out = sx.min(ex) - SAME_SIDE_PADDING;
            vec![Point::new(sx, sy), Point::new(x_out, sy), Point::new(x_out, ey), Point::new(ex, ey)]
        }
        Port::Right => {
            let x_out = sx.max(ex) + SAME_SIDE_PADDING;
            vec![Point::new(sx, sy), Point::new(x_out, sy), Point::new(x_out, ey), Point::new(ex, ey)]
        }
    }
}
