//! Reverse-stub / side-approach detection and port correction.

use super::*;
use super::path::port_outward;
use crate::layout::geometry::Point;
use crate::layout::{EdgeLayout, NodeLayout, PathGeometry, Port};
use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════
//  方案B：反向 stub 检测与端口翻转（Step 4e）
// ═══════════════════════════════════════════════════════════

/// 反向stub判定阈值：stub段在反方向延伸超过此距离（像素）即判定为反向stub。
/// 该值需大于 PORT_CLEARANCE(16px)，避免将正常stub段误判。
const REVERSE_STUB_THRESHOLD: f64 = 24.0;

/// 返回端口的对面端口
fn opposite_port(p: Port) -> Port {
    match p {
        Port::Top => Port::Bottom,
        Port::Bottom => Port::Top,
        Port::Left => Port::Right,
        Port::Right => Port::Left,
    }
}

/// 检测单个端点是否存在反向stub。
///
/// 反向stub的两种情况：
/// 1. **stub方向反了**：路径离开锚点后直接沿反方向走（max_forward < PORT_CLEARANCE
///    且 max_reverse > REVERSE_STUB_THRESHOLD）。
/// 2. **U型折返**：路径虽然在最后一小段（stub段，PORT_CLEARANCE长度内）方向正确，
///    但走出stub后几乎立即掉头（沿垂直轴移动不超过CHANNEL_SPACING后就沿反方向走很远），
///    说明路径为了接入错误端口而做了不必要的U型转弯。
fn has_reverse_stub(points: &[Point], anchor_idx: usize, side: Port) -> bool {
    if points.len() < 3 {
        return false;
    }
    let anchor = points[anchor_idx];
    let (out_dx, out_dy) = port_outward(side);

    // 情况1：直接反向（stub段本身方向反了）
    let mut max_forward = 0.0f64;
    let mut max_reverse = 0.0f64;
    for p in points {
        let dx = p.x - anchor.x;
        let dy = p.y - anchor.y;
        let forward_proj = dx * out_dx + dy * out_dy;
        if forward_proj > max_forward {
            max_forward = forward_proj;
        }
        if -forward_proj > max_reverse {
            max_reverse = -forward_proj;
        }
    }
    if max_forward < 16.0 && max_reverse > REVERSE_STUB_THRESHOLD {
        return true;
    }

    // 情况2：U型折返检测
    // 沿路径从anchor出发向主体方向遍历，跟踪走出stub后的方向变化。
    // 如果走出stub（fp>=16）后，在垂直于outward方向移动不超过 CHANNEL_SPACING*2
    // 的距离内，fp降到 -REVERSE_STUB_THRESHOLD 以下，说明是U型折返。
    let is_start = anchor_idx == 0;
    let traversal: Vec<Point> = if is_start {
        points.to_vec()
    } else {
        points.iter().rev().copied().collect()
    };

    // outward轴的垂直轴
    let (perp_dx, perp_dy) = (-out_dy, out_dx);

    let mut left_stub = false;
    let mut perp_at_stub_exit = 0.0f64;
    let mut max_perp_after_stub = 0.0f64;
    const CHANNEL_SPACING: f64 = 48.0;
    const U_TURN_PERP_LIMIT: f64 = CHANNEL_SPACING * 2.0;

    for p in &traversal {
        let dx = p.x - anchor.x;
        let dy = p.y - anchor.y;
        let fp = dx * out_dx + dy * out_dy;
        let pp = dx * perp_dx + dy * perp_dy;

        if !left_stub {
            if fp >= 16.0 {
                left_stub = true;
                perp_at_stub_exit = pp;
            }
        } else {
            let perp_since_exit = (pp - perp_at_stub_exit).abs();
            if perp_since_exit > max_perp_after_stub {
                max_perp_after_stub = perp_since_exit;
            }
            // 如果垂直轴移动距离还很小，但fp已经变负很多，说明U型折返
            if perp_since_exit < U_TURN_PERP_LIMIT && fp < -(REVERSE_STUB_THRESHOLD) {
                return true;
            }
            // 如果垂直轴已经移动很远了，说明是正常绕路，不再检查
            if max_perp_after_stub > U_TURN_PERP_LIMIT * 2.0 {
                break;
            }
        }
    }

    false
}

