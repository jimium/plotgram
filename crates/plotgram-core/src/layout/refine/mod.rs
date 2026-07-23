//! Layout ↔ Route 反馈循环（refine）
//!
//! 在节点布局 + 边路由完成后，检测折线边路径穿过非端点节点的情形，
//! 局部推开问题节点并重新路由，减少密集图的 `edge_node_crossings`。

use crate::ast::Diagram;
use crate::layout::{EdgeRoutingStrategy, LayoutResult};
use std::collections::HashSet;

mod crossing;
mod geometry;
mod overlap;
mod push;
mod reroute;
mod spline_fallback;

pub use crossing::analyze_edge_node_crossings;
pub(crate) use geometry::segment_intersects_aabb;
pub use geometry::segment_intersects_node;

pub use push::MomentumHistory;

/// refine 配置
#[derive(Debug, Clone, Copy)]
pub struct RefineConfig {
    pub enabled: bool,
    pub max_passes: usize,
    pub push_distance: f64,
    pub node_shrink: f64,
}

impl Default for RefineConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            // Iteration 3：有穿障时最多 2 轮（无穿障早退不变）
            max_passes: 2,
            push_distance: 40.0,
            // C11：略减小 shrink，降低贴边穿过漏检
            node_shrink: 1.0,
        }
    }
}

/// 穿障统计
#[derive(Debug, Clone, Default)]
pub struct RefineMetrics {
    pub edge_node_crossings: usize,
    pub problem_nodes: std::collections::HashMap<String, NodePushInfo>,
    pub edge_overlaps: usize,
}

/// 单个问题节点的推开信息
#[derive(Debug, Clone, Default)]
pub struct NodePushInfo {
    pub crossing_count: usize,
    pub push_fx: f64,
    pub push_fy: f64,
    pub edge_indices: Vec<usize>,
}

fn combined_crossing_score(metrics: &RefineMetrics) -> usize {
    metrics.edge_node_crossings * 10 + metrics.edge_overlaps
}

