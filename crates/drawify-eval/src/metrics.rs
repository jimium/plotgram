//! 布局质量指标计算模块
//!
//! 从 `LayoutResult` + `Diagram` 中提取一组可量化的质量指标，
//! 用于客观评估和对比不同布局算法 / 边路由策略的效果。
//!
//! 指标分类：
//! - 正确性：节点重叠、边穿节点（越低越好，理想值 0）
//! - 可读性：边交叉数（越低越好）
//! - 紧凑性：总面积、边总长度（越低越好）
//! - 均匀性：边长度方差（越低越好）
//! - 美观性：宽高比（接近 1.0 或黄金比更佳）

use drawify_core::types::DiagramType;
use drawify_core::ast::{Diagram};
use drawify_core::layout::{EdgeLayout, LayoutResult, NodeLayout};
use drawify_core::layout::refine::segment_intersects_node;
use drawify_core::layout::geometry::Point;
use std::collections::HashMap;

/// 布局质量评估结果
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LayoutMetrics {
    // ── 正确性指标（越低越好，理想值 0）──
    /// 节点重叠对数
    pub node_overlap_pairs: usize,
    /// 边穿过节点数（仅统计非起终节点的穿越）
    pub edge_node_crossings: usize,

    // ── 可读性指标 ──
    /// 边交叉数（两两边在非共享端点处的交叉）
    pub edge_crossings: usize,

    // ── 紧凑性指标 ──
    /// 画布总面积（total_width × total_height）
    pub total_area: f64,
    /// 所有边的长度之和（欧氏距离）
    pub total_edge_length: f64,
    /// 节点总面积
    pub total_node_area: f64,

    // ── 均匀性指标 ──
    /// 边长度标准差
    pub edge_length_stddev: f64,
    /// 边长度变异系数（stddev / mean，无量纲）
    pub edge_length_cv: f64,

    // ── 美观性指标 ──
    /// 画布宽高比（≥ 1.0）
    pub aspect_ratio: f64,
    /// 面积利用率（节点面积 / 画布面积）
    pub area_utilization: f64,

    // ── 路由友好性预测指标（事前度量，越高越不友好）──
    // Phase 0 候选度量，用于与事后 edge_node_crossings / edge_crossings 做相关性校准。
    // Phase 1.5：channel_congestion 由 RUDY 直线密度改为正交通道占用度
    // （RUDY 与 slab/正交路由器脱钩，r=0.01；正交通道占用度直接度量边竞争同一间隙的压力）。
    /// 正交通道占用度（穿过同一水平/垂直间隙的最大边数，≥ 0）
    #[serde(default)]
    pub channel_congestion: f64,
    /// 长边跨层数（rank 跨度 > 1 的边数；需 Sugiyama rank）
    #[serde(default)]
    pub long_edge_count: usize,
    /// group 间距缺口（Σ max(0, 所需通道宽 - 实际间距)）
    #[serde(default)]
    pub group_gap_deficit: f64,
    /// 穿障预测数（直线 center→center 穿过非端点节点的次数）
    #[serde(default)]
    pub predicted_crossings: usize,
    /// 端口冲突度（Σ 每侧 slot 需求超出可用长度的部分）
    #[serde(default)]
    pub port_conflict_score: f64,

    // ── 元信息 ──
    /// 节点数量
    pub node_count: usize,
    /// 边数量
    pub edge_count: usize,
}

impl LayoutMetrics {
    /// 从布局结果和图定义计算所有质量指标
    pub fn compute(diagram: &Diagram, result: &LayoutResult) -> Self {
        let node_overlap_pairs = count_node_overlaps(&result.nodes);
        let edge_node_crossings = count_edge_node_crossings(diagram, result);
        let edge_crossings = count_edge_crossings(result);

        let total_area = result.total_width * result.total_height;
        let total_edge_length = compute_total_edge_length(result);
        let total_node_area: f64 = result.nodes.values().map(|n| n.width * n.height).sum();

        let edge_lengths = compute_edge_lengths(result);
        let (edge_length_stddev, edge_length_cv) = compute_stddev_cv(&edge_lengths);

        let aspect_ratio = if result.total_height > 0.0 {
            (result.total_width / result.total_height)
                .max(1.0 / result.total_height.max(0.01) * result.total_width.max(0.01))
        } else {
            1.0
        };
        // 规范化：始终 >= 1.0
        let aspect_ratio = if aspect_ratio < 1.0 {
            1.0 / aspect_ratio
        } else {
            aspect_ratio
        };

        let area_utilization = if total_area > 0.0 {
            total_node_area / total_area
        } else {
            0.0
        };

        // ── 路由友好性预测指标（Phase 0 候选度量）──
        let channel_congestion = compute_channel_congestion(diagram, result);
        let long_edge_count = compute_long_edge_count(diagram, result);
        let group_gap_deficit = compute_group_gap_deficit(diagram, result);
        let predicted_crossings = compute_predicted_crossings(diagram, result);
        let port_conflict_score = compute_port_conflict_score(diagram, result);

        Self {
            node_overlap_pairs,
            edge_node_crossings,
            edge_crossings,
            total_area,
            total_edge_length,
            total_node_area,
            edge_length_stddev,
            edge_length_cv,
            aspect_ratio,
            area_utilization,
            channel_congestion,
            long_edge_count,
            group_gap_deficit,
            predicted_crossings,
            port_conflict_score,
            node_count: result.nodes.len(),
            edge_count: result.edges.len(),
        }
    }

