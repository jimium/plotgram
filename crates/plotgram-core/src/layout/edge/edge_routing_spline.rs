//! 障碍避让多段样条边路由模块
//!
//! 基于 Graphviz Spline-o-Matic 的两阶段算法：
//! Phase 1: 可见性图 + Dijkstra 求最短折线路径（障碍避让）
//! Phase 2: 折线 → 多段三次贝塞尔样条拟合（Catmull-Rom → Bezier，C1 连续）
//!
//! 与 `edge_routing_bezier` 的区别：
//! - bezier: 简单贝塞尔，控制点基于端口方向，**无障碍避让**，边可能穿过节点
//! - spline: 先用可见性图绕开障碍物，再拟合为平滑多段样条，**边不会穿过节点**

use crate::types::DiagramType;
use crate::ast::{Diagram};
use crate::layout::algorithm_config::AlgorithmOptionSpec;
use crate::layout::edge::edge_routing_bezier::{BezierConfig, BEZIER_OPTIONS};
use crate::layout::geometry::Point;
use crate::layout::{
    visibility, EdgeLayout, EdgeRoutingStrategy, LayoutResult, PathGeometry,
};
use crate::layout::edge::common::edge_geometry::{
    build_edge_labels, compute_bezier_controls, cubic_bezier_point, parse_label_t, point_at_path_t,
};
use crate::layout::edge::common::routing_skeleton::{
    finalize_edges, resolve_endpoints, RoutingContext,
};
use std::collections::HashMap;

const APPLICABLE_TYPES: &[DiagramType] = &[
    DiagramType::Flowchart,
    DiagramType::Architecture,
    DiagramType::State,
    DiagramType::Er,
];

/// 障碍避让多段样条路由策略（构造时注入已解析的 option）。
pub struct SplineRouting {
    config: BezierConfig,
}

impl Default for SplineRouting {
    fn default() -> Self {
        Self::from_options(&crate::layout::plan::ResolvedAlgoOptions::from_spec_defaults(
            BEZIER_OPTIONS,
        ))
    }
}

impl SplineRouting {
    pub fn from_options(options: &crate::layout::plan::ResolvedAlgoOptions) -> Self {
        Self {
            config: BezierConfig {
                tension: options.get_or_default(&BEZIER_OPTIONS[0]),
            },
        }
    }
}

impl EdgeRoutingStrategy for SplineRouting {
    fn name(&self) -> &'static str {
        "spline"
    }

    fn applicable_diagram_types(&self) -> &'static [DiagramType] {
        APPLICABLE_TYPES
    }

    fn supports_custom(&self) -> bool {
        true
    }

    fn option_specs(&self) -> &'static [AlgorithmOptionSpec] {
        BEZIER_OPTIONS
    }

    fn route(&self, diagram: &Diagram, result: LayoutResult) -> LayoutResult {
        route_edges_spline(diagram, result, self.config)
    }

    /// spline 在有障碍绕行时会退化为密集 Polyline（多段样条采样），
    /// 需要 refine 检测穿障并推开问题节点。
    fn supports_refine(&self) -> bool {
        true
    }

    /// spline 使用可见性图避障，S3 阶段会接入图级 `ObstacleIndex` 缓存。
    fn needs_obstacle_index(&self) -> bool {
        true
    }
}

/// 障碍物膨胀间距
///
/// 保留供参考：实际膨胀间距取自 [`constants::DEFAULT_NODE_MARGIN`]。
/// `ObstacleIndex::build` 不再接受固定 padding 参数。
#[allow(dead_code)]
const OBSTACLE_PADDING: f64 = 8.0;

/// 贝塞尔路径每段的采样点数
const BEZIER_SAMPLES_PER_SEGMENT: usize = 12;

