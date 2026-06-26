//! 端口冲突度
//!
//! 同侧多边汇入同一节点时，slot 分布是否合理。
//! 按邻居方向预测边在哪一侧汇入，检查每侧 slot 容量。
//! 容量 = 侧边长度 / SLOT_PITCH(40px)；需求 = 该侧边数 × SLOT_PITCH。

use crate::ast::Diagram;
use crate::layout::constants::ORTHO_SLOT_PITCH;
use crate::layout::LayoutResult;
use std::collections::HashMap;

/// 端口冲突评估结果
#[derive(Debug, Clone)]
pub struct PortConflictResult {
    /// 冲突度总分（Σ 每侧 slot 需求超出可用长度的部分）
    pub score: f64,
    /// 冲突节点
    pub conflict_nodes: Vec<PortConflictHotspot>,
}

/// 单个节点的端口冲突热点
#[derive(Debug, Clone)]
pub struct PortConflictHotspot {
    pub node_id: String,
    pub side: PortSide,
    pub required: f64,
    pub available: f64,
    pub deficit: f64,
}

/// 端口方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortSide {
    Top,
    Bottom,
    Left,
    Right,
}

/// 计算端口冲突度
pub fn evaluate(diagram: &Diagram, result: &LayoutResult) -> PortConflictResult {
    let mut neighbors: HashMap<&str, Vec<&str>> = HashMap::new();
    for rel in &diagram.relations {
        neighbors
            .entry(rel.from.as_str())
            .or_default()
            .push(rel.to.as_str());
        neighbors
            .entry(rel.to.as_str())
            .or_default()
            .push(rel.from.as_str());
    }

    let mut score = 0.0f64;
    let mut conflict_nodes = Vec::new();

    for (node_id, nl) in &result.nodes {
        let Some(neighs) = neighbors.get(node_id.as_str()) else {
            continue;
        };
        if neighs.is_empty() {
            continue;
        }

        let cx = nl.x + nl.width / 2.0;
        let cy = nl.y + nl.height / 2.0;

        let mut side_counts = [0usize; 4]; // [top, bottom, left, right]
        for &n_id in neighs {
            let Some(n_nl) = result.nodes.get(n_id) else {
                continue;
            };
            let nx = n_nl.x + n_nl.width / 2.0;
            let ny = n_nl.y + n_nl.height / 2.0;
            let dx = nx - cx;
            let dy = ny - cy;
            if dx.abs() > dy.abs() {
                if dx > 0.0 {
                    side_counts[3] += 1; // right
                } else {
                    side_counts[2] += 1; // left
                }
            } else if dy > 0.0 {
                side_counts[1] += 1; // bottom
            } else {
                side_counts[0] += 1; // top
            }
        }

        let side_lengths = [
            nl.width,  // top
            nl.width,  // bottom
            nl.height, // left
            nl.height, // right
        ];
        let sides = [
            PortSide::Top,
            PortSide::Bottom,
            PortSide::Left,
            PortSide::Right,
        ];

        for (i, &count) in side_counts.iter().enumerate() {
            if count == 0 {
                continue;
            }
            let required = count as f64 * ORTHO_SLOT_PITCH;
            let available = side_lengths[i];
            if required > available {
                let deficit = required - available;
                score += deficit;
                conflict_nodes.push(PortConflictHotspot {
                    node_id: node_id.clone(),
                    side: sides[i],
                    required,
                    available,
                    deficit,
                });
            }
        }
    }

    PortConflictResult { score, conflict_nodes }
}
