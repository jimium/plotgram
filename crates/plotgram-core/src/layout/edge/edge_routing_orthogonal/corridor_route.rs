//! 跨组边走廊（corridor）三段式路由。
//!
//! 源节点 → 源组边框 → 走廊车道 → 目标组边框 → 目标节点。
//! 非相邻组在走廊邻接图上 BFS 最短链，逐段串联。

use std::collections::{HashMap, HashSet, VecDeque};

use crate::ast::Relation;
use crate::layout::edge::edge_merge_policy::{
    edge_merge_context_with_groups, edges_may_share_trunk,
};
use crate::layout::geometry::Point;
use crate::layout::group::{CorridorAxis, GroupCorridor, GroupRoutingContext};
use crate::layout::{GroupLayout, Port};

use super::path::port_outward;
use super::profile::OrthoRoutingProfile;
use super::simplify::simplify_path;
use super::EPS;

/// 走廊内相邻车道间距（像素）
const CORRIDOR_LANE_PITCH: f64 = 18.0;
/// 端口 stub 默认长度
const DEFAULT_STUB_LEN: f64 = 24.0;

/// 跨组走廊路由计划
#[derive(Debug, Clone, Default)]
pub struct CorridorRoutePlan {
    /// edge_index → 走廊索引链（BFS 最短路径）
    pub chains: HashMap<usize, Vec<usize>>,
    /// (edge_index, corridor_index) → 车道序号
    pub lanes: HashMap<(usize, usize), usize>,
    /// corridor_index → 该走廊上的边数（用于车道居中）
    pub corridor_load: HashMap<usize, usize>,
}

/// 为跨组边规划走廊链与车道分配。
pub fn plan_corridor_routes(
    relations: &[Relation],
    group_ctx: &GroupRoutingContext,
    profile: &OrthoRoutingProfile,
) -> CorridorRoutePlan {
    if group_ctx.corridors.is_empty() {
        return CorridorRoutePlan::default();
    }

    let mut plan = CorridorRoutePlan::default();
    let mut corridor_edges: HashMap<usize, Vec<usize>> = HashMap::new();

    for (edge_index, rel) in relations.iter().enumerate() {
        let from_g = match group_ctx.node_leaf_group(rel.from.as_str()) {
            Some(g) => g,
            None => continue,
        };
        let to_g = match group_ctx.node_leaf_group(rel.to.as_str()) {
            Some(g) => g,
            None => continue,
        };
        if from_g == to_g {
            continue;
        }
        let Some(chain) = find_corridor_chain(from_g, to_g, &group_ctx.corridors) else {
            continue;
        };
        for &c_idx in &chain {
            corridor_edges.entry(c_idx).or_default().push(edge_index);
        }
        plan.chains.insert(edge_index, chain);
    }

    for (c_idx, mut edges) in corridor_edges {
        // 按 SuperEdgePair（leaf group 对）分组排序，使同组对的多边获得相邻 lane；
        // 组内再按 (from_id, to_id, edge_index) 确定性排序。
        edges.sort_by(|&a, &b| {
            let ra = &relations[a];
            let rb = &relations[b];
            let ga = super_edge_pair_key(group_ctx, ra.from.as_str(), ra.to.as_str());
            let gb = super_edge_pair_key(group_ctx, rb.from.as_str(), rb.to.as_str());
            ga.cmp(&gb)
                .then_with(|| ra.from.as_str().cmp(rb.from.as_str()))
                .then_with(|| ra.to.as_str().cmp(rb.to.as_str()))
                .then(a.cmp(&b))
        });
        edges.dedup();
        let (lane_map, lane_count) =
            assign_merge_aware_lanes(&edges, relations, group_ctx, profile);
        plan.corridor_load.insert(c_idx, lane_count);
        for (edge_index, lane) in lane_map {
            plan.lanes.insert((edge_index, c_idx), lane);
        }
    }

    plan
}

/// 按 merge policy 分配车道：仅 `edges_may_share_trunk` 为 true 的边可共用同一 lane。
fn assign_merge_aware_lanes(
    edges: &[usize],
    relations: &[Relation],
    group_ctx: &GroupRoutingContext,
    profile: &OrthoRoutingProfile,
) -> (HashMap<usize, usize>, usize) {
    if !profile.semantic_merge {
        let mut lanes = HashMap::new();
        for (lane, &edge_index) in edges.iter().enumerate() {
            lanes.insert(edge_index, lane);
        }
        return (lanes, edges.len());
    }

    let mut lanes: HashMap<usize, usize> = HashMap::new();
    let mut lane_occupants: Vec<Vec<usize>> = Vec::new();

    for &edge_index in edges {
        let rel = &relations[edge_index];
        let ctx = edge_merge_context_with_groups(
            rel.from.as_str(),
            rel.to.as_str(),
            edge_index,
            group_ctx.node_leaf_group(rel.from.as_str()),
            group_ctx.node_leaf_group(rel.to.as_str()),
        );

        let mut lane = 0usize;
        loop {
            let can_use = lane_occupants.get(lane).map_or(true, |occupants| {
                occupants.iter().all(|&other| {
                    let other_rel = &relations[other];
                    let other_ctx = edge_merge_context_with_groups(
                        other_rel.from.as_str(),
                        other_rel.to.as_str(),
                        other,
                        group_ctx.node_leaf_group(other_rel.from.as_str()),
                        group_ctx.node_leaf_group(other_rel.to.as_str()),
                    );
                    edges_may_share_trunk(&ctx, &other_ctx, profile.merge_policy_diagram_type())
                })
            });
            if can_use {
                break;
            }
            lane += 1;
        }

        if lane_occupants.len() <= lane {
            lane_occupants.resize(lane + 1, Vec::new());
        }
        lane_occupants[lane].push(edge_index);
        lanes.insert(edge_index, lane);
    }

    (lanes, lane_occupants.len())
}

/// 提取边的 SuperEdgePair 键（规范化的 leaf group 对）。
///
/// 同一对 leaf group 的多条边会产生相同的键，用于 corridor lane 排序时分组相邻。
/// 返回 `None` 的情况：节点不在任何 leaf group 中、或两端同属一个 leaf group。
fn super_edge_pair_key(
    group_ctx: &GroupRoutingContext,
    from_id: &str,
    to_id: &str,
) -> Option<(String, String)> {
    let from_g = group_ctx.node_leaf_group(from_id)?;
    let to_g = group_ctx.node_leaf_group(to_id)?;
    if from_g == to_g {
        return None;
    }
    let (a, b) = crate::layout::edge::common::edge_geometry::canonical_pair(from_g, to_g);
    Some((a.to_string(), b.to_string()))
}