/// 侧向接入检测：路径拐90度L弯才进入/离开端口，沿平行于边的方向走了较长距离。
///
/// 检测方式：从锚点沿路径前进，跳过所有沿outward轴向的连续段（标准stub 16px + 可能的扩展stub），
/// 找到第一个方向改变的拐点，然后检查拐点相邻段的主要方向是否为perp（平行于边）。
///
/// 这能正确处理fallback路径中使用的扩展stub（2.5x/4x/6x PORT_CLEARANCE）。
fn detect_side_approach(points: &[Point], anchor_idx: usize, side: Port) -> Option<Port> {
    if points.len() < 4 {
        return None;
    }

    let (out_dx, out_dy) = port_outward(side);
    let (perp_dx, perp_dy) = (-out_dy, out_dx);

    const SIDE_JOG_THRESHOLD: f64 = 48.0;

    let (corner, far) = if anchor_idx == 0 {
        // From端点：从p[0]（锚点）向前走，找到第一个不沿outward方向的拐点
        let mut corner_idx = 1usize;
        while corner_idx + 1 < points.len() {
            let a = points[corner_idx];
            let b = points[corner_idx + 1];
            let seg_dx = b.x - a.x;
            let seg_dy = b.y - a.y;
            let seg_fwd = seg_dx * out_dx + seg_dy * out_dy;
            let seg_perp = seg_dx * perp_dx + seg_dy * perp_dy;
            if seg_fwd > seg_perp.abs() && seg_fwd > 0.0 {
                corner_idx += 1;
            } else {
                break;
            }
        }
        if corner_idx + 1 >= points.len() {
            return None;
        }
        (points[corner_idx], points[corner_idx + 1])
    } else {
        // To端点：从p[last]（锚点）往回走，找到第一个不沿outward轴向的拐点
        let len = points.len();
        let mut corner_idx = len - 2;
        while corner_idx > 0 {
            let a = points[corner_idx - 1];
            let b = points[corner_idx];
            let seg_dx = b.x - a.x;
            let seg_dy = b.y - a.y;
            let seg_fwd = seg_dx * out_dx + seg_dy * out_dy;
            let seg_perp = seg_dx * perp_dx + seg_dy * perp_dy;
            if seg_fwd.abs() > seg_perp.abs() {
                corner_idx -= 1;
            } else {
                break;
            }
        }
        if corner_idx == 0 {
            return None;
        }
        (points[corner_idx], points[corner_idx - 1])
    };

    let seg_dx = far.x - corner.x;
    let seg_dy = far.y - corner.y;
    let seg_len = (seg_dx * seg_dx + seg_dy * seg_dy).sqrt();
    if seg_len < EPS {
        return None;
    }

    let seg_fwd = seg_dx * out_dx + seg_dy * out_dy;
    let seg_perp = seg_dx * perp_dx + seg_dy * perp_dy;

    if seg_perp.abs() > seg_fwd.abs() && seg_perp.abs() > SIDE_JOG_THRESHOLD {
        let suggested = match side {
            Port::Top | Port::Bottom => {
                if far.x > corner.x { Port::Right } else { Port::Left }
            }
            Port::Left | Port::Right => {
                if far.y > corner.y { Port::Bottom } else { Port::Top }
            }
        };
        return Some(suggested);
    }

    None
}