/// 在节点布局完成后，为所有边计算多段样条路径
pub fn route_edges_spline(
    diagram: &Diagram,
    result: LayoutResult,
    config: BezierConfig,
) -> LayoutResult {
    let relations = &diagram.relations;
    let tension = config.tension;
    let ctx = RoutingContext::new(diagram, &result);

    let node_list: Vec<(usize, &crate::layout::NodeLayout)> = result
        .nodes
        .iter()
        .enumerate()
        .map(|(i, (_, nl))| (i, nl))
        .collect();
    let node_id_to_idx: HashMap<&str, usize> = result
        .nodes
        .keys()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();

    let obstacle_index = visibility::ObstacleIndex::build(&node_list);

    let mut edges: Vec<EdgeLayout> = Vec::with_capacity(relations.len());

    for (i, rel) in relations.iter().enumerate() {
        let Some((ep, label_off)) = resolve_endpoints(&ctx, rel, i) else {
            edges.push(EdgeLayout::empty());
            continue;
        };

        let from_idx = node_id_to_idx.get(ep.from_id.as_str()).copied().unwrap_or(usize::MAX);
        let to_idx = node_id_to_idx.get(ep.to_id.as_str()).copied().unwrap_or(usize::MAX);

        let detour_path = obstacle_index.shortest_path(
            ep.start,
            ep.end,
            &[from_idx, to_idx],
        );

        let (geometry, sampled_for_label) = if detour_path.is_empty() {
            let cp = compute_bezier_controls(
                ep.start.x, ep.start.y, ep.end.x, ep.end.y,
                ep.from_port, ep.to_port, tension,
            );
            let sampled = sample_bezier(ep.start, cp[0], cp[1], ep.end, BEZIER_SAMPLES_PER_SEGMENT);
            let geometry = PathGeometry::Bezier {
                start: ep.start,
                end: ep.end,
                controls: cp,
            };
            (geometry, sampled)
        } else {
            let full_path = build_full_path(ep.start, &detour_path, ep.end);
            let sampled = fit_multi_segment_spline(&full_path, BEZIER_SAMPLES_PER_SEGMENT);
            let geometry = PathGeometry::Polyline { points: sampled.clone() };
            (geometry, sampled)
        };

        let middle_t = parse_label_t(rel);
        let labels = build_edge_labels(rel, middle_t, Point::new(label_off.ox, label_off.oy), |t| {
            point_at_path_t(&sampled_for_label, t)
        });

        edges.push(EdgeLayout {
            geometry,
            labels,
            from_port: ep.from_port,
            to_port: ep.to_port,
        });
    }

    finalize_edges(result, edges, diagram)
}

// ═══════════════════════════════════════════════════════════
//  多段样条拟合
// ═══════════════════════════════════════════════════════════

/// 拼接完整路径（起点 + 绕行点 + 终点）
fn build_full_path(
    start: Point,
    detour: &[Point],
    end: Point,
) -> Vec<Point> {
    let mut path = Vec::with_capacity(detour.len() + 2);
    path.push(start);
    if detour.len() >= 2 {
        path.extend_from_slice(&detour[1..]);
    } else {
        path.extend_from_slice(detour);
    }
    let last = *path.last().unwrap();
    if (last.x - end.x).abs() > 1.0 || (last.y - end.y).abs() > 1.0 {
        path.push(end);
    }
    path
}

/// 将折线路径拟合为多段三次贝塞尔样条并采样
fn fit_multi_segment_spline(
    path: &[Point],
    samples_per_segment: usize,
) -> Vec<Point> {
    let n = path.len();
    if n < 2 {
        return path.to_vec();
    }
    if n == 2 {
        return sample_bezier(path[0], path[0], path[1], path[1], samples_per_segment);
    }

    let mut result: Vec<Point> = Vec::with_capacity(n * samples_per_segment);
    result.push(path[0]);

    let alpha: f64 = 0.5;

    for i in 0..n - 1 {
        let p0 = if i == 0 { path[0] } else { path[i - 1] };
        let p1 = path[i];
        let p2 = path[i + 1];
        let p3 = if i + 2 < n { path[i + 2] } else { path[n - 1] };

        let (cp1, cp2) = catmull_rom_to_bezier_controls(p0, p1, p2, p3, alpha);

        let start_idx = if i == 0 { 0 } else { 1 };
        for s in start_idx..=samples_per_segment {
            let t = s as f64 / samples_per_segment as f64;
            let pt = cubic_bezier_point(p1, cp1, cp2, p2, t);
            if s > 0 || i == 0 {
                result.push(pt);
            }
        }
    }

    result
}

