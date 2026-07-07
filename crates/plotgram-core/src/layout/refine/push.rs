//! 问题节点推开与 momentum 抑制震荡。

use crate::layout::LayoutResult;
use std::collections::HashMap;

use super::{RefineConfig, RefineMetrics};

/// 节点推动 momentum 历史，检测方向反转并施加衰减以抑制震荡。
#[derive(Default)]
pub struct MomentumHistory {
    prev_direction: HashMap<String, (f64, f64)>,
    pub reversal_count: usize,
}

impl MomentumHistory {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn damp(&mut self, node_id: &str, fx: f64, fy: f64) -> (f64, f64) {
        if let Some(&(px, py)) = self.prev_direction.get(node_id) {
            let dot = fx * px + fy * py;
            if dot < 0.0 {
                self.reversal_count += 1;
                return (fx * 0.5, fy * 0.5);
            }
        }
        (fx, fy)
    }

    pub(crate) fn update(&mut self, node_id: &str, fx: f64, fy: f64) {
        let len = (fx * fx + fy * fy).sqrt();
        if len > f64::EPSILON {
            self.prev_direction
                .insert(node_id.to_string(), (fx / len, fy / len));
        }
    }
}

/// 对问题节点沿累积推开方向位移 `push_distance`。
pub(crate) fn push_problem_nodes(
    result: &mut LayoutResult,
    metrics: &RefineMetrics,
    config: &RefineConfig,
    momentum: &mut MomentumHistory,
) {
    let mut node_ids: Vec<&String> = metrics.problem_nodes.keys().collect();
    node_ids.sort();
    for node_id in &node_ids {
        let nid = node_id.as_str();
        let info = &metrics.problem_nodes[*node_id];
        let len = (info.push_fx * info.push_fx + info.push_fy * info.push_fy).sqrt();
        if len < f64::EPSILON {
            continue;
        }
        let (damped_fx, damped_fy) = momentum.damp(nid, info.push_fx, info.push_fy);
        let damped_len = (damped_fx * damped_fx + damped_fy * damped_fy).sqrt();
        if damped_len < f64::EPSILON {
            continue;
        }
        let dx = damped_fx / damped_len * config.push_distance;
        let dy = damped_fy / damped_len * config.push_distance;
        if let Some(nl) = result.nodes.get_mut(nid) {
            nl.x += dx;
            nl.y += dy;
        }
        momentum.update(nid, info.push_fx, info.push_fy);
    }
}