/// 尝试为跨组边构建走廊路径；失败时返回 `None` 由通用路由兜底。
///
/// 多跳链：中间组只外绕、不入组内部（P2 走廊硬契约）。
pub fn try_build_corridor_path(
    edge_index: usize,
    from_anchor: Point,
    to_anchor: Point,
    from_id: &str,
    to_id: &str,
    plan: &CorridorRoutePlan,
    group_ctx: &GroupRoutingContext,
    stub_len: f64,
    // 冻结后贴廊专用：廊带外绕 / 实边出口 / 入框避组。主链必须 false，避免 node_fp 漂移。
    outer_bypass: bool,
) -> Option<Vec<Point>> {
    let chain = plan.chains.get(&edge_index)?;
    if chain.is_empty() {
        return None;
    }

    let from_group_id = group_ctx.node_leaf_group(from_id)?;
    let stub = stub_len.max(DEFAULT_STUB_LEN * 0.5);
    let skirt_pad = group_ctx.border_shell_pad.max(12.0);

    let mut waypoints = vec![from_anchor];
    let mut current = from_anchor;
    let mut current_group = from_group_id;

    for (step, &c_idx) in chain.iter().enumerate() {
        let corridor = &group_ctx.corridors[c_idx];
        let lane = plan.lanes.get(&(edge_index, c_idx)).copied().unwrap_or(0);
        let lane_count = plan.corridor_load.get(&c_idx).copied().unwrap_or(1);
        let lane_coord = corridor_lane_coord(corridor, lane, lane_count);
        let cross_offset = corridor_cross_axis_offset(lane, lane_count);
        let is_first = step == 0;
        let is_last = step + 1 == chain.len();

        let next_group = if corridor.group_a == current_group {
            corridor.group_b.as_str()
        } else if corridor.group_b == current_group {
            corridor.group_a.as_str()
        } else {
            return None;
        };

        let current_gl = group_ctx.groups.get(current_group)?;
        let next_gl = group_ctx.groups.get(next_group)?;
        let (exit_side, entry_side) = corridor_sides(corridor, current_group, next_group)?;

        if is_first {
            if outer_bypass {
                // 出口停在组框实边上，禁止 corridor_point 把锚点拽到可能被埋的廊心。
                let exit_border = group_side_border_point(current_gl, exit_side, current);
                match corridor.axis {
                    CorridorAxis::Horizontal if matches!(exit_side, Port::Top | Port::Bottom) => {
                        let mut p = exit_border;
                        p.x += cross_offset;
                        append_stub_leg(&mut waypoints, &mut current, p, exit_side, stub);
                    }
                    CorridorAxis::Vertical if matches!(exit_side, Port::Left | Port::Right) => {
                        let mut p = exit_border;
                        p.y += cross_offset;
                        append_stub_leg(&mut waypoints, &mut current, p, exit_side, stub);
                    }
                    _ => {
                        append_stub_leg(&mut waypoints, &mut current, exit_border, exit_side, stub);
                    }
                }
            } else {
                let exit_border = border_point_on_side(
                    current_gl,
                    exit_side,
                    current,
                    corridor,
                    lane_coord,
                    cross_offset,
                );
                append_stub_leg(&mut waypoints, &mut current, exit_border, exit_side, stub);
            }
        } else {
            let join = corridor_point(corridor, lane_coord, current);
            ortho_connect(&mut waypoints, &mut current, join);
        }

        if is_last {
            let travel = Point::new(
                if corridor.axis == CorridorAxis::Vertical {
                    lane_coord
                } else {
                    to_anchor.x + cross_offset
                },
                if corridor.axis == CorridorAxis::Horizontal {
                    lane_coord
                } else {
                    to_anchor.y + cross_offset
                },
            );
            let corridor_entry = corridor_point(corridor, lane_coord, travel);
            let entry_border = border_point_on_side(
                next_gl,
                entry_side,
                corridor_entry,
                corridor,
                lane_coord,
                cross_offset,
            );
            corridor_travel_skirting_foreign(
                &mut waypoints,
                &mut current,
                corridor_entry,
                corridor,
                lane_coord,
                from_id,
                to_id,
                group_ctx,
                skirt_pad,
                outer_bypass,
            );
            if outer_bypass {
                let entry_ok = !foreign_point_in_group(entry_border, from_id, to_id, group_ctx);
                if entry_ok {
                    connect_skirting_foreign(
                        &mut waypoints,
                        &mut current,
                        entry_border,
                        from_id,
                        to_id,
                        group_ctx,
                        skirt_pad,
                    );
                }
            } else {
                ortho_connect(&mut waypoints, &mut current, entry_border);
            }
        } else {
            let approach = corridor_point(
                corridor,
                lane_coord,
                Point::new(
                    next_gl.x + next_gl.width * 0.5,
                    next_gl.y + next_gl.height * 0.5,
                ),
            );
            corridor_travel_skirting_foreign(
                &mut waypoints,
                &mut current,
                approach,
                corridor,
                lane_coord,
                from_id,
                to_id,
                group_ctx,
                skirt_pad,
                outer_bypass,
            );

            let next_c_idx = chain[step + 1];
            let next_corridor = group_ctx.corridors.get(next_c_idx)?;
            let next_lane = plan
                .lanes
                .get(&(edge_index, next_c_idx))
                .copied()
                .unwrap_or(0);
            let next_lane_count = plan.corridor_load.get(&next_c_idx).copied().unwrap_or(1);
            let next_lane_coord = corridor_lane_coord(next_corridor, next_lane, next_lane_count);
            skirt_around_group(
                &mut waypoints,
                &mut current,
                next_gl,
                next_group,
                next_corridor,
                next_lane_coord,
                skirt_pad,
                group_ctx,
            );
        }

        current_group = next_group;
    }

    if outer_bypass {
        connect_skirting_foreign(
            &mut waypoints,
            &mut current,
            to_anchor,
            from_id,
            to_id,
            group_ctx,
            skirt_pad,
        );
    } else {
        let final_side = infer_port_at_point(current, to_anchor);
        append_stub_leg(&mut waypoints, &mut current, to_anchor, final_side, stub);
        waypoints.push(to_anchor);
    }

    let path = simplify_path(waypoints, true);
    (path.len() >= 2).then_some(path)
}

