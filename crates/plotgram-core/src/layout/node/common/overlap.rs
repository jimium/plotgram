//! 重叠消除（Overlap Removal）模块
//!
//! 提供统一的 [`OverlapResolver`] trait 和多种实现：
//! - [`ForceDirectedResolver`]：基于力导向迭代（原有逻辑）
//! - [`BruteForceResolver`]：逐对确定性推开
//! - [`ChainedResolver`]：串联多个 resolver
//!
//! 可作为力导向布局的后处理步骤，
//! 也可独立应用于任何可能出现节点重叠的布局算法。
//!
//! 参考：Graphviz 的 overlap removal (Voronoi / prism 方法的简化版)

use crate::layout::NodeLayout;
use std::collections::HashMap;

/// 重叠消除配置
#[derive(Debug, Clone)]
pub struct OverlapConfig {
    /// 节点间最小间距
    pub margin: f64,
    /// 最大迭代次数
    pub max_iterations: usize,
    /// 每次移动的步长系数（越小越保守）
    pub step_factor: f64,
}

impl Default for OverlapConfig {
    fn default() -> Self {
        Self {
            margin: 8.0,
            max_iterations: 20,
            step_factor: 0.5,
        }
    }
}

/// 重叠消除策略 trait
///
/// 不同布局算法可按需选择或组合实现。
pub trait OverlapResolver {
    /// 就地消除 `nodes` 中的重叠。
    ///
    /// - `nodes`: 节点布局（会被修改）
    /// - `sizes`: 节点尺寸映射（只读，可为空）
    /// - `config`: 配置参数
    fn resolve(
        &self,
        nodes: &mut HashMap<String, NodeLayout>,
        sizes: &HashMap<String, (f64, f64)>,
        config: &OverlapConfig,
    );
}

/// 力导向重叠消除（封装原有 [`remove_overlaps`] 逻辑）
pub struct ForceDirectedResolver {
    /// 原始中心位置约束（可选，限制移动范围）
    pub original_positions: Option<HashMap<String, (f64, f64)>>,
}

impl ForceDirectedResolver {
    pub fn new() -> Self {
        Self {
            original_positions: None,
        }
    }

    /// 从当前节点位置快照构建约束。
    pub fn with_current_centers(nodes: &HashMap<String, NodeLayout>) -> Self {
        let original_positions = nodes
            .iter()
            .map(|(id, nl)| {
                (id.clone(), (nl.x + nl.width / 2.0, nl.y + nl.height / 2.0))
            })
            .collect();
        Self {
            original_positions: Some(original_positions),
        }
    }
}

impl Default for ForceDirectedResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl OverlapResolver for ForceDirectedResolver {
    fn resolve(
        &self,
        nodes: &mut HashMap<String, NodeLayout>,
        _sizes: &HashMap<String, (f64, f64)>,
        config: &OverlapConfig,
    ) {
        let result = remove_overlaps(nodes, self.original_positions.as_ref(), config);
        *nodes = result.nodes;
    }
}

/// 逐对确定性重叠消除
///
/// 每轮扫描所有节点对，发现重叠时沿最小重叠轴推开。
/// 比力导向更确定性，但可能产生连锁位移。
pub struct BruteForceResolver {
    /// 最大迭代轮数
    pub max_rounds: usize,
}

impl BruteForceResolver {
    pub fn new(max_rounds: usize) -> Self {
        Self { max_rounds }
    }
}

impl Default for BruteForceResolver {
    fn default() -> Self {
        Self::new(10)
    }
}