    /// 构造零值指标（用于超时等异常场景）
    pub fn zero_for(diagram: &Diagram) -> Self {
        Self {
            node_overlap_pairs: 0,
            edge_node_crossings: 0,
            edge_crossings: 0,
            total_area: 0.0,
            total_edge_length: 0.0,
            total_node_area: 0.0,
            edge_length_stddev: 0.0,
            edge_length_cv: 1.0,
            aspect_ratio: 1.0,
            area_utilization: 0.0,
            channel_congestion: 0.0,
            long_edge_count: 0,
            group_gap_deficit: 0.0,
            predicted_crossings: 0,
            port_conflict_score: 0.0,
            node_count: diagram.entities.len(),
            edge_count: diagram.relations.len(),
        }
    }

    /// 综合质量评分（0~100，越高越好）
    ///
    /// 评分维度及权重：
    /// - 正确性（40%）：节点重叠 + 边穿节点 + 边交叉
    /// - 紧凑性（20%）：面积利用率
    /// - 均匀性（20%）：边长 CV
    /// - 美观性（20%）：宽高比偏离度
    pub fn quality_score(&self) -> f64 {
        // ── 正确性（40%）──
        // 每个错误项扣分，归一化到边数
        let edge_count = self.edge_count.max(1) as f64;
        let node_count = self.node_count.max(1) as f64;
        let overlap_penalty = (self.node_overlap_pairs as f64
            / (node_count * (node_count - 1.0) / 2.0).max(1.0))
        .min(1.0);
        let edge_node_penalty = (self.edge_node_crossings as f64 / edge_count).min(1.0);
        let edge_cross_penalty = (self.edge_crossings as f64 / edge_count).min(1.0);
        let correctness = (1.0 - overlap_penalty) * 0.4
            + (1.0 - edge_node_penalty) * 0.35
            + (1.0 - edge_cross_penalty) * 0.25;

        // ── 紧凑性（20%）──
        // 面积利用率越高越好，上限 50% 视为满分
        let compactness = (self.area_utilization / 0.5).min(1.0);

        // ── 均匀性（20%）──
        // CV 越低越好，0 为满分，1.0 为 0 分
        let uniformity = (1.0 - self.edge_length_cv).max(0.0);

        // ── 美观性（20%）──
        // 宽高比越接近 1.0 越好，偏离越大扣分
        // 理想范围 1.0~1.6（黄金比），>5 严重偏离
        let ideal_ratio = 1.6;
        let ratio_deviation = if self.aspect_ratio <= ideal_ratio {
            0.0
        } else {
            (self.aspect_ratio - ideal_ratio) / 4.0 // >5.6 时 dev=1.0
        };
        let aesthetics = (1.0 - ratio_deviation).max(0.0);

        let score = correctness * 40.0 + compactness * 20.0 + uniformity * 20.0 + aesthetics * 20.0;
        (score * 100.0).round() / 100.0 // 保留两位小数
    }

    /// 生成单行摘要（适合终端输出）
    pub fn one_line_summary(&self) -> String {
        format!(
            "nodes={} edges={} overlaps={} edge_x_node={} edge_x_edge={} area={:.0} edge_len={:.1}±{:.1} cv={:.2} ratio={:.2} util={:.1}% score={:.1}",
            self.node_count,
            self.edge_count,
            self.node_overlap_pairs,
            self.edge_node_crossings,
            self.edge_crossings,
            self.total_area,
            self.total_edge_length,
            self.edge_length_stddev,
            self.edge_length_cv,
            self.aspect_ratio,
            self.area_utilization * 100.0,
            self.quality_score(),
        )
    }

