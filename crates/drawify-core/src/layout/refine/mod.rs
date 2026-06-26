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

pub use crossing::analyze_edge_node_crossings;
pub use geometry::segment_intersects_node;
pub(crate) use geometry::segment_intersects_aabb;

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
            max_passes: 1,
            push_distance: 40.0,
            node_shrink: 2.0,
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

    let t_cross = crate::layout::perf::Instant::now();
    let best_metrics = crossing::analyze_crossings(&result, diagram, config);
    crate::perf_log!("[perf]         analyze_crossings: {:.2}ms", t_cross.elapsed().as_secs_f64() * 1000.0);
    let mut best_score = combined_crossing_score(&best_metrics);
    if best_metrics.edge_node_crossings == 0 {
        return result;
    }

    let mut best_result = result.clone();
    let mut momentum = push::MomentumHistory::new();
    let mut passes_executed = 0usize;
    let mut total_push_count = 0usize;

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

        push::push_problem_nodes(&mut result, &metrics, config, &mut momentum);
        reroute::reroute_subset(&mut result, diagram, router, &edges_to_reroute);
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

    result.hints.refine_debug = Some(crate::layout::RefineDebugStats {
        push_count: total_push_count,
        momentum_reversals: momentum.reversal_count,
        passes_executed,
    });

    result
}

#[cfg(test)]
#[path = "refine_tests.rs"]
mod tests;