/// 沿走廊行驶：若廊心带会穿第三方组，则在廊带外侧绕过；廊心整段被埋时不落回廊心。
///
/// 关键：禁止先竖直落到廊心再检测——廊心 Y 落在第三方组内部时，落廊段本身已穿组
/// （tenant `b_worker→object_store` 廊 y=755 ⊂ tenant_a y∈[650,870]）。
fn corridor_travel_skirting_foreign(
    waypoints: &mut Vec<Point>,
    current: &mut Point,
    target: Point,
    corridor: &GroupCorridor,
    lane_coord: f64,
    from_id: &str,
    to_id: &str,
    group_ctx: &GroupRoutingContext,
    pad: f64,
    outer_bypass: bool,
) {
    let dest = corridor_point(corridor, lane_coord, target);

    if !outer_bypass {
        // 主链保守：落廊后若直行穿第三方组则逐个外绕回廊（贴廊前历史行为）。
        let on_corridor = corridor_point(corridor, lane_coord, *current);
        if (on_corridor.x - current.x).abs() > EPS || (on_corridor.y - current.y).abs() > EPS {
            ortho_connect(waypoints, current, on_corridor);
        }
        let endpoint_groups = group_ctx.endpoint_group_set(from_id, to_id);
        let mut blockers: Vec<(&str, &GroupLayout)> = Vec::new();
        for (gid, gl) in &group_ctx.groups {
            if endpoint_groups.contains(gid.as_str()) || gl.width <= 0.0 || gl.height <= 0.0 {
                continue;
            }
            if crate::layout::edge::common::geom_obstacle::segment_pierces_group_interior(
                *current, dest, gl,
            ) {
                blockers.push((gid.as_str(), gl));
            }
        }
        match corridor.axis {
            CorridorAxis::Horizontal => {
                let forward = dest.x >= current.x;
                blockers.sort_by(|a, b| {
                    let ka = a.1.x + a.1.width * 0.5;
                    let kb = b.1.x + b.1.width * 0.5;
                    if forward {
                        ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal)
                    } else {
                        kb.partial_cmp(&ka).unwrap_or(std::cmp::Ordering::Equal)
                    }
                });
            }
            CorridorAxis::Vertical => {
                let forward = dest.y >= current.y;
                blockers.sort_by(|a, b| {
                    let ka = a.1.y + a.1.height * 0.5;
                    let kb = b.1.y + b.1.height * 0.5;
                    if forward {
                        ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal)
                    } else {
                        kb.partial_cmp(&ka).unwrap_or(std::cmp::Ordering::Equal)
                    }
                });
            }
        }
        for (gid, gl) in blockers {
            if !crate::layout::edge::common::geom_obstacle::segment_pierces_group_interior(
                *current, dest, gl,
            ) {
                continue;
            }
            conservative_skirt_blocker_back_to_corridor(
                waypoints, current, gl, gid, corridor, lane_coord, dest, pad, group_ctx,
            );
        }
        ortho_connect(waypoints, current, dest);
        return;
    }

    let endpoint_groups = group_ctx.endpoint_group_set(from_id, to_id);

    let mut blockers: Vec<(&str, &GroupLayout)> = Vec::new();
    for (gid, gl) in &group_ctx.groups {
        if endpoint_groups.contains(gid.as_str()) {
            continue;
        }
        if gl.width <= 0.0 || gl.height <= 0.0 {
            continue;
        }
        if lane_segment_threatens_group(*current, dest, corridor, lane_coord, gl) {
            blockers.push((gid.as_str(), gl));
        }
    }

    match corridor.axis {
        CorridorAxis::Horizontal => {
            let forward = dest.x >= current.x;
            blockers.sort_by(|a, b| {
                let ka = a.1.x + a.1.width * 0.5;
                let kb = b.1.x + b.1.width * 0.5;
                if forward {
                    ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal)
                } else {
                    kb.partial_cmp(&ka).unwrap_or(std::cmp::Ordering::Equal)
                }
            });
        }
        CorridorAxis::Vertical => {
            let forward = dest.y >= current.y;
            blockers.sort_by(|a, b| {
                let ka = a.1.y + a.1.height * 0.5;
                let kb = b.1.y + b.1.height * 0.5;
                if forward {
                    ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal)
                } else {
                    kb.partial_cmp(&ka).unwrap_or(std::cmp::Ordering::Equal)
                }
            });
        }
    }

    if blockers.is_empty() {
        for (gid, gl) in &group_ctx.groups {
            if endpoint_groups.contains(gid.as_str()) || gl.width <= 0.0 || gl.height <= 0.0 {
                continue;
            }
            if point_in_group_interior(dest, gl)
                || point_in_group_interior(corridor_point(corridor, lane_coord, *current), gl)
            {
                blockers.push((gid.as_str(), gl));
            }
        }
        match corridor.axis {
            CorridorAxis::Horizontal => {
                let forward = dest.x >= current.x;
                blockers.sort_by(|a, b| {
                    let ka = a.1.x + a.1.width * 0.5;
                    let kb = b.1.x + b.1.width * 0.5;
                    if forward {
                        ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal)
                    } else {
                        kb.partial_cmp(&ka).unwrap_or(std::cmp::Ordering::Equal)
                    }
                });
            }
            CorridorAxis::Vertical => {
                let forward = dest.y >= current.y;
                blockers.sort_by(|a, b| {
                    let ka = a.1.y + a.1.height * 0.5;
                    let kb = b.1.y + b.1.height * 0.5;
                    if forward {
                        ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal)
                    } else {
                        kb.partial_cmp(&ka).unwrap_or(std::cmp::Ordering::Equal)
                    }
                });
            }
        }
    }

    if blockers.is_empty() {
        connect_skirting_foreign(
            waypoints,
            current,
            dest,
            from_id,
            to_id,
            group_ctx,
            pad,
        );
        return;
    }

    bypass_corridor_on_outer_side(
        waypoints,
        current,
        dest,
        corridor,
        lane_coord,
        &blockers,
        pad,
        group_ctx,
    );
}

/// 主链保守外绕：允许在 current.x 上先竖移（历史行为；埋廊场景留给 outer_bypass）。
fn conservative_skirt_blocker_back_to_corridor(
    waypoints: &mut Vec<Point>,
    current: &mut Point,
    blocker: &GroupLayout,
    blocker_id: &str,
    corridor: &GroupCorridor,
    lane_coord: f64,
    dest: Point,
    pad: f64,
    group_ctx: &GroupRoutingContext,
) {
    let left = blocker.x - pad;
    let right = blocker.x + blocker.width + pad;
    let top = blocker.y - pad;
    let bottom = blocker.y + blocker.height + pad;

    match corridor.axis {
        CorridorAxis::Horizontal => {
            let forward = dest.x >= current.x;
            let ahead_x = if forward { right } else { left };
            let top_clear =
                skirt_horizontal_clear(top, current.x, ahead_x, blocker_id, group_ctx);
            let bottom_clear =
                skirt_horizontal_clear(bottom, current.x, ahead_x, blocker_id, group_ctx);
            let top_cost = (current.y - top).abs() + (lane_coord - top).abs();
            let bottom_cost = (current.y - bottom).abs() + (lane_coord - bottom).abs();
            let side_y = match (top_clear, bottom_clear) {
                (true, false) => top,
                (false, true) => bottom,
                _ if top_cost <= bottom_cost => top,
                _ => bottom,
            };
            let rejoin = corridor_point(corridor, lane_coord, Point::new(ahead_x, lane_coord));
            ortho_connect(waypoints, current, Point::new(current.x, side_y));
            ortho_connect(waypoints, current, Point::new(rejoin.x, side_y));
            ortho_connect(waypoints, current, rejoin);
        }
        CorridorAxis::Vertical => {
            let forward = dest.y >= current.y;
            let ahead_y = if forward { bottom } else { top };
            let left_clear =
                skirt_vertical_clear(left, current.y, ahead_y, blocker_id, group_ctx);
            let right_clear =
                skirt_vertical_clear(right, current.y, ahead_y, blocker_id, group_ctx);
            let left_cost = (current.x - left).abs() + (lane_coord - left).abs();
            let right_cost = (current.x - right).abs() + (lane_coord - right).abs();
            let side_x = match (left_clear, right_clear) {
                (true, false) => left,
                (false, true) => right,
                _ if left_cost <= right_cost => left,
                _ => right,
            };
            let rejoin = corridor_point(corridor, lane_coord, Point::new(lane_coord, ahead_y));
            ortho_connect(waypoints, current, Point::new(side_x, current.y));
            ortho_connect(waypoints, current, Point::new(side_x, rejoin.y));
            ortho_connect(waypoints, current, rejoin);
        }
    }
}

/// 廊心投影段、落廊竖/横段是否威胁某第三方组。
fn lane_segment_threatens_group(
    from: Point,
    dest: Point,
    corridor: &GroupCorridor,
    lane_coord: f64,
    gl: &GroupLayout,
) -> bool {
    let projected = corridor_point(corridor, lane_coord, from);
    crate::layout::edge::common::geom_obstacle::segment_pierces_group_interior(from, projected, gl)
        || crate::layout::edge::common::geom_obstacle::segment_pierces_group_interior(
            projected, dest, gl,
        )
        || crate::layout::edge::common::geom_obstacle::segment_pierces_group_interior(from, dest, gl)
        || point_in_group_interior(projected, gl)
        || point_in_group_interior(dest, gl)
}

fn point_in_group_interior(p: Point, gl: &GroupLayout) -> bool {
    p.x > gl.x + EPS
        && p.x < gl.x + gl.width - EPS
        && p.y > gl.y + EPS
        && p.y < gl.y + gl.height - EPS
}