    /// 使用自定义权重计算综合质量评分
    pub fn quality_score_with_weights(&self, weights: &MetricWeights) -> f64 {
        let edge_count = self.edge_count.max(1) as f64;
        let node_count = self.node_count.max(1) as f64;
        let overlap_penalty = (self.node_overlap_pairs as f64
            / (node_count * (node_count - 1.0) / 2.0).max(1.0))
        .min(1.0);
        let edge_node_penalty = (self.edge_node_crossings as f64 / edge_count).min(1.0);
        let edge_cross_penalty = (self.edge_crossings as f64 / edge_count).min(1.0);
        let correctness = (1.0 - overlap_penalty) * 0.4
            + (1.0 - edge_node_penalty) * 0.35
            + (1.0 - edge_cross_penalty) * 0.25;

        let compactness = (self.area_utilization / 0.5).min(1.0);
        let uniformity = (1.0 - self.edge_length_cv).max(0.0);

        let ideal_ratio = 1.6;
        let ratio_deviation = if self.aspect_ratio <= ideal_ratio {
            0.0
        } else {
            (self.aspect_ratio - ideal_ratio) / 4.0
        };
        let aesthetics = (1.0 - ratio_deviation).max(0.0);

        let score = correctness * weights.correctness * 100.0
            + compactness * weights.compactness * 100.0
            + uniformity * weights.uniformity * 100.0
            + aesthetics * weights.aesthetics * 100.0;
        (score * 100.0).round() / 100.0
    }

    /// 质量等级
    pub fn grade(&self) -> QualityGrade {
        QualityGrade::from_score(self.quality_score())
    }

    /// 使用自定义权重计算质量等级
    pub fn grade_with_weights(&self, weights: &MetricWeights) -> QualityGrade {
        QualityGrade::from_score(self.quality_score_with_weights(weights))
    }

    /// 各维度得分明细（用于报告）
    pub fn dimension_scores(&self) -> DimensionScores {
        let edge_count = self.edge_count.max(1) as f64;
        let node_count = self.node_count.max(1) as f64;
        let overlap_penalty = (self.node_overlap_pairs as f64
            / (node_count * (node_count - 1.0) / 2.0).max(1.0))
        .min(1.0);
        let edge_node_penalty = (self.edge_node_crossings as f64 / edge_count).min(1.0);
        let edge_cross_penalty = (self.edge_crossings as f64 / edge_count).min(1.0);
        let correctness = (1.0 - overlap_penalty) * 0.4
            + (1.0 - edge_node_penalty) * 0.35
            + (1.0 - edge_cross_penalty) * 0.25;
        let compactness = (self.area_utilization / 0.5).min(1.0);
        let uniformity = (1.0 - self.edge_length_cv).max(0.0);
        let ideal_ratio = 1.6;
        let ratio_deviation = if self.aspect_ratio <= ideal_ratio {
            0.0
        } else {
            (self.aspect_ratio - ideal_ratio) / 4.0
        };
        let aesthetics = (1.0 - ratio_deviation).max(0.0);

        DimensionScores {
            correctness: (correctness * 100.0).round() / 100.0,
            compactness: (compactness * 100.0).round() / 100.0,
            uniformity: (uniformity * 100.0).round() / 100.0,
            aesthetics: (aesthetics * 100.0).round() / 100.0,
        }
    }
}

/// 各维度得分明细
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DimensionScores {
    pub correctness: f64,
    pub compactness: f64,
    pub uniformity: f64,
    pub aesthetics: f64,
}

// ═══════════════════════════════════════════════════════════
//  指标权重配置
// ═══════════════════════════════════════════════════════════

/// 指标权重配置
///
/// 不同图类型对质量维度的侧重不同：
/// - 流程图更看重正确性和美观性
/// - 架构图更看重紧凑性
/// - 思维导图更看重均匀性和美观性
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MetricWeights {
    /// 正确性权重
    pub correctness: f64,
    /// 紧凑性权重
    pub compactness: f64,
    /// 均匀性权重
    pub uniformity: f64,
    /// 美观性权重
    pub aesthetics: f64,
}

impl MetricWeights {
    /// 默认权重（与 `quality_score()` 一致）
    pub fn default_weights() -> Self {
        Self {
            correctness: 0.40,
            compactness: 0.20,
            uniformity: 0.20,
            aesthetics: 0.20,
        }
    }

