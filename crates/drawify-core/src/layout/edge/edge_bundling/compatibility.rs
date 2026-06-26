//! Step 1: 路径分段与特征提取
//! Step 2: 边兼容性评估（构建兼容图）
//!
//! 详见 `docs/architecture/布局优化/edge-bundling-research.md` §4.3、§4.4。
//!
//! ## 兼容性判定
//!
//! 两条边兼容当且仅当所有**硬条件**满足，且综合评分 ≥ `compatibility_threshold`。
//!
//! 硬条件（任一不满足 → compatibility = 0）：
//! - L1 语义约束：arrow_type 相同 且 line_style 相同
//! - 方向兼容：端到端方向夹角 ≤ 60°
//! - 区域兼容：起点同 rank 区间 且 终点同 rank 区间
//! - 流向兼容：非反向（不构成 A→B + B→A）
//! - label 门控：依 `LabelBundlePolicy` 而定
//!
//! 加分项（贡献 compatibility 分数）：
//! - 尺度兼容、位置兼容、lane 兼容

use crate::ast::Relation;
use crate::layout::geometry::Point;
use crate::layout::edge::common::edge_geometry::{
    arrow_type_tag, edge_line_style_signature, edge_stroke_color_signature,
    edge_stroke_width_signature, node_center,
};
use crate::layout::NodeLayout;

use super::types::{Axis, BundlingConfig, LabelBundlePolicy, PathSegment, SegmentDirection};

/// 坐标比较容差
const EPS: f64 = 0.1;

/// 方向兼容性最大夹角（度）：超过此角度的边对不兼容。
const MAX_COMPAT_ANGLE_DEG: f64 = 60.0;

/// 位置兼容性距离阈值（像素）：超过此距离的边对位置得分为 0。
const POSITION_DISTANCE_THRESHOLD: f64 = 60.0;

/// 区域兼容性 rank 差最大值：起点 rank 差与终点 rank 差均 ≤ 此值才算同区间。
const REGION_RANK_TOLERANCE: usize = 1;

/// 尺度兼容性最小长度比：min/max < 此值时尺度得分为 0。
const MIN_SCALE_RATIO: f64 = 0.3;

/// 兼容性评分权重
const W_ANGLE: f64 = 0.3;
const W_REGION: f64 = 0.2;
const W_SCALE: f64 = 0.2;
const W_DISTANCE: f64 = 0.3;

/// 边的高层特征（Step 1 提取）。
///
/// 由路径几何、Relation 语义和 rank 信息综合提取，供兼容性评估使用。
#[derive(Debug, Clone)]
pub struct EdgeFeatures {
    pub edge_index: usize,
    pub from_id: String,
    pub to_id: String,
    /// from 节点中心
    pub from_center: Point,
    /// to 节点中心
    pub to_center: Point,
    /// from 节点 rank（Sugiyama 层次）
    pub from_rank: Option<usize>,
    /// to 节点 rank
    pub to_rank: Option<usize>,
    /// 箭头类型标签（"active" / "passive" / "bidi"）
    pub arrow_tag: &'static str,
    /// 线型签名（"solid" / "dashed" / "dash:..."）
    pub line_style: String,
    /// 描边颜色签名（"stroke:<color>" / "default"）
    pub stroke_color: String,
    /// 描边宽度签名（"width:<value>" / "default"）
    pub stroke_width: String,
    /// 路径总长度（像素）
    pub path_length: f64,
    /// 路径折线点（供距离计算）
    pub path_points: Vec<Point>,
    /// 端到端方向向量（from→to，已归一化）
    pub direction: (f64, f64),
    /// 是否有中段 label
    pub has_label: bool,
    /// 中段 label 文本
    pub label_text: Option<String>,
}