/// 执行 refine 循环：统计穿障 → 推开问题节点 → 增量 re-route
pub fn run_refine(
    diagram: &Diagram,
    mut result: LayoutResult,
    router: &dyn EdgeRoutingStrategy,
    config: &RefineConfig,
) -> LayoutResult {
    if !config.enabled || config.max_passes == 0 {
        return result;
    }
    // C3 验证钩子：`PLOTGRAM_SKIP_REFINE=1` 只看 router 输出。
    if std::env::var_os("PLOTGRAM_SKIP_REFINE").is_some() {
        return result;
    }

    let t_cross = crate::layout::perf::Instant::now();
    let best_metrics = crossing::analyze_crossings(&result, diagram, config);
    crate::perf_log!(
        "[perf]         analyze_crossings: {:.2}ms",
        t_cross.elapsed().as_secs_f64() * 1000.0
    );
    let mut best_score = combined_crossing_score(&best_metrics);
    if best_metrics.edge_node_crossings == 0 && result.groups.is_empty() {
        return result;
    }

    // P3：有组图跳过 push（节点推开会在后续 group_frame 放大后制造新穿组，
    // 且 refine 内 lint 尚看不到最终组框）。穿组修复只走下方 fallback。
    // Phase B: 所有图类型使用 solver，节点已冻结，跳过 push。
    let skip_push = true;

    let entry_snapshot = result.clone();
    let entry_group_pierces = count_group_interior_edges(diagram, &entry_snapshot);

    let mut best_result = result.clone();
    let mut momentum = push::MomentumHistory::new();
    let mut passes_executed = 0usize;
    let mut total_push_count = 0usize;

    if !skip_push {
    for _ in 0..config.max_passes {
        let metrics = crossing::analyze_crossings(&result, diagram, config);
        if metrics.edge_node_crossings == 0 {
            break;
        }

        // 直接推节点 + 重路由，跳过 trial reroute（不推节点时重路由几乎无效）

        let mut edges_to_reroute: HashSet<usize> = HashSet::new();
        for info in metrics.problem_nodes.values() {
            edges_to_reroute.extend(info.edge_indices.iter().copied());
        }
        for node_id in metrics.problem_nodes.keys() {
            for (i, rel) in diagram.relations.iter().enumerate() {
                if rel.from.as_str() == node_id.as_str() || rel.to.as_str() == node_id.as_str() {
                    edges_to_reroute.insert(i);
                }
            }
        }

        let push_count = metrics
            .problem_nodes
            .values()
            .filter(|info| {
                let len = (info.push_fx * info.push_fx + info.push_fy * info.push_fy).sqrt();
                len >= f64::EPSILON
            })
            .count();
        total_push_count += push_count;

        let pre_push_nodes = result.nodes.clone();
        push::push_problem_nodes(&mut result, &metrics, config, &mut momentum);
        // 空间契约：refine 候选若压穿相邻 rank 层缝，整轮拒绝。
        // 不在反馈循环里“再推一次节点”修补，否则路由评分看到的是二次改写后的布局。
        let rank_scopes = crate::layout::space_budget::node_group_scopes(diagram);
        let no_reverse_pairs = HashSet::new();
        let budget = result
            .hints
            .space_budget
            .clone()
            .unwrap_or_else(|| crate::layout::space_budget::SpaceBudget::from_diagram(diagram));
        // Phase B: 所有图类型使用 solver，节点已冻结，不再执行 enforce_horizontal_gaps
        let rank_contract_broken = result.hints.sugiyama_ranks.as_ref().is_some_and(|ranks| {
            let mut before_probe = pre_push_nodes.clone();
            let before: HashSet<String> = crate::layout::space_budget::enforce_vertical_rank_gaps(
                &mut before_probe,
                &budget,
                ranks,
                &rank_scopes,
                &no_reverse_pairs,
            )
            .into_iter()
            .collect();
            let mut after_probe = result.nodes.clone();
            let after: HashSet<String> = crate::layout::space_budget::enforce_vertical_rank_gaps(
                &mut after_probe,
                &budget,
                ranks,
                &rank_scopes,
                &no_reverse_pairs,
            )
            .into_iter()
            .collect();
            !after.is_subset(&before)
        });
        result.hints.space_budget = Some(budget);
        if rank_contract_broken {
            result.nodes = pre_push_nodes;
            break;
        }
        // P3：push+reroute 不得留下穿组结果。
        // - after 穿组 → 恢复旧几何（无论 before 是否已穿；穿组修复交给 fallback）
        // - after 不穿组 → 保留（允许从穿组改善到避组）
        let group_maps = (!result.groups.is_empty())
            .then(|| crate::layout::lint::GroupInteriorMaps::new(diagram));
        let mut preserve_edges: std::collections::HashMap<usize, crate::layout::EdgeLayout> =
            std::collections::HashMap::new();
        if group_maps.is_some() {
            let mut locked: Vec<usize> = edges_to_reroute.iter().copied().collect();
            locked.sort_unstable();
            for ei in locked {
                if ei < result.edges.len() {
                    preserve_edges.insert(ei, result.edges[ei].clone());
                }
            }
        }
        reroute::reroute_subset(&mut result, diagram, router, &edges_to_reroute);
        if let Some(ref maps) = group_maps {
            let mut restored: Vec<usize> = preserve_edges.keys().copied().collect();
            restored.sort_unstable();
            for ei in restored {
                if crate::layout::lint::edge_crosses_group_interior_with_maps(
                    diagram, &result, ei, maps,
                ) {
                    if let Some(old) = preserve_edges.remove(&ei) {
                        result.edges[ei] = old;
                    }
                }
            }
        }
        passes_executed += 1;

        let new_metrics = crossing::analyze_crossings(&result, diagram, config);
        let new_score = combined_crossing_score(&new_metrics);
        if new_score < best_score {
            best_result = result.clone();
            best_score = new_score;
        } else {
            result = best_result;
            break;
        }
    }
    } // !skip_push

    result.hints.refine_debug = Some(crate::layout::RefineDebugStats {
        push_count: total_push_count,
        momentum_reversals: momentum.reversal_count,
        passes_executed,
        spline_fallback_count: 0,
    });

    // L5.1：穿组边 + 仅穿节点边均可进 dogleg。
    // 硬门禁在 `orthogonal_detour`（残 through / 新穿组一律拒）；不得为消 through 留下穿组。
    // 有组图仍 skip push；此处只是放开「已避组但 through」的末端试修写权。
    let final_metrics = crossing::analyze_crossings(&result, diagram, config);
    let mut fallback_edges: HashSet<usize> = HashSet::new();
    for info in final_metrics.problem_nodes.values() {
        fallback_edges.extend(info.edge_indices.iter().copied());
    }
    if !result.groups.is_empty() {
        let maps = crate::layout::lint::GroupInteriorMaps::new(diagram);
        for edge_index in 0..result.edges.len() {
            if crate::layout::lint::edge_crosses_group_interior_with_maps(
                diagram, &result, edge_index, &maps,
            ) {
                fallback_edges.insert(edge_index);
            }
        }
    }
    if !fallback_edges.is_empty() {
        spline_fallback::reroute_edges_with_spline(&mut result, diagram, &fallback_edges, config);
    }

    let after_group = count_group_interior_edges(diagram, &result);
    if after_group > entry_group_pierces {
        result = entry_snapshot;
    }

    result
}

