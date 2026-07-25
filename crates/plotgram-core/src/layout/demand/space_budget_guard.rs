//! SpaceBudget 兖底共享逻辑：节点位移判定 + 增量重路由。
//!
//! 抽取自 `pipeline.rs` S3 兖底块和 `route_feedback.rs` S3 兖底块，
//! 消除两处复制并修复 R-4 行为差异（route_feedback 缺 repulse_edges_only）。
//!
//! R11b：推节点逻辑已删除——coordinator 的 FrozenNodeProduct 保证 route 后不推节点；
//! budget 违规由 coordinator repair round 处理。

use crate::ast::Diagram;
use crate::layout::snap::grid_snap::EdgeSnapConfig;
use crate::layout::post_route;
use crate::layout::post_route::NODE_MOVE_REROUTE_EPS;
use crate::layout::demand::space_budget::SpaceBudget;
use crate::layout::{RoutingRecipeDyn, LayoutResult, NodeLayout};
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

/// 检测预算违规（R11b：推节点逻辑已删除，仅设置 budget hint）。
///
/// coordinator 的 FrozenNodeProduct 保证 route 后不推节点；
/// budget 违规由 coordinator repair round 处理。
/// 返回 (result, 空集合)——保留签名兼容性。
pub fn resolve_budget_violations(
    diagram: &Diagram,
    mut result: LayoutResult,
) -> (LayoutResult, HashSet<String>) {
    let budget = result
        .hints
        .space_budget
        .clone()
        .unwrap_or_else(|| SpaceBudget::from_diagram(diagram));

    result.hints.space_budget = Some(budget);
    (result, HashSet::new())
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
    router: &dyn RoutingRecipeDyn,
    moved: &HashSet<String>,
    edge_snap_config: &EdgeSnapConfig,
) -> LayoutResult {
    if !moved.is_empty() {
        // Slice B：构造 PreparedRoutingInput → router.route(&input) → 写回 edges。
        let frozen = crate::layout::routing::coordinator::FrozenNodeProduct::capture(&result);
        let input = crate::layout::routing::model::prepared::PreparedRoutingInput::prepare(
            &frozen,
            diagram,
            &result.hints,
            "",
            router.name(),
            Default::default(),
            crate::layout::routing::model::prepared::RoutingCanvas {
                width: result.total_width,
                height: result.total_height,
            },
        );
        let product = router.route(&input);
        result.edges = product.edges;
        if product.group_routing.is_some() {
            result.hints.group_routing = product.group_routing;
        }
        if product.route_annotations.is_some() {
            result.hints.route_annotations = product.route_annotations;
        }
        if product.orthogonal_debug.is_some() {
            result.hints.orthogonal_debug = product.orthogonal_debug;
        }
        post_route::repulse_edges_only(&mut result.edges, &result.groups, edge_snap_config);
    }
    result
}