impl OverlapResolver for BruteForceResolver {
    fn resolve(
        &self,
        nodes: &mut HashMap<String, NodeLayout>,
        _sizes: &HashMap<String, (f64, f64)>,
        config: &OverlapConfig,
    ) {
        // 排序保证迭代顺序确定（HashMap 迭代顺序随机）
        let mut ids: Vec<String> = nodes.keys().cloned().collect();
        ids.sort();
        let margin = config.margin;
        for _ in 0..self.max_rounds {
            let mut moved = false;
            for i in 0..ids.len() {
                for j in (i + 1)..ids.len() {
                    let (a_x, a_y, a_w, a_h) = {
                        let n = &nodes[&ids[i]];
                        (n.x, n.y, n.width, n.height)
                    };
                    let (b_x, b_y, b_w, b_h) = {
                        let n = &nodes[&ids[j]];
                        (n.x, n.y, n.width, n.height)
                    };

                    let overlap_x = (a_x + a_w + margin > b_x) && (b_x + b_w + margin > a_x);
                    let overlap_y = (a_y + a_h + margin > b_y) && (b_y + b_h + margin > a_y);
                    if !overlap_x || !overlap_y {
                        continue;
                    }

                    moved = true;
                    let a_cx = a_x + a_w / 2.0;
                    let a_cy = a_y + a_h / 2.0;
                    let b_cx = b_x + b_w / 2.0;
                    let b_cy = b_y + b_h / 2.0;

                    let overlap_x_amount =
                        (a_x + a_w + margin - b_x).min(b_x + b_w + margin - a_x);
                    let overlap_y_amount =
                        (a_y + a_h + margin - b_y).min(b_y + b_h + margin - a_y);

                    if overlap_x_amount < overlap_y_amount {
                        let shift = overlap_x_amount / 2.0 + 1.0;
                        let dir = if a_cx <= b_cx { -1.0 } else { 1.0 };
                        if let Some(nl) = nodes.get_mut(&ids[i]) {
                            nl.x += dir * shift;
                        }
                        if let Some(nl) = nodes.get_mut(&ids[j]) {
                            nl.x -= dir * shift;
                        }
                    } else {
                        let shift = overlap_y_amount / 2.0 + 1.0;
                        let dir = if a_cy <= b_cy { -1.0 } else { 1.0 };
                        if let Some(nl) = nodes.get_mut(&ids[i]) {
                            nl.y += dir * shift;
                        }
                        if let Some(nl) = nodes.get_mut(&ids[j]) {
                            nl.y -= dir * shift;
                        }
                    }
                }
            }
            if !moved {
                break;
            }
        }
    }
}

/// 串联多个 resolver，按顺序执行
pub struct ChainedResolver {
    pub resolvers: Vec<Box<dyn OverlapResolver>>,
}

impl ChainedResolver {
    pub fn new(resolvers: Vec<Box<dyn OverlapResolver>>) -> Self {
        Self { resolvers }
    }
}

impl OverlapResolver for ChainedResolver {
    fn resolve(
        &self,
        nodes: &mut HashMap<String, NodeLayout>,
        sizes: &HashMap<String, (f64, f64)>,
        config: &OverlapConfig,
    ) {
        for resolver in &self.resolvers {
            resolver.resolve(nodes, sizes, config);
        }
    }
}

/// 重叠检测结果
#[derive(Debug, Clone)]
pub struct OverlapResult {
    /// 消除重叠后的节点布局
    pub nodes: HashMap<String, NodeLayout>,
    /// 是否还有残留重叠
    pub has_residual_overlap: bool,
    /// 迭代次数
    pub iterations: usize,
}

/// 消除节点重叠（保留原始位置映射）
///
/// # Arguments
/// - `nodes`: 当前节点布局（会被移动但尽量靠近原始位置）
/// - `original_positions`: 原始中心位置（用于约束移动范围）
/// - `config`: 重叠消除配置
pub fn remove_overlaps(
    nodes: &HashMap<String, NodeLayout>,
    original_positions: Option<&HashMap<String, (f64, f64)>>,
    config: &OverlapConfig,
) -> OverlapResult {
    // 排序保证迭代顺序确定（HashMap 迭代顺序随机）
    let mut ids: Vec<String> = nodes.keys().cloned().collect();
    ids.sort();
    if ids.len() <= 1 {
        return OverlapResult {
            nodes: nodes.clone(),
            has_residual_overlap: false,
            iterations: 0,
        };
    }

    let mut current = nodes.clone();

    for _iter in 0..config.max_iterations {
        let mut moved = false;
        let mut max_overlap = 0.0f64;

        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                let left = &ids[i];
                let right = &ids[j];

                let nl = current.get(left).unwrap();
                let nr = current.get(right).unwrap();

                let (overlap_x, overlap_y) = compute_overlap(nl, nr, config.margin);
                if overlap_x <= 0.0 || overlap_y <= 0.0 {
                    continue;
                }

                moved = true;
                max_overlap = max_overlap.max(overlap_x.min(overlap_y));

                // 选择最小移动方向
                let (shift, dir_left, dir_right) = if overlap_x < overlap_y {
                    let s = overlap_x / 2.0 * config.step_factor;
                    let d = if nl.x + nl.width / 2.0 <= nr.x + nr.width / 2.0 {
                        (-s, s)
                    } else {
                        (s, -s)
                    };
                    ((d.0, 0.0), d, (-d.0, -d.1))
                } else {
                    let s = overlap_y / 2.0 * config.step_factor;
                    let d = if nl.y + nl.height / 2.0 <= nr.y + nr.height / 2.0 {
                        (-s, s)
                    } else {
                        (s, -s)
                    };
                    ((0.0, d.0), d, (-d.0, -d.1))
                };

                // 应用位移，但受原始位置约束
                apply_constrained_shift(
                    &mut current, left, shift, original_positions,
                );
                apply_constrained_shift(
                    &mut current, right, (-shift.0, -shift.1), original_positions,
                );

                let _ = (dir_left, dir_right);
            }
        }

        if !moved || max_overlap < 0.5 {
            break;
        }
    }

    // 检查残留重叠
    let has_residual = check_residual_overlaps(&current, &ids, config.margin);

    OverlapResult {
        nodes: current,
        has_residual_overlap: has_residual,
        iterations: config.max_iterations,
    }
}