fn count_group_interior_edges(diagram: &Diagram, result: &LayoutResult) -> usize {
    if result.groups.is_empty() {
        return 0;
    }
    // 与 lint / collinear 一致：按 (边, 无关组) 违规条数计。
    let report = crate::layout::lint::lint_layout(diagram, result);
    crate::layout::lint::LintMetricsSummary::from_report(&report).edge_crosses_group_interior
}

/// 与 lint `edge_through_node` 对齐：跳过端点 stub 段，全尺寸节点相交。
fn collect_lint_through_edge_indices(diagram: &Diagram, result: &LayoutResult) -> HashSet<usize> {
    let mut out = HashSet::new();
    let mut node_ids: Vec<&String> = result.nodes.keys().collect();
    node_ids.sort();

    for (index, edge) in result.edges.iter().enumerate() {
        if edge.path_len() < 2 {
            continue;
        }
        let Some(rel) = diagram.relations.get(index) else {
            continue;
        };
        let from_id = rel.from.as_str();
        let to_id = rel.to.as_str();
        let path = edge.path_points();
        let segment_count = path.len().saturating_sub(1);
        let skip_endpoints = segment_count > 2;

        for (seg_i, window) in path.windows(2).enumerate() {
            if skip_endpoints && (seg_i == 0 || seg_i == segment_count - 1) {
                continue;
            }
            let a = window[0];
            let b = window[1];
            for node_id in &node_ids {
                let node_id = node_id.as_str();
                if node_id == from_id || node_id == to_id {
                    continue;
                }
                let nl = &result.nodes[node_id];
                if segment_intersects_node(a, b, nl) {
                    out.insert(index);
                    break;
                }
            }
        }
    }
    out
}

/// L5.1：节点/组框冻结后的穿节点试修（与 lint 对齐）。
///
/// 有干净正交绕行则替换；`orthogonal_detour` 硬拒残 through / 新穿组。
/// 若整批试修抬高穿组计数则整批回退。
pub fn repair_through_edges_post_route(diagram: &Diagram, result: &mut LayoutResult) {
    let through_edges = collect_lint_through_edge_indices(diagram, result);
    if through_edges.is_empty() {
        return;
    }
    let entry_group = count_group_interior_edges(diagram, result);
    let snapshot = result.clone();
    spline_fallback::reroute_edges_with_spline(
        result,
        diagram,
        &through_edges,
        &RefineConfig::default(),
    );
    if count_group_interior_edges(diagram, result) > entry_group {
        *result = snapshot;
    }
}

/// 仅针对当前 lint `edge_crosses_group_interior` 边的局部试修。
///
/// 跨组边走激进裙边/换侧（`orthogonal_detour` 硬门禁）；
/// 整批后若穿组计数上升或 through 上升则回退。
pub fn repair_group_interior_edges_post_route(diagram: &Diagram, result: &mut LayoutResult) {
    if result.groups.is_empty() {
        return;
    }
    let group_edges = collect_lint_group_interior_edge_indices(diagram, result);
    if group_edges.is_empty() {
        return;
    }
    let entry_group = count_group_interior_edges(diagram, result);
    let entry_through = collect_lint_through_edge_indices(diagram, result).len();
    let snapshot = result.clone();
    let n = group_edges.len();

    // 仍穿组的边：激进裙边 / 换侧。
    let remain = collect_lint_group_interior_edge_indices(diagram, result);
    if !remain.is_empty() {
        spline_fallback::reroute_edges_with_spline_ex(
            result,
            diagram,
            &remain,
            &RefineConfig::default(),
            true,
        );
    }

    let after_group = count_group_interior_edges(diagram, result);
    let after_through = collect_lint_through_edge_indices(diagram, result).len();
    if after_group > entry_group || after_through > entry_through {
        *result = snapshot;
        crate::perf_log!(
            "[perf]     d_group_interior_repair: rolled_back (group {}→{} through {}→{}, tried={})",
            entry_group,
            after_group,
            entry_through,
            after_through,
            n
        );
    } else {
        crate::perf_log!(
            "[perf]     d_group_interior_repair: group {}→{} through {}→{} tried={}",
            entry_group,
            after_group,
            entry_through,
            after_through,
            n
        );
    }
}