fn foreign_point_in_group(
    p: Point,
    from_id: &str,
    to_id: &str,
    group_ctx: &GroupRoutingContext,
) -> bool {
    let endpoint_groups = group_ctx.endpoint_group_set(from_id, to_id);
    for (gid, gl) in &group_ctx.groups {
        if endpoint_groups.contains(gid.as_str()) || gl.width <= 0.0 || gl.height <= 0.0 {
            continue;
        }
        if point_in_group_interior(p, gl) {
            return true;
        }
    }
    false
}

/// 在所有阻挡组的同一外侧（上/下或左/右）旁路前进到 dest 的轴向投影外侧点。
fn bypass_corridor_on_outer_side(
    waypoints: &mut Vec<Point>,
    current: &mut Point,
    dest: Point,
    corridor: &GroupCorridor,
    _lane_coord: f64,
    blockers: &[(&str, &GroupLayout)],
    pad: f64,
    group_ctx: &GroupRoutingContext,
) {
    if blockers.is_empty() {
        ortho_connect(waypoints, current, dest);
        return;
    }

    match corridor.axis {
        CorridorAxis::Horizontal => {
            let forward = dest.x >= current.x;
            let mut union_left = f64::INFINITY;
            let mut union_right = f64::NEG_INFINITY;
            let mut union_top = f64::INFINITY;
            let mut union_bottom = f64::NEG_INFINITY;
            for (_, gl) in blockers {
                union_left = union_left.min(gl.x - pad);
                union_right = union_right.max(gl.x + gl.width + pad);
                union_top = union_top.min(gl.y - pad);
                union_bottom = union_bottom.max(gl.y + gl.height + pad);
            }
            let ahead_x = if forward { union_right } else { union_left };
            let top_clear = blockers.iter().all(|(gid, _)| {
                skirt_horizontal_clear(union_top, current.x, ahead_x, gid, group_ctx)
            });
            let bottom_clear = blockers.iter().all(|(gid, _)| {
                skirt_horizontal_clear(union_bottom, current.x, ahead_x, gid, group_ctx)
            });
            // 选侧：优先目标侧 / 当前外侧，缩短外绕（降 tight_sev）。
            let top_cost = (current.y - union_top).abs() + (dest.y - union_top).abs();
            let bottom_cost = (current.y - union_bottom).abs() + (dest.y - union_bottom).abs();
            let side_y = match (top_clear, bottom_clear) {
                (true, false) => union_top,
                (false, true) => union_bottom,
                _ if current.y <= union_top + EPS => union_top,
                _ if current.y >= union_bottom - EPS => union_bottom,
                _ if top_cost <= bottom_cost => union_top,
                _ => union_bottom,
            };

            let x_inside = current.x > union_left + EPS && current.x < union_right - EPS;
            let y_outside = current.y <= union_top + EPS || current.y >= union_bottom - EPS;

            // 硬规则：已在并集上/下方时，先水平走到 ahead；保持当前 Y（勿再抬到 side_y）。
            if y_outside {
                ortho_connect(waypoints, current, Point::new(ahead_x, current.y));
            } else if x_inside {
                let exit_x = if (current.x - union_left) <= (union_right - current.x) {
                    union_left
                } else {
                    union_right
                };
                ortho_connect(waypoints, current, Point::new(exit_x, side_y));
                ortho_connect(waypoints, current, Point::new(ahead_x, side_y));
            } else {
                ortho_connect(waypoints, current, Point::new(current.x, side_y));
                ortho_connect(waypoints, current, Point::new(ahead_x, side_y));
            }

            // dest 若仍在并集内部，停在 ahead 外侧，不落回被埋廊心。
            if point_in_group_interior(dest, blockers[0].1)
                || blockers
                    .iter()
                    .any(|(_, gl)| point_in_group_interior(dest, gl))
            {
                // 保持在 (ahead_x, side_y)，由后续入框 connect 收束。
                return;
            }
            // dest 已清：从外侧落到 dest（竖段在 ahead_x，X 在并集外）。
            ortho_connect(waypoints, current, Point::new(ahead_x, dest.y));
            ortho_connect(waypoints, current, dest);
        }
        CorridorAxis::Vertical => {
            let forward = dest.y >= current.y;
            let mut union_left = f64::INFINITY;
            let mut union_right = f64::NEG_INFINITY;
            let mut union_top = f64::INFINITY;
            let mut union_bottom = f64::NEG_INFINITY;
            for (_, gl) in blockers {
                union_left = union_left.min(gl.x - pad);
                union_right = union_right.max(gl.x + gl.width + pad);
                union_top = union_top.min(gl.y - pad);
                union_bottom = union_bottom.max(gl.y + gl.height + pad);
            }
            let ahead_y = if forward { union_bottom } else { union_top };
            let left_clear = blockers.iter().all(|(gid, _)| {
                skirt_vertical_clear(union_left, current.y, ahead_y, gid, group_ctx)
            });
            let right_clear = blockers.iter().all(|(gid, _)| {
                skirt_vertical_clear(union_right, current.y, ahead_y, gid, group_ctx)
            });
            let left_cost = (current.x - union_left).abs() + (dest.x - union_left).abs();
            let right_cost = (current.x - union_right).abs() + (dest.x - union_right).abs();
            let side_x = match (left_clear, right_clear) {
                (true, false) => union_left,
                (false, true) => union_right,
                _ if current.x <= union_left + EPS => union_left,
                _ if current.x >= union_right - EPS => union_right,
                _ if left_cost <= right_cost => union_left,
                _ => union_right,
            };

            let y_inside = current.y > union_top + EPS && current.y < union_bottom - EPS;
            let x_outside = current.x <= union_left + EPS || current.x >= union_right - EPS;
            if x_outside {
                ortho_connect(waypoints, current, Point::new(current.x, ahead_y));
            } else if y_inside {
                let exit_y = if (current.y - union_top) <= (union_bottom - current.y) {
                    union_top
                } else {
                    union_bottom
                };
                ortho_connect(waypoints, current, Point::new(side_x, current.y));
                ortho_connect(waypoints, current, Point::new(side_x, exit_y));
                ortho_connect(waypoints, current, Point::new(side_x, ahead_y));
            } else {
                ortho_connect(waypoints, current, Point::new(side_x, current.y));
                ortho_connect(waypoints, current, Point::new(side_x, ahead_y));
            }

            if blockers
                .iter()
                .any(|(_, gl)| point_in_group_interior(dest, gl))
            {
                return;
            }
            ortho_connect(waypoints, current, Point::new(dest.x, ahead_y));
            ortho_connect(waypoints, current, dest);
        }
    }
}

/// 正交接到目标；若 L/直达会穿第三方组则 U 形外绕。
fn connect_skirting_foreign(
    waypoints: &mut Vec<Point>,
    current: &mut Point,
    target: Point,
    from_id: &str,
    to_id: &str,
    group_ctx: &GroupRoutingContext,
    pad: f64,
) {
    let endpoint_groups = group_ctx.endpoint_group_set(from_id, to_id);
    for _ in 0..8 {
        let Some((gid, gl)) =
            first_foreign_blocker_on_ortho(*current, target, &endpoint_groups, group_ctx)
        else {
            ortho_connect(waypoints, current, target);
            return;
        };
        u_skirt_around_group(waypoints, current, gl, gid, target, pad, group_ctx);
        if (current.x - target.x).abs() <= EPS && (current.y - target.y).abs() <= EPS {
            return;
        }
    }
    ortho_connect(waypoints, current, target);
}

