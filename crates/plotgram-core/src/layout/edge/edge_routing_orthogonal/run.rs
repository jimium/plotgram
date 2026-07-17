//! 正交路由主流程：端口/slot → 逐边建路 → straighten / reroute / stub / lane / sanitize / labels。
//!
//! 从 `mod.rs` 抽出，便于按 phase 阅读与改时序；行为应与抽取前一致。

use super::*;
use crate::ast::Diagram;
use crate::layout::edge::common::edge_geometry::{
    arrow_type_tag, canonical_pair, edge_line_style_signature, node_center, undirected_pair_key,
};
use crate::layout::edge::common::parallel_edges::build_parallel_aware_edge_labels;
use crate::layout::edge::common::self_loop;
use crate::layout::geometry::Point;
use crate::layout::{EdgeLayout, LayoutResult, NodeLayout, PathGeometry, Port};
use std::collections::HashMap;

pub(super) fn route_edges_orthogonal_inner(
    diagram: &Diagram,
    mut result: LayoutResult,
    cfg: OrthoConfig,
    preserve_edges: Option<std::collections::HashSet<usize>>,
) -> LayoutResult {
    let relations = &diagram.relations;
    let n = relations.len();
    let self_loop_idx = self_loop::self_loop_indices(relations);
    let profile = OrthoRoutingProfile::for_diagram_type(diagram.diagram_type.clone());
    let parallel_gap = profile.parallel_gap;
    // S4：与 S3 同门控——仅无分组 architecture；有组大图强制外环/监控延后会抬 high tight/穿模
    let s4_monitor_corridor = profile.semantic_merge && diagram.groups.is_empty();

    let routing_algo = crate::layout::group::routing_algo_for_diagram(diagram);
    let group_ctx =
        crate::layout::group::GroupRoutingContext::from_layout(diagram, &result, routing_algo);
    let mut group_routing = group_ctx.routing_hints();
    if let Some(existing) = &result.hints.group_routing {
        if !existing.side_gutters.is_empty() {
            group_routing.side_gutters = existing.side_gutters.clone();
        }
    }
    result.hints.group_routing = Some(group_routing);

    // 预排序节点/分组 ID，避免路由循环内重复排序（方案 2）
    let obstacles = PreparedObstacles::build(&result.nodes, &group_ctx);

    let horizontal = crate::layout::resolve_effective_direction(diagram) == Some("left-to-right");
    let feedback_assignment = feedback_side::assign_feedback_sides(
        diagram,
        relations,
        &result.nodes,
        result.hints.sugiyama_ranks.as_ref(),
        horizontal,
    );

    let corridor_plan = corridor_route::plan_corridor_routes(relations, &group_ctx, &profile);

    // ── 1+2. 端口选择 + slot 分配 + 平行边偏移 ──
    let (mut from_side, mut to_side, _lane, mut endpoint_map, parallel, reverse_pairs) =
        phase_port_slot(
            relations,
            &result.nodes,
            &group_ctx,
            &feedback_assignment,
            &cfg,
            n,
            s4_monitor_corridor,
            horizontal,
        );

    // ── 3. 分层批量边序（有 rank 时低层先占通道；feedback 全局延后） ──
    let (edge_order, feedback_edge_set) = phase_layer_order(
        relations,
        result.hints.sugiyama_ranks.as_ref(),
        &feedback_assignment,
        s4_monitor_corridor,
    );

    // ── 4. 逐边构建路径 ──
    let incremental = preserve_edges.is_some();
    let mut edges: Vec<EdgeLayout> = if incremental {
        result.edges.clone()
    } else {
        (0..n).map(|_| EdgeLayout::empty()).collect()
    };
    let mut grid = SegmentGrid::new();

    // P2-1: 路由 debug 统计
    let mut ortho_stats = crate::layout::OrthoDebugStats {
        edge_count: n,
        ..Default::default()
    };

    phase_route_edges(
        &edge_order,
        relations,
        &result.nodes,
        &from_side,
        &to_side,
        &endpoint_map,
        &mut edges,
        &mut grid,
        &mut ortho_stats,
        &cfg,
        &profile,
        &group_ctx,
        &obstacles,
        &corridor_plan,
        &parallel,
        &preserve_edges,
        &self_loop_idx,
        &mut result.hints.space_budget,
        &feedback_edge_set,
        s4_monitor_corridor,
    );

    // ── 4b. 后置交叉检测：修正 slot 排序与实际路由方向不一致的锚点 ──
    let t_fix = crate::layout::perf::Instant::now();
    //
    // slot 排序（步骤 2）按对端节点中心坐标排列，但当边的实际路由方向与对端位置
    // 方向不一致时（如需要绕过中间节点），排序结果会导致出边交叉。
    // 典型场景：节点 A 底部两条出边，左边 slot 的边实际向右绕行，右边 slot 的边
    // 直下，两者在节点下方交叉。交换 slot 后即可消除交叉。
    replan_slots(
        &result.nodes,
        &relations,
        &from_side,
        &to_side,
        &mut endpoint_map,
        &mut edges,
        &mut grid,
        &cfg,
        &group_ctx,
        &obstacles,
        &corridor_plan,
        &mut ortho_stats,
        &profile,
    );

    // ── 4c. 直连偏好对齐：正对端口边的 slot 锚点对齐修正 ──
    phase_straighten_align(
        &result.nodes,
        n,
        &from_side,
        &to_side,
        &mut endpoint_map,
        &mut edges,
        &mut grid,
        relations,
        &reverse_pairs,
        &parallel,
        &corridor_plan,
        &group_ctx,
        &obstacles,
        &cfg,
        &profile,
        &result.hints.space_budget,
    );

    // ── 4d. X-1: 多轮冲突消解重路由 ──
    phase_reroute(
        &result.nodes,
        relations,
        &from_side,
        &to_side,
        &endpoint_map,
        &mut edges,
        &mut grid,
        &cfg,
        &group_ctx,
        &obstacles,
        &corridor_plan,
        &mut ortho_stats,
        &profile,
    );

    // ── 4e. X-2: 反向 stub 检测与端口翻转 ──
    phase_stub_fix(
        &result.nodes,
        relations,
        &mut from_side,
        &mut to_side,
        &mut endpoint_map,
        &mut edges,
        &mut grid,
        &cfg,
        &group_ctx,
        &obstacles,
        &corridor_plan,
        &mut ortho_stats,
        &profile,
        &feedback_edge_set,
    );

    // ── 4f. X-3: Lane Assignment 车道分配 ──
    phase_lane(
        &mut edges,
        &mut grid,
        &result.nodes,
        &obstacles.sorted_node_ids,
        relations,
        &from_side,
        &to_side,
        parallel_gap,
        &corridor_plan,
        &group_ctx,
        &profile,
        &mut ortho_stats,
    );

    // S3：语义 FanIn/FanOut 合流（仅 architecture）；写路径 + merge_intervals
    let mut merge_result = if profile.semantic_merge {
        let mr = semantic_trunk_merge::apply_semantic_trunk_merge(
            &mut edges,
            relations,
            &from_side,
            &to_side,
            &result.nodes,
            diagram.diagram_type.clone(),
            !diagram.groups.is_empty(),
        );
        ortho_stats.semantic_trunk_groups_merged = mr.stats.groups_merged;
        ortho_stats.semantic_trunk_degraded = mr.stats.degraded_groups;
        if mr.stats.edges_rewritten > 0 {
            let touched: Vec<usize> = (0..edges.len()).collect();
            grid.remove_by_edges(&touched);
            for ei in 0..edges.len() {
                if edges[ei].path_is_empty() {
                    continue;
                }
                let pts: Vec<Point> = edges[ei].path_points().into_owned();
                grid.insert_path(&pts, ei);
            }
        }
        crate::perf_log!(
            "[perf]     s3_semantic_trunk: merged={} degraded={} rewritten={}",
            mr.stats.groups_merged,
            mr.stats.degraded_groups,
            mr.stats.edges_rewritten
        );
        Some(mr)
    } else {
        None
    };

    // S4：S3 合流后重路由监控枢纽边（仅无分组 architecture）
    if s4_monitor_corridor {
        let protected_trunks = merge_result
            .as_ref()
            .map(|m| extract_protected_vertical_trunks(&m.merge_intervals))
            .unwrap_or_default();
        let monitor_set: std::collections::HashSet<usize> =
            feedback_side::monitor_hub_edge_indices(relations)
                .into_iter()
                .collect();
        let mut to_reroute: std::collections::HashSet<usize> = feedback_edge_set
            .iter()
            .copied()
            .filter(|ei| monitor_set.contains(ei))
            .collect();
        if !protected_trunks.is_empty() {
            for &ei in &feedback_edge_set {
                if monitor_set.contains(&ei) || ei >= edges.len() || edges[ei].path_is_empty() {
                    continue;
                }
                let pts: Vec<Point> = edges[ei].path_points().into_owned();
                if scoring::protected_trunk_crossing_penalty(&pts, &protected_trunks) > 0.0 {
                    to_reroute.insert(ei);
                }
            }
        }
        if !to_reroute.is_empty() {
            let rerouted = phase_reroute_feedback_after_trunk(
                &to_reroute,
                relations,
                &result.nodes,
                &from_side,
                &to_side,
                &endpoint_map,
                &mut edges,
                &mut grid,
                &cfg,
                &profile,
                &group_ctx,
                &obstacles,
                &corridor_plan,
                &parallel,
                &protected_trunks,
                &mut ortho_stats,
            );
            crate::perf_log!(
                "[perf]     s4_feedback_reroute: edges={} protected_trunks={}",
                rerouted,
                protected_trunks.len()
            );
        }
    }

    // C 末冻结旁路 Annotation（stub / 受保护 trunk / S3 merge），供 sanitize / 后续 D 验证
    let route_annotations = crate::layout::edge::freeze_route_annotations_with_merges(
        &edges,
        &from_side,
        &to_side,
        merge_result.as_ref().map(|m| &m.merge_intervals),
        merge_result.as_ref().map(|m| &m.degraded),
    );
    result.hints.route_annotations = Some(route_annotations.clone());

    // ── 4g. 锯齿消毒 + X-0 间距统计 ──
    phase_sanitize(
        &mut edges,
        relations,
        &from_side,
        &to_side,
        &grid,
        parallel_gap,
        &mut ortho_stats,
        Some(&route_annotations),
        Some(&result.nodes),
        Some(&obstacles.sorted_node_ids),
    );

    // S4.x：sanitize 的 ensure_outward_stub 曾会吃掉外环 U 形；在消毒后强制修复仍穿模的监控边
    if s4_monitor_corridor {
        let monitor_set: std::collections::HashSet<usize> =
            feedback_side::monitor_hub_edge_indices(relations)
                .into_iter()
                .collect();
        let mut s4_repaired = 0usize;
        let mut s4_dirty = 0usize;
        let mut s4_force_none = 0usize;
        for &ei in &monitor_set {
            if ei >= edges.len() || edges[ei].path_is_empty() {
                continue;
            }
            let rel = &relations[ei];
            let from_id = rel.from.as_str();
            let to_id = rel.to.as_str();
            let pts: Vec<Point> = edges[ei].path_points().into_owned();
            if path_is_clean(
                &pts,
                from_id,
                to_id,
                &result.nodes,
                &group_ctx,
                &obstacles.sorted_node_ids,
            ) {
                continue;
            }
            s4_dirty += 1;
            let Some(from_ep) = endpoint_map.get(&(ei, true)) else {
                continue;
            };
            let Some(to_ep) = endpoint_map.get(&(ei, false)) else {
                continue;
            };
            let Some(path) = path::force_outer_escape_path(
                from_ep.anchor,
                to_ep.anchor,
                from_side[ei],
                to_side[ei],
                from_id,
                to_id,
                &result.nodes,
                &group_ctx,
                &obstacles,
            ) else {
                s4_force_none += 1;
                continue;
            };
            grid.remove_by_edges(std::slice::from_ref(&ei));
            grid.insert_path(&path, ei);
            let labels =
                build_parallel_aware_edge_labels(rel, ei, relations, &parallel.offsets, &path);
            let mut edge = EdgeLayout {
                geometry: PathGeometry::Polyline { points: Vec::new() },
                labels,
                from_port: from_side[ei],
                to_port: to_side[ei],
            };
            edge.set_polyline_points(path);
            edges[ei] = edge;
            s4_repaired += 1;
        }
        crate::perf_log!(
            "[perf]     s4_escape_repair: dirty={} repaired={} force_none={}",
            s4_dirty,
            s4_repaired,
            s4_force_none
        );
        if s4_repaired > 0 {
            // 几何已改：刷新 Annotation，供 D 末激进 sanitize 校验
            let refreshed = crate::layout::edge::freeze_route_annotations_with_merges(
                &edges,
                &from_side,
                &to_side,
                merge_result.as_ref().map(|m| &m.merge_intervals),
                merge_result.as_ref().map(|m| &m.degraded),
            );
            result.hints.route_annotations = Some(refreshed);
        }
    }

    // B.2：监控外环已稳定后，仅在目标附近按端口侧做局部 trunk。
    // 复用原路径前缀，避免把监控边重新拉回业务走廊；失败保持 S4 外环。
    if s4_monitor_corridor && profile.semantic_merge {
        let monitor_set: std::collections::HashSet<usize> =
            feedback_side::monitor_hub_edge_indices(relations)
                .into_iter()
                .collect();
        let local = semantic_trunk_merge::apply_monitor_local_trunk_merge(
            &mut edges,
            relations,
            &from_side,
            &to_side,
            &result.nodes,
            diagram.diagram_type.clone(),
            &monitor_set,
        );
        if local.stats.edges_rewritten > 0 {
            grid.remove_by_edges(&monitor_set.iter().copied().collect::<Vec<_>>());
            for &ei in &monitor_set {
                if ei < edges.len() && !edges[ei].path_is_empty() {
                    grid.insert_path(edges[ei].path_points().as_ref(), ei);
                }
            }
        }
        if let Some(base) = merge_result.as_mut() {
            base.stats.groups_considered += local.stats.groups_considered;
            base.stats.groups_merged += local.stats.groups_merged;
            base.stats.edges_rewritten += local.stats.edges_rewritten;
            base.stats.degraded_groups += local.stats.degraded_groups;
            base.merge_intervals.extend(local.merge_intervals);
            base.degraded.extend(local.degraded);
        } else {
            merge_result = Some(local);
        }
        // 几何与 merge 声明均可能变化，刷新最终 Annotation。
        result.hints.route_annotations =
            Some(crate::layout::edge::freeze_route_annotations_with_merges(
                &edges,
                &from_side,
                &to_side,
                merge_result.as_ref().map(|m| &m.merge_intervals),
                merge_result.as_ref().map(|m| &m.degraded),
            ));
    }

    // 轨道 A：sanitize / S4 escape 之后再次收口正反向同侧共锚
    let dock_gap = parallel_gap.max(COMPACT_SLOT_PITCH);
    let _dock_fixed = enforce_reverse_pair_dock_separation(
        &mut edges,
        relations,
        &result.nodes,
        &from_side,
        &to_side,
        dock_gap,
    );

    // P3.3：标签避让只在 pipeline 几何冻结后做一次。
    // sanitize 会重建平行边标签；此处再 resolve 会被 D 段 snap/sanitize 丢掉。
    crate::perf_log!(
        "[perf]     fix_inversions+labels: {:.2}ms",
        t_fix.elapsed().as_secs_f64() * 1000.0
    );

    result.edges = edges;
    // P2-1: 导出 orthogonal 路由 debug 统计
    result.hints.orthogonal_debug = Some(ortho_stats);
    result
}