/// 反向 stub 与侧向接入检测、端口修正。
///
/// 问题场景：
/// 1. 反向stub：路径离开/进入端口后沿反方向折返（U型折返或直接反向）
/// 2. 侧向接入：路径拐90度L弯才进入端口（如从右侧来却进入Bottom端口），
///    这通常因走廊/障碍物导致实际来向与几何方向不一致。
///
/// 修正策略：
/// - 反向stub → 尝试翻转到对面端口（Bottom↔Top, Left↔Right）
/// - 侧向接入 → 尝试旋转到相邻端口（如Bottom→Right）
/// - 对每条有问题的边尝试多种端口组合，选最短且干净的路径
pub fn fix_reverse_stub_ports(
    nodes: &HashMap<String, NodeLayout>,
    relations: &[crate::ast::Relation],
    from_side: &mut [Port],
    to_side: &mut [Port],
    endpoint_map: &mut HashMap<(usize, bool), Endpoint>,
    edges: &mut Vec<EdgeLayout>,
    grid: &mut SegmentGrid,
    cfg: &OrthoConfig,
    group_ctx: &crate::layout::group::GroupRoutingContext,
    obstacles: &PreparedObstacles,
    corridor_plan: &corridor_route::CorridorRoutePlan,
    ortho_stats: &mut crate::layout::OrthoDebugStats,
    profile: &OrthoRoutingProfile,
) {
    let n = edges.len();
    if n == 0 {
        return;
    }

    let mut flipped_count = 0usize;

    #[derive(Copy, Clone, Debug)]
    enum PortFix {
        None,
        Flip,        // 翻转到对面端口（反向stub）
        Rotate(Port), // 旋转到指定相邻端口（侧向接入）
    }

    // 先收集所有需要修正的边及建议修正方式，避免边遍历边修改
    let mut edges_to_check: Vec<(usize, PortFix, PortFix, bool, bool)> = Vec::new();
    for ei in 0..n {
        if edges[ei].path_is_empty() {
            continue;
        }
        let points: Vec<Point> = edges[ei].path_points().into_owned();
        let from_rev = has_reverse_stub(&points, 0, from_side[ei]);
        let to_rev = has_reverse_stub(&points, points.len() - 1, to_side[ei]);
        let from_side_approach = if from_rev { None } else { detect_side_approach(&points, 0, from_side[ei]) };
        let to_side_approach = if to_rev { None } else { detect_side_approach(&points, points.len() - 1, to_side[ei]) };
        let orig_from_side = from_side_approach.is_some();
        let orig_to_side = to_side_approach.is_some();

        let from_fix = if from_rev {
            PortFix::Flip
        } else if let Some(suggested) = from_side_approach {
            PortFix::Rotate(suggested)
        } else {
            PortFix::None
        };
        let to_fix = if to_rev {
            PortFix::Flip
        } else if let Some(suggested) = to_side_approach {
            PortFix::Rotate(suggested)
        } else {
            PortFix::None
        };

        if !matches!(from_fix, PortFix::None) || !matches!(to_fix, PortFix::None) {
            edges_to_check.push((ei, from_fix, to_fix, orig_from_side, orig_to_side));
        }
    }

    for (ei, from_fix, to_fix, orig_from_has_side, orig_to_has_side) in edges_to_check {
        let orig_side_problems = (orig_from_has_side as i32) + (orig_to_has_side as i32);
        let old_from = from_side[ei];
        let old_to = to_side[ei];

        let Some(old_from_ep) = endpoint_map.get(&(ei, true)) else { continue };
        let Some(old_to_ep) = endpoint_map.get(&(ei, false)) else { continue };
        let Some(from_nl) = nodes.get(&old_from_ep.node_id) else { continue };
        let Some(to_nl) = nodes.get(&old_to_ep.node_id) else { continue };

        let old_points: Vec<Point> = edges[ei].path_points().into_owned();
        let old_path_len = path_length(&old_points);

        // 生成需要尝试的端口组合：
        // - Flip类型：尝试翻转到对面端口
        // - Rotate类型：尝试旋转到建议的相邻端口
        // - 同时尝试两端都修正的组合
        #[derive(Copy, Clone, Debug)]
        struct Attempt { from: Option<Port>, to: Option<Port> }

        let mut attempts: Vec<Attempt> = Vec::new();

        // 基于from_fix和to_fix生成候选端口列表（包含原端口作为选项）
        let from_candidates: Vec<Port> = match from_fix {
            PortFix::None => vec![old_from],
            PortFix::Flip => vec![old_from, opposite_port(old_from)],
            PortFix::Rotate(p) => vec![old_from, p],
        };
        let to_candidates: Vec<Port> = match to_fix {
            PortFix::None => vec![old_to],
            PortFix::Flip => vec![old_to, opposite_port(old_to)],
            PortFix::Rotate(p) => vec![old_to, p],
        };

        // 笛卡尔积生成所有组合
        for &fc in &from_candidates {
            for &tc in &to_candidates {
                if fc == old_from && tc == old_to {
                    continue; // 跳过不修改的组合（保持原路径）
                }
                attempts.push(Attempt { from: if fc == old_from { None } else { Some(fc) }, to: if tc == old_to { None } else { Some(tc) } });
            }
        }

        // 如果是反向stub单端问题，额外尝试双端翻转（翻转一端可能导致另一端也反向）
        if matches!(from_fix, PortFix::Flip) && matches!(to_fix, PortFix::None) {
            attempts.push(Attempt { from: Some(opposite_port(old_from)), to: Some(opposite_port(old_to)) });
        }
        if matches!(to_fix, PortFix::Flip) && matches!(from_fix, PortFix::None) {
            attempts.push(Attempt { from: Some(opposite_port(old_from)), to: Some(opposite_port(old_to)) });
        }

        let mut best: Option<(Port, Port, Endpoint, Endpoint, Vec<Point>, f64)> = None;

        let r_cfg = OrthoConfig {
            channel_margin: cfg.channel_margin + 10.0,
            ..*cfg
        };

        grid.remove_by_edges(&[ei]);

        for attempt in &attempts {
            let new_from = attempt.from.unwrap_or(old_from);
            let new_to = attempt.to.unwrap_or(old_to);

            let nf_anchor = if attempt.from.is_some() { slot_anchor(from_nl, new_from, 0.5) } else { old_from_ep.anchor };
            let nt_anchor = if attempt.to.is_some() { slot_anchor(to_nl, new_to, 0.5) } else { old_to_ep.anchor };

            let nf_ep = Endpoint {
                edge_index: ei,
                is_from: true,
                target_x: old_from_ep.target_x,
                target_y: old_from_ep.target_y,
                lane: old_from_ep.lane,
                node_id: old_from_ep.node_id.clone(),
                side: new_from,
                anchor: nf_anchor,
            };
            let nt_ep = Endpoint {
                edge_index: ei,
                is_from: false,
                target_x: old_to_ep.target_x,
                target_y: old_to_ep.target_y,
                lane: old_to_ep.lane,
                node_id: old_to_ep.node_id.clone(),
                side: new_to,
                anchor: nt_anchor,
            };

            let pair = EndpointPair { from: nf_ep.clone(), to: nt_ep.clone() };
            let (from_id, to_id) = relations
                .get(ei)
                .map(|rel| (rel.from.as_str(), rel.to.as_str()))
                .unwrap_or(("", ""));

            let mut path_stats = PathSelectStats::default();
            let candidate = validated_corridor_path(
                ei,
                nf_anchor,
                nt_anchor,
                from_id,
                to_id,
                corridor_plan,
                group_ctx,
                nodes,
                obstacles,
                r_cfg.channel_margin,
            )
            .unwrap_or_else(|| {
                let ctx = RoutingContext::new(nodes, group_ctx, grid, &r_cfg, profile, obstacles, None)
                    .with_strict_group_transit(should_strict_group_transit(
                        profile,
                        group_ctx,
                        from_id,
                        to_id,
                        corridor_plan.chains.contains_key(&ei),
                    ));
                select_best_path_with_scorer_stats(
                    &ctx,
                    &pair,
                    &DefaultScorer,
                    Some(&mut path_stats),
                    false,
                )
            });
            ortho_stats.total_candidates += path_stats.candidate_count;
            ortho_stats.hard_filter_reject_count += path_stats.hard_filter_reject_count;
            if path_stats.degraded {
                ortho_stats.degraded_count += 1;
            }

            if candidate.len() >= 2 {
                let candidate = simplify_path_preserving_stubs(candidate);
                let clean = path_is_clean(
                    &candidate,
                    pair.from_id(),
                    pair.to_id(),
                    nodes,
                    group_ctx,
                    &obstacles.sorted_node_ids,
                ) && path_avoids_group_interiors(
                    &candidate,
                    pair.from_id(),
                    pair.to_id(),
                    group_ctx,
                    &obstacles.sorted_group_ids,
                );
                let new_from_rev = has_reverse_stub(&candidate, 0, new_from);
                let new_to_rev = has_reverse_stub(&candidate, candidate.len() - 1, new_to);
                let new_from_side = if matches!(from_fix, PortFix::Rotate(_) | PortFix::Flip) {
                    detect_side_approach(&candidate, 0, new_from)
                } else {
                    None
                };
                let new_to_side = if matches!(to_fix, PortFix::Rotate(_) | PortFix::Flip) {
                    detect_side_approach(&candidate, candidate.len() - 1, new_to)
                } else {
                    None
                };
                let no_reverse = !new_from_rev && !new_to_rev;
                let new_len = path_length(&candidate);
                // 允许最长比原路径长20%，但优先选择更短的路径
                let len_ok = new_len <= old_path_len * 1.2 + 60.0;

                // 接受条件：
                // 1. 路径干净（不穿过节点/组内部）
                // 2. 无反向stub
                // 3. 长度可接受
                // 4. 问题修复检查（满足任一）：
                //    a) 总side_approach问题数减少
                //    b) 总side_approach问题数不变且路径更短
                //    c) 路径明显更短（<0.9倍原长）
                let new_from_has_side = new_from_side.is_some();
                let new_to_has_side = new_to_side.is_some();
                let new_side_problems = (new_from_has_side as i32) + (new_to_has_side as i32);
                let side_problems_improved = new_side_problems < orig_side_problems;
                let side_problems_same_or_better = new_side_problems <= orig_side_problems;
                let shorter = new_len < old_path_len;
                let significantly_shorter = new_len < old_path_len * 0.9;

                let accept = if side_problems_improved {
                    true
                } else if side_problems_same_or_better && shorter {
                    true
                } else if significantly_shorter {
                    true
                } else {
                    false
                };

                if clean && no_reverse && len_ok && accept {
                    let better = match &best {
                        None => true,
                        Some((_, _, _, _, _, best_len)) => new_len < *best_len,
                    };
                    if better {
                        best = Some((new_from, new_to, nf_ep, nt_ep, candidate, new_len));
                    }
                }
            }
        }

        match best {
            Some((new_from, new_to, nf_ep, nt_ep, candidate, _)) => {
                from_side[ei] = new_from;
                to_side[ei] = new_to;
                endpoint_map.insert((ei, true), nf_ep);
                endpoint_map.insert((ei, false), nt_ep);

                let labels = match relations.get(ei) {
                    Some(rel) => {
                        let middle_t = parse_label_t(rel);
                        build_edge_labels(rel, middle_t, Point::new(0.0, 0.0), |t| {
                            point_at_path_t(&candidate, t)
                        })
                    }
                    None => Vec::new(),
                };
                grid.insert_path(&candidate, ei);
                let mut edge = EdgeLayout {
                    geometry: PathGeometry::Polyline { points: Vec::new() },
                    labels,
                    from_port: new_from,
                    to_port: new_to,
                };
                edge.set_polyline_points(candidate);
                edges[ei] = edge;
                flipped_count += 1;
            }
            None => {
                grid.insert_path(&old_points, ei);
            }
        }
    }

    ortho_stats.flipped_stub_edges = flipped_count;
}