fn first_foreign_blocker_on_ortho<'a>(
    from: Point,
    to: Point,
    endpoint_groups: &HashSet<&str>,
    group_ctx: &'a GroupRoutingContext,
) -> Option<(&'a str, &'a GroupLayout)> {
    let bend = Point::new(to.x, from.y);
    let mut best: Option<(&str, &GroupLayout, f64)> = None;
    for (gid, gl) in &group_ctx.groups {
        if endpoint_groups.contains(gid.as_str()) || gl.width <= 0.0 || gl.height <= 0.0 {
            continue;
        }
        let hit = crate::layout::edge::common::geom_obstacle::segment_pierces_group_interior(
            from, to, gl,
        ) || crate::layout::edge::common::geom_obstacle::segment_pierces_group_interior(
            from, bend, gl,
        ) || crate::layout::edge::common::geom_obstacle::segment_pierces_group_interior(
            bend, to, gl,
        );
        if !hit {
            continue;
        }
        let cx = gl.x + gl.width * 0.5;
        let cy = gl.y + gl.height * 0.5;
        let dist = (from.x - cx).abs() + (from.y - cy).abs();
        if best.as_ref().is_none_or(|(_, _, d)| dist < *d) {
            best = Some((gid.as_str(), gl, dist));
        }
    }
    best.map(|(g, gl, _)| (g, gl))
}

/// 从当前点 U 形绕过组外侧，朝 target 方向推进到组外近侧。
///
/// 硬规则：X 与组重叠时禁止竖移；Y 与组重叠时禁止横移（避免切穿）。
fn u_skirt_around_group(
    waypoints: &mut Vec<Point>,
    current: &mut Point,
    blocker: &GroupLayout,
    blocker_id: &str,
    target: Point,
    pad: f64,
    group_ctx: &GroupRoutingContext,
) {
    let left = blocker.x - pad;
    let right = blocker.x + blocker.width + pad;
    let top = blocker.y - pad;
    let bottom = blocker.y + blocker.height + pad;

    let x_overlap = current.x > left + EPS && current.x < right - EPS;
    let y_overlap = current.y > top + EPS && current.y < bottom - EPS;

    let ahead_x = if target.x >= current.x { right } else { left };
    let ahead_y = if target.y >= current.y { bottom } else { top };

    let top_clear = skirt_horizontal_clear(top, current.x, ahead_x, blocker_id, group_ctx);
    let bottom_clear =
        skirt_horizontal_clear(bottom, current.x, ahead_x, blocker_id, group_ctx);
    let top_cost = (current.y - top).abs() + (target.y - top).abs();
    let bottom_cost = (current.y - bottom).abs() + (target.y - bottom).abs();
    let side_y = match (top_clear, bottom_clear) {
        (true, false) => top,
        (false, true) => bottom,
        _ if top_cost <= bottom_cost => top,
        _ => bottom,
    };

    let left_clear = skirt_vertical_clear(left, current.y, ahead_y, blocker_id, group_ctx);
    let right_clear = skirt_vertical_clear(right, current.y, ahead_y, blocker_id, group_ctx);
    let left_cost = (current.x - left).abs() + (target.x - left).abs();
    let right_cost = (current.x - right).abs() + (target.x - right).abs();
    let side_x = match (left_clear, right_clear) {
        (true, false) => left,
        (false, true) => right,
        _ if left_cost <= right_cost => left,
        _ => right,
    };

    if x_overlap && !y_overlap {
        // 上/下方：先水平出并集，再视需要抬到 side_y。
        ortho_connect(waypoints, current, Point::new(ahead_x, current.y));
        if (target.y - current.y).abs() > EPS {
            let y = if (side_y - target.y).abs() <= (current.y - target.y).abs() {
                side_y
            } else {
                current.y
            };
            ortho_connect(waypoints, current, Point::new(ahead_x, y));
        }
    } else if y_overlap && !x_overlap {
        ortho_connect(waypoints, current, Point::new(current.x, ahead_y));
        if (target.x - current.x).abs() > EPS {
            let x = if (side_x - target.x).abs() <= (current.x - target.x).abs() {
                side_x
            } else {
                current.x
            };
            ortho_connect(waypoints, current, Point::new(x, ahead_y));
        }
    } else if x_overlap && y_overlap {
        // 内部：先到最近外侧角（仍可能有出框短段，但避免长切穿）。
        ortho_connect(waypoints, current, Point::new(side_x, side_y));
        ortho_connect(waypoints, current, Point::new(ahead_x, side_y));
    } else if (target.x - current.x).abs() >= (target.y - current.y).abs() {
        ortho_connect(waypoints, current, Point::new(current.x, side_y));
        ortho_connect(waypoints, current, Point::new(ahead_x, side_y));
    } else {
        ortho_connect(waypoints, current, Point::new(side_x, current.y));
        ortho_connect(waypoints, current, Point::new(side_x, ahead_y));
    }
}

/// 在中间组外侧绕行落到下一段走廊；选侧时避开其它分组内部。
fn skirt_around_group(
    waypoints: &mut Vec<Point>,
    current: &mut Point,
    intermediate: &GroupLayout,
    intermediate_id: &str,
    next_corridor: &GroupCorridor,
    next_lane_coord: f64,
    pad: f64,
    group_ctx: &GroupRoutingContext,
) {
    let left = intermediate.x - pad;
    let right = intermediate.x + intermediate.width + pad;
    let top = intermediate.y - pad;
    let bottom = intermediate.y + intermediate.height + pad;

    let target = match next_corridor.axis {
        CorridorAxis::Horizontal => {
            let x = current
                .x
                .clamp(next_corridor.span_min, next_corridor.span_max);
            corridor_point(
                next_corridor,
                next_lane_coord,
                Point::new(x, next_corridor.coord),
            )
        }
        CorridorAxis::Vertical => {
            let y = current
                .y
                .clamp(next_corridor.span_min, next_corridor.span_max);
            corridor_point(
                next_corridor,
                next_lane_coord,
                Point::new(next_corridor.coord, y),
            )
        }
    };

    match next_corridor.axis {
        CorridorAxis::Horizontal => {
            let left_cost = (current.x - left).abs() + (target.x - left).abs();
            let right_cost = (current.x - right).abs() + (target.x - right).abs();
            let left_clear =
                skirt_vertical_clear(left, current.y, target.y, intermediate_id, group_ctx);
            let right_clear =
                skirt_vertical_clear(right, current.y, target.y, intermediate_id, group_ctx);
            let side_x = match (left_clear, right_clear) {
                (true, false) => left,
                (false, true) => right,
                _ if left_cost <= right_cost => left,
                _ => right,
            };
            ortho_connect(waypoints, current, Point::new(side_x, current.y));
            ortho_connect(waypoints, current, Point::new(side_x, target.y));
            ortho_connect(waypoints, current, target);
        }
        CorridorAxis::Vertical => {
            let top_cost = (current.y - top).abs() + (target.y - top).abs();
            let bottom_cost = (current.y - bottom).abs() + (target.y - bottom).abs();
            let top_clear =
                skirt_horizontal_clear(top, current.x, target.x, intermediate_id, group_ctx);
            let bottom_clear =
                skirt_horizontal_clear(bottom, current.x, target.x, intermediate_id, group_ctx);
            let side_y = match (top_clear, bottom_clear) {
                (true, false) => top,
                (false, true) => bottom,
                _ if top_cost <= bottom_cost => top,
                _ => bottom,
            };
            ortho_connect(waypoints, current, Point::new(current.x, side_y));
            ortho_connect(waypoints, current, Point::new(target.x, side_y));
            ortho_connect(waypoints, current, target);
        }
    }
}