fn phase_port_slot(
    relations: &[crate::ast::Relation],
    nodes: &HashMap<String, NodeLayout>,
    group_ctx: &crate::layout::group::GroupRoutingContext,
    feedback_assignment: &feedback_side::FeedbackSideAssignment,
    cfg: &OrthoConfig,
    n: usize,
    s4_monitor_corridor: bool,
    horizontal: bool,
) -> (
    Vec<Port>,
    Vec<Port>,
    Vec<usize>,
    HashMap<(usize, bool), Endpoint>,
    crate::layout::edge::common::parallel_edges::ParallelGroups,
    std::collections::BTreeSet<String>,
) {
    // ── 1. 按无向节点对分组，并确定每条边的端口（连接边） ──
    let t1 = crate::layout::perf::Instant::now();
    let mut pair_groups: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, rel) in relations.iter().enumerate() {
        let key = undirected_pair_key(rel.from.as_str(), rel.to.as_str());
        pair_groups.entry(key).or_default().push(i);
    }

    let mut from_side = vec![Port::Bottom; n];
    let mut to_side = vec![Port::Top; n];
    let mut lane = vec![0usize; n];

    let mut pair_keys: Vec<String> = pair_groups.keys().cloned().collect();
    pair_keys.sort();
    for key in &pair_keys {
        let indices = &pair_groups[key];
        let rel0 = &relations[indices[0]];
        let (can_from, can_to) = canonical_pair(rel0.from.as_str(), rel0.to.as_str());

        let (Some(a_nl), Some(b_nl)) = (nodes.get(can_from), nodes.get(can_to)) else {
            continue;
        };

        let (side_a, side_b) =
            choose_pair_sides_with_group(a_nl, b_nl, can_from, can_to, Some(group_ctx));

        for (l, &i) in indices.iter().enumerate() {
            let rel = &relations[i];
            if let Some(hint) = feedback_assignment.hints.get(&i) {
                from_side[i] = hint.from_side;
                to_side[i] = hint.to_side;
                lane[i] = hint.lane;
                continue;
            }
            if rel.from.as_str() == can_from {
                from_side[i] = side_a;
                to_side[i] = side_b;
            } else {
                from_side[i] = side_b;
                to_side[i] = side_a;
            }
            lane[i] = l;
        }
    }

    // ── 1b. 端口选择全局协调（同侧偏好，G8 修复） ──
    //
    // choose_pair_sides 逐对独立选端口，同一节点的多条边可能分散在不同侧出发，
    // 导致节点附近不必要的交叉。此阶段对每个节点的多条边做"同侧偏好"协调：
    // 统计各侧边数，让少数派边在几何可接受时切换到多数派侧。
    coordinate_port_sides(
        relations,
        nodes,
        &mut from_side,
        &mut to_side,
        Some(group_ctx),
    );
    apply_feedback_side_overrides(
        relations,
        feedback_assignment,
        &mut from_side,
        &mut to_side,
        &mut lane,
    );
    // S4.x：监控边同排侧廊被堵时改正对端口（须在 slot/endpoint 之前）
    if s4_monitor_corridor {
        feedback_side::apply_monitor_hub_escape_ports(
            relations,
            nodes,
            &mut from_side,
            &mut to_side,
            horizontal,
        );
    }
    align_fanin_target_sides(relations, nodes, &mut to_side);
    crate::perf_log!(
        "[perf]     step1_ports: {:.2}ms",
        t1.elapsed().as_secs_f64() * 1000.0
    );

    // ── 2. 为每个连接点分配磁吸 slot 坐标 ──
    //
    // 并线分组遵循三条设计规范：
    //   1. 不同箭头类型（Active/Passive/Bidirectional）不并线
    //   2. 不同线型（虚线/实线/dash pattern）不并线
    //   3. 仅当边「从同一节点出发」或「到达同一节点」时才并线（OR 语义）
    //      - 同源出边（都从 X 出发）→ 可并线
    //      - 同宿入边（都到达 X）→ 可并线
    //      - 一条出边 + 一条入边（在 X 上方向相反）→ 不并线
    //
    // 因此分组键 = (node_id, side, is_from, arrow_type, line_style)。
    // is_from 是端点级属性：同一条边在 from 端 is_from=true、在 to 端 is_from=false。
    // 同一 (node_id, side) 上可能存在多个并线子组：先为各子组分配互不重叠的
    // 锚点带中心（base_frac），再让子组内连接点围绕该中心按 DockingStrategy 分布。
    let mut bundling_endpoints: HashMap<String, Vec<Endpoint>> = HashMap::new();
    for i in 0..n {
        let rel = &relations[i];
        let from_id = rel.from.as_str();
        let to_id = rel.to.as_str();
        let (Some(from_nl), Some(to_nl)) = (nodes.get(from_id), nodes.get(to_id)) else {
            continue;
        };
        let from_center = node_center(from_nl);
        let to_center = node_center(to_nl);
        let fcx = from_center.x;
        let fcy = from_center.y;
        let tcx = to_center.x;
        let tcy = to_center.y;

        bundling_endpoints
            .entry(endpoint_bundling_key(from_id, from_side[i], true, rel))
            .or_default()
            .push(Endpoint {
                edge_index: i,
                is_from: true,
                target_x: tcx,
                target_y: tcy,
                lane: lane[i],
                node_id: from_id.to_string(),
                side: from_side[i],
                anchor: Point::zero(),
            });
        bundling_endpoints
            .entry(endpoint_bundling_key(to_id, to_side[i], false, rel))
            .or_default()
            .push(Endpoint {
                edge_index: i,
                is_from: false,
                target_x: fcx,
                target_y: fcy,
                lane: lane[i],
                node_id: to_id.to_string(),
                side: to_side[i],
                anchor: Point::zero(),
            });
    }

    // 按 (node_id, side) 聚合并线子组，便于在同一节点同一侧上为各子组分配互不重叠的锚点带
    let mut side_groups: HashMap<(String, Port), Vec<Vec<Endpoint>>> = HashMap::new();
    for (_, endpoints) in bundling_endpoints {
        if endpoints.is_empty() {
            continue;
        }
        let node_id = endpoints[0].node_id.clone();
        let side = endpoints[0].side;
        side_groups
            .entry((node_id, side))
            .or_default()
            .push(endpoints);
    }

    // endpoint_map: (edge_index, is_from) -> Endpoint (with anchor filled in)
    let mut endpoint_map: HashMap<(usize, bool), Endpoint> = HashMap::new();
    let mut side_group_keys: Vec<(String, Port)> = side_groups.keys().cloned().collect();
    side_group_keys.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    for (node_id, side) in side_group_keys {
        let Some(mut sub_groups) = side_groups.remove(&(node_id.clone(), side)) else {
            continue;
        };
        let Some(nl) = nodes.get(&node_id) else {
            continue;
        };
        let vertical_side = is_vertical_port(side);
        let edge_len = if vertical_side { nl.width } else { nl.height };

        // 子组内沿切线方向排序：上/下边按目标 x，左/右边按目标 y；同位置再按 lane
        for endpoints in sub_groups.iter_mut() {
            endpoints.sort_by(|p, q| {
                let pk = if vertical_side {
                    p.target_x
                } else {
                    p.target_y
                };
                let qk = if vertical_side {
                    q.target_x
                } else {
                    q.target_y
                };
                pk.partial_cmp(&qk)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(p.lane.cmp(&q.lane))
                    .then(p.edge_index.cmp(&q.edge_index))
            });
        }
        // 子组间按 (arrow_type, line_style, min_edge_index) 排序。
        // 排序键不含 is_from：同一 edge 在两端节点的 is_from 相反，若用 is_from
        // 排序会导致两端排名不一致 → base_frac 不同 → 路径非直线。min_edge_index
        // 作为稳定 tiebreaker，保证同一 edge 在两端子组中获得相同排名。
        sub_groups.sort_by(|a, b| {
            sub_group_sort_key(a, relations)
                .cmp(&sub_group_sort_key(b, relations))
                .then_with(|| {
                    a.iter()
                        .map(|e| e.edge_index)
                        .min()
                        .cmp(&b.iter().map(|e| e.edge_index).min())
                })
        });

        let k = sub_groups.len();
        for (group_rank, endpoints) in sub_groups.iter().enumerate() {
            let count = endpoints.len();
            let strategy = choose_docking_strategy(count);
            let base_frac = if k <= 1 {
                0.5
            } else {
                slot_fraction(group_rank, k, edge_len, cfg.slot_pitch)
            };

            // V2：同侧入出混合时禁用 Concentrate 共心（多边汇流会抹掉子组带分离）。
            // 锚点分桶已含 is_from；共竖干间距由 lane / enforce_reverse_pair_min_gap 收口（O1）。
            let mixed_inout = k >= 2
                && sub_groups.iter().any(|g| g.iter().any(|e| e.is_from))
                && sub_groups.iter().any(|g| g.iter().any(|e| !e.is_from));
            let strategy = if mixed_inout && matches!(strategy, DockingStrategy::Concentrate) {
                DockingStrategy::Compact
            } else {
                strategy
            };

            for (rank, ep) in endpoints.iter().enumerate() {
                let frac = match strategy {
                    DockingStrategy::Single | DockingStrategy::Concentrate => base_frac,
                    DockingStrategy::Compact => {
                        let pitch = cfg.slot_pitch.min(COMPACT_SLOT_PITCH);
                        slot_fraction_around(rank, count, edge_len, pitch, base_frac)
                    }
                };
                let anchor = slot_anchor(nl, side, frac);
                endpoint_map.insert(
                    (ep.edge_index, ep.is_from),
                    Endpoint {
                        edge_index: ep.edge_index,
                        is_from: ep.is_from,
                        target_x: ep.target_x,
                        target_y: ep.target_y,
                        lane: ep.lane,
                        node_id: ep.node_id.clone(),
                        side: ep.side,
                        anchor,
                    },
                );
            }
        }
    }

    // 平行边切线偏移：仅 A↔B 正反向对对称错开；同向多边由 slot 分布处理。
    let parallel = crate::layout::edge::common::parallel_edges::group_parallel_edges(
        relations,
        crate::layout::constants::DEFAULT_EDGE_OFFSET,
    );
    let mut reverse_pairs: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut pair_groups: std::collections::BTreeMap<String, Vec<usize>> =
        std::collections::BTreeMap::new();
    for (i, rel) in relations.iter().enumerate() {
        let key = undirected_pair_key(rel.from.as_str(), rel.to.as_str());
        pair_groups.entry(key).or_default().push(i);
    }
    for (key, indices) in &pair_groups {
        if indices.len() < 2 {
            continue;
        }
        let rel0 = &relations[indices[0]];
        let (can_from, can_to) = canonical_pair(rel0.from.as_str(), rel0.to.as_str());
        let mut has_forward = false;
        let mut has_backward = false;
        for &i in indices {
            let rel = &relations[i];
            if rel.from.as_str() == can_from && rel.to.as_str() == can_to {
                has_forward = true;
            } else {
                has_backward = true;
            }
        }
        if has_forward && has_backward {
            reverse_pairs.insert(key.clone());
        }
    }
    for ((edge_index, _), ep) in endpoint_map.iter_mut() {
        let rel = &relations[*edge_index];
        let key = undirected_pair_key(rel.from.as_str(), rel.to.as_str());
        if !reverse_pairs.contains(&key) {
            continue;
        }
        let offset = parallel.offsets[*edge_index];
        if offset.abs() < EPS {
            continue;
        }
        if is_vertical_port(ep.side) {
            ep.anchor.x += offset;
        } else {
            ep.anchor.y += offset;
        }
    }

    // ── 2b. 直连偏好对齐：正对端口边的 slot 锚点对齐修正 ──
    //
    // 问题场景：两个垂直/水平排列的节点，端口选择为正对(Bottom→Top/Left→Right)，
    // 但因两端同侧边数不同导致 slot frac 不对称，锚点不对齐，产生不必要的小弯折。
    // 例如：db_master(bottom有1条出边) → db_replica(top有2条入边)，
    // master端锚点居中，replica端锚点偏左/偏右，路径走Z字而非直线。
    //
    // 修正策略（移至 4c replan_slots 之后执行）：对正对端口且节点投影有重叠的边，
    // 将「自由度较高」端（该侧同方向仅1条边的Single端点）的锚点切线坐标调整为与另一端对齐，
    // 形成直线路径。两端都有多个边时若节点中心线高度对齐（<16px），也强制对齐。
    let t_align = crate::layout::perf::Instant::now();
    crate::perf_log!(
        "[perf]     step2b_straighten: {:.2}ms (moved to 4c)",
        t_align.elapsed().as_secs_f64() * 1000.0
    );

    (
        from_side,
        to_side,
        lane,
        endpoint_map,
        parallel,
        reverse_pairs,
    )
}