/// Catmull-Rom 样条 → 三次贝塞尔控制点
fn catmull_rom_to_bezier_controls(
    p0: Point,
    p1: Point,
    p2: Point,
    p3: Point,
    alpha: f64,
) -> (Point, Point) {
    let d01 = dist_pow(p0, p1, alpha);
    let d12 = dist_pow(p1, p2, alpha);
    let d23 = dist_pow(p2, p3, alpha);

    let (t1x, t1y) = if d01 < 1e-6 && d12 < 1e-6 {
        (p2.x - p0.x, p2.y - p0.y)
    } else if d01 < 1e-6 {
        (p2.x - p0.x, p2.y - p0.y)
    } else if d12 < 1e-6 {
        (p2.x - p1.x, p2.y - p1.y)
    } else {
        let w1 = d12 / (d01 + d12);
        (
            (p1.x - p0.x) * (1.0 - w1) + (p2.x - p1.x) * w1,
            (p1.y - p0.y) * (1.0 - w1) + (p2.y - p1.y) * w1,
        )
    };

    let (t2x, t2y) = if d12 < 1e-6 && d23 < 1e-6 {
        (p3.x - p1.x, p3.y - p1.y)
    } else if d12 < 1e-6 {
        (p3.x - p1.x, p3.y - p1.y)
    } else if d23 < 1e-6 {
        (p2.x - p1.x, p2.y - p1.y)
    } else {
        let w2 = d12 / (d12 + d23);
        (
            (p2.x - p1.x) * (1.0 - w2) + (p3.x - p2.x) * w2,
            (p2.y - p1.y) * (1.0 - w2) + (p3.y - p2.y) * w2,
        )
    };

    let cp1 = Point::new(p1.x + t1x / 3.0, p1.y + t1y / 3.0);
    let cp2 = Point::new(p2.x - t2x / 3.0, p2.y - t2y / 3.0);

    (cp1, cp2)
}

/// 距离的 alpha 次幂
fn dist_pow(a: Point, b: Point, alpha: f64) -> f64 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    (dx * dx + dy * dy).powf(alpha / 2.0)
}

/// 对单段贝塞尔曲线采样
fn sample_bezier(
    p0: Point,
    p1: Point,
    p2: Point,
    p3: Point,
    samples: usize,
) -> Vec<Point> {
    let samples = samples.max(2);
    (0..=samples)
        .map(|i| {
            let t = i as f64 / samples as f64;
            cubic_bezier_point(p0, p1, p2, p3, t)
        })
        .collect()
}

// ═══════════════════════════════════════════════════════════
//  单元测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::edge::common::test_fixtures::make_diagram_with_layout;

    #[test]
    fn test_spline_no_obstacle() {
        let (diagram, result) = make_diagram_with_layout(
            vec![("a", 40.0, 40.0), ("b", 260.0, 40.0)],
            vec![("a", "b", None)],
        );

        let routed = route_edges_spline(&diagram, result, BezierConfig::default());
        assert_eq!(routed.edges.len(), 1);
        assert!(routed.edges[0].path_len() >= 2);
    }

    #[test]
    fn test_spline_with_obstacle() {
        let (diagram, result) = make_diagram_with_layout(
            vec![("a", 120.0, 40.0), ("b", 120.0, 150.0), ("c", 120.0, 300.0)],
            vec![("a", "c", None)],
        );

        let routed = route_edges_spline(&diagram, result, BezierConfig::default());
        assert_eq!(routed.edges.len(), 1);
        assert!(routed.edges[0].path_len() >= 2);
    }

    #[test]
    fn test_catmull_rom_to_bezier() {
        let (cp1, cp2) = catmull_rom_to_bezier_controls(
            Point::new(0.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(20.0, 20.0),
            Point::new(30.0, 30.0),
            0.5,
        );
        assert!((cp1.x - cp1.y).abs() < 0.1);
        assert!((cp2.x - cp2.y).abs() < 0.1);
    }

    #[test]
    fn test_fit_multi_segment_spline() {
        let path = vec![Point::new(0.0, 0.0), Point::new(0.0, 50.0), Point::new(100.0, 50.0), Point::new(100.0, 100.0)];
        let sampled = fit_multi_segment_spline(&path, 10);
        assert!(sampled.len() >= path.len());
        assert!((sampled[0].x - 0.0).abs() < 0.1);
        assert!((sampled[sampled.len() - 1].x - 100.0).abs() < 0.1);
    }

    #[test]
    fn test_bidirectional_edges() {
        let (diagram, result) = make_diagram_with_layout(
            vec![("a", 40.0, 40.0), ("b", 40.0, 170.0)],
            vec![("a", "b", None), ("b", "a", None)],
        );

        let routed = route_edges_spline(&diagram, result, BezierConfig::default());
        assert_eq!(routed.edges.len(), 2);
    }

    #[test]
    fn test_sample_bezier() {
        let sampled = sample_bezier(Point::new(0.0, 0.0), Point::new(30.0, -10.0), Point::new(70.0, -10.0), Point::new(100.0, 0.0), 10);
        assert_eq!(sampled.len(), 11);
        assert!((sampled[0].x - 0.0).abs() < 0.1 && (sampled[0].y - 0.0).abs() < 0.1);
        assert!((sampled[10].x - 100.0).abs() < 0.1 && (sampled[10].y - 0.0).abs() < 0.1);
    }
}