    /// 按图类型获取推荐权重
    pub fn for_diagram_type(dt: &DiagramType) -> Self {
        match dt {
            DiagramType::Flowchart => Self {
                correctness: 0.40,
                compactness: 0.15,
                uniformity: 0.20,
                aesthetics: 0.25,
            },
            DiagramType::Architecture => Self {
                correctness: 0.30,
                compactness: 0.25,
                uniformity: 0.20,
                aesthetics: 0.25,
            },
            DiagramType::State => Self {
                correctness: 0.40,
                compactness: 0.15,
                uniformity: 0.25,
                aesthetics: 0.20,
            },
            DiagramType::Er => Self {
                correctness: 0.35,
                compactness: 0.20,
                uniformity: 0.25,
                aesthetics: 0.20,
            },
            DiagramType::Sequence => Self {
                correctness: 0.50,
                compactness: 0.20,
                uniformity: 0.15,
                aesthetics: 0.15,
            },
            DiagramType::Mindmap => Self {
                correctness: 0.25,
                compactness: 0.15,
                uniformity: 0.30,
                aesthetics: 0.30,
            },
            DiagramType::Custom(_) => Self::default_weights(),
        }
    }
}

// ═══════════════════════════════════════════════════════════
//  质量等级
// ═══════════════════════════════════════════════════════════

/// 质量等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum QualityGrade {
    /// >= 85 分
    Excellent,
    /// >= 70 分
    Good,
    /// >= 50 分
    Acceptable,
    /// < 50 分
    Poor,
}

impl QualityGrade {
    pub fn from_score(score: f64) -> Self {
        if score >= 85.0 {
            QualityGrade::Excellent
        } else if score >= 70.0 {
            QualityGrade::Good
        } else if score >= 50.0 {
            QualityGrade::Acceptable
        } else {
            QualityGrade::Poor
        }
    }
}

impl std::fmt::Display for QualityGrade {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QualityGrade::Excellent => write!(f, "优秀"),
            QualityGrade::Good => write!(f, "良好"),
            QualityGrade::Acceptable => write!(f, "可接受"),
            QualityGrade::Poor => write!(f, "较差"),
        }
    }
}

/// 计算节点重叠对数
fn count_node_overlaps(nodes: &HashMap<String, NodeLayout>) -> usize {
    let node_list: Vec<&NodeLayout> = nodes.values().collect();
    let mut count = 0;
    for i in 0..node_list.len() {
        for j in (i + 1)..node_list.len() {
            if rectangles_overlap(node_list[i], node_list[j]) {
                count += 1;
            }
        }
    }
    count
}

/// 两个矩形是否重叠（AABB 检测，含容差）
fn rectangles_overlap(a: &NodeLayout, b: &NodeLayout) -> bool {
    let eps = 0.5; // 半像素容差
    a.x + a.width > b.x + eps
        && b.x + b.width > a.x + eps
        && a.y + a.height > b.y + eps
        && b.y + b.height > a.y + eps
}

// ═══════════════════════════════════════════════════════════
//  边穿节点检测
// ═══════════════════════════════════════════════════════════

/// 计算边穿过非起终节点的次数
fn count_edge_node_crossings(diagram: &Diagram, result: &LayoutResult) -> usize {
    let mut count = 0;
    for (i, edge) in result.edges.iter().enumerate() {
        if edge.path_len() < 2 {
            continue;
        }
        // 获取这条边的起终节点 ID
        let rel = &diagram.relations[i];
        let from_id = rel.from.as_str();
        let to_id = rel.to.as_str();

        // 对路径的每段线段，检查是否穿过其他节点
        let path = edge.path_points();
        for window in path.windows(2) {
            let a = window[0];
            let b = window[1];
            for (node_id, nl) in &result.nodes {
                if node_id == from_id || node_id == to_id {
                    continue;
                }
                if segment_intersects_rect(a, b, nl) {
                    count += 1;
                }
            }
        }
    }
    count
}

/// 线段是否与矩形相交
///
/// 委托到 `drawify_core::layout::refine::segment_intersects_node`，
/// 确保评估器与 V2 路由后验证使用完全相同的穿障判定算法。
fn segment_intersects_rect(a: Point, b: Point, nl: &NodeLayout) -> bool {
    segment_intersects_node(a, b, nl)
}

// ═══════════════════════════════════════════════════════════
//  边交叉检测
// ═══════════════════════════════════════════════════════════

/// 计算边与边之间的交叉数
///
/// 将每条边采样为折线段，然后两两检测交叉。
/// 共享端点的边对不计算交叉（它们在端点处的"交叉"是合法的）。
fn count_edge_crossings(result: &LayoutResult) -> usize {
    let edges = &result.edges;
    if edges.len() < 2 {
        return 0;
    }

    // 将每条边采样为折线段列表
    let sampled: Vec<Vec<Point>> = edges.iter().map(|e| e.sampled_path(16)).collect();

    let mut count = 0;
    for i in 0..sampled.len() {
        for j in (i + 1)..sampled.len() {
            if edges_share_endpoint(&edges[i], &edges[j]) {
                continue;
            }
            if polylines_cross(&sampled[i], &sampled[j]) {
                count += 1;
            }
        }
    }
    count
}

