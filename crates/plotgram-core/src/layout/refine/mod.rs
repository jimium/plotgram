//! Layout ↔ Route 反馈循环（refine）
//!
//! 在节点布局 + 边路由完成后，检测折线边路径穿过非端点节点的情形，
//! 局部推开问题节点并重新路由，减少密集图的 `edge_node_crossings`。

use crate::ast::Diagram;
use crate::layout::{RoutingRecipeDyn, LayoutResult};
use std::collections::HashSet;

mod crossing;
mod geometry;
mod overlap;
mod push;
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

#[cfg(test)]
fn combined_crossing_score(metrics: &RefineMetrics) -> usize {
    metrics.edge_node_crossings * 10 + metrics.edge_overlaps
}

/// 执行 refine 循环：统计穿障 → 推开问题节点 → 增量 re-route
pub fn run_refine(
    diagram: &Diagram,
    mut result: LayoutResult,
    _router: &dyn RoutingRecipeDyn,
    config: &RefineConfig,
) -> LayoutResult {
    if !config.enabled || config.max_passes == 0 {
        return result;
    }

    let t_cross = crate::layout::perf::Instant::now();
    let best_metrics = crossing::analyze_crossings(&result, diagram, config);
    crate::perf_log!(
        "[perf]         analyze_crossings: {:.2}ms",
        t_cross.elapsed().as_secs_f64() * 1000.0
    );
    if best_metrics.edge_node_crossings == 0 && result.groups.is_empty() {
        return result;
    }

    // Phase B: 所有图类型使用 solver，节点已冻结；push 循环已退役。
    // 穿组 / through 修复只走下方 spline fallback。
    let entry_snapshot = result.clone();

    result.hints.refine_debug = Some(crate::layout::RefineDebugStats {
        push_count: 0,
        momentum_reversals: 0,
        passes_executed: 0,
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
        let maps = crate::layout::quality::lint::GroupInteriorMaps::new(diagram);
        for edge_index in 0..result.edges.len() {
            if crate::layout::quality::lint::edge_crosses_group_interior_with_maps(
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
    let entry_group_pierces = count_group_interior_edges(diagram, &entry_snapshot);
    if after_group > entry_group_pierces {
        result = entry_snapshot;
    }

    // Phase 2 红线：refine 结束后仍穿组则外框 U 形硬修（与 lint 同口径）
    if !result.groups.is_empty()
        && count_group_interior_edges(diagram, &result) > 0
    {
        let mut grid =
            crate::layout::routing::edge_routing_orthogonal::OrthoSegmentGrid::new();
        for (ei, edge) in result.edges.iter().enumerate() {
            if edge.path_is_empty() {
                continue;
            }
            grid.insert_path(&edge.path_points().into_owned(), ei);
        }
        let sorted: Vec<String> = {
            let mut g: Vec<String> = result.groups.keys().cloned().collect();
            g.sort();
            g
        };
        let group_ctx = crate::layout::group::GroupRoutingContext::from_layout(
            diagram,
            &result,
            crate::layout::group::routing_algo_for_diagram(diagram),
        );
        let n = crate::layout::routing::edge_routing_orthogonal::repair_group_interior_crossings(
            &mut result.edges,
            diagram,
            &result.groups,
            &group_ctx,
            &sorted,
            &mut grid,
        );
        if n > 0 {
            crate::perf_log!("[perf]     refine_phase2_group_repair: repaired={}", n);
        }
    }

    result
}

fn count_group_interior_edges(diagram: &Diagram, result: &LayoutResult) -> usize {
    if result.groups.is_empty() {
        return 0;
    }
    // 与 lint / collinear 一致：按 (边, 无关组) 违规条数计。
    let report = crate::layout::quality::lint::lint_layout(diagram, result);
    crate::layout::quality::lint::LintMetricsSummary::from_report(&report).edge_crosses_group_interior
}

// E4：`repair_through_edges_post_route` / `repair_group_interior_edges_post_route`
// 已删除——穿节点/穿组改由 E1 audit violation → E3 repair intent → Coordinator
// repair loop（`reroute_edges_for_repair`）承接，不再在 D 段原地写几何。

/// Slice E3：Coordinator repair loop 的 local re-solve 入口。
///
/// 复用 `orthogonal_detour` 重路由指定边集（硬拒残 through / 新穿组）；
/// `aggressive` 启用激进裙边/换侧（原穿组 repair 语义，E4 收编）。
/// 仅重路由 `edge_indices`，其余边不动；轮次控制与 best-snapshot
/// 保留由 Coordinator 负责。
pub(crate) fn reroute_edges_for_repair(
    result: &mut LayoutResult,
    diagram: &Diagram,
    edge_indices: &HashSet<usize>,
    aggressive: bool,
) {
    if edge_indices.is_empty() {
        return;
    }
    spline_fallback::reroute_edges_with_spline_ex(
        result,
        diagram,
        edge_indices,
        &RefineConfig::default(),
        aggressive,
    );
}

// E4：`separate_trunk_overlaps_post_route` 已删除——非语义 trunk 重合由 C 期
// `separate_unrelated_trunk_overlaps`（lane 逐段偏移）承接，不再在 D 段原地重分。

// E5：`recheck_lint_pierce_post_freeze` 已删除——lint 同语义只读复校由
// `RouteAuditor::audit_extended`（E1）承载，残余 degraded 记账与
// annotation.degraded 写入由 Coordinator E3 repair loop 统一产出。

#[cfg(test)]
#[path = "refine_tests.rs"]
mod tests;
