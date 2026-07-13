//! Global slot replan (Layer 3) after initial orthogonal routing.

use super::*;
use super::path::port_outward;
use crate::layout::geometry::Point;
use crate::layout::{EdgeLayout, NodeLayout, PathGeometry, Port};
use std::collections::HashMap;

/// 全局 Slot 重规划（Layer 3）：路由完成后根据实际出口方向全局重排 slot，
/// 替代 fix_slot_inversions 的冒泡交换+多次重路由。
///
/// 核心改进：
/// 1. 一次性全局排序（按实际出口方向），而非冒泡相邻交换
/// 2. 排序后一次性轻量重路由（phase1_only），而非每交换一对就重路由
/// 3. 覆盖所有倒挂情况，而非仅相邻对
pub fn replan_slots(
    nodes: &HashMap<String, NodeLayout>,
    relations: &[crate::ast::Relation],
    from_side: &[Port],
    to_side: &[Port],
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
    use std::collections::{BTreeMap, HashSet};

    let n = edges.len();

    // replan_slots 通用原则：
    // 按 (node_id, side) 分组后，同一侧所有端点应按"实际走向"排列以消除 stub 交叉。
    // 垂直端口(Top/Bottom)按 dx 升序（左→右）；水平端口(Left/Right)按 -dy 升序（上→下）。
    //
    // 不可拆分单元（锚点块）：初始分配中共享完全相同 tangent 坐标的端点集合
    // （Concentrate策略：4+条边共享同一锚点形成扇形汇流）必须保持为整体，
    // 不能被拆散。Compact(2-3条边)和Single(1条边)的端点各自有独立 tangent，
    // 可以自由重排。
    //
    // 这比按 bundling_key(is_from+arrow+style)分块更通用：bundling_key 按
    // "能否合并trunk"分组，但同组内 Compact 端点走向可能分化（一左一右），
    // 强行作为块会导致跨方向交叉无法修复。
    let mut side_endpoints: BTreeMap<String, Vec<(usize, bool)>> = BTreeMap::new();
    for i in 0..n {
        if edges[i].path_is_empty() {
            continue;
        }
        for &is_from in &[true, false] {
            if let Some(ep) = endpoint_map.get(&(i, is_from)) {
                let key = format!("{}|{:?}", ep.node_id, ep.side);
                side_endpoints.entry(key).or_default().push((i, is_from));
            }
        }
    }

    let mut edges_to_reroute: HashSet<usize> = HashSet::new();

    for (_side_key, ep_tuples) in &side_endpoints {
        if ep_tuples.len() < 2 {
            continue;
        }

        let first_ep = endpoint_map.get(&ep_tuples[0]).unwrap();
        let side = first_ep.side;
        let vertical_side = is_vertical_port(side);

        // 收集所有端点信息
        let mut ep_info: Vec<(usize, bool, f64, f64)> = Vec::new(); // (ei, ef, sort_key, tangent)
        for &(ei, ef) in ep_tuples {
            let ep = endpoint_map.get(&(ei, ef)).unwrap();
            let effective_dir = compute_effective_exit_dir(edges, ei, ef, side);
            let tangent = if vertical_side { ep.anchor.x } else { ep.anchor.y };
            let sort_key = if vertical_side {
                effective_dir
            } else {
                -effective_dir
            };
            ep_info.push((ei, ef, sort_key, tangent));
        }

        // 按 tangent 值分组，构建锚点块（共享同一 tangent 的端点为不可拆分单元）
        // 使用 BTreeMap 保证按 tangent 升序（即初始从左到右/从上到下顺序）
        let mut tangent_groups: BTreeMap<i64, Vec<(usize, bool, f64, f64)>> = BTreeMap::new();
        for info in &ep_info {
            let tangent_key = (info.3 * 1000.0).round() as i64; // 0.001 精度
            tangent_groups.entry(tangent_key).or_default().push(*info);
        }

        struct AnchorBlock {
            members: Vec<(usize, bool, f64, f64)>, // (ei, ef, sort_key, tangent)
            dir_key: f64,                         // 块代表方向
            _center_tangent: f64,                 // 中心 tangent（stable tiebreak）
        }

        let mut blocks: Vec<AnchorBlock> = Vec::new();
        for (_, members) in tangent_groups {
            let dir_sum: f64 = members.iter().map(|m| m.2).sum();
            let dir_key = dir_sum / members.len() as f64;
            let center_tangent: f64 = members.iter().map(|m| m.3).sum::<f64>() / members.len() as f64;
            blocks.push(AnchorBlock {
                members,
                dir_key,
                _center_tangent: center_tangent,
            });
        }

        // 块内按 sort_key 排序端点（同 key 时按 edge_index）
        for block in &mut blocks {
            block.members.sort_by(|a, b| {
                a.2
                    .partial_cmp(&b.2)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.0.cmp(&b.0))
            });
        }

        // 块间排序：dir_key → center_tangent → min(edge_index)，单次全序保证确定性
        blocks.sort_by(|a, b| {
            a.dir_key
                .partial_cmp(&b.dir_key)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(
                    a._center_tangent
                        .partial_cmp(&b._center_tangent)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
                .then_with(|| {
                    a.members
                        .iter()
                        .map(|m| m.0)
                        .min()
                        .cmp(&b.members.iter().map(|m| m.0).min())
                })
        });

        // 收集 tangent 池并按 (量化 tangent, 原始值, edge_index, is_from) 排序后分配
        let mut tangent_pool: Vec<(f64, usize, bool)> = Vec::new();
        for block in &blocks {
            for m in &block.members {
                tangent_pool.push((m.3, m.0, m.1));
            }
        }
        tangent_pool.sort_by(|a, b| {
            let ka = (a.0 * 1000.0).round() as i64;
            let kb = (b.0 * 1000.0).round() as i64;
            ka.cmp(&kb)
                .then(a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
                .then(a.1.cmp(&b.1))
                .then(a.2.cmp(&b.2))
        });

        let mut idx = 0;
        for block in &blocks {
            for m in &block.members {
                let new_tangent = tangent_pool[idx].0;
                idx += 1;
                let (ei, ef, _, _) = m;
                if let Some(ep) = endpoint_map.get_mut(&(*ei, *ef)) {
                    let current_tangent = if vertical_side { ep.anchor.x } else { ep.anchor.y };
                    if (current_tangent - new_tangent).abs() > EPS {
                        if vertical_side {
                            ep.anchor.x = new_tangent;
                        } else {
                            ep.anchor.y = new_tangent;
                        }
                        edges_to_reroute.insert(*ei);
                    }
                }
            }
        }
    }

    if edges_to_reroute.is_empty() {
        return;
    }

    let mut edge_vec: Vec<usize> = edges_to_reroute.into_iter().collect();
    edge_vec.sort_unstable();
    grid.remove_by_edges(&edge_vec);

    for &ei in &edge_vec {
        let Some(from_ep) = endpoint_map.get(&(ei, true)) else {
            continue;
        };
        let Some(to_ep) = endpoint_map.get(&(ei, false)) else {
            continue;
        };


        let (from_id, to_id) = relations
            .get(ei)
            .map(|rel| (rel.from.as_str(), rel.to.as_str()))
            .unwrap_or(("", ""));
        let ctx = OrthoRoutingContext::new(nodes, group_ctx, grid, cfg, profile, obstacles, None)
            .with_strict_group_transit(should_strict_group_transit(
                profile,
                group_ctx,
                from_id,
                to_id,
                corridor_plan.chains.contains_key(&ei),
                false,
            ))
            // slot 重排后的重路由：允许升档外框绕行
            .with_corridor_boost(true);
        let pair = EndpointPair {
            from: from_ep.clone(),
            to: to_ep.clone(),
        };

        let mut path_stats = PathSelectStats::default();
        let path = validated_corridor_path(
            ei,
            from_ep.anchor,
            to_ep.anchor,
            from_id,
            to_id,
            corridor_plan,
            group_ctx,
            nodes,
            obstacles,
            cfg.channel_margin,
        )
        .unwrap_or_else(|| {
            select_best_path_with_scorer_stats(
                &ctx,
                &pair,
                &DefaultScorer,
                Some(&mut path_stats),
                true,
            )
        });
        ortho_stats.total_candidates += path_stats.candidate_count;
        ortho_stats.hard_filter_reject_count += path_stats.hard_filter_reject_count;
        if path_stats.degraded {
            ortho_stats.degraded_count += 1;
        }

        let labels = if path.len() >= 2 {
            match relations.get(ei) {
                Some(rel) => {
                    crate::layout::edge::common::parallel_edges::build_parallel_aware_edge_labels_auto(
                        rel, ei, relations, &path,
                    )
                }
                None => Vec::new(),
            }
        } else {
            Vec::new()
        };

        grid.insert_path(&path, ei);

        let mut edge = EdgeLayout {
            geometry: PathGeometry::Polyline { points: Vec::new() },
            labels,
            from_port: from_side[ei],
            to_port: to_side[ei],
        };
        edge.set_polyline_points(path);

        edges[ei] = edge;
    }
}


/// 从已路由的边路径中提取"有效出口方向"——即锚点出发后第一个非 stub 的
/// 切线位移分量。
///
/// - 垂直端口 (Top/Bottom)：返回水平位移（正=向右，负=向左）
/// - 水平端口 (Left/Right)：返回垂直位移（正=向下，负=向上）
///
/// stub 段是锚点沿端口外延方向的短线段（长度 PORT_CLEARANCE），
/// 需要跳过 stub 才能获得实际路由方向。
fn compute_effective_exit_dir(
    edges: &[EdgeLayout],
    edge_index: usize,
    is_from: bool,
    side: Port,
) -> f64 {
    let points: Vec<Point> = edges[edge_index].path_points().into_owned();
    if points.len() < 3 {
        return 0.0;
    }

    let vertical_side = is_vertical_port(side);

    // 从锚点端开始遍历路径点，跳过 stub 段（沿端口外延方向的段），
    // 找到第一个有切线位移的点
    let start_idx = if is_from { 0 } else { points.len() - 1 };
    let anchor = points[start_idx];

    // stub 方向：端口外延方向
    let (out_dx, out_dy) = port_outward(side);

    // 从锚点出发，沿路径跳过 stub 段
    let iter_range: Box<dyn Iterator<Item = usize>> = if is_from {
        Box::new(1..points.len())
    } else {
        Box::new((0..points.len().saturating_sub(1)).rev())
    };

    for idx in iter_range {
        let p = points[idx];
        let dx = p.x - anchor.x;
        let dy = p.y - anchor.y;

        // 跳过仍在 stub 方向上的点（沿端口外延方向移动）
        let is_stub = if out_dx.abs() > EPS {
            // 水平 stub（Left/Right 端口）：dx 与 out_dx 同号
            dx * out_dx > EPS && dy.abs() < EPS
        } else {
            // 垂直 stub（Top/Bottom 端口）：dy 与 out_dy 同号
            dy * out_dy > EPS && dx.abs() < EPS
        };

        if !is_stub {
            // 找到第一个非 stub 点，返回切线位移
            return if vertical_side { dx } else { dy };
        }
    }

    0.0
}
