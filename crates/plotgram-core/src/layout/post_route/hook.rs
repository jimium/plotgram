//! 路由后处理钩子：按算法声明是否需要 PRS 等 architecture 专用后处理。

use crate::ast::Diagram;
use crate::layout::{EdgeRoutingStrategy, LayoutResult};
use std::collections::HashMap;

/// 节点位移判定为「需要增量重路由」的最小欧氏距离（px）。
pub const NODE_MOVE_REROUTE_EPS: f64 = 2.0;

/// 可保留边占比低于该阈值时退回全图重路由。
pub const MIN_PRESERVE_RATIO: f64 = 0.10;

/// 路由后钩子（Phase 5：替代 `algo == "architecture"` 硬编码）。
pub(crate) trait PostRouteHook {
    fn after_route(
        &self,
        diagram: &Diagram,
        result: LayoutResult,
        router: &dyn EdgeRoutingStrategy,
        gf_spec: &crate::layout::group::frame::GroupFrameSpec,
        edge_snap_config: &crate::layout::snap::grid_snap::EdgeSnapConfig,
        leaf_pad: crate::layout::engines::common::group_bounds::GroupPadding,
    ) -> LayoutResult;
}

/// 无后处理。
pub(crate) struct NoopPostRouteHook;

impl PostRouteHook for NoopPostRouteHook {
    fn after_route(
        &self,
        _diagram: &Diagram,
        result: LayoutResult,
        _router: &dyn EdgeRoutingStrategy,
        _gf_spec: &crate::layout::group::frame::GroupFrameSpec,
        _edge_snap_config: &crate::layout::snap::grid_snap::EdgeSnapConfig,
        _leaf_pad: crate::layout::engines::common::group_bounds::GroupPadding,
    ) -> LayoutResult {
        result
    }
}

/// Architecture：PRS 扩壳 + 内容包络 + 必要时增量重路由。
pub(crate) struct ArchitecturePostRouteHook;

impl PostRouteHook for ArchitecturePostRouteHook {
    fn after_route(
        &self,
        diagram: &Diagram,
        mut result: LayoutResult,
        router: &dyn EdgeRoutingStrategy,
        gf_spec: &crate::layout::group::frame::GroupFrameSpec,
        edge_snap_config: &crate::layout::snap::grid_snap::EdgeSnapConfig,
        leaf_pad: crate::layout::engines::common::group_bounds::GroupPadding,
    ) -> LayoutResult {
        let t_prs = crate::layout::perf::Instant::now();
        let prs_grew =
            super::shell_expand::post_route_shell_expand(diagram, &mut result);
        let prs_ms = t_prs.elapsed().as_secs_f64() * 1000.0;
        if prs_grew {
            let pre_positions: HashMap<String, (f64, f64)> = result
                .nodes
                .iter()
                .map(|(id, n)| (id.clone(), (n.x, n.y)))
                .collect();
            crate::layout::group::frame::resolve_all_sibling_overlaps(gf_spec, diagram, &mut result);
            let moved_nodes = crate::layout::demand::space_budget_guard::diff_moved_nodes(
                &pre_positions,
                &result.nodes,
            );
            if !moved_nodes.is_empty() {
                result = router.route_after_node_moves(diagram, result, &moved_nodes);
            }
        }
        let container_pad =
            crate::layout::engines::common::group_bounds::container_padding_for_leaf(leaf_pad);
        crate::layout::group::frame::expand_groups_to_contain_contents(
            diagram,
            &mut result.groups,
            &result.nodes,
            leaf_pad,
            container_pad,
        );
        crate::layout::snap::grid_snap::update_canvas_bounds(
            &mut result,
            crate::layout::constants::DEFAULT_PADDING,
        );
        if prs_grew {
            super::border_repulse::repulse_edges_only(
                &mut result.edges,
                &result.groups,
                edge_snap_config,
            );
        }
        if let Some(debug) = result.hints.gutter_budget_debug.as_mut() {
            debug.prs_ms = prs_ms;
            debug.prs_grew = prs_grew;
        } else {
            result.hints.gutter_budget_debug = Some(crate::layout::GutterBudgetDebug {
                prs_ms,
                prs_grew,
                ..Default::default()
            });
        }
        result
    }
}

/// 算法后路由行为画像（替代 `algo == "architecture"` 硬编码）。
///
/// 将算法名映射到具体的 `PostRouteHook` 实例，避免在调用点散落字符串匹配。
pub(crate) enum AlgoProfile {
    /// 架构图：PRS 扩壳 + 内容包络 + 必要时增量重路由
    Architecture,
    /// 其他算法：无后处理
    Default,
}

impl AlgoProfile {
    pub(crate) fn from_algo(algo: &str) -> Self {
        match algo {
            "architecture" => AlgoProfile::Architecture,
            _ => AlgoProfile::Default,
        }
    }

    pub(crate) fn post_route_hook(self) -> Box<dyn PostRouteHook> {
        match self {
            AlgoProfile::Architecture => Box::new(ArchitecturePostRouteHook),
            AlgoProfile::Default => Box::new(NoopPostRouteHook),
        }
    }
}
