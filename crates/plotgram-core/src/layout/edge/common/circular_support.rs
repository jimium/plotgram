//! circular 布局与 circular 边路由共享的数据类型与工具。
//!
//! 此模块下沉了原先 `node::circular::common` 中被边路由反向依赖的类型，
//! 使 `edge` 模块不再 `use crate::layout::node::`。
//! `node::circular::common` 通过 re-export 保持原有路径可用。

use crate::types::DiagramType;
use crate::ast::Diagram;
use crate::layout::{LayoutHints, NodeLayout};
use std::collections::HashMap;

/// circular 布局与 circular 边路由共享的适用图表类型
///
/// `State` 是 circular 布局的默认图类型（`state` 门面共享本引擎）；
/// `Er` 允许用户显式切换到 circular 布局作为替代方案。
pub const APPLICABLE_TYPES: &[DiagramType] = &[
    DiagramType::State,
    DiagramType::Er,
];

/// 单个圆环上的节点分组（供布局写入、边路由读取）
#[derive(Debug, Clone)]
pub struct CircleGroup {
    pub center: (f64, f64),
    pub radius: f64,
    /// `diagram.entities` 中的实体下标，按圆周角序排列
    pub entity_indices: Vec<usize>,
}

/// 从节点坐标反推单个圆环分组（hints 缺失时的回退路径）
pub fn infer_single_circle_from_nodes(
    diagram: &Diagram,
    nodes: &HashMap<String, NodeLayout>,
) -> Option<CircleGroup> {
    if nodes.is_empty() {
        return None;
    }

    let mut centers = Vec::new();
    for (idx, entity) in diagram.entities.iter().enumerate() {
        let id = entity.id.as_str();
        let nl = nodes.get(id)?;
        centers.push((
            nl.x + nl.width / 2.0,
            nl.y + nl.height / 2.0,
            idx,
        ));
    }

    let cx: f64 = centers.iter().map(|c| c.0).sum::<f64>() / centers.len() as f64;
    let cy: f64 = centers.iter().map(|c| c.1).sum::<f64>() / centers.len() as f64;
    let radius: f64 = centers
        .iter()
        .map(|(x, y, _)| ((x - cx).powi(2) + (y - cy).powi(2)).sqrt())
        .sum::<f64>()
        / centers.len() as f64;

    let mut ordered: Vec<(usize, f64)> = centers
        .iter()
        .map(|(x, y, idx)| (*idx, (y - cy).atan2(x - cx)))
        .collect();
    ordered.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    Some(CircleGroup {
        center: (cx, cy),
        radius: radius.max(1.0),
        entity_indices: ordered.into_iter().map(|(idx, _)| idx).collect(),
    })
}

/// 解析圆环分组：优先使用布局阶段写入的 hints，缺失时从节点坐标反推。
pub fn resolve_circle_groups(
    diagram: &Diagram,
    nodes: &HashMap<String, NodeLayout>,
    hints: &LayoutHints,
) -> Vec<CircleGroup> {
    if let Some(circular) = &hints.circular {
        if !circular.circles.is_empty() {
            return circular.circles.clone();
        }
    }
    infer_single_circle_from_nodes(diagram, nodes)
        .map(|c| vec![c])
        .unwrap_or_default()
}