/// 计算两个节点矩形之间的重叠量
fn compute_overlap(nl: &NodeLayout, nr: &NodeLayout, margin: f64) -> (f64, f64) {
    let cx_dist = (nl.x + nl.width / 2.0 - nr.x - nr.width / 2.0).abs();
    let cy_dist = (nl.y + nl.height / 2.0 - nr.y - nr.height / 2.0).abs();
    let half_w = (nl.width + nr.width) / 2.0 + margin;
    let half_h = (nl.height + nr.height) / 2.0 + margin;

    (half_w - cx_dist, half_h - cy_dist)
}

/// 应用位移但受原始位置约束
fn apply_constrained_shift(
    nodes: &mut HashMap<String, NodeLayout>,
    id: &str,
    (dx, dy): (f64, f64),
    original_positions: Option<&HashMap<String, (f64, f64)>>,
) {
    if let Some(nl) = nodes.get(id) {
        let mut new_x = nl.x + dx;
        let mut new_y = nl.y + dy;

        // 约束：不能离开原始位置太远
        if let Some(originals) = original_positions {
            if let Some(&(ox, oy)) = originals.get(id) {
                let max_dist = (nl.width.min(nl.height) * 0.8).max(60.0);
                let cx = new_x + nl.width / 2.0;
                let cy = new_y + nl.height / 2.0;
                let dist = ((cx - ox).powi(2) + (cy - oy).powi(2)).sqrt();
                if dist > max_dist {
                    let scale = max_dist / dist;
                    new_x = ox + (cx - ox) * scale - nl.width / 2.0;
                    new_y = oy + (cy - oy) * scale - nl.height / 2.0;
                }
            }
        }

        nodes.insert(id.to_string(), NodeLayout {
            x: new_x,
            y: new_y,
            ..*nl
        });
    }
}

/// 检查是否有残留重叠
fn check_residual_overlaps(
    nodes: &HashMap<String, NodeLayout>,
    ids: &[String],
    margin: f64,
) -> bool {
    for i in 0..ids.len() {
        for j in (i + 1)..ids.len() {
            let nl = nodes.get(&ids[i]).unwrap();
            let nr = nodes.get(&ids[j]).unwrap();
            let (ox, oy) = compute_overlap(nl, nr, margin);
            if ox > 0.5 && oy > 0.5 {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remove_overlap_non_overlapping() {
        let nodes = HashMap::from([
            ("a".into(), NodeLayout { x: 0.0, y: 0.0, width: 100.0, height: 50.0, ..Default::default() }),
            ("b".into(), NodeLayout { x: 200.0, y: 0.0, width: 100.0, height: 50.0, ..Default::default() }),
        ]);

        let result = remove_overlaps(&nodes, None, &OverlapConfig::default());
        // 不重叠，应保持不变
        let a = result.nodes.get("a").unwrap();
        assert!((a.x - 0.0).abs() < 0.1);
    }

    #[test]
    fn test_remove_overlap_overlapping() {
        let nodes = HashMap::from([
            ("a".into(), NodeLayout { x: 0.0, y: 0.0, width: 100.0, height: 50.0, ..Default::default() }),
            ("b".into(), NodeLayout { x: 50.0, y: 10.0, width: 100.0, height: 50.0, ..Default::default() }),
        ]);

        let result = remove_overlaps(&nodes, None, &OverlapConfig::default());
        // 重叠应被消除
        let a = result.nodes.get("a").unwrap();
        let b = result.nodes.get("b").unwrap();
        let (ox, oy) = compute_overlap(a, b, OverlapConfig::default().margin);
        assert!(ox <= 0.5 || oy <= 0.5);
    }

    #[test]
    fn test_compute_overlap_no_overlap() {
        let a = NodeLayout { x: 0.0, y: 0.0, width: 100.0, height: 50.0, ..Default::default() };
        let b = NodeLayout { x: 200.0, y: 100.0, width: 100.0, height: 50.0, ..Default::default() };
        let (ox, oy) = compute_overlap(&a, &b, 8.0);
        assert!(ox < 0.0 || oy < 0.0, "nodes far apart should not overlap in at least one axis");
    }

    #[test]
    fn test_compute_overlap_with_overlap() {
        let a = NodeLayout { x: 0.0, y: 0.0, width: 100.0, height: 50.0, ..Default::default() };
        let b = NodeLayout { x: 50.0, y: 10.0, width: 100.0, height: 50.0, ..Default::default() };
        let (ox, oy) = compute_overlap(&a, &b, 8.0);
        assert!(ox > 0.0);
        assert!(oy > 0.0);
    }
}