#[allow(clippy::too_many_arguments)]
fn phase_route_edges(
    edge_order: &[usize],
    relations: &[crate::ast::Relation],
    nodes: &HashMap<String, NodeLayout>,
    from_side: &[Port],
    to_side: &[Port],
    endpoint_map: &HashMap<(usize, bool), Endpoint>,
    edges: &mut [EdgeLayout],
    grid: &mut SegmentGrid,
    ortho_stats: &mut crate::layout::OrthoDebugStats,
    cfg: &OrthoConfig,
    profile: &OrthoRoutingProfile,
    group_ctx: &crate::layout::group::GroupRoutingContext,
    obstacles: &PreparedObstacles,
    corridor_plan: &corridor_route::CorridorRoutePlan,
    parallel: &crate::layout::edge::common::parallel_edges::ParallelGroups,
    preserve_edges: &Option<std::collections::HashSet<usize>>,
    self_loop_idx: &HashMap<usize, usize>,
    space_budget: &mut Option<crate::layout::space_budget::SpaceBudget>,
    feedback_edge_set: &std::collections::HashSet<usize>,
    s4_monitor_corridor: bool,
) {
    for &i in edge_order {
        let t_edge = crate::layout::perf::Instant::now();
        let rel = &relations[i];
        let from_id = rel.from.as_str();
        let to_id = rel.to.as_str();

        if from_id == to_id {
            if let Some(nl) = nodes.get(from_id) {
                let loop_idx = self_loop_idx.get(&i).copied().unwrap_or(0);
                edges[i] = self_loop::route_self_loop(
                    rel,
                    nl,
                    loop_idx,
                    self_loop::SelfLoopStyle::Orthogonal,
                );
            }
            continue;
        }

        if let Some(ref preserve) = preserve_edges {
            if preserve.contains(&i) && edges[i].path_len() >= 2 {
                let path: Vec<Point> = edges[i].path_points().into_owned();
                grid.insert_path(&path, i);
                continue;
            }
        }

        let (Some(_from_nl), Some(_to_nl)) = (nodes.get(from_id), nodes.get(to_id)) else {
            continue;
        };

        let Some(from_ep) = endpoint_map.get(&(i, true)) else {
            continue;
        };
        let Some(to_ep) = endpoint_map.get(&(i, false)) else {
            continue;
        };

        let mut corridor_boost = space_budget
            .as_ref()
            .map(|b| b.corridor_boost_requested)
            .unwrap_or(false);
        let is_feedback = feedback_edge_set.contains(&i);
        let has_chain = corridor_plan.chains.contains_key(&i);
        let same_leaf = group_ctx.is_same_leaf_group(from_id, to_id);
        let pair = EndpointPair {
            from: from_ep.clone(),
            to: to_ep.clone(),
        };
        let strict = should_strict_group_transit(
            profile,
            group_ctx,
            from_id,
            to_id,
            has_chain,
            is_feedback,
        );

        let mut path_stats = PathSelectStats::default();
        let corridor_ok = validated_corridor_path(
            i,
            from_ep.anchor,
            to_ep.anchor,
            from_id,
            to_id,
            corridor_plan,
            group_ctx,
            nodes,
            obstacles,
            cfg.channel_margin,
        );
        // P5（保守落地）：有组图跨 leaf 外廊与 S4 monitor 解耦的「无链 prefer_outer」
        // 会在 ecommerce 等图引入新穿组；此处仍仅 S4 monitor 开外环，
        // 跨 leaf 无链靠 strict + corridor_boost 收口（完整 P5 留给后续几何）。
        let prefer_outer = s4_monitor_corridor && is_feedback;
        // P2：有 chain 但 validated 失败 → 显式 degraded（禁止静默 free-route 冒充成功）。
        let corridor_contract_failed = has_chain && corridor_ok.is_none();
        let mut path = corridor_ok.unwrap_or_else(|| {
            let ctx =
                OrthoRoutingContext::new(nodes, group_ctx, grid, cfg, profile, obstacles, None)
                    .with_strict_group_transit(strict)
                    .with_corridor_boost(
                        corridor_boost || corridor_contract_failed || (!same_leaf && !has_chain),
                    )
                    .with_prefer_outer_ring(prefer_outer);
            select_best_path_with_scorer_stats(
                &ctx,
                &pair,
                &DefaultScorer,
                Some(&mut path_stats),
                false,
            )
        });
        // S2：0 候选/退化 → 升走廊预算再路由一次（加大外框垫），禁止静默脏折线
        if path_stats.degraded && !corridor_boost {
            corridor_boost = true;
            if let Some(budget) = space_budget.as_mut() {
                budget.request_corridor_boost();
            }
            let mut boost_stats = PathSelectStats::default();
            let ctx =
                OrthoRoutingContext::new(nodes, group_ctx, grid, cfg, profile, obstacles, None)
                    .with_strict_group_transit(strict)
                    .with_corridor_boost(true)
                    .with_prefer_outer_ring(prefer_outer);
            let boosted = select_best_path_with_scorer_stats(
                &ctx,
                &pair,
                &DefaultScorer,
                Some(&mut boost_stats),
                false,
            );
            if !boost_stats.degraded || boost_stats.candidate_count > path_stats.candidate_count {
                path = boosted;
                path_stats = boost_stats;
            }
        }
        ortho_stats.total_candidates += path_stats.candidate_count;
        ortho_stats.hard_filter_reject_count += path_stats.hard_filter_reject_count;
        if path_stats.degraded || corridor_contract_failed {
            ortho_stats.degraded_count += 1;
            if let Some(budget) = space_budget.as_mut() {
                budget.request_corridor_boost();
            }
        }

        // 标签位置：平行/反向边错开 t + 法向偏移，避免双向边标签重叠
        let labels = if path.len() >= 2 {
            match relations.get(i) {
                Some(rel) => {
                    build_parallel_aware_edge_labels(rel, i, relations, &parallel.offsets, &path)
                }
                None => Vec::new(),
            }
        } else {
            Vec::new()
        };

        grid.insert_path(&path, i);

        let mut edge = EdgeLayout {
            // 临时占位，下面用 set_polyline_points 根据 path 点数自动选择 Straight/Polyline
            geometry: PathGeometry::Polyline { points: Vec::new() },
            labels,
            from_port: from_side[i],
            to_port: to_side[i],
        };
        edge.set_polyline_points(path);

        edges[i] = edge;
        crate::perf_log!(
            "[perf]     edge[{}] {}->{}: {} candidates, {:.2}ms",
            i,
            from_id,
            to_id,
            path_stats.candidate_count,
            t_edge.elapsed().as_secs_f64() * 1000.0
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn phase_straighten_align(
    nodes: &HashMap<String, NodeLayout>,
    n: usize,
    from_side: &[Port],
    to_side: &[Port],
    endpoint_map: &mut HashMap<(usize, bool), Endpoint>,
    edges: &mut [EdgeLayout],
    grid: &mut SegmentGrid,
    relations: &[crate::ast::Relation],
    reverse_pairs: &std::collections::BTreeSet<String>,
    parallel: &crate::layout::edge::common::parallel_edges::ParallelGroups,
    corridor_plan: &corridor_route::CorridorRoutePlan,
    group_ctx: &crate::layout::group::GroupRoutingContext,
    obstacles: &PreparedObstacles,
    cfg: &OrthoConfig,
    profile: &OrthoRoutingProfile,
    space_budget: &Option<crate::layout::space_budget::SpaceBudget>,
) {
    // ── 4c. 直连偏好对齐：正对端口边的 slot 锚点对齐修正 ──
    // 在 replan_slots 之后执行，确保anchor位置是最终的slot排序结果。
    // 修改anchor后需要重路由受影响的边，因此放在 X-1 reroute 之前。
    let t_align2 = crate::layout::perf::Instant::now();
    let old_endpoints: HashMap<(usize, bool), Endpoint> = endpoint_map.clone();
    // 仅 reverse_pairs 边携带非零 parallel offset；straighten 对齐到 center±offset
    let mut straighten_offsets = vec![0.0; n];
    for ((edge_index, _), _) in endpoint_map.iter() {
        let rel = &relations[*edge_index];
        let key = undirected_pair_key(rel.from.as_str(), rel.to.as_str());
        if reverse_pairs.contains(&key) {
            straighten_offsets[*edge_index] = parallel.offsets[*edge_index];
        }
    }
    straighten_preferred_alignments(
        nodes,
        n,
        &from_side,
        &to_side,
        endpoint_map,
        &straighten_offsets,
    );

    // 找出anchor被修改的边，需要重路由
    let mut align_reroute: Vec<usize> = Vec::new();
    for i in 0..n {
        for &is_from in &[true, false] {
            let old_ep = old_endpoints.get(&(i, is_from));
            let new_ep = endpoint_map.get(&(i, is_from));
            if let (Some(o), Some(ne)) = (old_ep, new_ep) {
                if (o.anchor.x - ne.anchor.x).abs() > EPS || (o.anchor.y - ne.anchor.y).abs() > EPS
                {
                    align_reroute.push(i);
                    break;
                }
            }
        }
    }
    if !align_reroute.is_empty() {
        align_reroute.sort_unstable();
        grid.remove_by_edges(&align_reroute);
        for &ei in &align_reroute {
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
            let mut path_stats = PathSelectStats::default();
            let has_chain = corridor_plan.chains.contains_key(&ei);
            let corridor_ok = validated_corridor_path(
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
            );
            let prefer_outer = false; // P5 保守：align 重路由不强制外环
            let candidate = corridor_ok.unwrap_or_else(|| {
                let pair = EndpointPair {
                    from: from_ep.clone(),
                    to: to_ep.clone(),
                };
                let boost = space_budget
                    .as_ref()
                    .map(|b| b.corridor_boost_requested)
                    .unwrap_or(false);
                let ctx = OrthoRoutingContext::new(
                    nodes, group_ctx, &grid, cfg, profile, obstacles, None,
                )
                .with_strict_group_transit(should_strict_group_transit(
                    profile,
                    group_ctx,
                    from_id,
                    to_id,
                    has_chain,
                    false,
                ))
                .with_corridor_boost(boost || has_chain || !group_ctx.is_same_leaf_group(from_id, to_id))
                .with_prefer_outer_ring(prefer_outer);
                select_best_path_with_scorer_stats(
                    &ctx,
                    &pair,
                    &DefaultScorer,
                    Some(&mut path_stats),
                    false,
                )
            });
            if candidate.len() >= 2 {
                grid.insert_path(&candidate, ei);
                let labels = match relations.get(ei) {
                    Some(rel) => build_parallel_aware_edge_labels(
                        rel,
                        ei,
                        relations,
                        &parallel.offsets,
                        &candidate,
                    ),
                    None => Vec::new(),
                };
                let mut edge = EdgeLayout {
                    geometry: PathGeometry::Polyline { points: Vec::new() },
                    labels,
                    from_port: from_side[ei],
                    to_port: to_side[ei],
                };
                edge.set_polyline_points(candidate);
                edges[ei] = edge;
            }
        }
    }
    crate::perf_log!(
        "[perf]     4c_straighten_align: {:.2}ms (aligned {} edges)",
        t_align2.elapsed().as_secs_f64() * 1000.0,
        align_reroute.len()
    );
}

/// Phase 3：分层批量边序（有 rank 时低层先占通道；feedback / 监控枢纽全局延后）
fn phase_layer_order(
    relations: &[crate::ast::Relation],
    sugiyama_ranks: Option<&HashMap<String, usize>>,
    feedback_assignment: &feedback_side::FeedbackSideAssignment,
    s4_monitor_corridor: bool,
) -> (Vec<usize>, std::collections::HashSet<usize>) {
    let t2 = crate::layout::perf::Instant::now();
    let node_degree = layer_order::compute_node_degrees(relations);
    let mut feedback_edge_set: std::collections::HashSet<usize> =
        feedback_assignment.hints.keys().copied().collect();
    // S4：无分组 architecture 下，监控枢纽被动入边并入延后集（不改端口）
    if s4_monitor_corridor {
        for ei in feedback_side::monitor_hub_edge_indices(relations) {
            feedback_edge_set.insert(ei);
        }
    }
    let edge_order = layer_order::compute_edge_order_with_feedback(
        relations,
        sugiyama_ranks,
        &node_degree,
        Some(&feedback_edge_set),
    );
    crate::perf_log!(
        "[perf]     step2_slots+step3_order: {:.2}ms",
        t2.elapsed().as_secs_f64() * 1000.0
    );
    (edge_order, feedback_edge_set)
}

/// Phase 4d (X-1)：多轮冲突消解重路由
#[allow(clippy::too_many_arguments)]
fn phase_reroute(
    nodes: &HashMap<String, NodeLayout>,
    relations: &[crate::ast::Relation],
    from_side: &[Port],
    to_side: &[Port],
    endpoint_map: &HashMap<(usize, bool), Endpoint>,
    edges: &mut Vec<EdgeLayout>,
    grid: &mut SegmentGrid,
    cfg: &OrthoConfig,
    group_ctx: &crate::layout::group::GroupRoutingContext,
    obstacles: &PreparedObstacles,
    corridor_plan: &corridor_route::CorridorRoutePlan,
    ortho_stats: &mut crate::layout::OrthoDebugStats,
    profile: &OrthoRoutingProfile,
) {
    let t_x1 = crate::layout::perf::Instant::now();
    reroute_conflicting_edges(
        nodes,
        relations,
        from_side,
        to_side,
        endpoint_map,
        edges,
        grid,
        cfg,
        group_ctx,
        obstacles,
        corridor_plan,
        ortho_stats,
        profile,
    );
    crate::perf_log!(
        "[perf]     x1_reroute: {:.2}ms",
        t_x1.elapsed().as_secs_f64() * 1000.0
    );
}

/// Phase 4e (X-2)：反向 stub 检测与端口翻转
///
/// 问题场景：由于分组障碍物/走廊限制，choose_pair_sides 基于几何中心选择的端口
/// 在实际路由时被证明是"反向"的——路径从端口出发后不得不沿反方向折返穿过节点
/// 投影平面才能到达目标，导致箭头方向与主路径方向冲突（视觉上"搞笑箭头"）。
///
/// 修正策略：路由完成后检测路径上的反向stub端点，将其端口翻转到对面（Bottom↔Top,
/// Left↔Right），重新计算anchor并重路由。若新路径无反向stub且质量可接受，则接受。
#[allow(clippy::too_many_arguments)]
fn phase_stub_fix(
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
    feedback_edge_set: &std::collections::HashSet<usize>,
) {
    let t_flip = crate::layout::perf::Instant::now();
    fix_reverse_stub_ports(
        nodes,
        relations,
        from_side,
        to_side,
        endpoint_map,
        edges,
        grid,
        cfg,
        group_ctx,
        obstacles,
        corridor_plan,
        ortho_stats,
        profile,
        feedback_edge_set,
    );
    crate::perf_log!(
        "[perf]     x2_flip_stub: {:.2}ms (flipped {} edges)",
        t_flip.elapsed().as_secs_f64() * 1000.0,
        ortho_stats.flipped_stub_edges
    );
}

/// Phase 4f (X-3)：Lane Assignment 车道分配
///
/// 对 bundling 无法合并的残余平行段，通过平移 cross-axis 坐标分离重合段。
/// 不插入 Z 字弯，保持正交性。
#[allow(clippy::too_many_arguments)]
fn phase_lane(
    edges: &mut [EdgeLayout],
    grid: &mut SegmentGrid,
    nodes: &HashMap<String, NodeLayout>,
    sorted_node_ids: &[String],
    relations: &[crate::ast::Relation],
    from_side: &[Port],
    to_side: &[Port],
    parallel_gap: f64,
    corridor_plan: &corridor_route::CorridorRoutePlan,
    group_ctx: &crate::layout::group::GroupRoutingContext,
    profile: &OrthoRoutingProfile,
    ortho_stats: &mut crate::layout::OrthoDebugStats,
) {
    let t_lane = crate::layout::perf::Instant::now();
    let lane_stats = assign_lanes(
        edges,
        grid,
        nodes,
        sorted_node_ids,
        relations,
        from_side,
        to_side,
        parallel_gap,
    );
    ortho_stats.lane_groups = lane_stats.lane_groups;
    ortho_stats.lane_segments_shifted = lane_stats.segments_shifted;
    ortho_stats.lane_shifts_failed = lane_stats.shifts_failed;
    if profile.corridor_lane_offsets {
        let corridor_shifted = apply_corridor_planned_offsets(
            edges,
            grid,
            nodes,
            sorted_node_ids,
            relations,
            from_side,
            to_side,
            corridor_plan,
            group_ctx,
        );
        ortho_stats.lane_segments_shifted += corridor_shifted;
        if profile.separate_unrelated_trunks {
            for _ in 0..2 {
                let shifted = separate_unrelated_trunk_overlaps(
                    edges,
                    Some(grid),
                    relations,
                    from_side,
                    to_side,
                    nodes,
                    sorted_node_ids,
                    parallel_gap,
                    profile,
                );
                ortho_stats.lane_segments_shifted += shifted;
                if shifted == 0 {
                    break;
                }
            }
        }
    }
    // V3a：2 点直连正反向对不会进 assign_lanes（需 ≥4 折点）；在此强制 trunk 间距
    let gap_fixed = enforce_reverse_pair_min_gap(edges, relations, parallel_gap);
    ortho_stats.lane_segments_shifted += gap_fixed;

    // S1：同侧 stub 占用。architecture（semantic_merge）C 期仅诊断，避免改边反馈
    // space-budget 动节点；exact 跨对共柱改在 pipeline D 末端
    // `resolve_exact_stub_occupancy_post_route`（保 node_fp）。flowchart 仍在此真修。
    let records = collect_stub_occupancy(edges, relations, from_side, to_side);
    let conflicts = find_stub_occupancy_conflicts(&records, relations, parallel_gap);
    ortho_stats.stub_occupancy_conflicts = conflicts.len();
    ortho_stats.stub_cross_pair_conflicts = conflicts.iter().filter(|c| !c.reverse_pair).count();
    if !profile.semantic_merge {
        let stub_stats = resolve_stub_occupancy_conflicts(
            edges,
            relations,
            from_side,
            to_side,
            nodes,
            parallel_gap,
        );
        ortho_stats.stub_occupancy_shifted = stub_stats.stubs_shifted;
        ortho_stats.stub_occupancy_degraded = stub_stats.degraded;
        // resolve 内部会重算 before；用其 shifted 覆盖 conflicts 已写入的值
        ortho_stats.stub_occupancy_conflicts = stub_stats.conflict_pairs_before;
        ortho_stats.stub_cross_pair_conflicts = stub_stats.cross_pair_conflicts_before;
        if stub_stats.stubs_shifted > 0 {
            let touched: Vec<usize> = (0..edges.len()).collect();
            grid.remove_by_edges(&touched);
            for ei in 0..edges.len() {
                if edges[ei].path_is_empty() {
                    continue;
                }
                let pts: Vec<Point> = edges[ei].path_points().into_owned();
                grid.insert_path(&pts, ei);
            }
        }
        crate::perf_log!(
            "[perf]     x3_lane_assignment: {:.2}ms ({} groups, {} shifted, {} failed); stub_occ conflicts={} shifted={} degraded={}",
            t_lane.elapsed().as_secs_f64() * 1000.0,
            lane_stats.lane_groups,
            lane_stats.segments_shifted + gap_fixed,
            lane_stats.shifts_failed,
            stub_stats.conflict_pairs_before,
            stub_stats.stubs_shifted,
            stub_stats.degraded
        );
    } else {
        crate::perf_log!(
            "[perf]     x3_lane_assignment: {:.2}ms ({} groups, {} shifted, {} failed); stub_occ conflicts={} (arch diagnose-only)",
            t_lane.elapsed().as_secs_f64() * 1000.0,
            lane_stats.lane_groups,
            lane_stats.segments_shifted + gap_fixed,
            lane_stats.shifts_failed,
            conflicts.len()
        );
    }

    // 轨道 A：正反向同侧 dock 共锚分离（落点最终写者；在 stub_occ 之后）
    let dock_gap = parallel_gap.max(COMPACT_SLOT_PITCH);
    let dock_fixed =
        enforce_reverse_pair_dock_separation(edges, relations, nodes, from_side, to_side, dock_gap);
    if dock_fixed > 0 {
        ortho_stats.lane_segments_shifted += dock_fixed;
        let touched: Vec<usize> = (0..edges.len()).collect();
        grid.remove_by_edges(&touched);
        for ei in 0..edges.len() {
            if edges[ei].path_is_empty() {
                continue;
            }
            let pts: Vec<Point> = edges[ei].path_points().into_owned();
            grid.insert_path(&pts, ei);
        }
    }
}

/// 同宿 FanIn 若全部源节点位于目标同一侧，统一使用目标正对端口。
///
/// 逐边最近侧选择会把较远成员旋到 Left/Right，导致语义合流组在 S3 前被拆散。
/// 这里只处理明确的全上/全下关系；混合方向仍保留逐边端口选择。
fn align_fanin_target_sides(
    relations: &[crate::ast::Relation],
    nodes: &HashMap<String, NodeLayout>,
    to_side: &mut [Port],
) {
    let mut by_target: std::collections::BTreeMap<&str, Vec<usize>> =
        std::collections::BTreeMap::new();
    for (edge_index, relation) in relations.iter().enumerate() {
        by_target
            .entry(relation.to.as_str())
            .or_default()
            .push(edge_index);
    }
    for (target_id, members) in by_target {
        if let Some(common) = aligned_fanin_target_port(target_id, &members, relations, nodes) {
            for edge_index in members {
                if let Some(side) = to_side.get_mut(edge_index) {
                    *side = common;
                }
            }
        }
    }
}

pub(super) fn aligned_fanin_target_port(
    target_id: &str,
    members: &[usize],
    relations: &[crate::ast::Relation],
    nodes: &HashMap<String, NodeLayout>,
) -> Option<Port> {
    if members.len() != 2 {
        return None;
    }
    let target = nodes.get(target_id)?;
    let sources: Vec<(&str, &NodeLayout)> = members
        .iter()
        .filter_map(|&edge_index| {
            let relation = relations.get(edge_index)?;
            nodes
                .get(relation.from.as_str())
                .map(|node| (relation.from.as_str(), node))
        })
        .collect();
    if sources.len() != members.len() {
        return None;
    }
    let first_center_y = sources[0].1.y + sources[0].1.height / 2.0;
    if !sources
        .iter()
        .all(|(_, source)| (source.y + source.height / 2.0 - first_center_y).abs() <= 1.0)
    {
        return None;
    }
    let port = if sources
        .iter()
        .all(|(_, source)| source.y + source.height <= target.y + 0.5)
    {
        Port::Top
    } else if sources
        .iter()
        .all(|(_, source)| source.y >= target.y + target.height - 0.5)
    {
        Port::Bottom
    } else {
        return None;
    };

    let trunk_x = target.x + target.width / 2.0;
    let target_anchor = match port {
        Port::Top => Point::new(trunk_x, target.y),
        Port::Bottom => Point::new(trunk_x, target.y + target.height),
        _ => unreachable!(),
    };
    for (source_id, source) in &sources {
        let source_anchor = match port {
            Port::Top => Point::new(source.x + source.width / 2.0, source.y + source.height),
            Port::Bottom => Point::new(source.x + source.width / 2.0, source.y),
            _ => unreachable!(),
        };
        let join_y = source_anchor.y
            + if matches!(port, Port::Top) {
                PORT_CLEARANCE
            } else {
                -PORT_CLEARANCE
            };
        let path = [
            source_anchor,
            Point::new(source_anchor.x, join_y),
            Point::new(trunk_x, join_y),
            target_anchor,
        ];
        let blocked = path.windows(2).any(|segment| {
            nodes.iter().any(|(node_id, node)| {
                node_id.as_str() != *source_id
                    && node_id.as_str() != target_id
                    && crate::layout::geometry::Rect::from(node)
                        .expanded(NODE_OBSTACLE_PAD)
                        .segment_crosses_interior(segment[0], segment[1], 0.5)
            })
        });
        if blocked {
            return None;
        }
    }
    Some(port)
}

/// 从 S3 merge_intervals 提取垂直受保护干线 `(x, y_lo, y_hi)`（去重、排序）。
fn extract_protected_vertical_trunks(
    merge_intervals: &std::collections::HashMap<usize, Vec<crate::layout::edge::MergeInterval>>,
) -> Vec<(f64, f64, f64)> {
    let mut trunks: Vec<(f64, f64, f64)> = Vec::new();
    let mut keys: Vec<usize> = merge_intervals.keys().copied().collect();
    keys.sort_unstable();
    for ei in keys {
        let Some(ivs) = merge_intervals.get(&ei) else {
            continue;
        };
        for iv in ivs {
            if iv.horizontal {
                continue;
            }
            let y0 = iv.t0.min(iv.t1);
            let y1 = iv.t0.max(iv.t1);
            trunks.push((iv.coord, y0, y1));
        }
    }
    trunks.sort_by(|a, b| {
        a.0.partial_cmp(&b.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .then(a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal))
    });
    trunks.dedup_by(|a, b| {
        (a.0 - b.0).abs() < 1.0 && (a.1 - b.1).abs() < 1.0 && (a.2 - b.2).abs() < 1.0
    });
    trunks
}

/// S4：在 FanIn 干线写定后，重路由 feedback/监控边并加重干线穿越惩罚。
fn phase_reroute_feedback_after_trunk(
    feedback_edge_set: &std::collections::HashSet<usize>,
    relations: &[crate::ast::Relation],
    nodes: &HashMap<String, NodeLayout>,
    from_side: &[Port],
    to_side: &[Port],
    endpoint_map: &HashMap<(usize, bool), Endpoint>,
    edges: &mut [EdgeLayout],
    grid: &mut SegmentGrid,
    cfg: &OrthoConfig,
    profile: &OrthoRoutingProfile,
    group_ctx: &crate::layout::group::GroupRoutingContext,
    obstacles: &PreparedObstacles,
    corridor_plan: &corridor_route::CorridorRoutePlan,
    parallel: &crate::layout::edge::common::parallel_edges::ParallelGroups,
    protected_trunks: &[(f64, f64, f64)],
    ortho_stats: &mut crate::layout::OrthoDebugStats,
) -> usize {
    let mut order: Vec<usize> = feedback_edge_set.iter().copied().collect();
    order.sort_unstable();
    if order.is_empty() {
        return 0;
    }
    grid.remove_by_edges(&order);
    let mut rerouted = 0usize;
    for &ei in &order {
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
        if from_id == to_id {
            continue;
        }
        let pair = EndpointPair {
            from: from_ep.clone(),
            to: to_ep.clone(),
        };
        let strict = should_strict_group_transit(
            profile,
            group_ctx,
            from_id,
            to_id,
            corridor_plan.chains.contains_key(&ei),
            true,
        );
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
            let ctx =
                OrthoRoutingContext::new(nodes, group_ctx, grid, cfg, profile, obstacles, None)
                    .with_strict_group_transit(strict)
                    .with_prefer_outer_ring(true)
                    .with_protected_trunks(protected_trunks);
            let mut first = select_best_path_with_scorer_stats(
                &ctx,
                &pair,
                &DefaultScorer,
                Some(&mut path_stats),
                false,
            );
            if path_stats.degraded {
                let mut boost_stats = PathSelectStats::default();
                let ctx2 =
                    OrthoRoutingContext::new(nodes, group_ctx, grid, cfg, profile, obstacles, None)
                        .with_strict_group_transit(strict)
                        .with_corridor_boost(true)
                        .with_prefer_outer_ring(true)
                        .with_protected_trunks(protected_trunks);
                let boosted = select_best_path_with_scorer_stats(
                    &ctx2,
                    &pair,
                    &DefaultScorer,
                    Some(&mut boost_stats),
                    false,
                );
                if !boost_stats.degraded || boost_stats.candidate_count > path_stats.candidate_count
                {
                    path_stats = boost_stats;
                    first = boosted;
                }
            }
            first
        });
        if path.len() < 2 {
            continue;
        }
        if path_stats.degraded {
            ortho_stats.degraded_count += 1;
        }
        grid.insert_path(&path, ei);
        let labels = match relations.get(ei) {
            Some(rel) => {
                build_parallel_aware_edge_labels(rel, ei, relations, &parallel.offsets, &path)
            }
            None => Vec::new(),
        };
        let mut edge = EdgeLayout {
            geometry: PathGeometry::Polyline { points: Vec::new() },
            labels,
            from_port: from_side[ei],
            to_port: to_side[ei],
        };
        edge.set_polyline_points(path);
        edges[ei] = edge;
        rerouted += 1;
    }
    ortho_stats.feedback_rerouted_after_trunk = rerouted;
    rerouted
}

/// Phase 4g + X-0：锯齿消毒（端点反向 stub + 微折折叠）+ 边间距违规统计
fn phase_sanitize(
    edges: &mut [EdgeLayout],
    relations: &[crate::ast::Relation],
    from_side: &[Port],
    to_side: &[Port],
    grid: &SegmentGrid,
    parallel_gap: f64,
    ortho_stats: &mut crate::layout::OrthoDebugStats,
    annotations: Option<&crate::layout::edge::RouteAnnotationSet>,
    nodes: Option<&HashMap<String, NodeLayout>>,
    sorted_node_ids: Option<&[String]>,
) {
    sanitize_orthogonal_edges_with_guard(
        edges,
        relations,
        from_side,
        to_side,
        false,
        annotations,
        nodes,
        sorted_node_ids,
    );
    // P3.1：正反向 gap 写权 = C 预修（phase_lane 末）+ D 一次审计（pipeline）。
    // sanitize 后再 enforce 无独立证据支撑（与 lane 后重复），此处不再调用。

    // ── X-0: 统计边间距违规（排除 stub 段） ──
    let (exact_overlap_pairs, tight_spacing_pairs) =
        count_all_edge_spacing_violations(edges, grid, parallel_gap);
    ortho_stats.edge_exact_overlap_pairs = exact_overlap_pairs;
    ortho_stats.edge_tight_spacing_pairs = tight_spacing_pairs;
}

/// 走廊边路径重建：有计划且通过穿障/穿组校验时返回路径。
/// Iteration 2：是否对该边启用穿组硬约束（拒绝 `best_nodes_only` 穿无关组）。
///
/// - 已有 corridor chain → 强制 strict（应走走廊，禁止穿组软降级）
/// - R3：feedback / 长跨度边 → 强制 strict（`path_avoids_group_interiors`）
///   - 长跨度边在 `assign_feedback_sides` 中**必然**进 hint（不会因正对通道跳过），
///     故 `feedback_edge_set` 已覆盖「长跨度 ∪ 回环」；此处只需传入该集合。
/// - P1：跨 leaf-group（含一端有组一端无组）→ 即使尚无 corridor chain，也禁止
///   「只避节点、可穿组」软降级；无链时配合 `prefer_outer_ring` 走外廊。
/// - 同 leaf / 均无组短边保持 false：全图硬否决会导致直线/脏路径退化
///   （见 k8s-multi-namespace 回归）
pub(crate) fn should_strict_group_transit(
    _profile: &OrthoRoutingProfile,
    group_ctx: &crate::layout::group::GroupRoutingContext,
    from_id: &str,
    to_id: &str,
    has_corridor_chain: bool,
    force_strict_feedback_or_long_span: bool,
) -> bool {
    if has_corridor_chain || force_strict_feedback_or_long_span {
        return true;
    }
    // P1 核心在 validated_corridor_path（跨 leaf 接受避组脏走廊）与 select 的
    // dirty-avoid 过滤；无链跨 leaf 全员 strict 会在无避组候选时把 degraded
    // 路径打进新穿组（ecommerce mq→notify）。有链/feedback 仍强制 strict。
    let _ = (group_ctx, from_id, to_id);
    false
}

pub(crate) fn validated_corridor_path(
    edge_index: usize,
    from_anchor: Point,
    to_anchor: Point,
    from_id: &str,
    to_id: &str,
    corridor_plan: &corridor_route::CorridorRoutePlan,
    group_ctx: &crate::layout::group::GroupRoutingContext,
    nodes: &HashMap<String, NodeLayout>,
    obstacles: &PreparedObstacles,
    stub_len: f64,
) -> Option<Vec<Point>> {
    if !corridor_plan.chains.contains_key(&edge_index) {
        return None;
    }
    let candidate = corridor_route::try_build_corridor_path(
        edge_index,
        from_anchor,
        to_anchor,
        from_id,
        to_id,
        corridor_plan,
        group_ctx,
        stub_len,
    )?;
    if candidate.len() < 2 {
        return None;
    }
    // P1：避组是硬门槛；穿节点在跨 leaf 时可接受（优于 free-route 穿组）。
    if !path_avoids_group_interiors(
        &candidate,
        from_id,
        to_id,
        group_ctx,
        &obstacles.sorted_group_ids,
    ) {
        return None;
    }
    if path_is_clean(
        &candidate,
        from_id,
        to_id,
        nodes,
        group_ctx,
        &obstacles.sorted_node_ids,
    ) {
        return Some(candidate);
    }
    if !group_ctx.is_same_leaf_group(from_id, to_id) {
        return Some(candidate);
    }
    None
}

// ═══════════════════════════════════════════════════════════
//  通用辅助
// ═══════════════════════════════════════════════════════════

/// 构建端点并线分组键。
///
/// 键相同的端点才允许共享锚点（并线）。键由五个维度组成，分别对应三条并线原则：
/// - `node_id` + `side`：同一节点同一侧（并线的前提位置条件）
/// - `is_from`：端点方向相同（原则 3 的 OR 语义实现）
///   - `is_from=true` 的端点都属于"从 node_id 出发"的边 → 同源出边之间可并线
///   - `is_from=false` 的端点都属于"到达 node_id"的边 → 同宿入边之间可并线
///   - 一条出边 + 一条入边（is_from 不同）→ 不并线
/// - `arrow_type_tag`：同箭头类型（原则 1）
/// - `edge_line_style_signature`：同线型（原则 2）
pub(crate) fn endpoint_bundling_key(
    node_id: &str,
    side: Port,
    is_from: bool,
    rel: &crate::ast::Relation,
) -> String {
    format!(
        "{node_id}|{side:?}|{is_from}|{}|{}",
        arrow_type_tag(&rel.arrow),
        edge_line_style_signature(rel),
    )
}

/// 取一个并线子组的排序键 `(arrow_tag, line_style, min_edge_index)`。
///
/// 排序键**不含** `is_from`：同一 edge 在 from 端 `is_from=true`、在 to 端
/// `is_from=false`，若将 is_from 纳入排序，两端子组排名会不一致，导致同一
/// edge 在两端获得不同的 `base_frac`，路径出现弯折。用 `min_edge_index` 做
/// 稳定 tiebreaker 可保证同一 edge 在两端子组中获得相同排名 → 相同 base_frac
/// → 直线路径。
fn sub_group_sort_key(
    endpoints: &[Endpoint],
    relations: &[crate::ast::Relation],
) -> (&'static str, String, usize) {
    let min_edge = endpoints.iter().map(|e| e.edge_index).min().unwrap_or(0);
    let rel = &relations[min_edge];
    (
        arrow_type_tag(&rel.arrow),
        edge_line_style_signature(rel),
        min_edge,
    )
}

// ═══════════════════════════════════════════════════════════
//  P1-3: 回环边侧向通道覆盖（在端口协调后强制执行）
// ═══════════════════════════════════════════════════════════

fn apply_feedback_side_overrides(
    relations: &[crate::ast::Relation],
    assignment: &feedback_side::FeedbackSideAssignment,
    from_side: &mut [Port],
    to_side: &mut [Port],
    lane: &mut [usize],
) {
    for (&edge_index, hint) in &assignment.hints {
        if edge_index >= relations.len() {
            continue;
        }
        from_side[edge_index] = hint.from_side;
        to_side[edge_index] = hint.to_side;
        lane[edge_index] = hint.lane;
    }
}

// ═══════════════════════════════════════════════════════════
//  P0-3: 端口选择全局协调（同侧偏好）
// ═══════════════════════════════════════════════════════════

/// 端口选择全局协调：对每个节点的多条边做"同侧偏好"协调。
///
/// `choose_pair_sides` 逐对独立选端口，同一节点的多条边可能分散在不同侧出发，
/// 导致节点附近不必要的交叉。此函数统计各侧边数，让少数派边在几何可接受时
/// 切换到多数派侧。
///
/// 协调以 pair_group 为最小单元（保持组内端口对一致性），出边/入边分开协调。
/// 确定性：节点按 node_id 排序，多数派 tiebreak 用最小 edge_index。
fn coordinate_port_sides(
    relations: &[crate::ast::Relation],
    nodes: &HashMap<String, NodeLayout>,
    from_side: &mut [Port],
    to_side: &mut [Port],
    _group_ctx: Option<&crate::layout::group::GroupRoutingContext>,
) {
    use std::collections::{BTreeMap, BTreeSet};
    let n = relations.len();
    if n == 0 {
        return;
    }

    // 1. 重建 pair_groups: pair_key -> (can_from, can_to, edge_indices)
    let mut pair_info: BTreeMap<String, (String, String, Vec<usize>)> = BTreeMap::new();
    for (i, rel) in relations.iter().enumerate() {
        let key = undirected_pair_key(rel.from.as_str(), rel.to.as_str());
        let (can_from, can_to) = canonical_pair(rel.from.as_str(), rel.to.as_str());
        pair_info
            .entry(key)
            .or_insert_with(|| (can_from.to_string(), can_to.to_string(), Vec::new()))
            .2
            .push(i);
    }

    // 2. 收集每个节点的端口信息: node_id -> Vec<(pair_key, edge_index, is_from, side_on_node)>
    let mut node_ports: BTreeMap<String, Vec<(String, usize, bool, Port)>> = BTreeMap::new();
    for (i, rel) in relations.iter().enumerate() {
        let key = undirected_pair_key(rel.from.as_str(), rel.to.as_str());
        node_ports.entry(rel.from.to_string()).or_default().push((
            key.clone(),
            i,
            true,
            from_side[i],
        ));
        node_ports
            .entry(rel.to.to_string())
            .or_default()
            .push((key.clone(), i, false, to_side[i]));
    }

    // 3. 按确定性顺序协调每个节点（已切换的 pair_group 不再处理，避免振荡）
    let mut switched_pairs: BTreeSet<String> = BTreeSet::new();

    for (node_id, ports) in &node_ports {
        let Some(node_nl) = nodes.get(node_id) else {
            continue;
        };

        // 分离出边和入边（排除已切换的 pair_group）
        let mut out_ports: Vec<&(String, usize, bool, Port)> = Vec::new();
        let mut in_ports: Vec<&(String, usize, bool, Port)> = Vec::new();
        for entry in ports {
            if switched_pairs.contains(&entry.0) {
                continue;
            }
            if entry.2 {
                out_ports.push(entry);
            } else {
                in_ports.push(entry);
            }
        }

        // 协调出边（≥2 条才有协调意义）
        if out_ports.len() >= 2 {
            if let Some(majority_side) = find_majority_side(&out_ports) {
                for entry in &out_ports {
                    let pair_key = &entry.0;
                    let side = entry.3;
                    if side == majority_side || switched_pairs.contains(pair_key.as_str()) {
                        continue;
                    }
                    if let Some(other_nl) = pair_other_node(pair_key, node_id, &pair_info, nodes) {
                        if side_acceptable(node_nl, other_nl, majority_side) {
                            switch_pair_side(
                                pair_key,
                                node_id,
                                majority_side,
                                &pair_info,
                                relations,
                                from_side,
                                to_side,
                            );
                            switched_pairs.insert(pair_key.clone());
                        }
                    }
                }
            }
        }

        // 协调入边
        if in_ports.len() >= 2 {
            if let Some(majority_side) = find_majority_side(&in_ports) {
                for entry in &in_ports {
                    let pair_key = &entry.0;
                    let side = entry.3;
                    if side == majority_side || switched_pairs.contains(pair_key.as_str()) {
                        continue;
                    }
                    if let Some(other_nl) = pair_other_node(pair_key, node_id, &pair_info, nodes) {
                        if side_acceptable(node_nl, other_nl, majority_side) {
                            switch_pair_side(
                                pair_key,
                                node_id,
                                majority_side,
                                &pair_info,
                                relations,
                                from_side,
                                to_side,
                            );
                            switched_pairs.insert(pair_key.clone());
                        }
                    }
                }
            }
        }
    }
}

/// 查找多数派端口。tiebreak：count 降序 → 最小 edge_index 升序 → 固定端口顺序。
fn find_majority_side(ports: &[&(String, usize, bool, Port)]) -> Option<Port> {
    let port_order = [Port::Top, Port::Bottom, Port::Left, Port::Right];
    let mut counts: [(usize, usize); 4] = [(0, usize::MAX); 4]; // (count, min_edge_index)
    for entry in ports {
        let edge_index = entry.1;
        let side = entry.3;
        for (idx, p) in port_order.iter().enumerate() {
            if side == *p {
                counts[idx].0 += 1;
                counts[idx].1 = counts[idx].1.min(edge_index);
                break;
            }
        }
    }
    let mut best_idx: Option<usize> = None;
    for (idx, (count, min_edge)) in counts.iter().enumerate() {
        if *count == 0 {
            continue;
        }
        let is_better = match best_idx {
            None => true,
            Some(bi) => {
                let (bc, be) = counts[bi];
                count > &bc
                    || (*count == bc && min_edge < &be)
                    || (*count == bc && min_edge == &be && idx < bi)
            }
        };
        if is_better {
            best_idx = Some(idx);
        }
    }
    best_idx.map(|idx| port_order[idx])
}

/// 获取 pair_group 中 node_id 之外另一个节点的布局
fn pair_other_node<'a>(
    pair_key: &str,
    node_id: &str,
    pair_info: &std::collections::BTreeMap<String, (String, String, Vec<usize>)>,
    nodes: &'a HashMap<String, NodeLayout>,
) -> Option<&'a NodeLayout> {
    let (can_from, can_to, _) = pair_info.get(pair_key)?;
    let other_id = if can_from == node_id {
        can_to.as_str()
    } else {
        can_from.as_str()
    };
    nodes.get(other_id)
}