fn skirt_vertical_clear(
    x: f64,
    y0: f64,
    y1: f64,
    skip_group: &str,
    group_ctx: &GroupRoutingContext,
) -> bool {
    let ymin = y0.min(y1);
    let ymax = y0.max(y1);
    for (gid, gl) in &group_ctx.groups {
        if gid.as_str() == skip_group || gl.width <= 0.0 || gl.height <= 0.0 {
            continue;
        }
        if x <= gl.x + EPS || x >= gl.x + gl.width - EPS {
            continue;
        }
        if ymax <= gl.y + EPS || ymin >= gl.y + gl.height - EPS {
            continue;
        }
        return false;
    }
    true
}

fn skirt_horizontal_clear(
    y: f64,
    x0: f64,
    x1: f64,
    skip_group: &str,
    group_ctx: &GroupRoutingContext,
) -> bool {
    let xmin = x0.min(x1);
    let xmax = x0.max(x1);
    for (gid, gl) in &group_ctx.groups {
        if gid.as_str() == skip_group || gl.width <= 0.0 || gl.height <= 0.0 {
            continue;
        }
        if y <= gl.y + EPS || y >= gl.y + gl.height - EPS {
            continue;
        }
        if xmax <= gl.x + EPS || xmin >= gl.x + gl.width - EPS {
            continue;
        }
        return false;
    }
    true
}

fn find_corridor_chain(
    from_group: &str,
    to_group: &str,
    corridors: &[GroupCorridor],
) -> Option<Vec<usize>> {
    if from_group == to_group {
        return None;
    }

    let mut adj: HashMap<&str, Vec<(usize, &str)>> = HashMap::new();
    for (idx, c) in corridors.iter().enumerate() {
        adj.entry(c.group_a.as_str())
            .or_default()
            .push((idx, c.group_b.as_str()));
        adj.entry(c.group_b.as_str())
            .or_default()
            .push((idx, c.group_a.as_str()));
    }

    let mut visited: HashSet<&str> = HashSet::from([from_group]);
    let mut queue: VecDeque<(&str, Vec<usize>)> = VecDeque::from([(from_group, Vec::new())]);

    while let Some((current, chain)) = queue.pop_front() {
        if current == to_group {
            return Some(chain);
        }
        let mut neighbors: Vec<(usize, &str)> = adj.get(current).cloned().unwrap_or_default();
        neighbors.sort_by_key(|(idx, neighbor)| (*idx, *neighbor));
        for (c_idx, neighbor) in neighbors {
            if visited.insert(neighbor) {
                let mut next = chain.clone();
                next.push(c_idx);
                queue.push_back((neighbor, next));
            }
        }
    }
    None
}

fn corridor_sides(
    corridor: &GroupCorridor,
    from_group: &str,
    to_group: &str,
) -> Option<(Port, Port)> {
    let _ = to_group;
    match corridor.axis {
        CorridorAxis::Vertical => {
            if corridor.group_a == from_group {
                Some((Port::Right, Port::Left))
            } else {
                Some((Port::Left, Port::Right))
            }
        }
        CorridorAxis::Horizontal => {
            if corridor.group_a == from_group {
                Some((Port::Bottom, Port::Top))
            } else {
                Some((Port::Top, Port::Bottom))
            }
        }
    }
}

fn corridor_lane_coord(corridor: &GroupCorridor, lane: usize, lane_count: usize) -> f64 {
    let center = (lane_count.saturating_sub(1)) as f64 * 0.5;
    let offset = (lane as f64 - center) * CORRIDOR_LANE_PITCH;
    corridor.coord + offset
}

/// 垂直于走廊轴的 lane 偏移（水平走廊分离垂直汇入段 x，垂直走廊分离水平汇入段 y）。
pub(crate) fn corridor_cross_axis_offset(lane: usize, lane_count: usize) -> f64 {
    let center = (lane_count.saturating_sub(1)) as f64 * 0.5;
    (lane as f64 - center) * CORRIDOR_LANE_PITCH
}

fn corridor_point(corridor: &GroupCorridor, lane_coord: f64, reference: Point) -> Point {
    match corridor.axis {
        CorridorAxis::Vertical => Point::new(
            lane_coord,
            reference.y.clamp(corridor.span_min, corridor.span_max),
        ),
        CorridorAxis::Horizontal => Point::new(
            reference.x.clamp(corridor.span_min, corridor.span_max),
            lane_coord,
        ),
    }
}

/// 组框某侧上的点（不贴廊心）。
fn group_side_border_point(gl: &GroupLayout, side: Port, reference: Point) -> Point {
    match side {
        Port::Right => Point::new(gl.x + gl.width, reference.y.clamp(gl.y, gl.y + gl.height)),
        Port::Left => Point::new(gl.x, reference.y.clamp(gl.y, gl.y + gl.height)),
        Port::Bottom => Point::new(reference.x.clamp(gl.x, gl.x + gl.width), gl.y + gl.height),
        Port::Top => Point::new(reference.x.clamp(gl.x, gl.x + gl.width), gl.y),
    }
}

fn border_point_on_side(
    gl: &GroupLayout,
    side: Port,
    reference: Point,
    corridor: &GroupCorridor,
    lane_coord: f64,
    cross_offset: f64,
) -> Point {
    let mut point = group_side_border_point(gl, side, reference);
    match corridor.axis {
        CorridorAxis::Horizontal if matches!(side, Port::Top | Port::Bottom) => {
            point.x += cross_offset;
        }
        CorridorAxis::Vertical if matches!(side, Port::Left | Port::Right) => {
            point.y += cross_offset;
        }
        _ => {}
    }
    corridor_point(corridor, lane_coord, point)
}

/// 取边在走廊链上的 cross-axis 偏移（用于通用路由回退后的干线分离）。
pub(crate) fn planned_cross_axis_offset_for_edge(
    edge_index: usize,
    plan: &CorridorRoutePlan,
    group_ctx: &GroupRoutingContext,
) -> Option<(CorridorAxis, f64)> {
    let chain = plan.chains.get(&edge_index)?;
    let c_idx = *chain.first()?;
    let corridor = group_ctx.corridors.get(c_idx)?;
    let lane = plan.lanes.get(&(edge_index, c_idx)).copied().unwrap_or(0);
    let lane_count = plan.corridor_load.get(&c_idx).copied().unwrap_or(1);
    let offset = corridor_cross_axis_offset(lane, lane_count);
    if offset.abs() < EPS {
        return None;
    }
    Some((corridor.axis, offset))
}

fn append_stub_leg(
    waypoints: &mut Vec<Point>,
    current: &mut Point,
    target: Point,
    side: Port,
    stub_len: f64,
) {
    let (ox, oy) = port_outward(side);
    let stub = Point::new(current.x + ox * stub_len, current.y + oy * stub_len);
    if (stub.x - current.x).abs() > EPS || (stub.y - current.y).abs() > EPS {
        waypoints.push(stub);
        *current = stub;
    }
    ortho_connect(waypoints, current, target);
}

fn ortho_connect(waypoints: &mut Vec<Point>, current: &mut Point, target: Point) {
    if (current.x - target.x).abs() < EPS && (current.y - target.y).abs() < EPS {
        return;
    }
    if (current.x - target.x).abs() > EPS && (current.y - target.y).abs() > EPS {
        waypoints.push(Point::new(target.x, current.y));
        *current = Point::new(target.x, current.y);
    }
    if (current.x - target.x).abs() > EPS || (current.y - target.y).abs() > EPS {
        waypoints.push(target);
        *current = target;
    }
}