/// 两条边是否共享端点（同一对节点间的边）
fn edges_share_endpoint(a: &EdgeLayout, b: &EdgeLayout) -> bool {
    if a.path_is_empty() || b.path_is_empty() {
        return false;
    }
    let a_start = a.path_start().unwrap();
    let a_end = a.path_end().unwrap();
    let b_start = b.path_start().unwrap();
    let b_end = b.path_end().unwrap();

    // 检查是否共享任一端点（容差 2px）
    let eps = 2.0;
    let near = |p1: Point, p2: Point| -> bool {
        (p1.x - p2.x).abs() < eps && (p1.y - p2.y).abs() < eps
    };

    near(a_start, b_start) || near(a_start, b_end) || near(a_end, b_start) || near(a_end, b_end)
}

/// 两条线段是否真正交叉
fn segments_cross(a1: Point, a2: Point, b1: Point, b2: Point) -> bool {
    let d1 = cross(b1, b2, a1);
    let d2 = cross(b1, b2, a2);
    let d3 = cross(a1, a2, b1);
    let d4 = cross(a1, a2, b2);
    let eps = 0.5;

    if ((d1 > eps && d2 < -eps) || (d1 < -eps && d2 > eps))
        && ((d3 > eps && d4 < -eps) || (d3 < -eps && d4 > eps))
    {
        return true;
    }
    false
}