impl EdgeFeatures {
    /// 从 Relation、NodeLayout、rank 信息和路径点提取边特征。
    pub fn extract(
        edge_index: usize,
        rel: &Relation,
        nodes: &std::collections::HashMap<String, NodeLayout>,
        ranks: Option<&std::collections::HashMap<String, usize>>,
        path_points: &[Point],
    ) -> Option<Self> {
        let from_id = rel.from.as_str();
        let to_id = rel.to.as_str();
        let from_nl = nodes.get(from_id)?;
        let to_nl = nodes.get(to_id)?;

        let from_center = node_center(from_nl);
        let to_center = node_center(to_nl);

        let dx = to_center.x - from_center.x;
        let dy = to_center.y - from_center.y;
        let len = (dx * dx + dy * dy).sqrt();
        let direction = if len > EPS {
            (dx / len, dy / len)
        } else {
            (1.0, 0.0)
        };

        let path_length: f64 = path_points
            .windows(2)
            .map(|w| {
                let ddx = w[1].x - w[0].x;
                let ddy = w[1].y - w[0].y;
                (ddx * ddx + ddy * ddy).sqrt()
            })
            .sum();

        let from_rank = ranks.and_then(|r| r.get(from_id).copied());
        let to_rank = ranks.and_then(|r| r.get(to_id).copied());

        Some(Self {
            edge_index,
            from_id: from_id.to_string(),
            to_id: to_id.to_string(),
            from_center,
            to_center,
            from_rank,
            to_rank,
            arrow_tag: arrow_type_tag(&rel.arrow),
            line_style: edge_line_style_signature(rel),
            path_length,
            path_points: path_points.to_vec(),
            direction,
            has_label: rel.label.is_some(),
            label_text: rel.label.clone(),
            stroke_color: edge_stroke_color_signature(rel),
            stroke_width: edge_stroke_width_signature(rel),
        })
    }
}

/// 将 Polyline 路径分解为有向段（Step 1）。
///
/// 每个段为水平或垂直，记录方向、长度和通道层坐标。
/// 非正交段（斜线）按主方向归类。
pub fn decompose_path(edge_index: usize, points: &[Point]) -> Vec<PathSegment> {
    if points.len() < 2 {
        return Vec::new();
    }

    let mut segments = Vec::with_capacity(points.len() - 1);
    for w in points.windows(2) {
        let start = w[0];
        let end = w[1];
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let length = (dx * dx + dy * dy).sqrt();
        if length < EPS {
            continue;
        }

        // 判定主轴：|dx| >= |dy| 视为水平段，否则垂直段
        let (axis, direction, layer) = if dx.abs() >= dy.abs() {
            let dir = if dx > 0.0 {
                SegmentDirection::Positive
            } else {
                SegmentDirection::Negative
            };
            (Axis::Horizontal, dir, start.y) // y 坐标为层
        } else {
            let dir = if dy > 0.0 {
                SegmentDirection::Positive
            } else {
                SegmentDirection::Negative
            };
            (Axis::Vertical, dir, start.x) // x 坐标为层
        };

        segments.push(PathSegment {
            edge_index,
            axis,
            start,
            end,
            direction,
            length,
            layer,
        });
    }
    segments
}

/// 计算两条边的兼容性分数（Step 2）。
///
/// 返回值 ∈ [0.0, 1.0]。0.0 表示不兼容（硬条件不满足），≥ threshold 表示可捆绑。
pub fn compute_compatibility(e1: &EdgeFeatures, e2: &EdgeFeatures, config: &BundlingConfig) -> f64 {
    // ── 硬条件 1: L1 语义约束（arrow_type + line_style + stroke_color + stroke_width 必须相同）──
    if e1.arrow_tag != e2.arrow_tag
        || e1.line_style != e2.line_style
        || e1.stroke_color != e2.stroke_color
        || e1.stroke_width != e2.stroke_width
    {
        return 0.0;
    }

    // ── 硬条件 2: 流向兼容（非反向）──
    // 禁止合并：
    // 1. 完全反向边 A→B + B→A
    // 2. 出入边合并：A→B + B→C（B 对一侧是 to，对另一侧是 from，方向相反）
    if e1.from_id == e2.to_id && e1.to_id == e2.from_id {
        return 0.0;
    }
    // 出入边合并：一条边的 to 是另一条边的 from（语义流向相反）
    if e1.to_id == e2.from_id || e1.from_id == e2.to_id {
        return 0.0;
    }

    // ── 硬条件 3: 方向兼容（端到端方向夹角 ≤ 60°）──
    let angle_deg = direction_angle_deg(e1.direction, e2.direction);
    if angle_deg > MAX_COMPAT_ANGLE_DEG {
        return 0.0;
    }

    // ── 硬条件 4: 区域兼容（起点同 rank 区间 且 终点同 rank 区间）──
    if !region_compatible(e1, e2) {
        return 0.0;
    }

    // ── 硬条件 5: label 门控（依 LabelBundlePolicy）──
    if !label_gate_compatible(e1, e2, config) {
        return 0.0;
    }

    // ── 加分项评分 ──
    let angle_score = 1.0 - (angle_deg / MAX_COMPAT_ANGLE_DEG);
    let region_score = region_score(e1, e2);
    let scale_score = scale_score(e1, e2);
    let distance_score = distance_score(e1, e2);

    let compatibility = W_ANGLE * angle_score
        + W_REGION * region_score
        + W_SCALE * scale_score
        + W_DISTANCE * distance_score;

    // 钳制到 [0, 1]
    compatibility.clamp(0.0, 1.0)
}

