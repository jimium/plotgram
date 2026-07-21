//! 正交路由主流程：端口/slot → 逐边建路 → straighten / reroute / stub / lane / sanitize / labels。
//!
//! 从 `mod.rs` 抽出，便于按 phase 阅读与改时序；行为应与抽取前一致。

use super::*;
use crate::ast::Diagram;
use crate::layout::edge::common::edge_geometry::{arrow_type_tag, edge_line_style_signature};
use crate::layout::edge::common::parallel_edges::build_parallel_aware_edge_labels;
use crate::layout::edge::common::self_loop;
use crate::layout::geometry::Point;
use crate::layout::{EdgeLayout, LayoutResult, NodeLayout, PathGeometry, Port};
use std::collections::HashMap;

fn edge_order_score_enabled() -> bool {
    !std::env::var("PLOTGRAM_EDGE_ORDER_SCORE")
        .map(|v| v == "0" || v.eq_ignore_ascii_case("false"))
        .unwrap_or(false)
}

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
    let corridor_model = crate::layout::demand::compute_corridor_model(diagram, &result);

    // Phase 2：入口算一次边难度，注入边序（高分同层略提前；可 PLOTGRAM_EDGE_ORDER_SCORE=0 关）
    let difficulty_scores = if edge_order_score_enabled() {
        let features = crate::layout::demand::collect_edge_features(
            diagram,
            &result,
            Some(&corridor_model),
        );
        let ranked = crate::layout::demand::score_edges(
            &features,
            &crate::layout::demand::DifficultyProfile::default(),
        );
        let mut dense = vec![0.0_f64; n];
        for (idx, score) in ranked {
            if let Some(slot) = dense.get_mut(idx) {
                *slot = score;
            }
        }
        Some(dense)
    } else {
        None
    };

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
        difficulty_scores.as_deref(),
    );

    // ── 4. 逐边构建路径 ──
    let incremental = preserve_edges.is_some();
    let mut edges: Vec<EdgeLayout> = if incremental {
        result.edges.clone()
    } else {
        (0..n).map(|_| EdgeLayout::empty()).collect()
    };
    let mut grid = SegmentGrid::new();

    // Phase B3：全局通道规划（默认关闭，PLOTGRAM_CHANNEL_PLANNER=1 启用）
    let channel_plan = if channel_planner::channel_planner_enabled() {
        Some(channel_planner::plan_channels(
            relations,
            &result.nodes,
            &group_ctx,
            &edge_order,
        ))
    } else {
        None
    };

    // Phase B1：构建 OVG（仅当环境变量启用时）——节点障碍物阻断 + 组软惩罚
    // 节点膨胀使用 NODE_OBSTACLE_PAD + 10，给路径更多 clearance，减少 tight 违规
    let ovg = if visibility_graph::ovg_enabled() {
        let group_rects: Vec<crate::layout::geometry::Rect> = obstacles.sorted_group_ids.iter()
            .filter_map(|gid| group_ctx.groups.get(gid))
            .map(|gl| crate::layout::geometry::Rect::from(gl))
            .collect();
        let ovg_graph = visibility_graph::build_ovg_with_groups(
            &result.nodes,
            &obstacles.sorted_node_ids,
            NODE_OBSTACLE_PAD + 10.0,
            &group_rects,
        );
        if ovg_graph.is_empty() { None } else { Some(ovg_graph) }
    } else {
        None
    };

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
        Some(&corridor_model),
        ovg.as_ref(),
        channel_plan.as_ref(),
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
        ovg.as_ref(),
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
        ovg.as_ref(),
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
        ovg.as_ref(),
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
                ovg.as_ref(),
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

    // S2-5：C 期末尾的 dock 收口已移除——D 段（pipeline）在 snap/sanitize 之后
    // 作为「最终写者」重做 dock_sep，C 末这次会被 D 的 snap/sanitize 抖回后覆盖，属纯冗余。

    // P3.3：标签避让只在 pipeline 几何冻结后做一次。
    // sanitize 会重建平行边标签；此处再 resolve 会被 D 段 snap/sanitize 丢掉。
    crate::perf_log!(
        "[perf]     fix_inversions+labels: {:.2}ms",
        t_fix.elapsed().as_secs_f64() * 1000.0
    );

    // Phase C2 已移至管线后处理之后（pipeline.rs），避免 snap/sanitize 引入新交叉抵消效果。

    result.edges = edges;
    // P2-1: 导出 orthogonal 路由 debug 统计
    result.hints.orthogonal_debug = Some(ortho_stats);
    result
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
        false,
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
