//! 正交路由主流程：端口/slot → 逐边建路 → straighten / reroute / stub / lane / sanitize / labels。
//!
//! 从 `mod.rs` 抽出，便于按 phase 阅读与改时序；行为应与抽取前一致。
//!
//! # P2-4 写权契约（Write Authority Contract）
//!
//! 每个几何字段有且仅有一个最终写者，消除写权竞争。
//!
//! | 字段 | 最终写者 | 只读者 |
//! |------|---------|--------|
//! | from_side / to_side / endpoint_map | phase_port_correction (4b) | reroute, lane, sanitize, S3, S4 |
//! | edge.geometry (初版) | phase_route_edges (4) | — |
//! | edge.geometry (终版) | phase_sanitize (4g) + D段 snap/sanitize_ext | — |
//! | edge.labels | D段 label_resolve（pipeline.rs） | — |
//! | route_annotations | C末 freeze + S4.x refresh | sanitize（只读校验） |
//! | grid (SegmentGrid) | 各阶段自行维护（remove+insert） | scorer（只读查询） |
//!
//! 原则：
//! - 上游阶段写初版，下游阶段只读或做最终修正
//! - 禁止中间阶段修改下游已冻结的字段
//! - 违反写权契约的修改必须通过回归证明

use super::*;
use crate::ast::Diagram;
use crate::layout::routing::common::edge_geometry::{arrow_type_tag, edge_line_style_signature};
use crate::layout::geometry::Point;
use crate::layout::{EdgeLayout, LayoutResult, NodeLayout, PathGeometry, Port};
use std::collections::HashMap;

/// R7 7c：正交 annotation 的**单一同源生成器**。
///
/// annotation = 当前 edge 几何 + bundle solution（`merge_result` 携带的
/// `merge_intervals` / `degraded`）。C 末冻结、S4.x escape 修复、B.2 局部 trunk
/// 三处**几何变更后**各生成一次——同一生成器、同一来源（bundle solution），
/// 取代此前散落三处的 `freeze_route_annotations_with_merges` 直调（消除多源刷新链）。
/// 每处只在其上游几何真正变动时重生（sanitize / D 段各自需要当前快照）。
fn route_annotations_from_solution(
    edges: &[EdgeLayout],
    from_side: &[Port],
    to_side: &[Port],
    merge_result: Option<&semantic_trunk_merge::SemanticTrunkMergeResult>,
) -> crate::layout::routing::RouteAnnotationSet {
    crate::layout::routing::freeze_route_annotations_with_merges(
        edges,
        from_side,
        to_side,
        merge_result.map(|m| &m.merge_intervals),
        merge_result.map(|m| &m.degraded),
    )
}

