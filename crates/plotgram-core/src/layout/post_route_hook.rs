//! 布局后路由钩子：按算法声明是否需要 PRS 等 architecture 专用后处理。

use crate::ast::Diagram;
use crate::layout::{EdgeRoutingStrategy, LayoutResult};
use std::collections::{HashMap, HashSet};

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
        gf_spec: &crate::layout::group_frame::GroupFrameSpec,
        edge_snap_config: &crate::layout::grid_snap::EdgeSnapConfig,
        leaf_pad: crate::layout::node::common::group_bounds::GroupPadding,
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
        _gf_spec: &crate::layout::group_frame::GroupFrameSpec,
        _edge_snap_config: &crate::layout::grid_snap::EdgeSnapConfig,
        _leaf_pad: crate::layout::node::common::group_bounds::GroupPadding,
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
        gf_spec: &crate::layout::group_frame::GroupFrameSpec,
        edge_snap_config: &crate::layout::grid_snap::EdgeSnapConfig,
        leaf_pad: crate::layout::node::common::group_bounds::GroupPadding,
    ) -> LayoutResult {
        let t_prs = crate::layout::perf::Instant::now();
        let prs_grew =
            crate::layout::group::post_route_shell::post_route_shell_expand(diagram, &mut result);
        let prs_ms = t_prs.elapsed().as_secs_f64() * 1000.0;
        if prs_grew {
            let pre_positions: HashMap<String, (f64, f64)> = result
                .nodes
                .iter()
                .map(|(id, n)| (id.clone(), (n.x, n.y)))
                .collect();
            crate::layout::group_frame::resolve_all_sibling_overlaps(gf_spec, diagram, &mut result);
            let moved_nodes: HashSet<String> = result
                .nodes
                .iter()
                .filter_map(|(id, n)| {
                    pre_positions.get(id).and_then(|(px, py)| {
                        let dx = n.x - px;
                        let dy = n.y - py;
                        if (dx * dx + dy * dy).sqrt() >= NODE_MOVE_REROUTE_EPS {
                            Some(id.clone())
                        } else {
                            None
                        }
                    })
                })
                .collect();
            if !moved_nodes.is_empty() {
                result = router.route_after_node_moves(diagram, result, &moved_nodes);
            }
        }
        let container_pad =
            crate::layout::node::common::group_bounds::container_padding_for_leaf(leaf_pad);
        crate::layout::group_frame::expand_groups_to_contain_contents(
            diagram,
            &mut result.groups,
            &result.nodes,
            leaf_pad,
            container_pad,
        );
        crate::layout::grid_snap::update_canvas_bounds(
            &mut result,
            crate::layout::constants::DEFAULT_PADDING,
        );
        if prs_grew {
            crate::layout::edge_postprocess::repulse_edges_only(
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

pub(crate) fn post_route_hook_for(algo: &str) -> Box<dyn PostRouteHook> {
    match algo {
        "architecture" => Box::new(ArchitecturePostRouteHook),
        _ => Box::new(NoopPostRouteHook),
    }
}