fn cross(o: Point, a: Point, b: Point) -> f64 {
    (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
}

/// 两条折线是否交叉
fn polylines_cross(a: &[Point], b: &[Point]) -> bool {
    if a.len() < 2 || b.len() < 2 {
        return false;
    }
    for wa in a.windows(2) {
        for wb in b.windows(2) {
            if segments_cross(wa[0], wa[1], wb[0], wb[1]) {
                return true;
            }
        }
    }
    false
}

// ═══════════════════════════════════════════════════════════
//  边长度统计
// ═══════════════════════════════════════════════════════════

fn compute_edge_lengths(result: &LayoutResult) -> Vec<f64> {
    result
        .edges
        .iter()
        .filter(|e| e.path_len() >= 2)
        .map(|e| polyline_length(&e.sampled_path(16)))
        .collect()
}

fn compute_total_edge_length(result: &LayoutResult) -> f64 {
    compute_edge_lengths(result).iter().sum()
}

fn polyline_length(path: &[Point]) -> f64 {
    path.windows(2)
        .map(|w| {
            let dx = w[1].x - w[0].x;
            let dy = w[1].y - w[0].y;
            (dx * dx + dy * dy).sqrt()
        })
        .sum()
}

/// 计算标准差和变异系数
fn compute_stddev_cv(values: &[f64]) -> (f64, f64) {
    if values.is_empty() {
        return (0.0, 0.0);
    }
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    if mean < 0.01 {
        return (0.0, 0.0);
    }
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n;
    let stddev = variance.sqrt();
    let cv = stddev / mean;
    (stddev, cv)
}

// ═══════════════════════════════════════════════════════════
//  路由友好性预测指标（Phase 0 候选度量）
//
//  这些度量基于布局完成后的节点/group 位置 + 图拓扑，不依赖实际路由结果，
//  用于预测路由阶段的质量。与事后 edge_node_crossings / edge_crossings 做
//  Pearson 相关性分析，确定 V1 评估器的五维权重。
// ═══════════════════════════════════════════════════════════

/// 正交通道占用度：穿过同一水平/垂直间隙的最大边数。
///
/// Phase 1.5 改进：原 RUDY 直线密度与 slab/正交路由器脱钩（r=0.01）。
/// 新度量直接建模正交路由的瓶颈——节点行/列之间的"间隙"。
/// 每条边若其两端位于不同行（列），则必须跨越其间所有水平（垂直）间隙；
/// 统计每个间隙被多少条边跨越，取峰值。
///
/// 复杂度 O(|E| × |V|)（间隙数 ≤ |V|）。
fn compute_channel_congestion(diagram: &Diagram, result: &LayoutResult) -> f64 {
    if diagram.relations.is_empty() || result.nodes.is_empty() {
        return 0.0;
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

    // 不足两行/两列则无间隙可竞争
    if y_centers.len() < 2 && x_centers.len() < 2 {
        return 0.0;
    }

    let mut h_demand = vec![0usize; y_centers.len().saturating_sub(1)];
    let mut v_demand = vec![0usize; x_centers.len().saturating_sub(1)];

    for rel in &diagram.relations {
        let (Some(from), Some(to)) = (result.nodes.get(rel.from.as_str()), result.nodes.get(rel.to.as_str())) else {
            continue;
        };
        let fy = from.y + from.height / 2.0;
        let ty = to.y + to.height / 2.0;
        let fx = from.x + from.width / 2.0;
        let tx = to.x + to.width / 2.0;

        // 水平间隙 i（位于 y_centers[i] 与 y_centers[i+1] 之间）被跨越的条件：
        // 间隙完全落在 [y_lo, y_hi] 内部
        if !h_demand.is_empty() {
            let (y_lo, y_hi) = if fy <= ty { (fy, ty) } else { (ty, fy) };
            for i in 0..h_demand.len() {
                if y_centers[i] >= y_lo && y_centers[i + 1] <= y_hi {
                    h_demand[i] += 1;
                }
            }
        }

        // 垂直间隙同理
        if !v_demand.is_empty() {
            let (x_lo, x_hi) = if fx <= tx { (fx, tx) } else { (tx, fx) };
            for i in 0..v_demand.len() {
                if x_centers[i] >= x_lo && x_centers[i + 1] <= x_hi {
                    v_demand[i] += 1;
                }
            }
        }
    }

    let peak_h = h_demand.iter().copied().max().unwrap_or(0);
    let peak_v = v_demand.iter().copied().max().unwrap_or(0);
    (peak_h + peak_v) as f64
}

/// 长边跨层数（rank 跨度 > 1 的边数）。
///
/// 需要 `LayoutHints.sugiyama_ranks`；非 Sugiyama 布局返回 0。
fn compute_long_edge_count(diagram: &Diagram, result: &LayoutResult) -> usize {
    let Some(ranks) = &result.hints.sugiyama_ranks else {
        return 0;
    };
    let mut count = 0;
    for rel in &diagram.relations {
        match (ranks.get(rel.from.as_str()), ranks.get(rel.to.as_str())) {
            (Some(&r1), Some(&r2)) => {
                if r1.abs_diff(r2) > 1 {
                    count += 1;
                }
            }
            _ => {}
        }
    }
    count
}

/// group 间距缺口：Σ max(0, 所需通道宽 - 实际间距)。
///
/// 对每对相邻 group，计算其实际间距与跨 group 边所需通道宽度（cross_edges × 16px）的差值。
fn compute_group_gap_deficit(diagram: &Diagram, result: &LayoutResult) -> f64 {
    if result.groups.len() < 2 {
        return 0.0;
    }

    const EDGE_CHANNEL_WIDTH: f64 = 16.0;

    // 构建 entity_id → group_id 映射
    let entity_group: HashMap<&str, &str> = diagram
        .entities
        .iter()
        .filter_map(|e| e.group_id.as_ref().map(|g| (e.id.as_str(), g.as_str())))
        .collect();

    let groups: Vec<(&String, &drawify_core::layout::GroupLayout)> = result.groups.iter().collect();
    let mut deficit = 0.0f64;

    for i in 0..groups.len() {
        for j in (i + 1)..groups.len() {
            let (g1_id, g1) = groups[i];
            let (g2_id, g2) = groups[j];

            // 计算两 group AABB 间距
            let gap = aabb_gap(
                (g1.x, g1.y, g1.x + g1.width, g1.y + g1.height),
                (g2.x, g2.y, g2.x + g2.width, g2.y + g2.height),
            );
            if gap.is_infinite() {
                continue; // 重叠，跳过
            }

            // 统计跨 group 边数
            let cross_edges = diagram
                .relations
                .iter()
                .filter(|r| {
                    let from_g = entity_group.get(r.from.as_str()).copied();
                    let to_g = entity_group.get(r.to.as_str()).copied();
                    matches!((from_g, to_g), (Some(a), Some(b)) if
                        (a == g1_id.as_str() && b == g2_id.as_str()) ||
                        (a == g2_id.as_str() && b == g1_id.as_str()))
                })
                .count();

            if cross_edges == 0 {
                continue;
            }

            let required = cross_edges as f64 * EDGE_CHANNEL_WIDTH;
            if required > gap {
                deficit += required - gap;
            }
        }
    }

    deficit
}

/// 两个 AABB 的最小间距（重叠时返回 +inf）
fn aabb_gap(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> f64 {
    let dx = (a.0 - b.2).max(b.0 - a.2).max(0.0);
    let dy = (a.1 - b.3).max(b.1 - a.3).max(0.0);
    if dx == 0.0 && dy == 0.0 {
        f64::INFINITY // 重叠
    } else {
        (dx * dx + dy * dy).sqrt()
    }
}

/// 穿障预测数：对每条边用直线（center→center）检测穿过非端点节点的次数。
///
/// 这是 bezier / straight 路由穿障的上界估计。
///
/// Phase 1.5：委托给 V1 评估器的 `crossing_predict::evaluate`，确保与评估器内部
/// 度量一致（含 margin 膨胀 + slab 相交算法），避免校准基线错配。
fn compute_predicted_crossings(diagram: &Diagram, result: &LayoutResult) -> usize {
    drawify_core::layout::friendliness::crossing_predict::evaluate(diagram, result).count
}

/// 端口冲突度：对每个节点，按邻居方向预测边在哪一侧汇入，检查每侧 slot 容量。
///
/// 容量 = 侧边长度 / SLOT_PITCH(40px)；需求 = 该侧边数 × SLOT_PITCH。
fn compute_port_conflict_score(diagram: &Diagram, result: &LayoutResult) -> f64 {
    const SLOT_PITCH: f64 = 40.0;

    // 构建每个节点的邻居列表
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

    let mut total_deficit = 0.0f64;
    for (node_id, nl) in &result.nodes {
        let Some(neighs) = neighbors.get(node_id.as_str()) else {
            continue;
        };
        if neighs.is_empty() {
            continue;
        }

        let cx = nl.x + nl.width / 2.0;
        let cy = nl.y + nl.height / 2.0;

        // 按方向分侧：top / bottom / left / right
        let mut side_counts = [0usize; 4]; // [top, bottom, left, right]
        for &n_id in neighs {
            let Some(n_nl) = result.nodes.get(n_id) else {
                continue;
            };
            let nx = n_nl.x + n_nl.width / 2.0;
            let ny = n_nl.y + n_nl.height / 2.0;
            let dx = nx - cx;
            let dy = ny - cy;
            // 按主导方向分侧
            if dx.abs() > dy.abs() {
                if dx > 0.0 {
                    side_counts[3] += 1; // right
                } else {
                    side_counts[2] += 1; // left
                }
            } else {
                if dy > 0.0 {
                    side_counts[1] += 1; // bottom
                } else {
                    side_counts[0] += 1; // top
                }
            }
        }

        // 每侧容量
        let side_lengths = [
            nl.width,  // top
            nl.width,  // bottom
            nl.height, // left
            nl.height, // right
        ];
        for (i, &count) in side_counts.iter().enumerate() {
            if count == 0 {
                continue;
            }
            let required = count as f64 * SLOT_PITCH;
            let available = side_lengths[i];
            if required > available {
                total_deficit += required - available;
            }
        }
    }

    total_deficit
}

#[cfg(test)]
mod tests {
    use super::*;
    use drawify_core::layout::{PathGeometry, Port};

    fn sample_layout() -> LayoutResult {
        LayoutResult {
            nodes: HashMap::from([
                (
                    "a".to_string(),
                    NodeLayout {
                        x: 0.0,
                        y: 0.0,
                        width: 100.0,
                        height: 50.0,
                        ..Default::default()
                    },
                ),
                (
                    "b".to_string(),
                    NodeLayout {
                        x: 200.0,
                        y: 0.0,
                        width: 100.0,
                        height: 50.0,
                        ..Default::default()
                    },
                ),
            ]),
            groups: HashMap::new(),
            edges: vec![EdgeLayout {
                geometry: PathGeometry::Straight {
                    start: Point::new(100.0, 25.0),
                    end: Point::new(200.0, 25.0),
                },
                labels: vec![],
                from_port: Port::Right,
                to_port: Port::Left,
            }],
            total_width: 300.0,
            total_height: 50.0,
            hints: Default::default(),
        }
    }

    fn sample_diagram() -> Diagram {
        use drawify_core::ast::*;
        use drawify_core::types::DiagramType;
        Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: vec![],
            entities: vec![
                Entity {
                    id: Identifier::new_unchecked("a"),
                    label: "A".to_string(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span: Span::dummy(),
                },
                Entity {
                    id: Identifier::new_unchecked("b"),
                    label: "B".to_string(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span: Span::dummy(),
                },
            ],
            relations: vec![Relation {
                from: Identifier::new_unchecked("a"),
                to: Identifier::new_unchecked("b"),
                arrow: ArrowType::Active,
                label: None,
                head_label: None,
                tail_label: None,
                attributes: AttributeMap::default(),
                span: Span::dummy(),
            }],
            groups: vec![],
            style_decls: vec![],
            doc_comment: None,
            source_info: SourceInfo {
                file: None,
                line_count: 1,
            },
        }
    }

    #[test]
    fn test_no_overlaps() {
        let result = sample_layout();
        assert_eq!(count_node_overlaps(&result.nodes), 0);
    }

    #[test]
    fn test_overlapping_nodes() {
        let mut result = sample_layout();
        // 让 b 与 a 重叠
        result.nodes.insert(
            "c".to_string(),
            NodeLayout {
                x: 50.0,
                y: 0.0,
                width: 100.0,
                height: 50.0,
                ..Default::default()
            },
        );
        assert!(count_node_overlaps(&result.nodes) > 0);
    }

    #[test]
    fn test_no_edge_node_crossings() {
        let diagram = sample_diagram();
        let result = sample_layout();
        assert_eq!(count_edge_node_crossings(&diagram, &result), 0);
    }

    #[test]
    fn test_edge_crossings_none() {
        let result = sample_layout();
        assert_eq!(count_edge_crossings(&result), 0);
    }

    #[test]
    fn test_compute_metrics() {
        let diagram = sample_diagram();
        let result = sample_layout();
        let metrics = LayoutMetrics::compute(&diagram, &result);

        assert_eq!(metrics.node_count, 2);
        assert_eq!(metrics.edge_count, 1);
        assert_eq!(metrics.node_overlap_pairs, 0);
        assert_eq!(metrics.edge_node_crossings, 0);
        assert_eq!(metrics.edge_crossings, 0);
        assert!(metrics.total_area > 0.0);
        assert!(metrics.total_edge_length > 0.0);
        assert!(metrics.area_utilization > 0.0 && metrics.area_utilization <= 1.0);
    }

    #[test]
    fn test_stddev_cv() {
        let values = vec![10.0, 10.0, 10.0];
        let (stddev, cv) = compute_stddev_cv(&values);
        assert!(stddev.abs() < 0.001);
        assert!(cv.abs() < 0.001);

        let values = vec![5.0, 15.0];
        let (stddev, cv) = compute_stddev_cv(&values);
        assert!(stddev > 0.0);
        assert!(cv > 0.0);
    }

    #[test]
    fn test_one_line_summary() {
        let diagram = sample_diagram();
        let result = sample_layout();
        let metrics = LayoutMetrics::compute(&diagram, &result);
        let summary = metrics.one_line_summary();
        assert!(summary.contains("nodes=2"));
        assert!(summary.contains("edges=1"));
    }

    #[test]
    fn test_segment_intersects_rect() {
        let nl = NodeLayout {
            x: 50.0,
            y: 0.0,
            width: 100.0,
            height: 50.0,
            ..Default::default()
        };
        // 线段穿过矩形
        assert!(segment_intersects_rect(Point::new(0.0, 25.0), Point::new(200.0, 25.0), &nl));
        // 线段在矩形上方
        assert!(!segment_intersects_rect(Point::new(0.0, -10.0), Point::new(200.0, -10.0), &nl));
    }

    #[test]
    fn test_segments_cross() {
        // X 形交叉
        assert!(segments_cross(
            Point::new(0.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(0.0, 10.0),
            Point::new(10.0, 0.0),
        ));
        // 平行不交叉
        assert!(!segments_cross(
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(0.0, 10.0),
            Point::new(10.0, 10.0),
        ));
    }

    #[test]
    fn test_quality_grade() {
        assert_eq!(QualityGrade::from_score(90.0), QualityGrade::Excellent);
        assert_eq!(QualityGrade::from_score(75.0), QualityGrade::Good);
        assert_eq!(QualityGrade::from_score(55.0), QualityGrade::Acceptable);
        assert_eq!(QualityGrade::from_score(30.0), QualityGrade::Poor);
    }

    #[test]
    fn test_metric_weights_for_type() {
        let w = MetricWeights::for_diagram_type(&DiagramType::Flowchart);
        assert!((w.correctness + w.compactness + w.uniformity + w.aesthetics - 1.0).abs() < 0.01);

        let w = MetricWeights::for_diagram_type(&DiagramType::Mindmap);
        assert!(w.uniformity > 0.25);
        assert!(w.aesthetics > 0.25);
    }

    #[test]
    fn test_quality_score_with_weights() {
        let diagram = sample_diagram();
        let result = sample_layout();
        let metrics = LayoutMetrics::compute(&diagram, &result);

        let default_score = metrics.quality_score();
        let weighted_score = metrics.quality_score_with_weights(&MetricWeights::default_weights());
        assert!((default_score - weighted_score).abs() < 0.1);
    }

    #[test]
    fn test_dimension_scores() {
        let diagram = sample_diagram();
        let result = sample_layout();
        let metrics = LayoutMetrics::compute(&diagram, &result);
        let dims = metrics.dimension_scores();
        assert!(dims.correctness >= 0.0 && dims.correctness <= 1.0);
        assert!(dims.compactness >= 0.0 && dims.compactness <= 1.0);
    }
}
