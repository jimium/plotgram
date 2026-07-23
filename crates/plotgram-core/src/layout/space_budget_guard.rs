//! SpaceBudget 兜底共享逻辑:节点位移判定 + budget 违规消解 + 增量重路由。
//!
//! 抽取自 `pipeline.rs` S3 兜底块和 `route_feedback.rs` S3 兜底块,
//! 消除两处复制并修复 R-4 行为差异(route_feedback 缺 repulse_edges_only)。

use crate::ast::Diagram;
use crate::layout::grid_snap::EdgeSnapConfig;
use crate::layout::post_route;
use crate::layout::post_route::NODE_MOVE_REROUTE_EPS;
use crate::layout::space_budget::{
    enforce_vertical_rank_gaps, has_node_aabb_overlaps, horizontal_gap_violations,
    node_group_scopes, resolve_residual_with_budget_and_ranks, reverse_relation_pairs, SpaceBudget,
};
use crate::layout::{EdgeRoutingStrategy, LayoutResult, NodeLayout};
use std::collections::{HashMap, HashSet};

/// 对比节点位移,返回移动距离 >= NODE_MOVE_REROUTE_EPS 的节点 id 集合。
///
/// 用于 space_budget 兜底和 PRS 扩壳后判定哪些节点需要增量重路由。
pub fn diff_moved_nodes(
    pre: &HashMap<String, (f64, f64)>,
    current: &HashMap<String, NodeLayout>,
) -> HashSet<String> {
    current
        .iter()
        .filter_map(|(id, n)| {
            pre.get(id).and_then(|(px, py)| {
                let dx = n.x - px;
                let dy = n.y - py;
                if (dx * dx + dy * dy).sqrt() >= NODE_MOVE_REROUTE_EPS {
                    Some(id.clone())
                } else {
                    None
                }
            })
        })
        .collect()
}

/// 检测水平缝违反或 AABB 节点重叠 → 推开 → 返回移动的节点集合。
///
/// 同时按布局 rank 执行竖向最小层缝 enforce（修复 refine 上推吃掉邻 rank 缝）。
///
/// 若无违反且 space_budget 未设置,则设置 budget hint。
/// 返回 (处理后的 result, 移动的节点集合)。
///
/// Phase 7: 使用 coordinate solver 的图类型（flowchart/state/ER）节点已冻结，
/// 跳过水平缝消解，仅保留竖向 rank 缝守约。
pub fn resolve_budget_violations(
    diagram: &Diagram,
    mut result: LayoutResult,
) -> (LayoutResult, HashSet<String>) {
    let budget = result
        .hints
        .space_budget
        .clone()
        .unwrap_or_else(|| SpaceBudget::from_diagram(diagram));
    
    // Phase B: 所有图类型使用 solver，节点已冻结，跳过水平缝消解
    
    let pre: HashMap<String, (f64, f64)> = result
        .nodes
        .iter()
        .map(|(id, n)| (id.clone(), (n.x, n.y)))
        .collect();
    
    // 竖向 rank 缝：有组图也会被 refine 推贴边（gap_y=0 不算 AABB），必须守约。
    // 依 rank 整带移动，不按投影重叠猜“同列”，避免误推无关跨组节点。
    if let Some(ranks) = result.hints.sugiyama_ranks.as_ref() {
        let scopes = node_group_scopes(diagram);
        let reverse_pairs = reverse_relation_pairs(diagram);
        enforce_vertical_rank_gaps(&mut result.nodes, &budget, ranks, &scopes, &reverse_pairs);
    }

    result.hints.space_budget = Some(budget);
    let moved = diff_moved_nodes(&pre, &result.nodes);
    if !moved.is_empty() {
        crate::perf_log!("[fallback] budget residual: {} moved nodes", moved.len());
    }
    (result, moved)
}

/// 对移动的节点做增量重路由 + repulse(对齐 pipeline.rs S3 兜底)。
///
/// 调用方可在 `resolve_budget_violations` 和此函数之间插入
/// `recompute_group_bounds` 等几何刷新(pipeline.rs 路径需要)。
///
/// `edge_snap_config` 由调用方传入(pipeline.rs 可能有 `snap:false` 覆盖)。
pub fn reroute_and_repulse(
    diagram: &Diagram,
    mut result: LayoutResult,
    router: &dyn EdgeRoutingStrategy,
    moved: &HashSet<String>,
    edge_snap_config: &EdgeSnapConfig,
) -> LayoutResult {
    if !moved.is_empty() {
        result = router.route_after_node_moves(diagram, result, moved);
        post_route::repulse_edges_only(&mut result.edges, &result.groups, edge_snap_config);
    }
    result
}
