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

/// 端口翻转后，若目标 (node, side) 上已有**其它边**锚定在 frac=0.5 附近，
/// 沿该侧切线方向错开，避免入边/出边共用同一端口锚点造成重合。
///
/// `base` 为默认锚点（通常 slot_anchor(nl, side, 0.5)）；返回去冲突后的锚点。
fn deconflict_flip_anchor(
    base: Point,
    node_id: &str,
    side: Port,
    nl: &NodeLayout,
    ei: usize,
    endpoint_map: &HashMap<(usize, bool), Endpoint>,
) -> Point {
    const SEP: f64 = 14.0;
    // Top/Bottom 端口沿 x 错开；Left/Right 端口沿 y 错开。
    let vertical = is_vertical_port(side);
    let occupied: Vec<f64> = endpoint_map
        .values()
        .filter(|e| e.node_id == node_id && e.side == side && e.edge_index != ei)
        .map(|e| if vertical { e.anchor.x } else { e.anchor.y })
        .collect();
    if occupied.is_empty() {
        return base;
    }
    let cur = if vertical { base.x } else { base.y };
    if occupied.iter().all(|&o| (o - cur).abs() >= SEP) {
        return base;
    }
    let (lo, hi) = if vertical {
        (
            nl.x + nl.width * SLOT_MARGIN_RATIO,
            nl.x + nl.width * (1.0 - SLOT_MARGIN_RATIO),
        )
    } else {
        (
            nl.y + nl.height * SLOT_MARGIN_RATIO,
            nl.y + nl.height * (1.0 - SLOT_MARGIN_RATIO),
        )
    };
    for mult in [1.0_f64, -1.0, 2.0, -2.0] {
        let cand = (cur + mult * SEP).clamp(lo, hi);
        if occupied.iter().all(|&o| (o - cand).abs() >= SEP - 1.0) {
            return if vertical {
                Point::new(cand, base.y)
            } else {
                Point::new(base.x, cand)
            };
        }
    }
    base
}

/// 返回端口的对面端口
fn opposite_port(p: Port) -> Port {
    match p {
        Port::Top => Port::Bottom,
        Port::Bottom => Port::Top,
        Port::Left => Port::Right,
        Port::Right => Port::Left,
    }
}

/// 路径长度 ≤ PORT_CLEARANCE 且终点不在目标节点端口边上 → 非法退化 stub。
fn is_degenerate_stub_path(points: &[Point], to_nl: &NodeLayout, to_side: Port) -> bool {
    if points.len() < 2 {
        return true;
    }
    let mut len = 0.0;
    for w in points.windows(2) {
        let dx = w[1].x - w[0].x;
        let dy = w[1].y - w[0].y;
        len += (dx * dx + dy * dy).sqrt();
    }
    if len > PORT_CLEARANCE + 1.0 {
        return false;
    }
    let end = points[points.len() - 1];
    !point_near_node_port_edge(end, to_nl, to_side)
}