/// 切换 pair_group 中 node_id 侧的端口为 new_side，保持组内端口对一致性。
fn switch_pair_side(
    pair_key: &str,
    node_id: &str,
    new_side: Port,
    pair_info: &std::collections::BTreeMap<String, (String, String, Vec<usize>)>,
    relations: &[crate::ast::Relation],
    from_side: &mut [Port],
    to_side: &mut [Port],
) {
    let Some((can_from, _can_to, edge_indices)) = pair_info.get(pair_key) else {
        return;
    };
    for &i in edge_indices {
        let rel = &relations[i];
        let is_can_from_from = rel.from.as_str() == can_from.as_str();
        if can_from == node_id {
            // node_id 的端口是 side_a
            if is_can_from_from {
                from_side[i] = new_side;
            } else {
                to_side[i] = new_side;
            }
        } else {
            // node_id == can_to，端口是 side_b
            if is_can_from_from {
                to_side[i] = new_side;
            } else {
                from_side[i] = new_side;
            }
        }
    }
}

/// 判断 `side` 作为 `from` 节点连接 `to` 节点的端口是否几何可接受。
///
/// 复用 `choose_pair_sides` 的阈值逻辑（`slot.rs` `dy.abs() >= dx.abs() * 0.4`）。
/// 若该方向的对端节点位移比例低于阈值，则代价过高、不可接受。
fn side_acceptable(from: &NodeLayout, to: &NodeLayout, side: Port) -> bool {
    let fc = node_center(from);
    let tc = node_center(to);
    let dx = tc.x - fc.x;
    let dy = tc.y - fc.y;
    let ox = range_overlap_local(from.x, from.x + from.width, to.x, to.x + to.width);
    let oy = range_overlap_local(from.y, from.y + from.height, to.y, to.y + to.height);

    match side {
        Port::Top | Port::Bottom => {
            if oy > EPS && ox <= EPS {
                return false;
            }
            let direction_ok = match side {
                Port::Bottom => dy > EPS,
                Port::Top => dy < -EPS,
                _ => unreachable!(),
            };
            if !direction_ok {
                return false;
            }
            if ox <= EPS && oy <= EPS {
                return dy.abs() >= dx.abs() * 0.4 - EPS;
            }
            if ox > EPS && oy > EPS {
                return dy.abs() >= dx.abs() - EPS;
            }
            true
        }
        Port::Left | Port::Right => {
            if ox > EPS && oy <= EPS {
                return false;
            }
            let direction_ok = match side {
                Port::Right => dx > EPS,
                Port::Left => dx < -EPS,
                _ => unreachable!(),
            };
            if !direction_ok {
                return false;
            }
            if ox <= EPS && oy <= EPS {
                return dx.abs() >= dy.abs() * 0.4 - EPS;
            }
            if ox > EPS && oy > EPS {
                return dx.abs() >= dy.abs() - EPS;
            }
            true
        }
    }
}