/// D 末（节点冻结后）：分离 lint 判定的残余非语义 trunk 重合对。
///
/// 路由期 `separate_unrelated_trunk_overlaps` 在 snap/repulse/sanitize 之前运行，
/// 分离结果被后续管线重新贴靠合并；本 pass 在几何冻结后按 lint 口径重分。
/// 仅动边、节点冻结；若整批抬高 through 或穿组计数则整批回退。
pub fn separate_trunk_overlaps_post_route(diagram: &Diagram, result: &mut LayoutResult) {
    let entry_through = collect_lint_through_edge_indices(diagram, result).len();
    let entry_group = count_group_interior_edges(diagram, result);
    let snapshot = result.clone();
    let sep = crate::layout::edge::edge_routing_orthogonal::separate_unrelated_trunk_overlaps_post_route(
        diagram, result,
    );
    if sep == 0 {
        return;
    }
    let after_through = collect_lint_through_edge_indices(diagram, result).len();
    let after_group = count_group_interior_edges(diagram, result);
    if after_through > entry_through || after_group > entry_group {
        *result = snapshot;
        crate::perf_log!(
            "[perf]     d_trunk_separate: rolled_back (sep={sep} through {entry_through}→{after_through} group {entry_group}→{after_group})"
        );
    } else {
        crate::perf_log!(
            "[perf]     d_trunk_separate: sep={sep} through {entry_through}→{after_through} group {entry_group}→{after_group}"
        );
    }
}

/// 与 lint `edge_crosses_group_interior` 对齐的边下标集合。
fn collect_lint_group_interior_edge_indices(
    diagram: &Diagram,
    result: &LayoutResult,
) -> HashSet<usize> {
    let mut out = HashSet::new();
    if result.groups.is_empty() {
        return out;
    }
    let maps = crate::layout::lint::GroupInteriorMaps::new(diagram);
    for edge_index in 0..result.edges.len() {
        if crate::layout::lint::edge_crosses_group_interior_with_maps(
            diagram, result, edge_index, &maps,
        ) {
            out.insert(edge_index);
        }
    }
    out
}

/// N2：折线冻结点（`repair_through` 之后）用 lint 同语义做**只读复校**。
///
/// - 不改折点几何（禁止为消计数引入新穿模）。
/// - 诊断：仍 through / crosses 的边数与边下标。
/// - 若存在 `route_annotations`，对残留 dirty 边写入 `degraded` 原因（显式可见，不发明新契约类型）。
pub fn recheck_lint_pierce_post_freeze(diagram: &Diagram, result: &mut LayoutResult) {
    let through = collect_lint_through_edge_indices(diagram, result);
    let group_count = count_group_interior_edges(diagram, result);

    if through.is_empty() && group_count == 0 {
        crate::perf_log!(
            "[perf]     n2_lint_recheck: through=0 group_interior=0 (clean after repair)"
        );
        return;
    }

    let mut through_ids: Vec<usize> = through.iter().copied().collect();
    through_ids.sort_unstable();
    crate::perf_log!(
        "[warn] N2 折线冻结复校：repair 后仍 dirty — through_edges={} {:?} group_interior_violations={}",
        through_ids.len(),
        through_ids,
        group_count
    );

    // 显式 degraded：复用 annotation.degraded，不改 points。
    if let Some(annotations) = result.hints.route_annotations.as_mut() {
        for &ei in &through_ids {
            if let Some(ann) = annotations.edges.iter_mut().find(|a| a.edge_index == ei) {
                if ann.degraded.is_none() {
                    ann.degraded = Some("n2_lint_recheck:edge_through_node".to_string());
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "refine_tests.rs"]
mod tests;
