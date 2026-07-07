//! 穿障预测度
//!
//! 基于直线连接（center→center）预测穿障概率，作为 bezier / straight 路由的友好度估计。
//! 复用 `refine::segment_intersects_aabb` 做线段-AABB 相交检测。

use crate::ast::Diagram;
use crate::layout::geometry::{Point, Rect};
use crate::layout::LayoutResult;
use crate::layout::refine::segment_intersects_aabb;
use super::node_outside_segment_bbox;

/// 穿障预测评估结果
#[derive(Debug, Clone)]
pub struct CrossingPredictResult {
    /// 预测穿障总数
    pub count: usize,
    /// 热点边索引（有穿障的边）
    pub edge_indices: Vec<usize>,
    /// 每条热点边的穿障次数
    pub edge_crossing_counts: Vec<(usize, usize)>,
}

/// 计算穿障预测度
///
/// 对每条边用直线（from 中心 → to 中心）检测穿过非端点节点的次数。
///
/// Phase 1.5：移除 margin 膨胀。margin 膨胀虽能提高 vs edge_crossings 相关性，
/// 但降低了 vs edge_node_crossings 相关性（0.56 → 0.52），而 enc 是更严格的验收
/// 阈值（> 0.6）。直接用节点 AABB 做相交检测。
pub fn evaluate(diagram: &Diagram, result: &LayoutResult) -> CrossingPredictResult {
    let mut count = 0;
    let mut edge_indices = Vec::new();
    let mut edge_crossing_counts = Vec::new();

    for (i, rel) in diagram.relations.iter().enumerate() {
        let (Some(from), Some(to)) =
            (result.nodes.get(rel.from.as_str()), result.nodes.get(rel.to.as_str()))
        else {
            continue;
        };
        let p1 = Point::new(from.x + from.width / 2.0, from.y + from.height / 2.0);
        let p2 = Point::new(to.x + to.width / 2.0, to.y + to.height / 2.0);

        let mut edge_crossings = 0;
        for (node_id, nl) in &result.nodes {
            if node_id == rel.from.as_str() || node_id == rel.to.as_str() {
                continue;
            }
            // AABB 预过滤：快速跳过远距离节点
            if node_outside_segment_bbox(p1, p2, nl) {
                continue;
            }

            if segment_intersects_aabb(p1, p2, Rect::from(nl)) {
                edge_crossings += 1;
                count += 1;
            }
        }

        if edge_crossings > 0 {
            edge_indices.push(i);
            edge_crossing_counts.push((i, edge_crossings));
        }
    }

    CrossingPredictResult {
        count,
        edge_indices,
        edge_crossing_counts,
    }
}