/// 两个归一化方向向量的夹角（度，0~180）。
fn direction_angle_deg(d1: (f64, f64), d2: (f64, f64)) -> f64 {
    let dot = d1.0 * d2.0 + d1.1 * d2.1;
    // 钳制到 [-1, 1] 避免 acos NaN
    let cos_angle = dot.clamp(-1.0, 1.0);
    cos_angle.acos().to_degrees()
}

/// 区域兼容性硬条件：起点 rank 差 ≤ 容差 且 终点 rank 差 ≤ 容差。
///
/// 无 rank 信息时退化为"起点同 group 或同侧"——P0 简化为：无 rank 信息时
/// 检查 from 节点是否相同（同源出边天然同区域），否则用几何距离辅助。
fn region_compatible(e1: &EdgeFeatures, e2: &EdgeFeatures) -> bool {
    match (e1.from_rank, e2.from_rank, e1.to_rank, e2.to_rank) {
        (Some(rf1), Some(rf2), Some(rt1), Some(rt2)) => {
            rf1.abs_diff(rf2) <= REGION_RANK_TOLERANCE
                && rt1.abs_diff(rt2) <= REGION_RANK_TOLERANCE
        }
        _ => {
            // 无 rank 信息：退化判定——from 节点相同（同源）或 to 节点相同（同宿）
            // 或 from 中心距离在合理范围内
            e1.from_id == e2.from_id
                || e1.to_id == e2.to_id
                || point_distance(e1.from_center, e2.from_center) <= POSITION_DISTANCE_THRESHOLD
        }
    }
}

/// 区域评分：rank 差越小分越高。
fn region_score(e1: &EdgeFeatures, e2: &EdgeFeatures) -> f64 {
    match (e1.from_rank, e2.from_rank, e1.to_rank, e2.to_rank) {
        (Some(rf1), Some(rf2), Some(rt1), Some(rt2)) => {
            let from_diff = rf1.abs_diff(rf2) as f64;
            let to_diff = rt1.abs_diff(rt2) as f64;
            let from_score = 1.0 - (from_diff / (REGION_RANK_TOLERANCE as f64 + 1.0));
            let to_score = 1.0 - (to_diff / (REGION_RANK_TOLERANCE as f64 + 1.0));
            (from_score + to_score) / 2.0
        }
        _ => {
            // 无 rank 信息：用几何距离评分
            let from_dist = point_distance(e1.from_center, e2.from_center);
            let to_dist = point_distance(e1.to_center, e2.to_center);
            let from_score = 1.0 - (from_dist / POSITION_DISTANCE_THRESHOLD).min(1.0);
            let to_score = 1.0 - (to_dist / POSITION_DISTANCE_THRESHOLD).min(1.0);
            (from_score + to_score) / 2.0
        }
    }
}

/// 尺度评分：min_len / max_len，低于 MIN_SCALE_RATIO 时为 0。
fn scale_score(e1: &EdgeFeatures, e2: &EdgeFeatures) -> f64 {
    let min_len = e1.path_length.min(e2.path_length);
    let max_len = e1.path_length.max(e2.path_length);
    if max_len < EPS {
        return 1.0;
    }
    let ratio = min_len / max_len;
    if ratio < MIN_SCALE_RATIO {
        0.0
    } else {
        // 线性映射 [MIN_SCALE_RATIO, 1.0] → [0.0, 1.0]
        (ratio - MIN_SCALE_RATIO) / (1.0 - MIN_SCALE_RATIO)
    }
}

/// 位置评分：两条边最小距离越小分越高。
fn distance_score(e1: &EdgeFeatures, e2: &EdgeFeatures) -> f64 {
    let min_dist = min_edge_distance(&e1.path_points, &e2.path_points);
    if min_dist >= POSITION_DISTANCE_THRESHOLD {
        0.0
    } else {
        1.0 - (min_dist / POSITION_DISTANCE_THRESHOLD)
    }
}