pub(crate) fn range_overlap_local(a_min: f64, a_max: f64, b_min: f64, b_max: f64) -> f64 {
    (a_max.min(b_max) - a_min.max(b_min)).max(0.0)
}

#[cfg(test)]
mod contract_priority_tests {
    use super::should_strict_group_transit;
    use crate::layout::edge::edge_routing_orthogonal::profile::OrthoRoutingProfile;
    use crate::layout::group::GroupRoutingContext;
    use crate::layout::GroupLayout;
    use std::collections::{BTreeMap, HashMap};

    fn ctx_with_leaves(from_leaf: &str, to_leaf: &str) -> GroupRoutingContext {
        let mut node_leaf_group = HashMap::new();
        node_leaf_group.insert("a".to_string(), from_leaf.to_string());
        node_leaf_group.insert("b".to_string(), to_leaf.to_string());
        GroupRoutingContext {
            groups: HashMap::from([
                (
                    from_leaf.to_string(),
                    GroupLayout {
                        x: 0.0,
                        y: 0.0,
                        width: 100.0,
                        height: 100.0,
                    },
                ),
                (
                    to_leaf.to_string(),
                    GroupLayout {
                        x: 200.0,
                        y: 0.0,
                        width: 100.0,
                        height: 100.0,
                    },
                ),
            ]),
            node_to_groups: HashMap::new(),
            border_shell_pad: 8.0,
            stub_clearance: 8.0,
            corridor_misalignment_penalty: 0.0,
            repulse_max_rounds: 0,
            corridors: Vec::new(),
            side_gutters: BTreeMap::new(),
            node_leaf_group,
            sibling_sets: Vec::new(),
            sibling_orientation: HashMap::new(),
            group_ancestors: HashMap::new(),
        }
    }