fn point_near_node_port_edge(p: Point, nl: &NodeLayout, side: Port) -> bool {
    const TOL: f64 = 2.0;
    match side {
        Port::Top => {
            (p.y - nl.y).abs() <= TOL
                && p.x >= nl.x - TOL
                && p.x <= nl.x + nl.width + TOL
        }
        Port::Bottom => {
            (p.y - (nl.y + nl.height)).abs() <= TOL
                && p.x >= nl.x - TOL
                && p.x <= nl.x + nl.width + TOL
        }
        Port::Left => {
            (p.x - nl.x).abs() <= TOL
                && p.y >= nl.y - TOL
                && p.y <= nl.y + nl.height + TOL
        }
        Port::Right => {
            (p.x - (nl.x + nl.width)).abs() <= TOL
                && p.y >= nl.y - TOL
                && p.y <= nl.y + nl.height + TOL
        }
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
    let is_start = anchor_idx == 0;

    // 情况0：首段（从锚点出发的第一段）直接反向——即使后续有正向行程也算反向 stub。
    // 修复 c.layout-stress-nested 写/读/批量：Bottom 端口却先向上伸入节点。
    {
        let neighbor = if is_start {
            points.get(1).copied()
        } else {
            points.get(points.len().saturating_sub(2)).copied()
        };
        if let Some(n) = neighbor {
            let dx = n.x - anchor.x;
            let dy = n.y - anchor.y;
            let proj = dx * out_dx + dy * out_dy;
            // 从锚点看邻居：start 应沿 outward；end 的 prev 也应在 outward 一侧（路径从外进入）
            if proj < -1.0 {
                return true;
            }
        }
    }

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

// ─── PortFix / Attempt: 端口修正候选的类型定义（模块级） ───

#[derive(Copy, Clone, Debug)]
enum PortFix {
    None,
    Flip,        // 翻转到对面端口（反向stub）
    Rotate(Port), // 旋转到指定相邻端口（侧向接入）
}

#[derive(Copy, Clone, Debug)]
struct Attempt {
    from: Option<Port>,
    to: Option<Port>,
}

/// 待修正边的信息：边索引 + from/to 修正方式 + 原 side_approach 标记。
struct EdgeToCheck {
    ei: usize,
    from_fix: PortFix,
    to_fix: PortFix,
    orig_from_has_side: bool,
    orig_to_has_side: bool,
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
    // 侧通道边（回环 / 长跨度）：禁止 stub_fix 把 Left/Right 改成 Top/Bottom，
    // 否则会重新走穿节点列的「捷径」。
    side_channel_edges: &std::collections::HashSet<usize>,
) {
    let n = edges.len();
    if n == 0 {
        return;
    }

    let mut flipped_count = 0usize;

    // Phase 1: 收集所有需要修正的边及建议修正方式，避免边遍历边修改
    let edges_to_check = collect_edges_to_check(
        edges,
        from_side,
        to_side,
        endpoint_map,
        nodes,
        side_channel_edges,
    );

    // Phase 2: 逐边生成候选端口组合，评估选最优
    let r_cfg = OrthoConfig {
        channel_margin: cfg.channel_margin + 10.0,
        ..*cfg
    };

    for etc in edges_to_check {
        let ei = etc.ei;
        let orig_side_problems = (etc.orig_from_has_side as i32) + (etc.orig_to_has_side as i32);
        let old_from = from_side[ei];
        let old_to = to_side[ei];

        let Some(old_from_ep) = endpoint_map.get(&(ei, true)) else { continue };
        let Some(old_to_ep) = endpoint_map.get(&(ei, false)) else { continue };
        let Some(from_nl) = nodes.get(&old_from_ep.node_id) else { continue };
        let Some(to_nl) = nodes.get(&old_to_ep.node_id) else { continue };

        let old_points: Vec<Point> = edges[ei].path_points().into_owned();
        let old_path_len = path_length(&old_points);

        let attempts = generate_attempts(etc.from_fix, etc.to_fix, old_from, old_to);

        grid.remove_by_edges(&[ei]);

        let mut best: Option<(Port, Port, Endpoint, Endpoint, Vec<Point>, f64)> = None;

        for attempt in &attempts {
            if let Some(result) = evaluate_attempt(
                ei,
                attempt,
                old_from,
                old_to,
                old_from_ep,
                old_to_ep,
                from_nl,
                to_nl,
                etc.from_fix,
                etc.to_fix,
                orig_side_problems,
                old_path_len,
                &old_points,
                relations,
                nodes,
                grid,
                &r_cfg,
                profile,
                group_ctx,
                obstacles,
                corridor_plan,
                ortho_stats,
                endpoint_map,
                side_channel_edges.contains(&ei),
            ) {
                let new_len = result.5;
                let better = match &best {
                    None => true,
                    Some((_, _, _, _, _, best_len)) => new_len < *best_len,
                };
                if better {
                    best = Some(result);
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
                        crate::layout::edge::common::parallel_edges::build_parallel_aware_edge_labels_auto(
                            rel, ei, relations, &candidate,
                        )
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

/// 收集所有需要修正的边及建议修正方式。
///
/// 跳过空路径边和侧通道边。对每条边检测 from/to 端点的反向 stub、
/// 侧向接入、退化 stub，生成对应的 `PortFix` 建议。
fn collect_edges_to_check(
    edges: &[EdgeLayout],
    from_side: &[Port],
    to_side: &[Port],
    endpoint_map: &HashMap<(usize, bool), Endpoint>,
    nodes: &HashMap<String, NodeLayout>,
    side_channel_edges: &std::collections::HashSet<usize>,
) -> Vec<EdgeToCheck> {
    let mut edges_to_check: Vec<EdgeToCheck> = Vec::new();
    for ei in 0..edges.len() {
        if edges[ei].path_is_empty() {
            continue;
        }
        // 侧通道边保持 Left/Right（或 LR 下的 Top/Bottom），不参与 stub 翻转/旋转。
        if side_channel_edges.contains(&ei) {
            continue;
        }
        let points: Vec<Point> = edges[ei].path_points().into_owned();
        let Some(to_ep) = endpoint_map.get(&(ei, false)) else {
            continue;
        };
        let Some(to_nl) = nodes.get(&to_ep.node_id) else {
            continue;
        };

        // 退化 stub：路径极短且终点不在目标节点端口边 → 强制翻正对端口
        let degenerate = is_degenerate_stub_path(&points, to_nl, to_side[ei]);

        let from_rev = has_reverse_stub(&points, 0, from_side[ei]);
        let to_rev = has_reverse_stub(&points, points.len() - 1, to_side[ei]);
        let from_side_approach = if from_rev || degenerate {
            None
        } else {
            detect_side_approach(&points, 0, from_side[ei])
        };
        let to_side_approach = if to_rev || degenerate {
            None
        } else {
            detect_side_approach(&points, points.len() - 1, to_side[ei])
        };
        let orig_from_side = from_side_approach.is_some();
        let orig_to_side = to_side_approach.is_some();

        let from_fix = if from_rev || degenerate {
            PortFix::Flip
        } else if let Some(suggested) = from_side_approach {
            PortFix::Rotate(suggested)
        } else {
            PortFix::None
        };
        let to_fix = if to_rev || degenerate {
            PortFix::Flip
        } else if let Some(suggested) = to_side_approach {
            PortFix::Rotate(suggested)
        } else {
            PortFix::None
        };

        if !matches!(from_fix, PortFix::None) || !matches!(to_fix, PortFix::None) {
            edges_to_check.push(EdgeToCheck {
                ei,
                from_fix,
                to_fix,
                orig_from_has_side: orig_from_side,
                orig_to_has_side: orig_to_side,
            });
        }
    }
    edges_to_check
}

/// 基于 from_fix / to_fix 生成候选端口组合。
///
/// - Flip类型：尝试翻转到对面端口
/// - Rotate类型：尝试旋转到建议的相邻端口
/// - 同时尝试两端都修正的组合
/// - 单端 Flip 时额外尝试双端翻转（翻转一端可能导致另一端也反向）
fn generate_attempts(
    from_fix: PortFix,
    to_fix: PortFix,
    old_from: Port,
    old_to: Port,
) -> Vec<Attempt> {
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
            attempts.push(Attempt {
                from: if fc == old_from { None } else { Some(fc) },
                to: if tc == old_to { None } else { Some(tc) },
            });
        }
    }

    // 如果是反向stub单端问题，额外尝试双端翻转（翻转一端可能导致另一端也反向）
    if matches!(from_fix, PortFix::Flip) && matches!(to_fix, PortFix::None) {
        attempts.push(Attempt {
            from: Some(opposite_port(old_from)),
            to: Some(opposite_port(old_to)),
        });
    }
    if matches!(to_fix, PortFix::Flip) && matches!(from_fix, PortFix::None) {
        attempts.push(Attempt {
            from: Some(opposite_port(old_from)),
            to: Some(opposite_port(old_to)),
        });
    }

    attempts
}

/// 评估单个端口组合候选：重新路由 + 检查接受条件。
///
/// 返回 `(new_from, new_to, nf_ep, nt_ep, candidate_path, path_len)` 若可接受；
/// 否则返回 `None`。调用方负责比较 `path_len` 选最优。
#[allow(clippy::too_many_arguments)]
fn evaluate_attempt(
    ei: usize,
    attempt: &Attempt,
    old_from: Port,
    old_to: Port,
    old_from_ep: &Endpoint,
    old_to_ep: &Endpoint,
    from_nl: &NodeLayout,
    to_nl: &NodeLayout,
    from_fix: PortFix,
    to_fix: PortFix,
    orig_side_problems: i32,
    old_path_len: f64,
    old_points: &[Point],
    relations: &[crate::ast::Relation],
    nodes: &HashMap<String, NodeLayout>,
    grid: &SegmentGrid,
    r_cfg: &OrthoConfig,
    profile: &OrthoRoutingProfile,
    group_ctx: &crate::layout::group::GroupRoutingContext,
    obstacles: &PreparedObstacles,
    corridor_plan: &corridor_route::CorridorRoutePlan,
    ortho_stats: &mut crate::layout::OrthoDebugStats,
    endpoint_map: &HashMap<(usize, bool), Endpoint>,
    force_strict_feedback_or_long_span: bool,
) -> Option<(Port, Port, Endpoint, Endpoint, Vec<Point>, f64)> {
    let new_from = attempt.from.unwrap_or(old_from);
    let new_to = attempt.to.unwrap_or(old_to);

    let nf_anchor = if attempt.from.is_some() {
        deconflict_flip_anchor(
            slot_anchor(from_nl, new_from, 0.5),
            &old_from_ep.node_id,
            new_from,
            from_nl,
            ei,
            endpoint_map,
        )
    } else {
        old_from_ep.anchor
    };
    let nt_anchor = if attempt.to.is_some() {
        deconflict_flip_anchor(
            slot_anchor(to_nl, new_to, 0.5),
            &old_to_ep.node_id,
            new_to,
            to_nl,
            ei,
            endpoint_map,
        )
    } else {
        old_to_ep.anchor
    };

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

    let pair = EndpointPair {
        from: nf_ep.clone(),
        to: nt_ep.clone(),
    };
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
        let ctx = OrthoRoutingContext::new(nodes, group_ctx, grid, r_cfg, profile, obstacles, None)
            .with_strict_group_transit(should_strict_group_transit(
                profile,
                group_ctx,
                from_id,
                to_id,
                corridor_plan.chains.contains_key(&ei),
                force_strict_feedback_or_long_span,
            ))
            // 换端口重试：升档外框通道
            .with_corridor_boost(true);
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

    if candidate.len() < 2 {
        return None;
    }

    let candidate = simplify_path(candidate, true);
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
    let still_degenerate = is_degenerate_stub_path(&candidate, to_nl, new_to);
    // 允许最长比原路径长20%，但优先选择更短的路径
    let len_ok = new_len <= old_path_len * 1.2 + 60.0
        || is_degenerate_stub_path(old_points, to_nl, old_to);

    // 接受条件：
    // 1. 路径干净（不穿过节点/组内部）
    // 2. 无反向stub
    // 3. 长度可接受
    // 4. 问题修复检查（满足任一）：
    //    a) 总side_approach问题数减少
    //    b) 总side_approach问题数不变且路径更短
    //    c) 路径明显更短（<0.9倍原长）
    //    d) 原路径为退化 stub，新路径非退化
    let new_from_has_side = new_from_side.is_some();
    let new_to_has_side = new_to_side.is_some();
    let new_side_problems = (new_from_has_side as i32) + (new_to_has_side as i32);
    let side_problems_improved = new_side_problems < orig_side_problems;
    let side_problems_same_or_better = new_side_problems <= orig_side_problems;
    let shorter = new_len < old_path_len;
    let significantly_shorter = new_len < old_path_len * 0.9;
    let fixes_degenerate =
        is_degenerate_stub_path(old_points, to_nl, old_to) && !still_degenerate;

    let accept = if fixes_degenerate {
        true
    } else if side_problems_improved {
        true
    } else if side_problems_same_or_better && shorter {
        true
    } else if significantly_shorter {
        true
    } else {
        false
    };

    if clean && no_reverse && !still_degenerate && len_ok && accept {
        Some((new_from, new_to, nf_ep, nt_ep, candidate, new_len))
    } else {
        None
    }
}