pub(super) fn route_edges_orthogonal_inner(
    diagram: &Diagram,
    mut result: LayoutResult,
    cfg: OrthoConfig,
    preserve_edges: Option<std::collections::HashSet<usize>>,
) -> LayoutResult {
    let draft = draft::OrthogonalDraft::compile(diagram, &mut result, &cfg);
    let draft::OrthogonalDraft {
        profile,
        parallel_gap,
        s4_monitor_corridor,
        horizontal,
        self_loop_idx,
        group_ctx,
        obstacles,
        corridor_plan,
        corridor_model,
        _from_side: _,
        _to_side: _,
        _endpoint_map: _,
        parallel,
        _reverse_pairs: _,
        edge_order,
        feedback_edge_set,
        channel_plan,
        use_two_round,
        group_rects,
        ovg,
        endpoint_assignments,
    } = draft;

    // Slice C1.4：从 EndpointAssignment 派生只读 side 视图（供尚未迁移的下游函数使用）。
    let from_side: Vec<Port> = endpoint_assignments.iter().map(|ea| ea.from_port).collect();
    let to_side: Vec<Port> = endpoint_assignments.iter().map(|ea| ea.to_port).collect();

    let relations = &diagram.relations;
    let n = relations.len();

    if std::env::var("PLOTGRAM_ANCHOR_DEBUG")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        for (i, rel) in relations.iter().enumerate() {
            if rel.from.as_str().contains("revise") || rel.to.as_str().contains("revise") {
                let ea = &endpoint_assignments[i];
                eprintln!(
                    "[anchor_dbg] after_port_slot edge[{}] {} -> {} | from={:?}@{:?} to={:?}@{:?}",
                    i, rel.from, rel.to, ea.from_port, ea.from_anchor, ea.to_port, ea.to_anchor
                );
            }
        }
    }

    // ── 4. 逐边构建路径（Slice C2a：输出 RoutePath）──
    let incremental = preserve_edges.is_some();
    let existing_edges: Vec<EdgeLayout> = if incremental {
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

    let initial = phase_route_edges(
        &edge_order,
        relations,
        &result.nodes,
        &endpoint_assignments,
        &existing_edges,
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
        // P3-1: 两轮模式下第一轮禁 OVG
        if use_two_round { None } else { ovg.as_ref() },
        channel_plan.as_ref(),
        // P3-1: 两轮模式下第一轮为 first_pass
        use_two_round,
    );

    // Slice C2b：路径求解器直接操作 RoutePath，桥接在 solve_paths 之后重建。
    let mut paths = initial.paths;
    let mut labels = initial.labels;

    // ── 4b. 端口修正已删除（Slice C1：pre-route PortAssignmentSolver 为唯一写者）──
    let t_fix = crate::layout::perf::Instant::now();

    // ── 4c. R6: PathAssignmentSolver 单一主流程 ──
    // R6：单一路径求解主流程，取代原 two-round / deferred OVG / conflict reroute
    // 三套并行控制流；直接消费 obstacles / OVG / group_rects。
    let degraded = path_solver::solve_paths(
        &result.nodes,
        relations,
        &endpoint_assignments,
        &mut paths,
        &mut labels,
        &mut grid,
        &cfg,
        &group_ctx,
        &obstacles,
        ovg.as_ref(),
        &group_rects,
        &mut ortho_stats,
        &profile,
        parallel_gap,
    );
    crate::perf_log!(
        "[perf]     r6_path_solver: {} degraded edges",
        degraded.len()
    );

    // Bridge: RoutePath -> EdgeLayout（C2c 将删除此桥接，下游直接消费 RoutePath）
    let mut edges: Vec<EdgeLayout> = (0..n)
        .map(|i| {
            let pts = paths[i].points().to_vec();
            if pts.is_empty() {
                EdgeLayout::empty()
            } else {
                let mut edge = EdgeLayout {
                    geometry: PathGeometry::Polyline { points: Vec::new() },
                    labels: labels[i].clone(),
                    from_port: endpoint_assignments[i].from_port,
                    to_port: endpoint_assignments[i].to_port,
                };
                // 写者归属（E6）：C 段自环边自产几何桥，freeze 前 solver 内部。
                edge.set_polyline_points(pts);
                edge
            }
        })
        .collect();

    // ── 4d. X-3: Lane Assignment 车道分配 ──
    // Slice C3.2：solve 从 paths 读取，返回 Vec<LaneAssignment> 供 RouteSolution 携带。
    let lane_assignments = phase_lane(
        &paths,
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
    // Slice C3.3：同时产出 Vec<BundleSolution> 供 RouteSolution 携带。
    let mut bundle_solutions: Vec<crate::layout::routing::model::BundleSolution> = Vec::new();
    let merge_result = if profile.semantic_merge {
        let (mr, bs) = semantic_trunk_merge::apply_semantic_trunk_merge(
            &mut edges,
            relations,
            &from_side,
            &to_side,
            &result.nodes,
            diagram.diagram_type.clone(),
            !diagram.groups.is_empty(),
        );
        bundle_solutions = bs;
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

    // Phase 2 红线：穿无关组硬修复（lane/semantic merge 之后）
    let group_repaired = path_kernel::repair_group_interior_crossings(
        &mut edges,
        diagram,
        &result.groups,
        &group_ctx,
        &obstacles.sorted_group_ids,
        &mut grid,
    );
    if group_repaired > 0 {
        crate::perf_log!(
            "[perf]     phase2_group_interior_repair: repaired={}",
            group_repaired
        );
    }

    // C 末冻结旁路 Annotation（stub / 受保护 trunk / S3 merge），供 sanitize / 后续 D 验证
    // R7 7c：经单一同源生成器 [`route_annotations_from_solution`]（merge_result 为唯一来源）。
    let route_annotations = route_annotations_from_solution(
        &edges,
        &from_side,
        &to_side,
        merge_result.as_ref(),
    );
    result.hints.route_annotations = Some(route_annotations.clone());

    // ── 4g. X-0 间距统计（canonicalize 已迁至 D 段唯一写点，Phase 0 策略 B）──
    phase_sanitize(&edges, &grid, parallel_gap, &mut ortho_stats);

    // S2-5：C 期末尾的 dock 收口已移除——D 段（pipeline）在 snap/sanitize 之后
    // 作为「最终写者」重做 dock_sep，C 末这次会被 D 的 snap/sanitize 抖回后覆盖，属纯冗余。

    // P3.3：标签避让只在 pipeline 几何冻结后做一次。
    // 写权契约：edge.labels 的最终写者是 D段 label_resolve（pipeline.rs），
    // C段内 sanitize 重建的平行边标签会被 D 段 snap/sanitize 后再次 resolve。
    crate::perf_log!(
        "[perf]     fix_inversions+labels: {:.2}ms",
        t_fix.elapsed().as_secs_f64() * 1000.0
    );

    // Phase C2 已移至管线后处理之后（pipeline.rs），避免 snap/sanitize 引入新交叉抵消效果。

    result.edges = edges;

    // Slice C2c：组装 RouteSolution（ports + paths + lanes + bundles）供 Recipe 直接消费。
    {
        use crate::layout::routing::model::{
            GeometryMaterializer, RouteSolution, RoutingDiagnostics, StableEdgeId,
        };
        let sol_paths: Vec<crate::layout::routing::model::RoutePath> = result
            .edges
            .iter()
            .map(|e| GeometryMaterializer::lift_geometry(&e.geometry))
            .collect();
        let sol_diags = RoutingDiagnostics {
            degraded: degraded
                .iter()
                .map(|&(ei, reason)| (StableEdgeId(ei), reason))
                .collect(),
            notes: Vec::new(),
        };
        result.hints.route_solution = Some(RouteSolution {
            ports: endpoint_assignments.clone(),
            paths: sol_paths,
            lanes: lane_assignments,
            bundles: bundle_solutions,
            annotations: result.hints.route_annotations.clone().unwrap_or_default(),
            diagnostics: sol_diags,
            score: Default::default(),
        });
    }

    // A-0：三段契约只读诊断（不改几何，仅量化现状供后续改善对比）。
    let contract_diag = contract::diagnose_contract_violations(
        relations,
        &result.nodes,
        &result.edges,
        &group_ctx,
        horizontal,
    );
    ortho_stats.contract_stub_violations = contract_diag.stub_violations;
    ortho_stats.contract_approach_violations = contract_diag.approach_violations;
    ortho_stats.contract_unnatural_to_port = contract_diag.unnatural_to_port;
    ortho_stats.contract_away_segments = contract_diag.away_segments;
    ortho_stats.contract_away_edges = contract_diag.away_edges;
    crate::perf_log!(
        "[perf]     contract_diag: stub={} approach={} unnatural_to_port={} away_segs={} away_edges={}",
        contract_diag.stub_violations,
        contract_diag.approach_violations,
        contract_diag.unnatural_to_port,
        contract_diag.away_segments,
        contract_diag.away_edges
    );
    // P2-1: 导出 orthogonal 路由 debug 统计
    result.hints.orthogonal_debug = Some(ortho_stats);
    result
}

/// P3-1: 拥堵检测——统计每个通道坐标的负载，返回经过拥堵区域的边索引。
///
/// 算法：
/// 1. 遍历所有边的路径段，按 cross-axis 坐标分桶（精度 4px）
/// 2. 统计每个桶的段数（负载）
/// 3. 负载 >= threshold 的桶标记为拥堵
/// 4. 返回经过拥堵桶的边索引（去重、按序）
pub(super) fn detect_congestion(paths: &[crate::layout::routing::model::solution::RoutePath], threshold: usize) -> Vec<usize> {
    use std::collections::BTreeMap;
    const BUCKET_WIDTH: f64 = 4.0;

    // 垂直段按 x 分桶，水平段按 y 分桶
    // key = (is_vertical, bucket_coord), value = 段数
    let mut v_buckets: BTreeMap<i64, usize> = BTreeMap::new();
    let mut h_buckets: BTreeMap<i64, usize> = BTreeMap::new();

    for path in paths.iter() {
        if path.is_empty() {
            continue;
        }
        let pts = path.points();
        for w in pts.windows(2) {
            let dx = (w[1].x - w[0].x).abs();
            let dy = (w[1].y - w[0].y).abs();
            if dy < 1.0 && dx > 1.0 {
                // 水平段：按 y 分桶
                let key = (w[0].y / BUCKET_WIDTH).round() as i64;
                *h_buckets.entry(key).or_default() += 1;
            } else if dx < 1.0 && dy > 1.0 {
                // 垂直段：按 x 分桶
                let key = (w[0].x / BUCKET_WIDTH).round() as i64;
                *v_buckets.entry(key).or_default() += 1;
            }
        }
    }

    // 收集拥堵桶
    let congested_v: std::collections::HashSet<i64> = v_buckets
        .iter()
        .filter(|(_, &count)| count >= threshold)
        .map(|(&k, _)| k)
        .collect();
    let congested_h: std::collections::HashSet<i64> = h_buckets
        .iter()
        .filter(|(_, &count)| count >= threshold)
        .map(|(&k, _)| k)
        .collect();

    if congested_v.is_empty() && congested_h.is_empty() {
        return Vec::new();
    }

    // 找出经过拥堵桶的边
    let mut result: Vec<usize> = Vec::new();
    for (ei, path) in paths.iter().enumerate() {
        if path.is_empty() {
            continue;
        }
        let pts = path.points();
        let hits_congestion = pts.windows(2).any(|w| {
            let dx = (w[1].x - w[0].x).abs();
            let dy = (w[1].y - w[0].y).abs();
            if dy < 1.0 && dx > 1.0 {
                let key = (w[0].y / BUCKET_WIDTH).round() as i64;
                congested_h.contains(&key)
            } else if dx < 1.0 && dy > 1.0 {
                let key = (w[0].x / BUCKET_WIDTH).round() as i64;
                congested_v.contains(&key)
            } else {
                false
            }
        });
        if hits_congestion {
            result.push(ei);
        }
    }
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

#[cfg(test)]
mod contract_priority_tests {
    use super::should_strict_group_transit;
    use crate::layout::routing::edge_routing_orthogonal::profile::OrthoRoutingProfile;
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
