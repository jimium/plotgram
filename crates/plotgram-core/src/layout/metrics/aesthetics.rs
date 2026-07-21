//! 美学指标采集（观测基建，不改变算法行为）
//!
//! 为 benchmarks 门禁提供可量化的视觉质量维度：
//! - 弯折数（bend count）
//! - 组边框贴边（hugging violations）
//! - 对称偏差（symmetry deviation）
//! - 自环净空（self-loop clearance）
//! - 边交叉数（total crossings）
//! - 路径绕行比（detour ratio）

use crate::ast::Diagram;
use crate::layout::geometry::Point;
use crate::layout::types::{EdgeLayout, LayoutResult, PathGeometry};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 美学指标报告（随 gate-baseline 输出）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AestheticsReport {
    pub bends: BendMetrics,
    pub border_proximity: BorderProximityMetrics,
    pub symmetry: SymmetryMetrics,
    pub self_loops: SelfLoopMetrics,
    pub crossings: CrossingMetrics,
    pub detour: DetourMetrics,
    pub edge_length: EdgeLengthMetrics,
    pub channel_utilization: ChannelUtilization,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BendMetrics {
    /// 所有边弯折总数
    pub total: usize,
    /// 平均每条边弯折数
    pub avg_per_edge: f64,
    /// 单边最大弯折数
    pub max_single_edge: usize,
    /// 弯折过多的边数（超过自适应阈值）
    pub excessive_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BorderProximityMetrics {
    /// 边段到最近组边框的最小距离 (px)
    pub min_distance_px: f64,
    /// 距组边框 < 12px 的边段数
    pub segments_within_12px: usize,
    /// 贴边违规数（距离 < 12px 且平行长度 > 40px）
    pub hugging_violations: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SymmetryMetrics {
    /// 检测到的 fan-out 节点数（出度 ≥ 2）
    pub fan_out_nodes: usize,
    /// 平均对称偏差（0=完美对称，1=完全不对称）
    pub avg_deviation: f64,
    /// 最大对称偏差
    pub max_deviation: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SelfLoopMetrics {
    /// 自环边数
    pub count: usize,
    /// 自环与邻居的最小净空 (px)；无自环时为 f64::MAX 序列化用 9999.0
    pub min_clearance_px: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CrossingMetrics {
    /// 边交叉总数（不含共享端点的边对）
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DetourMetrics {
    /// 平均绕行比（路径长度 / 直线距离）
    pub avg_ratio: f64,
    /// 最大绕行比
    pub max_ratio: f64,
}

/// 边长均匀性指标
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EdgeLengthMetrics {
    /// 同层边长的变异系数（CV = std / mean）；无层信息时为全局 CV
    pub intra_layer_cv: f64,
    /// 最长边 / 最短边 比值
    pub max_min_ratio: f64,
    /// 异常长边数（超过均值 + 2*std）
    pub outlier_count: usize,
}

/// 通道利用率指标
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelUtilization {
    /// 走廊中实际有边通过的占比
    pub corridor_usage_ratio: f64,
    /// 最拥堵通道的边数
    pub max_channel_load: usize,
    /// 未使用走廊数
    pub unused_corridors: usize,
}

// ─── 计算入口 ───────────────────────────────────────────────────────────────

/// 计算完整美学指标报告。
pub fn compute_aesthetics(diagram: &Diagram, result: &LayoutResult) -> AestheticsReport {
    let bends = compute_bend_metrics(result);
    let border_proximity = compute_border_proximity(result);
    let symmetry = compute_symmetry(diagram, result);
    let self_loops = compute_self_loop_metrics(diagram, result);
    let crossings = compute_crossing_metrics(result);
    let detour = compute_detour_metrics(result);
    let edge_length = compute_edge_length_metrics(diagram, result);
    let channel_utilization = compute_channel_utilization(result);
    AestheticsReport {
        bends,
        border_proximity,
        symmetry,
        self_loops,
        crossings,
        detour,
        edge_length,
        channel_utilization,
    }
}

// ─── 弯折指标 ───────────────────────────────────────────────────────────────

fn compute_bend_metrics(result: &LayoutResult) -> BendMetrics {
    let edges = &result.edges;
    if edges.is_empty() {
        return BendMetrics {
            total: 0,
            avg_per_edge: 0.0,
            max_single_edge: 0,
            excessive_count: 0,
        };
    }

    let node_count = result.nodes.len();
    // 自适应阈值：max(4, node_count / 5)
    let threshold = 4usize.max(node_count / 5);

    let mut total = 0usize;
    let mut max_single = 0usize;
    let mut excessive = 0usize;

    for edge in edges {
        let bends = count_bends(edge);
        total += bends;
        if bends > max_single {
            max_single = bends;
        }
        if bends > threshold {
            excessive += 1;
        }
    }

    BendMetrics {
        total,
        avg_per_edge: total as f64 / edges.len() as f64,
        max_single_edge: max_single,
        excessive_count: excessive,
    }
}

/// 单边弯折数 = 方向变化次数（正交路径中非共线中间点数）
fn count_bends(edge: &EdgeLayout) -> usize {
    let points = match &edge.geometry {
        PathGeometry::Polyline { points } => points.as_slice(),
        _ => return 0,
    };
    if points.len() < 3 {
        return 0;
    }
    let mut bends = 0;
    for i in 1..points.len() - 1 {
        let prev = points[i - 1];
        let cur = points[i];
        let next = points[i + 1];
        // 方向变化：前后两段不共线
        let dx1 = cur.x - prev.x;
        let dy1 = cur.y - prev.y;
        let dx2 = next.x - cur.x;
        let dy2 = next.y - cur.y;
        // 共线判定：叉积 ≈ 0
        let cross = dx1 * dy2 - dy1 * dx2;
        if cross.abs() > 0.01 {
            bends += 1;
        }
    }
    bends
}

// ─── 组边框距离指标 ─────────────────────────────────────────────────────────

const HUGGING_DISTANCE: f64 = 12.0;
const HUGGING_PARALLEL_LEN: f64 = 40.0;

fn compute_border_proximity(result: &LayoutResult) -> BorderProximityMetrics {
    if result.groups.is_empty() || result.edges.is_empty() {
        return BorderProximityMetrics {
            min_distance_px: 9999.0,
            segments_within_12px: 0,
            hugging_violations: 0,
        };
    }

    // 收集所有组边框线段（4 条边）
    let group_borders: Vec<(f64, f64, f64, f64)> = result
        .groups
        .values()
        .flat_map(|gl| {
            let x1 = gl.x;
            let y1 = gl.y;
            let x2 = gl.x + gl.width;
            let y2 = gl.y + gl.height;
            vec![
                (x1, y1, x2, y1), // top
                (x1, y2, x2, y2), // bottom
                (x1, y1, x1, y2), // left
                (x2, y1, x2, y2), // right
            ]
        })
        .collect();

    let mut min_dist = f64::MAX;
    let mut within_12 = 0usize;
    let mut hugging = 0usize;

    for edge in &result.edges {
        let points = match &edge.geometry {
            PathGeometry::Polyline { points } => points.as_slice(),
            _ => continue,
        };
        for i in 0..points.len().saturating_sub(1) {
            let seg = (points[i].x, points[i].y, points[i + 1].x, points[i + 1].y);
            let seg_len = segment_length(seg);
            if seg_len < 1.0 {
                continue; // 跳过极短段
            }
            for border in &group_borders {
                let dist = segment_to_segment_distance(seg, *border);
                if dist < min_dist {
                    min_dist = dist;
                }
                if dist < HUGGING_DISTANCE {
                    within_12 += 1;
                    // 贴边违规：距离近且平行长度足够
                    let parallel_len = parallel_overlap_length(seg, *border);
                    if parallel_len > HUGGING_PARALLEL_LEN {
                        hugging += 1;
                    }
                    break; // 每条边段只计一次
                }
            }
        }
    }

    BorderProximityMetrics {
        min_distance_px: if min_dist == f64::MAX { 9999.0 } else { min_dist },
        segments_within_12px: within_12,
        hugging_violations: hugging,
    }
}

// ─── 对称性指标 ─────────────────────────────────────────────────────────────

fn compute_symmetry(diagram: &Diagram, result: &LayoutResult) -> SymmetryMetrics {
    // 构建 fan-out 映射：from_id → [to_id, ...]
    let mut fan_out: HashMap<&str, Vec<&str>> = HashMap::new();
    for rel in &diagram.relations {
        let from = rel.from.as_str();
        let to = rel.to.as_str();
        if from != to {
            fan_out.entry(from).or_default().push(to);
        }
    }

    let mut deviations = Vec::new();
    for (parent_id, children) in &fan_out {
        if children.len() < 2 {
            continue;
        }
        let Some(parent_nl) = result.nodes.get(*parent_id) else {
            continue;
        };
        let parent_cx = parent_nl.x + parent_nl.width / 2.0;

        // 收集子节点中心 x 坐标
        let child_centers: Vec<f64> = children
            .iter()
            .filter_map(|cid| result.nodes.get(*cid))
            .map(|nl| nl.x + nl.width / 2.0)
            .collect();

        if child_centers.len() < 2 {
            continue;
        }

        // 对称偏差：子节点重心与父节点中心的偏移 / 子节点展开宽度
        let centroid: f64 = child_centers.iter().sum::<f64>() / child_centers.len() as f64;
        let spread = child_centers
            .iter()
            .map(|c| (c - centroid).abs())
            .sum::<f64>()
            / child_centers.len() as f64;

        if spread > 1.0 {
            let deviation = ((centroid - parent_cx).abs() / spread).min(1.0);
            deviations.push(deviation);
        }
    }

    let fan_out_nodes = deviations.len();
    let avg_deviation = if deviations.is_empty() {
        0.0
    } else {
        deviations.iter().sum::<f64>() / deviations.len() as f64
    };
    let max_deviation = deviations.iter().cloned().fold(0.0f64, f64::max);

    SymmetryMetrics {
        fan_out_nodes,
        avg_deviation,
        max_deviation,
    }
}

// ─── 自环指标 ───────────────────────────────────────────────────────────────

fn compute_self_loop_metrics(diagram: &Diagram, result: &LayoutResult) -> SelfLoopMetrics {
    let self_loop_indices: Vec<usize> = diagram
        .relations
        .iter()
        .enumerate()
        .filter(|(_, rel)| rel.from.as_str() == rel.to.as_str())
        .map(|(i, _)| i)
        .collect();

    if self_loop_indices.is_empty() {
        return SelfLoopMetrics {
            count: 0,
            min_clearance_px: 9999.0,
        };
    }

    let mut min_clearance = f64::MAX;

    for &idx in &self_loop_indices {
        let Some(edge) = result.edges.get(idx) else {
            continue;
        };
        let points = match &edge.geometry {
            PathGeometry::Polyline { points } => points.as_slice(),
            _ => continue,
        };
        if points.len() < 3 {
            continue;
        }

        // 自环的起终点是同一节点，找该节点的 bbox
        let rel = &diagram.relations[idx];
        let Some(nl) = result.nodes.get(rel.from.as_str()) else {
            continue;
        };
        let node_rect = (nl.x, nl.y, nl.x + nl.width, nl.y + nl.height);

        // 自环路径中离节点边框最近的非端点距离
        for pt in &points[1..points.len() - 1] {
            let dist = point_to_rect_distance(*pt, node_rect);
            if dist < min_clearance {
                min_clearance = dist;
            }
        }
    }

    SelfLoopMetrics {
        count: self_loop_indices.len(),
        min_clearance_px: if min_clearance == f64::MAX {
            9999.0
        } else {
            min_clearance
        },
    }
}

// ─── 边交叉指标 ─────────────────────────────────────────────────────────────

fn compute_crossing_metrics(result: &LayoutResult) -> CrossingMetrics {
    let edges = &result.edges;
    if edges.len() < 2 {
        return CrossingMetrics { total: 0 };
    }

    let mut total = 0usize;
    for i in 0..edges.len() {
        for j in (i + 1)..edges.len() {
            if edges_share_endpoint(&edges[i], &edges[j]) {
                continue;
            }
            if polylines_cross(&edges[i], &edges[j]) {
                total += 1;
            }
        }
    }
    CrossingMetrics { total }
}

fn edges_share_endpoint(a: &EdgeLayout, b: &EdgeLayout) -> bool {
    let a_pts = match &a.geometry {
        PathGeometry::Polyline { points } if points.len() >= 2 => points,
        _ => return false,
    };
    let b_pts = match &b.geometry {
        PathGeometry::Polyline { points } if points.len() >= 2 => points,
        _ => return false,
    };
    let a_start = a_pts[0];
    let a_end = a_pts[a_pts.len() - 1];
    let b_start = b_pts[0];
    let b_end = b_pts[b_pts.len() - 1];
    let eps = 2.0;
    points_close(a_start, b_start, eps)
        || points_close(a_start, b_end, eps)
        || points_close(a_end, b_start, eps)
        || points_close(a_end, b_end, eps)
}

fn polylines_cross(a: &EdgeLayout, b: &EdgeLayout) -> bool {
    let a_pts = match &a.geometry {
        PathGeometry::Polyline { points } => points.as_slice(),
        _ => return false,
    };
    let b_pts = match &b.geometry {
        PathGeometry::Polyline { points } => points.as_slice(),
        _ => return false,
    };
    for i in 0..a_pts.len().saturating_sub(1) {
        for j in 0..b_pts.len().saturating_sub(1) {
            if segments_intersect(
                a_pts[i],
                a_pts[i + 1],
                b_pts[j],
                b_pts[j + 1],
            ) {
                return true;
            }
        }
    }
    false
}

// ─── 绕行比指标 ─────────────────────────────────────────────────────────────

fn compute_detour_metrics(result: &LayoutResult) -> DetourMetrics {
    let mut ratios = Vec::new();

    for edge in &result.edges {
        let points = match &edge.geometry {
            PathGeometry::Polyline { points } if points.len() >= 2 => points.as_slice(),
            _ => continue,
        };
        let start = points[0];
        let end = points[points.len() - 1];
        let straight = ((end.x - start.x).powi(2) + (end.y - start.y).powi(2)).sqrt();
        if straight < 1.0 {
            continue; // 自环或极短边跳过
        }
        let path_len = polyline_length(points);
        ratios.push(path_len / straight);
    }

    if ratios.is_empty() {
        return DetourMetrics {
            avg_ratio: 0.0,
            max_ratio: 0.0,
        };
    }

    DetourMetrics {
        avg_ratio: ratios.iter().sum::<f64>() / ratios.len() as f64,
        max_ratio: ratios.iter().cloned().fold(0.0f64, f64::max),
    }
}

// ─── 边长均匀性指标 ─────────────────────────────────────────────────────────

fn compute_edge_length_metrics(diagram: &Diagram, result: &LayoutResult) -> EdgeLengthMetrics {
    let relations = &diagram.relations;
    let ranks = result.hints.sugiyama_ranks.as_ref();

    // 收集每条边的路径长度
    let mut lengths: Vec<f64> = Vec::new();
    // 按层分组（from 节点的 rank）
    let mut layer_lengths: HashMap<usize, Vec<f64>> = HashMap::new();

    for (i, edge) in result.edges.iter().enumerate() {
        let points = match &edge.geometry {
            PathGeometry::Polyline { points } if points.len() >= 2 => points.as_slice(),
            _ => continue,
        };
        let len = polyline_length(points);
        if len < 1.0 {
            continue; // 极短边跳过
        }
        lengths.push(len);

        // 按 from 节点的 rank 分组
        if let (Some(ranks), Some(rel)) = (ranks, relations.get(i)) {
            if let Some(&rank) = ranks.get(rel.from.as_str()) {
                layer_lengths.entry(rank).or_default().push(len);
            }
        }
    }

    if lengths.is_empty() {
        return EdgeLengthMetrics {
            intra_layer_cv: 0.0,
            max_min_ratio: 0.0,
            outlier_count: 0,
        };
    }

    // 全局统计
    let mean = lengths.iter().sum::<f64>() / lengths.len() as f64;
    let variance = lengths.iter().map(|l| (l - mean).powi(2)).sum::<f64>() / lengths.len() as f64;
    let std_dev = variance.sqrt();
    let min_len = lengths.iter().cloned().fold(f64::MAX, f64::min);
    let max_len = lengths.iter().cloned().fold(0.0f64, f64::max);

    // 同层 CV：各层 CV 的加权平均（权重 = 层内边数）
    let intra_layer_cv = if layer_lengths.is_empty() {
        // 无层信息，用全局 CV
        if mean > 0.0 { std_dev / mean } else { 0.0 }
    } else {
        let mut weighted_cv = 0.0;
        let mut total_edges = 0usize;
        for (_, lens) in &layer_lengths {
            if lens.len() < 2 {
                continue; // 单边层无法计算 CV
            }
            let l_mean = lens.iter().sum::<f64>() / lens.len() as f64;
            let l_var = lens.iter().map(|l| (l - l_mean).powi(2)).sum::<f64>() / lens.len() as f64;
            let l_cv = if l_mean > 0.0 { l_var.sqrt() / l_mean } else { 0.0 };
            weighted_cv += l_cv * lens.len() as f64;
            total_edges += lens.len();
        }
        if total_edges > 0 { weighted_cv / total_edges as f64 } else { 0.0 }
    };

    // 异常长边：超过 mean + 2*std
    let threshold = mean + 2.0 * std_dev;
    let outlier_count = lengths.iter().filter(|&&l| l > threshold).count();

    EdgeLengthMetrics {
        intra_layer_cv,
        max_min_ratio: if min_len > 0.0 { max_len / min_len } else { 0.0 },
        outlier_count,
    }
}

// ─── 通道利用率指标 ─────────────────────────────────────────────────────────

fn compute_channel_utilization(result: &LayoutResult) -> ChannelUtilization {
    let corridors = match &result.hints.group_routing {
        Some(hints) if !hints.corridors.is_empty() => &hints.corridors,
        _ => {
            return ChannelUtilization {
                corridor_usage_ratio: 0.0,
                max_channel_load: 0,
                unused_corridors: 0,
            };
        }
    };

    // 收集所有边的段
    let mut all_segments: Vec<(f64, f64, f64, f64)> = Vec::new();
    for edge in &result.edges {
        let points = match &edge.geometry {
            PathGeometry::Polyline { points } if points.len() >= 2 => points.as_slice(),
            _ => continue,
        };
        for w in points.windows(2) {
            all_segments.push((w[0].x, w[0].y, w[1].x, w[1].y));
        }
    }

    if all_segments.is_empty() {
        return ChannelUtilization {
            corridor_usage_ratio: 0.0,
            max_channel_load: 0,
            unused_corridors: corridors.len(),
        };
    }

    const CORRIDOR_PROXIMITY: f64 = 8.0;
    let mut used_count = 0usize;
    let mut max_load = 0usize;

    for corridor in corridors {
        // 检查哪些边段经过该走廊（平行且坐标接近）
        let mut load = 0usize;
        for &(x1, y1, x2, y2) in &all_segments {
            let passes = match corridor.axis {
                crate::layout::group::CorridorAxis::Vertical => {
                    // 竖走廊：垂直段且 x 接近 coord，y 范围在 span 内
                    let is_vert = (x1 - x2).abs() < 0.01;
                    if !is_vert { continue; }
                    (x1 - corridor.coord).abs() < CORRIDOR_PROXIMITY
                        && y1.min(y2) < corridor.span_max
                        && y1.max(y2) > corridor.span_min
                }
                crate::layout::group::CorridorAxis::Horizontal => {
                    // 横走廊：水平段且 y 接近 coord，x 范围在 span 内
                    let is_horiz = (y1 - y2).abs() < 0.01;
                    if !is_horiz { continue; }
                    (y1 - corridor.coord).abs() < CORRIDOR_PROXIMITY
                        && x1.min(x2) < corridor.span_max
                        && x1.max(x2) > corridor.span_min
                }
            };
            if passes {
                load += 1;
            }
        }
        if load > 0 {
            used_count += 1;
        }
        if load > max_load {
            max_load = load;
        }
    }

    let total = corridors.len();
    ChannelUtilization {
        corridor_usage_ratio: if total > 0 { used_count as f64 / total as f64 } else { 0.0 },
        max_channel_load: max_load,
        unused_corridors: total - used_count,
    }
}

// ─── 几何工具 ───────────────────────────────────────────────────────────────

fn segment_length(seg: (f64, f64, f64, f64)) -> f64 {
    ((seg.2 - seg.0).powi(2) + (seg.3 - seg.1).powi(2)).sqrt()
}

fn polyline_length(points: &[Point]) -> f64 {
    points
        .windows(2)
        .map(|w| ((w[1].x - w[0].x).powi(2) + (w[1].y - w[0].y).powi(2)).sqrt())
        .sum()
}

fn points_close(a: Point, b: Point, eps: f64) -> bool {
    (a.x - b.x).abs() < eps && (a.y - b.y).abs() < eps
}

/// 点到矩形边框的最小距离（在矩形外为正，内部为 0）
fn point_to_rect_distance(p: Point, rect: (f64, f64, f64, f64)) -> f64 {
    let (x1, y1, x2, y2) = rect;
    let dx = (x1 - p.x).max(0.0).max(p.x - x2);
    let dy = (y1 - p.y).max(0.0).max(p.y - y2);
    (dx * dx + dy * dy).sqrt()
}

/// 两线段间最小距离（简化：取端点到对方线段的距离最小值）
fn segment_to_segment_distance(
    a: (f64, f64, f64, f64),
    b: (f64, f64, f64, f64),
) -> f64 {
    let pa1 = Point::new(a.0, a.1);
    let pa2 = Point::new(a.2, a.3);
    let pb1 = Point::new(b.0, b.1);
    let pb2 = Point::new(b.2, b.3);
    let d1 = point_to_segment_dist(pa1, pb1, pb2);
    let d2 = point_to_segment_dist(pa2, pb1, pb2);
    let d3 = point_to_segment_dist(pb1, pa1, pa2);
    let d4 = point_to_segment_dist(pb2, pa1, pa2);
    d1.min(d2).min(d3).min(d4)
}

fn point_to_segment_dist(p: Point, a: Point, b: Point) -> f64 {
    let abx = b.x - a.x;
    let aby = b.y - a.y;
    let apx = p.x - a.x;
    let apy = p.y - a.y;
    let len_sq = abx * abx + aby * aby;
    if len_sq < 1e-10 {
        return ((p.x - a.x).powi(2) + (p.y - a.y).powi(2)).sqrt();
    }
    let t = ((apx * abx + apy * aby) / len_sq).clamp(0.0, 1.0);
    let proj_x = a.x + t * abx;
    let proj_y = a.y + t * aby;
    ((p.x - proj_x).powi(2) + (p.y - proj_y).powi(2)).sqrt()
}

/// 两线段的平行重叠长度（仅对水平/垂直段有效）
fn parallel_overlap_length(
    a: (f64, f64, f64, f64),
    b: (f64, f64, f64, f64),
) -> f64 {
    let a_horiz = (a.1 - a.3).abs() < 0.01;
    let b_horiz = (b.1 - b.3).abs() < 0.01;
    let a_vert = (a.0 - a.2).abs() < 0.01;
    let b_vert = (b.0 - b.2).abs() < 0.01;

    if a_horiz && b_horiz {
        // 两条水平段：在同一条水平线上才有重叠
        if (a.1 - b.1).abs() > HUGGING_DISTANCE {
            return 0.0;
        }
        let a_lo = a.0.min(a.2);
        let a_hi = a.0.max(a.2);
        let b_lo = b.0.min(b.2);
        let b_hi = b.0.max(b.2);
        return (a_hi.min(b_hi) - a_lo.max(b_lo)).max(0.0);
    }
    if a_vert && b_vert {
        if (a.0 - b.0).abs() > HUGGING_DISTANCE {
            return 0.0;
        }
        let a_lo = a.1.min(a.3);
        let a_hi = a.1.max(a.3);
        let b_lo = b.1.min(b.3);
        let b_hi = b.1.max(b.3);
        return (a_hi.min(b_hi) - a_lo.max(b_lo)).max(0.0);
    }
    0.0
}

/// 线段相交检测（含共线重叠）
fn segments_intersect(a: Point, b: Point, c: Point, d: Point) -> bool {
    let d1 = cross(c, d, a);
    let d2 = cross(c, d, b);
    let d3 = cross(a, b, c);
    let d4 = cross(a, b, d);

    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
    {
        return true;
    }

    // 共线端点落在线段上
    const EPS: f64 = 0.01;
    if d1.abs() < EPS && on_segment(c, d, a) {
        return true;
    }
    if d2.abs() < EPS && on_segment(c, d, b) {
        return true;
    }
    if d3.abs() < EPS && on_segment(a, b, c) {
        return true;
    }
    if d4.abs() < EPS && on_segment(a, b, d) {
        return true;
    }
    false
}

fn cross(a: Point, b: Point, c: Point) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

fn on_segment(a: Point, b: Point, p: Point) -> bool {
    p.x >= a.x.min(b.x) - 0.01
        && p.x <= a.x.max(b.x) + 0.01
        && p.y >= a.y.min(b.y) - 0.01
        && p.y <= a.y.max(b.y) + 0.01
}