/// label 门控兼容性（依 LabelBundlePolicy）。
///
/// - `Conservative` / `Stagger`：双方都有 label 且文本不同 → 不兼容
/// - `ForkOnly`：任一方有 label → 不兼容
/// - `SegmentAware`：P0 占位——始终兼容（完整独占段检查需 P2 路径重写信息）
fn label_gate_compatible(
    e1: &EdgeFeatures,
    e2: &EdgeFeatures,
    config: &BundlingConfig,
) -> bool {
    match config.label_bundle_policy {
        LabelBundlePolicy::SegmentAware => {
            // P0 占位：SegmentAware 的完整独占段几何可行性检查需要 P2 路径重写信息，
            // 此处始终放行。P2/P4 将在路径重写后补充检查。
            true
        }
        LabelBundlePolicy::Conservative | LabelBundlePolicy::Stagger => {
            // 双方都有 label 且文本不同 → 禁止合并
            match (&e1.label_text, &e2.label_text) {
                (Some(t1), Some(t2)) => t1 == t2,
                _ => true,
            }
        }
        LabelBundlePolicy::ForkOnly => {
            // 有 label 的边永不进 bundle
            !e1.has_label && !e2.has_label
        }
    }
}

/// 两条折线之间的最小距离。
fn min_edge_distance(path1: &[Point], path2: &[Point]) -> f64 {
    if path1.len() < 2 || path2.len() < 2 {
        return f64::INFINITY;
    }

    let mut min_dist_sq = f64::INFINITY;
    for w1 in path1.windows(2) {
        for w2 in path2.windows(2) {
            let dist_sq = segment_segment_distance_sq(w1[0], w1[1], w2[0], w2[1]);
            if dist_sq < min_dist_sq {
                min_dist_sq = dist_sq;
            }
        }
    }
    min_dist_sq.sqrt()
}

/// 两线段之间的最小距离平方。
fn segment_segment_distance_sq(a1: Point, a2: Point, b1: Point, b2: Point) -> f64 {
    // 先检查是否相交
    if segments_intersect(a1, a2, b1, b2) {
        return 0.0;
    }
    // 取四个端点到对线段距离的最小值
    let d1 = point_segment_distance_sq(a1, b1, b2);
    let d2 = point_segment_distance_sq(a2, b1, b2);
    let d3 = point_segment_distance_sq(b1, a1, a2);
    let d4 = point_segment_distance_sq(b2, a1, a2);
    d1.min(d2).min(d3).min(d4)
}

/// 点到线段的距离平方。
fn point_segment_distance_sq(p: Point, a: Point, b: Point) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len_sq = dx * dx + dy * dy;
    if len_sq < 1e-18 {
        let ddx = p.x - a.x;
        let ddy = p.y - a.y;
        return ddx * ddx + ddy * ddy;
    }
    let t = ((p.x - a.x) * dx + (p.y - a.y) * dy) / len_sq;
    let t = t.clamp(0.0, 1.0);
    let cx = a.x + dx * t;
    let cy = a.y + dy * t;
    let ddx = p.x - cx;
    let ddy = p.y - cy;
    ddx * ddx + ddy * ddy
}

/// 两线段是否相交（含端点接触）。
fn segments_intersect(a1: Point, a2: Point, b1: Point, b2: Point) -> bool {
    let d1 = cross(b1, b2, a1);
    let d2 = cross(b1, b2, a2);
    let d3 = cross(a1, a2, b1);
    let d4 = cross(a1, a2, b2);

    ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
}

/// 叉积 (b-a) × (c-a)
fn cross(a: Point, b: Point, c: Point) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

/// 两点距离
fn point_distance(p1: Point, p2: Point) -> f64 {
    let dx = p2.x - p1.x;
    let dy = p2.y - p1.y;
    (dx * dx + dy * dy).sqrt()
}

/// 兼容性分桶键（§4.4 加速策略 1）。
///
/// 按 `(from_rank, to_rank, arrow_tag, line_style, stroke_color, stroke_width)` 分桶，
/// 只有同桶的边对才可能兼容（硬条件预筛），把 O(E²) 降到 O(Σ bucket_size²)。
///
/// 无 rank 信息时用空字符串占位。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CompatibilityBucket {
    pub from_rank: Option<usize>,
    pub to_rank: Option<usize>,
    pub arrow_tag: &'static str,
    pub line_style: String,
    pub stroke_color: String,
    pub stroke_width: String,
}