fn infer_port_at_point(from: Point, to: Point) -> Port {
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    if dx.abs() >= dy.abs() {
        if dx > 0.0 {
            Port::Right
        } else {
            Port::Left
        }
    } else if dy > 0.0 {
        Port::Bottom
    } else {
        Port::Top
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::GroupLayout;
    use crate::types::DiagramType;

    fn sample_corridor() -> GroupCorridor {
        GroupCorridor {
            axis: CorridorAxis::Vertical,
            coord: 120.0,
            span_min: 10.0,
            span_max: 200.0,
            group_a: "left".into(),
            group_b: "right".into(),
        }
    }

    #[test]
    fn finds_corridor_chain_between_adjacent_groups() {
        let corridors = vec![sample_corridor()];
        let chain = find_corridor_chain("left", "right", &corridors).unwrap();
        assert_eq!(chain, vec![0]);
    }

    #[test]
    fn assigns_lanes_deterministically() {
        let mut groups = HashMap::new();
        groups.insert(
            "left".into(),
            GroupLayout {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 80.0,
            },
        );
        groups.insert(
            "right".into(),
            GroupLayout {
                x: 140.0,
                y: 0.0,
                width: 100.0,
                height: 80.0,
            },
        );
        let ctx = GroupRoutingContext {
            groups,
            node_to_groups: HashMap::new(),
            border_shell_pad: 12.0,
            stub_clearance: 24.0,
            corridor_misalignment_penalty: 80.0,
            repulse_max_rounds: 2,
            corridors: vec![sample_corridor()],
            side_gutters: std::collections::BTreeMap::new(),
            node_leaf_group: HashMap::from([
                ("a".into(), "left".into()),
                ("b".into(), "right".into()),
            ]),
            sibling_sets: vec![],
            sibling_orientation: HashMap::new(),
            group_ancestors: HashMap::new(),
        };
        let relations = vec![Relation {
            from: crate::ast::Identifier::new_unchecked("a"),
            to: crate::ast::Identifier::new_unchecked("b"),
            arrow: crate::ast::ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: crate::ast::AttributeMap::default(),
            span: crate::ast::Span::dummy(),
        }];
        let plan = plan_corridor_routes(
            &relations,
            &ctx,
            &OrthoRoutingProfile::for_diagram_type(crate::types::DiagramType::Flowchart),
        );
        assert_eq!(plan.chains.get(&0).map(|c| c.as_slice()), Some(&[0][..]));
        assert_eq!(plan.lanes.get(&(0, 0)), Some(&0));
    }

    fn make_relation(from: &str, to: &str) -> Relation {
        Relation {
            from: crate::ast::Identifier::new_unchecked(from),
            to: crate::ast::Identifier::new_unchecked(to),
            arrow: crate::ast::ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: crate::ast::AttributeMap::default(),
            span: crate::ast::Span::dummy(),
        }
    }

    fn make_ctx(node_leaf_group: HashMap<String, String>) -> GroupRoutingContext {
        let mut groups = HashMap::new();
        groups.insert(
            "left".into(),
            GroupLayout {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 80.0,
            },
        );
        groups.insert(
            "right".into(),
            GroupLayout {
                x: 140.0,
                y: 0.0,
                width: 100.0,
                height: 80.0,
            },
        );
        GroupRoutingContext {
            groups,
            node_to_groups: HashMap::new(),
            border_shell_pad: 12.0,
            stub_clearance: 24.0,
            corridor_misalignment_penalty: 80.0,
            repulse_max_rounds: 2,
            corridors: vec![sample_corridor()],
            side_gutters: std::collections::BTreeMap::new(),
            node_leaf_group,
            sibling_sets: vec![],
            sibling_orientation: HashMap::new(),
            group_ancestors: HashMap::new(),
        }
    }

    #[test]
    fn super_edge_pair_edges_get_adjacent_lanes() {
        // 4 条边跨 left→right corridor：
        //   edge 0: a1→b1 (left→right)
        //   edge 1: a2→b2 (left→right)  — 与 edge 0 同 SuperEdgePair
        //   edge 2: a3→b3 (left→right)  — 与 edge 0 同 SuperEdgePair
        //   edge 3: a4→b4 (left→right)  — 与 edge 0 同 SuperEdgePair
        // 期望：4 条边都同属一个 SuperEdgePair，获得 lane 0,1,2,3（相邻）
        let ctx = make_ctx(HashMap::from([
            ("a1".into(), "left".into()),
            ("a2".into(), "left".into()),
            ("a3".into(), "left".into()),
            ("a4".into(), "left".into()),
            ("b1".into(), "right".into()),
            ("b2".into(), "right".into()),
            ("b3".into(), "right".into()),
            ("b4".into(), "right".into()),
        ]));
        let relations = vec![
            make_relation("a1", "b1"),
            make_relation("a2", "b2"),
            make_relation("a3", "b3"),
            make_relation("a4", "b4"),
        ];
        let plan = plan_corridor_routes(
            &relations,
            &ctx,
            &OrthoRoutingProfile::for_diagram_type(crate::types::DiagramType::Flowchart),
        );
        // 所有边应分配到 corridor 0 的 lane 0..3
        for edge_idx in 0..4 {
            assert!(
                plan.lanes.contains_key(&(edge_idx, 0)),
                "edge {} 应在 corridor 0 分到 lane",
                edge_idx
            );
        }
        // lane 值应为 0,1,2,3 的排列（相邻分配）
        let mut lanes: Vec<usize> = (0..4)
            .map(|i| plan.lanes.get(&(i, 0)).copied().unwrap_or(usize::MAX))
            .collect();
        lanes.sort();
        assert_eq!(lanes, vec![0, 1, 2, 3], "lane 应为 0..3 的排列");
    }

    #[test]
    fn super_edge_pair_groups_adjacent_lanes_across_pairs() {
        // 两组 SuperEdgePair，每组 2 条边：
        //   Pair A (left→right): edge 0 (a1→b1), edge 1 (a2→b2)
        //   Pair B (left→right): edge 2 (a3→b3), edge 3 (a4→b4)
        // 但 a1/a2 在 left，a3/a4 也在 left —— 同一个 leaf group pair
        // 所以实际上 4 条边同属一个 SuperEdgePair，期望 lane 0..3
        // 改为测试不同 leaf group pair 的情况：
        //   corridor left→right，4 条边都跨此 corridor
        //   但 a1,a2 在 left_sub，b1,b2 在 right_sub（SuperEdgePair: left_sub|right_sub）
        //   a3,a4 在 left_other，b3,b4 在 right_other（SuperEdgePair: left_other|right_other）
        // 由于 corridor 是 left|right 级别，node_leaf_group 映射到 left/right
        // 所以所有 4 条边同属 SuperEdgePair (left,right)，无法测试跨 pair 分组
        // 改为测试：同 SuperEdgePair 的边是否获得连续 lane
        let ctx = make_ctx(HashMap::from([
            ("a1".into(), "left".into()),
            ("a2".into(), "left".into()),
            ("a3".into(), "left".into()),
            ("b1".into(), "right".into()),
            ("b2".into(), "right".into()),
            ("b3".into(), "right".into()),
        ]));
        let relations = vec![
            make_relation("a1", "b1"),
            make_relation("a3", "b3"), // 不同 SuperEdgePair 子组，但同 leaf group pair
            make_relation("a2", "b2"),
        ];
        let plan = plan_corridor_routes(
            &relations,
            &ctx,
            &OrthoRoutingProfile::for_diagram_type(crate::types::DiagramType::Flowchart),
        );
        // 排序后应为 a1→b1, a2→b2, a3→b3（按 from_id 然后 to_id）
        // lane: a1→b1=0, a2→b2=1, a3→b3=2
        assert_eq!(
            plan.lanes.get(&(0, 0)),
            Some(&0),
            "edge 0 (a1→b1) 应为 lane 0"
        );
        assert_eq!(
            plan.lanes.get(&(2, 0)),
            Some(&1),
            "edge 2 (a2→b2) 应为 lane 1"
        );
        assert_eq!(
            plan.lanes.get(&(1, 0)),
            Some(&2),
            "edge 1 (a3→b3) 应为 lane 2"
        );
    }

    #[test]
    fn unrelated_edges_on_corridor_get_separate_lanes_on_architecture() {
        // private→data 风格：同源可共 lane，无关边必须分 lane
        let ctx = make_ctx(HashMap::from([
            ("auth".into(), "left".into()),
            ("biz".into(), "left".into()),
            ("redis".into(), "right".into()),
            ("db".into(), "right".into()),
        ]));
        let relations = vec![make_relation("auth", "redis"), make_relation("biz", "db")];
        let plan = plan_corridor_routes(
            &relations,
            &ctx,
            &OrthoRoutingProfile::for_diagram_type(DiagramType::Architecture),
        );
        let l0 = plan.lanes.get(&(0, 0)).copied().unwrap();
        let l1 = plan.lanes.get(&(1, 0)).copied().unwrap();
        assert_ne!(l0, l1, "无关边应分到不同 lane");
    }

    #[test]
    fn corridor_path_uses_lane_coord_for_vertical_trunk() {
        let ctx = make_ctx(HashMap::from([
            ("a1".into(), "left".into()),
            ("a2".into(), "left".into()),
            ("b1".into(), "right".into()),
            ("b2".into(), "right".into()),
        ]));
        let relations = vec![make_relation("a1", "b1"), make_relation("a2", "b2")];
        let plan = plan_corridor_routes(
            &relations,
            &ctx,
            &OrthoRoutingProfile::for_diagram_type(DiagramType::Architecture),
        );
        let lane0 = plan.lanes.get(&(0, 0)).copied().unwrap_or(0);
        let lane1 = plan.lanes.get(&(1, 0)).copied().unwrap_or(1);
        let coord0 = corridor_lane_coord(&sample_corridor(), lane0, 2);
        let coord1 = corridor_lane_coord(&sample_corridor(), lane1, 2);
        assert!(
            (coord0 - coord1).abs() >= CORRIDOR_LANE_PITCH - EPS,
            "无关边 lane 坐标应至少相距 {CORRIDOR_LANE_PITCH}px"
        );

        // 不同 y 锚点迫使走廊段沿垂直轴行走，验证路径 x 使用 lane_coord
        let path0 = try_build_corridor_path(
            0,
            Point::new(80.0, 20.0),
            Point::new(160.0, 70.0),
            "a1",
            "b1",
            &plan,
            &ctx,
            DEFAULT_STUB_LEN,
            false,
        )
        .expect("corridor path");
        let path1 = try_build_corridor_path(
            1,
            Point::new(80.0, 50.0),
            Point::new(160.0, 70.0),
            "a2",
            "b2",
            &plan,
            &ctx,
            DEFAULT_STUB_LEN,
            false,
        )
        .expect("corridor path");

        let x0: Vec<f64> = path0.iter().map(|p| p.x).collect();
        let x1: Vec<f64> = path1.iter().map(|p| p.x).collect();
        assert!(
            x0.iter().any(|&x| (x - coord0).abs() < EPS),
            "path0 应经过 lane0 x={coord0}: {x0:?}"
        );
        assert!(
            x1.iter().any(|&x| (x - coord1).abs() < EPS),
            "path1 应经过 lane1 x={coord1}: {x1:?}"
        );
    }

    #[test]
    fn same_source_fan_out_may_share_corridor_lane() {
        let ctx = make_ctx(HashMap::from([
            ("lb".into(), "left".into()),
            ("auth".into(), "right".into()),
            ("biz".into(), "right".into()),
        ]));
        let relations = vec![make_relation("lb", "auth"), make_relation("lb", "biz")];
        let plan = plan_corridor_routes(
            &relations,
            &ctx,
            &OrthoRoutingProfile::for_diagram_type(DiagramType::Architecture),
        );
        let l0 = plan.lanes.get(&(0, 0)).copied().unwrap();
        let l1 = plan.lanes.get(&(1, 0)).copied().unwrap();
        assert_eq!(l0, l1, "同源 fan-out 可共用 lane");
    }

    #[test]
    fn horizontal_corridor_separates_vertical_trunk_by_cross_axis_offset() {
        let corridor = GroupCorridor {
            axis: CorridorAxis::Horizontal,
            coord: 100.0,
            span_min: 10.0,
            span_max: 200.0,
            group_a: "top".into(),
            group_b: "bottom".into(),
        };
        let mut groups = HashMap::new();
        groups.insert(
            "top".into(),
            GroupLayout {
                x: 0.0,
                y: 0.0,
                width: 120.0,
                height: 60.0,
            },
        );
        groups.insert(
            "bottom".into(),
            GroupLayout {
                x: 0.0,
                y: 140.0,
                width: 120.0,
                height: 60.0,
            },
        );
        let ctx = GroupRoutingContext {
            groups,
            node_to_groups: HashMap::new(),
            border_shell_pad: 12.0,
            stub_clearance: 24.0,
            corridor_misalignment_penalty: 80.0,
            repulse_max_rounds: 2,
            corridors: vec![corridor],
            side_gutters: std::collections::BTreeMap::new(),
            node_leaf_group: HashMap::from([
                ("a1".into(), "top".into()),
                ("a2".into(), "top".into()),
                ("b1".into(), "bottom".into()),
                ("b2".into(), "bottom".into()),
            ]),
            sibling_sets: vec![],
            sibling_orientation: HashMap::new(),
            group_ancestors: HashMap::new(),
        };
        let relations = vec![make_relation("a1", "b1"), make_relation("a2", "b2")];
        let plan = plan_corridor_routes(
            &relations,
            &ctx,
            &OrthoRoutingProfile::for_diagram_type(DiagramType::Architecture),
        );
        let path0 = try_build_corridor_path(
            0,
            Point::new(40.0, 30.0),
            Point::new(80.0, 170.0),
            "a1",
            "b1",
            &plan,
            &ctx,
            DEFAULT_STUB_LEN,
            false,
        )
        .expect("corridor path");
        let path1 = try_build_corridor_path(
            1,
            Point::new(70.0, 30.0),
            Point::new(80.0, 170.0),
            "a2",
            "b2",
            &plan,
            &ctx,
            DEFAULT_STUB_LEN,
            false,
        )
        .expect("corridor path");

        let min_trunk = 12.0;
        let trunk_x0: Vec<f64> = path0
            .windows(2)
            .filter(|w| (w[0].x - w[1].x).abs() < EPS && (w[0].y - w[1].y).abs() > min_trunk)
            .map(|w| w[0].x)
            .collect();
        let trunk_x1: Vec<f64> = path1
            .windows(2)
            .filter(|w| (w[0].x - w[1].x).abs() < EPS && (w[0].y - w[1].y).abs() > min_trunk)
            .map(|w| w[0].x)
            .collect();
        assert!(
            !trunk_x0.is_empty() && !trunk_x1.is_empty(),
            "应有垂直 trunk 段"
        );
        let dx = (trunk_x0[0] - trunk_x1[0]).abs();
        assert!(
            dx >= CORRIDOR_LANE_PITCH - EPS,
            "水平走廊无关边垂直汇入 x 应分离 ≥ pitch: {dx}, paths: {path0:?} {path1:?}"
        );
    }
}