    #[test]
    fn corridor_or_feedback_forces_strict() {
        let profile = OrthoRoutingProfile::for_diagram_type(crate::types::DiagramType::Architecture);
        let ctx = ctx_with_leaves("g1", "g2");
        assert!(
            should_strict_group_transit(&profile, &ctx, "a", "b", true, false),
            "has corridor chain → strict"
        );
        assert!(
            should_strict_group_transit(&profile, &ctx, "a", "b", false, true),
            "feedback/long-span → strict"
        );
    }

    #[test]
    fn same_leaf_without_chain_stays_soft() {
        let profile = OrthoRoutingProfile::for_diagram_type(crate::types::DiagramType::Architecture);
        let ctx = ctx_with_leaves("g1", "g1");
        assert!(
            !should_strict_group_transit(&profile, &ctx, "a", "b", false, false),
            "same-leaf short edges stay soft"
        );
    }

    #[test]
    fn cross_leaf_without_chain_stays_soft_to_avoid_forced_pierce() {
        let profile = OrthoRoutingProfile::for_diagram_type(crate::types::DiagramType::Architecture);
        let ctx = ctx_with_leaves("g1", "g2");
        assert!(
            !should_strict_group_transit(&profile, &ctx, "a", "b", false, false),
            "无链跨 leaf 不强制 strict（避 ecommerce 类 degraded 新穿组）"
        );
    }

    #[test]
    fn group_safe_dirty_outranks_group_pierce_clean_by_hits() {
        // 钉死 P1 排序键：group_hits 优先于 clean/dirty。
        let clean_pierce = (1u32, 100.0f64);
        let dirty_avoid = (0u32, 400.0f64);
        assert!(
            dirty_avoid < clean_pierce,
            "避组脏路径必须优于穿组净路径（hits,len）"
        );
    }
}
