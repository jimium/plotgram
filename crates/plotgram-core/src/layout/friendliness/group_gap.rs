//! group 间距充裕度
//!
//! 对每对相邻 group，计算其实际间距与跨 group 边所需通道宽度（cross_edges × 16px）的差值。
//! 间距不足 → 跨 group 边被迫绕 group 外圈，产生长绕行。

use crate::ast::Diagram;
use crate::layout::{GroupLayout, LayoutResult};
use std::collections::HashMap;

/// 每条跨 group 边所需通道宽度（经验值，取 orthogonal COMPACT_SLOT_PITCH）
const EDGE_CHANNEL_WIDTH: f64 = 16.0;

/// group 间距评估结果
#[derive(Debug, Clone)]
pub struct GroupGapResult {
    /// 间距缺口总和（Σ max(0, 所需通道宽 - 实际间距)）
    pub deficit: f64,
    /// 不足的 group 对
    pub insufficient_pairs: Vec<GroupGapHotspot>,
}

/// 单个 group 间距不足热点
#[derive(Debug, Clone)]
pub struct GroupGapHotspot {
    pub group1: String,
    pub group2: String,
    pub gap: f64,
    pub required: f64,
    pub deficit: f64,
}

/// 计算 group 间距充裕度
pub fn evaluate(diagram: &Diagram, result: &LayoutResult) -> GroupGapResult {
    if result.groups.len() < 2 {
        return GroupGapResult {
            deficit: 0.0,
            insufficient_pairs: vec![],
        };
    }

    let entity_group: HashMap<&str, &str> = diagram
        .entities
        .iter()
        .filter_map(|e| e.group_id.as_ref().map(|g| (e.id.as_str(), g.as_str())))
        .collect();

    // 预计算 group-pair 跨边计数：O(|E|)，替代原 O(|G|^2 * |E|)
    let mut cross_counts: HashMap<(&str, &str), usize> = HashMap::new();
    for rel in &diagram.relations {
        let from_g = entity_group.get(rel.from.as_str()).copied();
        let to_g = entity_group.get(rel.to.as_str()).copied();
        if let (Some(a), Some(b)) = (from_g, to_g) {
            if a != b {
                let key = if a < b { (a, b) } else { (b, a) };
                *cross_counts.entry(key).or_default() += 1;
            }
        }
    }

    let groups: Vec<(&String, &GroupLayout)> = result.groups.iter().collect();
    let mut deficit = 0.0f64;
    let mut insufficient_pairs = Vec::new();

    for i in 0..groups.len() {
        for j in (i + 1)..groups.len() {
            let (g1_id, g1) = groups[i];
            let (g2_id, g2) = groups[j];

            let gap = aabb_gap(
                (g1.x, g1.y, g1.x + g1.width, g1.y + g1.height),
                (g2.x, g2.y, g2.x + g2.width, g2.y + g2.height),
            );
            if gap.is_infinite() {
                continue;
            }

            let key = if g1_id.as_str() < g2_id.as_str() {
                (g1_id.as_str(), g2_id.as_str())
            } else {
                (g2_id.as_str(), g1_id.as_str())
            };
            let cross_edges = cross_counts.get(&key).copied().unwrap_or(0);
            if cross_edges == 0 {
                continue;
            }

            let required = cross_edges as f64 * EDGE_CHANNEL_WIDTH;
            if required > gap {
                let d = required - gap;
                deficit += d;
                insufficient_pairs.push(GroupGapHotspot {
                    group1: g1_id.clone(),
                    group2: g2_id.clone(),
                    gap,
                    required,
                    deficit: d,
                });
            }
        }
    }

    GroupGapResult {
        deficit,
        insufficient_pairs,
    }
}

/// 两个 AABB 的最小间距（重叠时返回 +inf）
fn aabb_gap(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> f64 {
    let dx = (a.0 - b.2).max(b.0 - a.2).max(0.0);
    let dy = (a.1 - b.3).max(b.1 - a.3).max(0.0);
    if dx == 0.0 && dy == 0.0 {
        f64::INFINITY
    } else {
        (dx * dx + dy * dy).sqrt()
    }
}
