//! Path building for orthogonal edge routing

use super::*;
use crate::layout::geometry::{Axis, Point, Rect, EPS};
use crate::layout::group::{prefer_corridor_coord, CorridorAxis, GroupCorridor};
use crate::layout::{GroupLayout, Port};
use std::collections::{HashMap, HashSet};

/// 计算分组间垂直于指定段方向的通道间隙中点坐标。
/// - axis=Vertical：段沿垂直方向延伸（y 轴为主轴），找 x 方向间隙中点（垂直通道 x 坐标）；
/// - axis=Horizontal：段沿水平方向延伸（x 轴为主轴），找 y 方向间隙中点（水平通道 y 坐标）。
/// 只考虑主轴范围重叠的分组对（同一行/列），避免不同行/列分组的干扰。
fn group_gap_midpoints_on_axis(groups: &HashMap<String, GroupLayout>, axis: Axis) -> Vec<f64> {
    let mut ranges: Vec<(f64, f64, f64, f64)> = groups
        .values()
        .map(|g| {
            let r = Rect::from(g);
            let (main_lo, main_hi) = r.range_on_axis(axis);
            let (cross_lo, cross_hi) = r.cross_range_on_axis(axis);
            (cross_lo, cross_hi, main_lo, main_hi)
        })
        .collect();
    ranges.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut mids = Vec::new();
    for i in 0..ranges.len() {
        for j in i + 1..ranges.len() {
            let (_ac_lo, ac_hi, am_lo, am_hi) = ranges[i];
            let (bc_lo, _bc_hi, bm_lo, bm_hi) = ranges[j];
            if am_hi <= bm_lo || bm_hi <= am_lo {
                continue;
            }
            if ac_hi < bc_lo {
                mids.push((ac_hi + bc_lo) / 2.0);
            }
        }
    }
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
) -> Vec<f64> {
    let mut coords = Vec::new();

    let mut node_ids: Vec<&String> = nodes.keys().collect();
    node_ids.sort();
    for nid in &node_ids {
        if nid.as_str() == from_id || nid.as_str() == to_id {
            continue;
        }
        let r = Rect::from(&nodes[*nid]);
        let (lo, hi) = r.cross_range_on_axis(axis);
        coords.push((lo - node_pad) - margin);
        coords.push((hi + node_pad) + margin);
    }

    let mut group_ids: Vec<&String> = groups.keys().collect();
    group_ids.sort();
    for gid in &group_ids {
        if endpoint_groups.contains(gid.as_str()) {
            continue;
        }
        if let Some(gl) = groups.get(*gid) {
            let r = Rect::from(gl);
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

/// P2-1: 带 debug 统计的路径选择
pub fn select_best_path_with_scorer_stats(
    ctx: &RoutingContext,
    pair: &EndpointPair,
    scorer: &dyn CandidateScorer,
    mut stats: Option<&mut PathSelectStats>,
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

    // P0-1: 三阶段硬过滤 + 退化。
    // 优先级：严格干净（避开节点+分组）> 节点干净（避开节点，可能穿分组）> 脏候选（穿节点）
    // 当分组铺满画布时，严格干净候选可能不存在，退化为节点干净候选以避免穿节点。
    // 先检查 path_is_clean（节点），通过后再检查 path_avoids_group_interiors（分组），
    // 避免 nodes_only 候选重复执行节点检查。
    let mut best_strict: Option<(f64, Vec<Point>)> = None;
    let mut best_nodes_only: Option<(f64, Vec<Point>)> = None;
    let mut best_dirty: Option<(f64, Vec<Point>)> = None;
    let mut strict_count = 0usize;
    let mut nodes_only_count = 0usize;
    let mut candidate_count = 0usize;

    // Phase 1: 基础候选 + 通道绕行 + Z-fold（开销低，覆盖大多数场景）
    let mut phase1 = build_candidate_paths(
        sx, sy, from_side, ex, ey, to_side, PORT_CLEARANCE, PORT_CLEARANCE,
    );
    phase1.extend(build_channel_detours(
        sx, sy, from_side, ex, ey, to_side, pair, ctx,
    ));
    phase1.extend(build_obstacle_aware_z_folds(
        sx, sy, from_side, ex, ey, to_side, pair, ctx,
    ));
    candidate_count += phase1.len();

    for path in phase1 {
        if path_is_clean(&path, from_id, to_id, ctx.nodes, ctx.group_ctx) {
            let score = scorer.score(&path, ctx, pair);
            if path_avoids_group_interiors(&path, from_id, to_id, ctx.group_ctx) {
                strict_count += 1;
                if best_strict.as_ref().is_none_or(|(bs, _)| score < *bs) {
                    best_strict = Some((score, path));
                }
            } else {
                nodes_only_count += 1;
                if best_nodes_only.as_ref().is_none_or(|(bs, _)| score < *bs) {
                    best_nodes_only = Some((score, path));
                }
            }
        } else {
            let score = path_length(&path) + path.len().saturating_sub(2) as f64 * BEND_PENALTY;
            if best_dirty.as_ref().is_none_or(|(bs, _)| score < *bs) {
                best_dirty = Some((score, path));
            }
        }
    }

    // Phase 2: 若 Phase 1 未找到严格干净候选，生成阶梯候选（开销高，但覆盖复杂场景）
    if best_strict.is_none() {
        let mut phase2 = build_staircase_candidates(
            sx, sy, from_side, ex, ey, to_side, pair, ctx, FoldOrder::VerticalFirst,
        );
        phase2.extend(build_staircase_candidates(
            sx, sy, from_side, ex, ey, to_side, pair, ctx, FoldOrder::HorizontalFirst,
        ));
        candidate_count += phase2.len();

        for path in phase2 {
            if path_is_clean(&path, from_id, to_id, ctx.nodes, ctx.group_ctx) {
                let score = scorer.score(&path, ctx, pair);
                if path_avoids_group_interiors(&path, from_id, to_id, ctx.group_ctx) {
                    strict_count += 1;
                    if best_strict.as_ref().is_none_or(|(bs, _)| score < *bs) {
                        best_strict = Some((score, path));
                    }
                } else {
                    nodes_only_count += 1;
                    if best_nodes_only.as_ref().is_none_or(|(bs, _)| score < *bs) {
                        best_nodes_only = Some((score, path));
                    }
                }
            } else {
                let score = path_length(&path) + path.len().saturating_sub(2) as f64 * BEND_PENALTY;
                if best_dirty.as_ref().is_none_or(|(bs, _)| score < *bs) {
                    best_dirty = Some((score, path));
                }
            }
        }
    }

    // P2-1: 记录统计
    if let Some(s) = stats.as_mut() {
        s.candidate_count = candidate_count;
        s.hard_filter_reject_count = candidate_count.saturating_sub(strict_count + nodes_only_count);
        s.degraded = best_strict.is_none() && best_nodes_only.is_none();
    }

    best_strict
        .or(best_nodes_only)
        .or(best_dirty)
        .map(|(_, p)| p)
        .unwrap_or_else(|| vec![start, end])
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
            simplify_path_preserving_stubs(path)
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
                candidates.push(simplify_path_preserving_stubs(path));
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
    ctx: &RoutingContext,
    endpoint_groups: &HashSet<&str>,
    margin: f64,
) -> Vec<Vec<Point>> {
    let nodes = ctx.nodes;
    let groups = &ctx.group_ctx.groups;
    let from_id = pair.from_id();
    let to_id = pair.to_id();

    let mut folds = collect_obstacle_boundaries_on_axis(
        axis, nodes, groups, endpoint_groups, from_id, to_id,
        NODE_OBSTACLE_PAD, GROUP_OBSTACLE_PAD, margin,
    );
    folds.extend(group_gap_midpoints_on_axis(groups, axis));

    folds.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    folds.dedup_by(|a, b| (*a - *b).abs() < 1.0);

    let s_main = axis.main_coord(s1);
    let e_main = axis.main_coord(e1);

    folds.into_iter().map(|fold| {
        let p1 = axis.point(s_main, fold);
        let p2 = axis.point(e_main, fold);
        simplify_path_preserving_stubs(vec![
            Point::new(sx, sy), s1, p1, p2, e1, Point::new(ex, ey),
        ])
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
    ctx: &RoutingContext,
) -> Vec<Vec<Point>> {
    let from_vertical = is_vertical_port(from_side);
    let to_vertical = is_vertical_port(to_side);
    let mixed = from_vertical != to_vertical;

    let endpoint_groups: HashSet<&str> = ctx
        .group_ctx
        .node_to_groups
        .get(pair.from_id())
        .into_iter()
        .flatten()
        .chain(ctx.group_ctx.node_to_groups.get(pair.to_id()).into_iter().flatten())
        .map(|s| s.as_str())
        .collect();

    let (out_fx, out_fy) = port_outward(from_side);
    let (out_tx, out_ty) = port_outward(to_side);
    let s1 = Point::new(sx + out_fx * PORT_CLEARANCE, sy + out_fy * PORT_CLEARANCE);
    let e1 = Point::new(ex + out_tx * PORT_CLEARANCE, ey + out_ty * PORT_CLEARANCE);

    let margin = ctx.cfg.channel_margin;
    let mut candidates = Vec::new();

    if from_vertical || mixed {
        candidates.extend(generate_axis_folds(
            Axis::Horizontal, s1, e1, sx, sy, ex, ey, pair, ctx, &endpoint_groups, margin,
        ));
    }
    if !from_vertical || mixed {
        candidates.extend(generate_axis_folds(
            Axis::Vertical, s1, e1, sx, sy, ex, ey, pair, ctx, &endpoint_groups, margin,
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
    ctx: &RoutingContext,
    fold_order: FoldOrder,
) -> Vec<Vec<Point>> {
    let from_id = pair.from_id();
    let to_id = pair.to_id();
    let nodes = ctx.nodes;
    let groups = &ctx.group_ctx.groups;
    let margin = ctx.cfg.channel_margin;

    let from_vertical = is_vertical_port(from_side);
    let _to_vertical = is_vertical_port(to_side);

    let endpoint_groups: HashSet<&str> = ctx
        .group_ctx
        .node_to_groups
        .get(from_id)
        .into_iter()
        .flatten()
        .chain(ctx.group_ctx.node_to_groups.get(to_id).into_iter().flatten())
        .map(|s| s.as_str())
        .collect();

    let (out_fx, out_fy) = port_outward(from_side);
    let (out_tx, out_ty) = port_outward(to_side);
    let s1 = Point::new(sx + out_fx * PORT_CLEARANCE, sy + out_fy * PORT_CLEARANCE);
    let e1 = Point::new(ex + out_tx * PORT_CLEARANCE, ey + out_ty * PORT_CLEARANCE);

    let axis = if from_vertical { Axis::Vertical } else { Axis::Horizontal };

    let mut fold_coords = collect_obstacle_boundaries_on_axis(
        axis.other(), nodes, groups, &endpoint_groups, from_id, to_id,
        NODE_OBSTACLE_PAD, GROUP_OBSTACLE_PAD, margin,
    );
    let mut channel_coords = collect_obstacle_boundaries_on_axis(
        axis, nodes, groups, &endpoint_groups, from_id, to_id,
        NODE_OBSTACLE_PAD, GROUP_OBSTACLE_PAD, margin,
    );

    fold_coords.extend(group_gap_midpoints_on_axis(groups, axis.other()));
    channel_coords.extend(group_gap_midpoints_on_axis(groups, axis));

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
                    candidates.push(simplify_path_preserving_stubs(vec![
                        Point::new(sx, sy), s1, p1, p2, p3, e1, Point::new(ex, ey),
                    ]));
                }
            }
        }
        FoldOrder::HorizontalFirst => {
            for &channel in &channel_coords {
                for &fold in &fold_coords {
                    let p1 = axis.point(s_main, channel);
                    let p2 = axis.point(fold, channel);
                    let p3 = axis.point(fold, e_cross);
                    candidates.push(simplify_path_preserving_stubs(vec![
                        Point::new(sx, sy), s1, p1, p2, p3, e1, Point::new(ex, ey),
                    ]));
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
    ctx: &RoutingContext,
    endpoint_groups: &HashSet<&str>,
    margins: &[f64],
    base_margin: f64,
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

    let mut node_ids: Vec<&String> = nodes.keys().collect();
    node_ids.sort();
    for nid in &node_ids {
        if nid.as_str() == from_id || nid.as_str() == to_id {
            continue;
        }
        let r = Rect::from(&nodes[*nid]);
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

    let mut group_ids: Vec<&String> = groups.keys().collect();
    group_ids.sort();
    for gid in &group_ids {
        if endpoint_groups.contains(gid.as_str()) {
            continue;
        }
        if let Some(gl) = groups.get(*gid) {
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

    for &margin in margins {
        channel_coords.push(channel_coord_on_axis(
            max_cross, true, groups, &ctx.group_ctx.corridors,
            axis, margin, band_lo, band_hi,
        ));
        channel_coords.push(channel_coord_on_axis(
            min_cross, false, groups, &ctx.group_ctx.corridors,
            axis, margin, band_lo, band_hi,
        ));
    }

    all_bounds.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
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
        candidates.push(simplify_path_preserving_stubs(vec![
            Point::new(sx, sy),
            s1,
            p_mid1,
            p_mid2,
            e1,
            Point::new(ex, ey),
        ]));
    }
    candidates
}

/// Side-channel detour candidates: when start/end nodes span across intermediate nodes,
/// open a channel from the side of the node column to bypass them.
///
/// Only generated when start/end ports are co-axial (both vertical / both horizontal)
/// and there are actual obstacle nodes between them.
///
/// P0-1: 多档 channel_margin [18, 28, 40]，逐档尝试以提供更多绕行候选。
///
/// 改进：不仅在全球 min_left/max_right 处生成通道，还在每个障碍物边界处
/// 生成通道候选（含相邻障碍物之间的间隙），大幅增加找到干净路径的概率。
fn build_channel_detours(
    sx: f64,
    sy: f64,
    from_side: Port,
    ex: f64,
    ey: f64,
    to_side: Port,
    pair: &EndpointPair,
    ctx: &RoutingContext,
) -> Vec<Vec<Point>> {
    let from_id = pair.from_id();
    let to_id = pair.to_id();
    let base_margin = ctx.cfg.channel_margin;

    let endpoint_groups: HashSet<&str> = ctx
        .group_ctx
        .node_to_groups
        .get(from_id)
        .into_iter()
        .flatten()
        .chain(ctx.group_ctx.node_to_groups.get(to_id).into_iter().flatten())
        .map(|s| s.as_str())
        .collect();

    let from_vertical = is_vertical_port(from_side);
    let to_vertical = is_vertical_port(to_side);
    if from_vertical != to_vertical {
        return Vec::new();
    }

    let margins = [base_margin, 28.0, 40.0];
    let axis = if from_vertical { Axis::Vertical } else { Axis::Horizontal };

    build_channel_detours_on_axis(
        axis, sx, sy, from_side, ex, ey, to_side, pair, ctx,
        &endpoint_groups, &margins, base_margin,
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
        for g in groups.values() {
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
        for g in groups.values() {
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
        let mut variants = vec![simplify_path(l1)];
        if !from_vertical && (ey - sy).abs() > EPS {
            variants.push(simplify_path(vec![Point::new(sx, sy), Point::new(sx, ey), Point::new(ex, ey)]));
        }
        if from_vertical && (ex - sx).abs() > EPS {
            variants.push(simplify_path(vec![Point::new(sx, sy), Point::new(ex, sy), Point::new(ex, ey)]));
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
                    simplify_path(vec![Point::new(sx, sy), Point::new(sx, yj), Point::new(ex, yj), Point::new(ex, ey)])
                })
                .collect()
        } else {
            ratios
                .iter()
                .map(|r| {
                    let xj = sx + (ex - sx) * r;
                    simplify_path(vec![Point::new(sx, sy), Point::new(xj, sy), Point::new(xj, ey), Point::new(ex, ey)])
                })
                .collect()
        }
    } else {
        vec![simplify_path(same_side_path(sx, sy, from_side, ex, ey))]
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

pub fn append_routed_segments(
    segments: &mut Vec<RoutedSegment>,
    path: &[Point],
    edge_index: usize,
) {
    for window in path.windows(2) {
        segments.push(RoutedSegment {
            x1: window[0].x,
            y1: window[0].y,
            x2: window[1].x,
            y2: window[1].y,
            edge_index,
        });
    }
}