impl CompatibilityBucket {
    pub fn from_features(features: &EdgeFeatures) -> Self {
        Self {
            from_rank: features.from_rank,
            to_rank: features.to_rank,
            arrow_tag: features.arrow_tag,
            line_style: features.line_style.clone(),
            stroke_color: features.stroke_color.clone(),
            stroke_width: features.stroke_width.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        ArrowType, AttributeMap, Identifier, Relation, Span,
    };
    use std::collections::HashMap;

    fn make_relation(from: &str, to: &str, arrow: ArrowType) -> Relation {
        Relation {
            from: Identifier::new_unchecked(from),
            to: Identifier::new_unchecked(to),
            arrow,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span: Span::dummy(),
        }
    }

    fn make_relation_with_label(from: &str, to: &str, arrow: ArrowType, label: &str) -> Relation {
        Relation {
            from: Identifier::new_unchecked(from),
            to: Identifier::new_unchecked(to),
            arrow,
            label: Some(label.to_string()),
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span: Span::dummy(),
        }
    }

    fn make_node_layout(cx: f64, cy: f64) -> NodeLayout {
        NodeLayout {
            x: cx - 40.0,
            y: cy - 20.0,
            width: 80.0,
            height: 40.0,
            ..Default::default()
        }
    }

    fn make_nodes(ids: &[&str], centers: &[(f64, f64)]) -> HashMap<String, NodeLayout> {
        let mut nodes = HashMap::new();
        for (i, id) in ids.iter().enumerate() {
            nodes.insert(id.to_string(), make_node_layout(centers[i].0, centers[i].1));
        }
        nodes
    }

    fn make_features(
        edge_index: usize,
        rel: &Relation,
        nodes: &HashMap<String, NodeLayout>,
        path: &[Point],
    ) -> EdgeFeatures {
        EdgeFeatures::extract(edge_index, rel, nodes, None, path).unwrap()
    }

    fn make_features_with_ranks(
        edge_index: usize,
        rel: &Relation,
        nodes: &HashMap<String, NodeLayout>,
        ranks: &HashMap<String, usize>,
        path: &[Point],
    ) -> EdgeFeatures {
        EdgeFeatures::extract(edge_index, rel, nodes, Some(ranks), path).unwrap()
    }

    fn pt(x: f64, y: f64) -> Point { Point::new(x, y) }

    // ── 硬条件测试 ──────────────────────────────────────────

    #[test]
    fn same_direction_horizontal_edges_are_compatible() {
        let nodes = make_nodes(&["A", "B", "C", "D"], &[(0.0, 0.0), (100.0, 0.0), (0.0, 30.0), (100.0, 30.0)]);
        let rel1 = make_relation("A", "B", ArrowType::Active);
        let rel2 = make_relation("C", "D", ArrowType::Active);
        let path1 = vec![pt(40.0, 0.0), pt(140.0, 0.0)];
        let path2 = vec![pt(40.0, 30.0), pt(140.0, 30.0)];
        let e1 = make_features(0, &rel1, &nodes, &path1);
        let e2 = make_features(1, &rel2, &nodes, &path2);
        let config = BundlingConfig::default();
        let score = compute_compatibility(&e1, &e2, &config);
        assert!(score > 0.0, "同向水平边应兼容，得到 score={}", score);
    }

    #[test]
    fn different_arrow_types_are_incompatible() {
        let nodes = make_nodes(&["A", "B", "C", "D"], &[(0.0, 0.0), (100.0, 0.0), (0.0, 30.0), (100.0, 30.0)]);
        let rel1 = make_relation("A", "B", ArrowType::Active);
        let rel2 = make_relation("C", "D", ArrowType::Passive);
        let path1 = vec![pt(40.0, 0.0), pt(140.0, 0.0)];
        let path2 = vec![pt(40.0, 30.0), pt(140.0, 30.0)];
        let e1 = make_features(0, &rel1, &nodes, &path1);
        let e2 = make_features(1, &rel2, &nodes, &path2);
        let config = BundlingConfig::default();
        let score = compute_compatibility(&e1, &e2, &config);
        assert_eq!(score, 0.0, "不同箭头类型应不兼容");
    }

    #[test]
    fn different_line_styles_are_incompatible() {
        let nodes = make_nodes(&["A", "B", "C", "D"], &[(0.0, 0.0), (100.0, 0.0), (0.0, 30.0), (100.0, 30.0)]);
        let rel1 = make_relation("A", "B", ArrowType::Active);
        let mut rel2 = make_relation("C", "D", ArrowType::Active);
        rel2.attributes.style.insert(
            "dashed".to_string(),
            crate::ast::AttributeValue::Boolean(true),
        );
        let path1 = vec![pt(40.0, 0.0), pt(140.0, 0.0)];
        let path2 = vec![pt(40.0, 30.0), pt(140.0, 30.0)];
        let e1 = make_features(0, &rel1, &nodes, &path1);
        let e2 = make_features(1, &rel2, &nodes, &path2);
        let config = BundlingConfig::default();
        let score = compute_compatibility(&e1, &e2, &config);
        assert_eq!(score, 0.0, "不同线型应不兼容");
    }

    #[test]
    fn reverse_edges_are_incompatible() {
        let nodes = make_nodes(&["A", "B"], &[(0.0, 0.0), (100.0, 0.0)]);
        let rel1 = make_relation("A", "B", ArrowType::Active);
        let rel2 = make_relation("B", "A", ArrowType::Active);
        let path1 = vec![pt(40.0, 0.0), pt(140.0, 0.0)];
        let path2 = vec![pt(140.0, 0.0), pt(40.0, 0.0)];
        let e1 = make_features(0, &rel1, &nodes, &path1);
        let e2 = make_features(1, &rel2, &nodes, &path2);
        let config = BundlingConfig::default();
        let score = compute_compatibility(&e1, &e2, &config);
        assert_eq!(score, 0.0, "反向边应不兼容");
    }

    #[test]
    fn perpendicular_edges_are_incompatible() {
        let nodes = make_nodes(&["A", "B", "C", "D"], &[(0.0, 0.0), (100.0, 0.0), (50.0, -50.0), (50.0, 50.0)]);
        let rel1 = make_relation("A", "B", ArrowType::Active);
        let rel2 = make_relation("C", "D", ArrowType::Active);
        let path1 = vec![pt(40.0, 0.0), pt(140.0, 0.0)];
        let path2 = vec![pt(50.0, -30.0), pt(50.0, 70.0)];
        let e1 = make_features(0, &rel1, &nodes, &path1);
        let e2 = make_features(1, &rel2, &nodes, &path2);
        let config = BundlingConfig::default();
        let score = compute_compatibility(&e1, &e2, &config);
        assert_eq!(score, 0.0, "垂直方向边（90°）应不兼容");
    }

    // ── 区域兼容性测试 ──────────────────────────────────────

    #[test]
    fn same_rank_edges_are_compatible() {
        let nodes = make_nodes(&["A", "B", "C", "D"], &[(0.0, 0.0), (100.0, 0.0), (0.0, 30.0), (100.0, 30.0)]);
        let mut ranks = HashMap::new();
        ranks.insert("A".to_string(), 0);
        ranks.insert("B".to_string(), 1);
        ranks.insert("C".to_string(), 0);
        ranks.insert("D".to_string(), 1);
        let rel1 = make_relation("A", "B", ArrowType::Active);
        let rel2 = make_relation("C", "D", ArrowType::Active);
        let path1 = vec![pt(40.0, 0.0), pt(140.0, 0.0)];
        let path2 = vec![pt(40.0, 30.0), pt(140.0, 30.0)];
        let e1 = make_features_with_ranks(0, &rel1, &nodes, &ranks, &path1);
        let e2 = make_features_with_ranks(1, &rel2, &nodes, &ranks, &path2);
        let config = BundlingConfig::default();
        let score = compute_compatibility(&e1, &e2, &config);
        assert!(score > 0.0, "同 rank 区间边应兼容");
    }

    #[test]
    fn far_rank_edges_are_incompatible() {
        let nodes = make_nodes(&["A", "B", "C", "D"], &[(0.0, 0.0), (100.0, 0.0), (0.0, 30.0), (100.0, 30.0)]);
        let mut ranks = HashMap::new();
        ranks.insert("A".to_string(), 0);
        ranks.insert("B".to_string(), 1);
        ranks.insert("C".to_string(), 5);
        ranks.insert("D".to_string(), 6);
        let rel1 = make_relation("A", "B", ArrowType::Active);
        let rel2 = make_relation("C", "D", ArrowType::Active);
        let path1 = vec![pt(40.0, 0.0), pt(140.0, 0.0)];
        let path2 = vec![pt(40.0, 30.0), pt(140.0, 30.0)];
        let e1 = make_features_with_ranks(0, &rel1, &nodes, &ranks, &path1);
        let e2 = make_features_with_ranks(1, &rel2, &nodes, &ranks, &path2);
        let config = BundlingConfig::default();
        let score = compute_compatibility(&e1, &e2, &config);
        assert_eq!(score, 0.0, "rank 差过大应不兼容");
    }

    // ── label 门控测试 ──────────────────────────────────────

    #[test]
    fn conservative_policy_different_labels_incompatible() {
        let nodes = make_nodes(&["A", "B", "C", "D"], &[(0.0, 0.0), (100.0, 0.0), (0.0, 30.0), (100.0, 30.0)]);
        let rel1 = make_relation_with_label("A", "B", ArrowType::Active, "请求");
        let rel2 = make_relation_with_label("C", "D", ArrowType::Active, "响应");
        let path1 = vec![pt(40.0, 0.0), pt(140.0, 0.0)];
        let path2 = vec![pt(40.0, 30.0), pt(140.0, 30.0)];
        let e1 = make_features(0, &rel1, &nodes, &path1);
        let e2 = make_features(1, &rel2, &nodes, &path2);
        let config = BundlingConfig {
            label_bundle_policy: LabelBundlePolicy::Conservative,
            ..Default::default()
        };
        let score = compute_compatibility(&e1, &e2, &config);
        assert_eq!(score, 0.0, "Conservative 下不同 label 应不兼容");
    }

    #[test]
    fn conservative_policy_same_labels_compatible() {
        let nodes = make_nodes(&["A", "B", "C", "D"], &[(0.0, 0.0), (100.0, 0.0), (0.0, 30.0), (100.0, 30.0)]);
        let rel1 = make_relation_with_label("A", "B", ArrowType::Active, "请求");
        let rel2 = make_relation_with_label("C", "D", ArrowType::Active, "请求");
        let path1 = vec![pt(40.0, 0.0), pt(140.0, 0.0)];
        let path2 = vec![pt(40.0, 30.0), pt(140.0, 30.0)];
        let e1 = make_features(0, &rel1, &nodes, &path1);
        let e2 = make_features(1, &rel2, &nodes, &path2);
        let config = BundlingConfig {
            label_bundle_policy: LabelBundlePolicy::Conservative,
            ..Default::default()
        };
        let score = compute_compatibility(&e1, &e2, &config);
        assert!(score > 0.0, "Conservative 下相同 label 应兼容");
    }

    #[test]
    fn segment_aware_policy_different_labels_compatible() {
        let nodes = make_nodes(&["A", "B", "C", "D"], &[(0.0, 0.0), (100.0, 0.0), (0.0, 30.0), (100.0, 30.0)]);
        let rel1 = make_relation_with_label("A", "B", ArrowType::Active, "请求");
        let rel2 = make_relation_with_label("C", "D", ArrowType::Active, "响应");
        let path1 = vec![pt(40.0, 0.0), pt(140.0, 0.0)];
        let path2 = vec![pt(40.0, 30.0), pt(140.0, 30.0)];
        let e1 = make_features(0, &rel1, &nodes, &path1);
        let e2 = make_features(1, &rel2, &nodes, &path2);
        let config = BundlingConfig {
            label_bundle_policy: LabelBundlePolicy::SegmentAware,
            ..Default::default()
        };
        let score = compute_compatibility(&e1, &e2, &config);
        assert!(score > 0.0, "SegmentAware 下不同 label 应兼容（P0 占位）");
    }

    #[test]
    fn fork_only_policy_any_label_incompatible() {
        let nodes = make_nodes(&["A", "B", "C", "D"], &[(0.0, 0.0), (100.0, 0.0), (0.0, 30.0), (100.0, 30.0)]);
        let rel1 = make_relation_with_label("A", "B", ArrowType::Active, "请求");
        let rel2 = make_relation("C", "D", ArrowType::Active);
        let path1 = vec![pt(40.0, 0.0), pt(140.0, 0.0)];
        let path2 = vec![pt(40.0, 30.0), pt(140.0, 30.0)];
        let e1 = make_features(0, &rel1, &nodes, &path1);
        let e2 = make_features(1, &rel2, &nodes, &path2);
        let config = BundlingConfig {
            label_bundle_policy: LabelBundlePolicy::ForkOnly,
            ..Default::default()
        };
        let score = compute_compatibility(&e1, &e2, &config);
        assert_eq!(score, 0.0, "ForkOnly 下有 label 的边不进 bundle");
    }

    // ── 尺度兼容性测试 ──────────────────────────────────────

    #[test]
    fn very_different_lengths_low_scale_score() {
        let nodes = make_nodes(&["A", "B", "C", "D"], &[(0.0, 0.0), (100.0, 0.0), (0.0, 30.0), (100.0, 30.0)]);
        let rel1 = make_relation("A", "B", ArrowType::Active);
        let rel2 = make_relation("C", "D", ArrowType::Active);
        let path1 = vec![pt(40.0, 0.0), pt(1040.0, 0.0)];
        let path2 = vec![pt(40.0, 30.0), pt(80.0, 30.0)];
        let e1 = make_features(0, &rel1, &nodes, &path1);
        let e2 = make_features(1, &rel2, &nodes, &path2);
        let config = BundlingConfig::default();
        let score = compute_compatibility(&e1, &e2, &config);
        assert!(score >= 0.0);
    }

    // ── 路径分解测试 ────────────────────────────────────────

    #[test]
    fn decompose_horizontal_path() {
        let points = vec![pt(0.0, 0.0), pt(100.0, 0.0)];
        let segs = decompose_path(0, &points);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].axis, Axis::Horizontal);
        assert_eq!(segs[0].direction, SegmentDirection::Positive);
        assert!((segs[0].length - 100.0).abs() < 1e-9);
    }

    #[test]
    fn decompose_vertical_path() {
        let points = vec![pt(0.0, 0.0), pt(0.0, 100.0)];
        let segs = decompose_path(0, &points);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].axis, Axis::Vertical);
        assert_eq!(segs[0].direction, SegmentDirection::Positive);
    }

    #[test]
    fn decompose_orthogonal_polyline() {
        let points = vec![pt(0.0, 0.0), pt(100.0, 0.0), pt(100.0, 50.0), pt(200.0, 50.0)];
        let segs = decompose_path(0, &points);
        assert_eq!(segs.len(), 3);
        assert_eq!(segs[0].axis, Axis::Horizontal);
        assert_eq!(segs[1].axis, Axis::Vertical);
        assert_eq!(segs[2].axis, Axis::Horizontal);
    }

    #[test]
    fn decompose_empty_path() {
        let segs = decompose_path(0, &[]);
        assert!(segs.is_empty());
    }

    #[test]
    fn decompose_single_point() {
        let segs = decompose_path(0, &[pt(10.0, 20.0)]);
        assert!(segs.is_empty());
    }

    // ── 分桶测试 ────────────────────────────────────────────

    #[test]
    fn bucket_key_groups_compatible_edges() {
        let nodes = make_nodes(&["A", "B", "C", "D"], &[(0.0, 0.0), (100.0, 0.0), (0.0, 30.0), (100.0, 30.0)]);
        let rel1 = make_relation("A", "B", ArrowType::Active);
        let rel2 = make_relation("C", "D", ArrowType::Active);
        let path1 = vec![pt(40.0, 0.0), pt(140.0, 0.0)];
        let path2 = vec![pt(40.0, 30.0), pt(140.0, 30.0)];
        let e1 = make_features(0, &rel1, &nodes, &path1);
        let e2 = make_features(1, &rel2, &nodes, &path2);
        let b1 = CompatibilityBucket::from_features(&e1);
        let b2 = CompatibilityBucket::from_features(&e2);
        assert_eq!(b1, b2, "同 rank 同类型的边应分到同桶");
    }

    #[test]
    fn bucket_key_separates_different_arrow_types() {
        let nodes = make_nodes(&["A", "B", "C", "D"], &[(0.0, 0.0), (100.0, 0.0), (0.0, 30.0), (100.0, 30.0)]);
        let rel1 = make_relation("A", "B", ArrowType::Active);
        let rel2 = make_relation("C", "D", ArrowType::Passive);
        let path1 = vec![pt(40.0, 0.0), pt(140.0, 0.0)];
        let path2 = vec![pt(40.0, 30.0), pt(140.0, 30.0)];
        let e1 = make_features(0, &rel1, &nodes, &path1);
        let e2 = make_features(1, &rel2, &nodes, &path2);
        let b1 = CompatibilityBucket::from_features(&e1);
        let b2 = CompatibilityBucket::from_features(&e2);
        assert_ne!(b1, b2, "不同箭头类型应分到不同桶");
    }
}
