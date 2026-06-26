//! 正交通道占用度（通道拥堵度）
//!
//! Phase 1.5 改进：原 RUDY 直线密度与 slab/正交路由器脱钩（Pearson r=0.01）。
//! 新度量直接建模正交路由的瓶颈——节点行/列之间的"间隙"。
//!
//! 每条边若其两端位于不同行（列），则必须跨越其间所有水平（垂直）间隙；
//! 统计每个间隙被多少条边跨越，取峰值（水平 + 垂直）作为拥堵度分数。
//! 热点区域为峰值间隙的包围框。

use crate::ast::Diagram;
use crate::layout::LayoutResult;

/// 通道拥堵度评估结果
#[derive(Debug, Clone)]
pub struct CongestionResult {
    /// 拥堵度分数（穿过同一间隙的最大边数，≥ 0；越高越拥堵）
    pub score: f64,
    /// 热点区域（峰值间隙的包围框）
    pub hotspot_bbox: Option<(f64, f64, f64, f64)>,
}

/// 二分查找：定位被 [lo, hi] 完全包含的间隙范围
///
/// 间隙 i 在 `centers[i]` 与 `centers[i+1]` 之间。间隙 i 被完全包含当且仅当：
/// `centers[i] >= lo && centers[i+1] <= hi`。
///
/// 返回 `[start, end)` 区间，其中所有间隙都满足上述条件。
/// 复杂度 O(log N)，替代原 O(N) 线性扫描。
#[inline]
fn gap_range(centers: &[f64], lo: f64, hi: f64, num_gaps: usize) -> std::ops::Range<usize> {
    let start = centers.partition_point(|&y| y < lo);
    // centers[1..] 中第一个 > hi 的索引，即第一个不满足 centers[i+1] <= hi 的间隙
    let end = centers[1..].partition_point(|&y| y <= hi);
    start..end.min(num_gaps)
}

/// 计算正交通道占用度
pub fn evaluate(diagram: &Diagram, result: &LayoutResult) -> CongestionResult {
    if diagram.relations.is_empty() || result.nodes.is_empty() {
        return CongestionResult {
            score: 0.0,
            hotspot_bbox: None,
        };
    }

    // 收集节点中心 y / x，去重排序，定义间隙
    let mut y_centers: Vec<f64> = result
        .nodes
        .values()
        .map(|n| n.y + n.height / 2.0)
        .collect();
    y_centers.sort_by(|a, b| a.partial_cmp(b).unwrap());
    y_centers.dedup_by(|a, b| (*a - *b).abs() < 1.0);

    let mut x_centers: Vec<f64> = result
        .nodes
        .values()
        .map(|n| n.x + n.width / 2.0)
        .collect();
    x_centers.sort_by(|a, b| a.partial_cmp(b).unwrap());
    x_centers.dedup_by(|a, b| (*a - *b).abs() < 1.0);

    if y_centers.len() < 2 && x_centers.len() < 2 {
        return CongestionResult {
            score: 0.0,
            hotspot_bbox: None,
        };
    }

    let mut h_demand = vec![0usize; y_centers.len().saturating_sub(1)];
    let mut v_demand = vec![0usize; x_centers.len().saturating_sub(1)];

    for rel in &diagram.relations {
        let (Some(from), Some(to)) =
            (result.nodes.get(rel.from.as_str()), result.nodes.get(rel.to.as_str()))
        else {
            continue;
        };
        let fy = from.y + from.height / 2.0;
        let ty = to.y + to.height / 2.0;
        let fx = from.x + from.width / 2.0;
        let tx = to.x + to.width / 2.0;

        // 水平间隙 i（位于 y_centers[i] 与 y_centers[i+1] 之间）被跨越：
        // 间隙完全落在 [y_lo, y_hi] 内部
        if !h_demand.is_empty() {
            let (y_lo, y_hi) = if fy <= ty { (fy, ty) } else { (ty, fy) };
            let range = gap_range(&y_centers, y_lo, y_hi, h_demand.len());
            for i in range {
                h_demand[i] += 1;
            }
        }

        // 垂直间隙同理
        if !v_demand.is_empty() {
            let (x_lo, x_hi) = if fx <= tx { (fx, tx) } else { (tx, fx) };
            let range = gap_range(&x_centers, x_lo, x_hi, v_demand.len());
            for i in range {
                v_demand[i] += 1;
            }
        }
    }

    // enumerate() 产出 (index, value)，max_by_key 按 value 取最大
    let (peak_h_idx, peak_h) = h_demand
        .iter()
        .copied()
        .enumerate()
        .max_by_key(|&(_, c)| c)
        .unwrap_or((0, 0));
    let (peak_v_idx, peak_v) = v_demand
        .iter()
        .copied()
        .enumerate()
        .max_by_key(|&(_, c)| c)
        .unwrap_or((0, 0));

    let score = (peak_h + peak_v) as f64;
    if score < f64::EPSILON {
        return CongestionResult {
            score: 0.0,
            hotspot_bbox: None,
        };
    }

    // 热点包围框：取峰值更高的那个间隙，扩展为全图宽/高的条带
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for nl in result.nodes.values() {
        min_x = min_x.min(nl.x);
        min_y = min_y.min(nl.y);
        max_x = max_x.max(nl.x + nl.width);
        max_y = max_y.max(nl.y + nl.height);
    }

    let hotspot_bbox = if peak_h >= peak_v && peak_h > 0 {
        // 水平间隙条带：横跨全图宽度
        Some((
            min_x,
            y_centers[peak_h_idx],
            max_x,
            y_centers[peak_h_idx + 1],
        ))
    } else if peak_v > 0 {
        // 垂直间隙条带：纵跨全图高度
        Some((
            x_centers[peak_v_idx],
            min_y,
            x_centers[peak_v_idx + 1],
            max_y,
        ))
    } else {
        None
    };

    CongestionResult { score, hotspot_bbox }
}
