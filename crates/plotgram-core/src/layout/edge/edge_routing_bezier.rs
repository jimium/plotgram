//! 贝塞尔边路由模块
//!
//! 在节点布局完成后为每条边计算三次贝塞尔曲线路径。
//! 控制点沿端口方向自适应延伸，可通过 `edge_routing: bezier { tension: … }` 调节弧度。
//!
//! 障碍避让：路由完成后采样曲线检测穿障，穿障的边退化到 spline 绕行折线。

use crate::types::DiagramType;
use crate::ast::{Diagram};
use crate::layout::algorithm_config::{AlgorithmOptionSpec, OptionKind};
use crate::layout::geometry::Point;
use crate::layout::{EdgeLayout, EdgeRoutingStrategy, LayoutResult, PathGeometry};
use crate::layout::edge::common::edge_geometry::{
    build_edge_labels, compute_bezier_controls,
    cubic_bezier_point, parse_label_t, DEFAULT_BEZIER_TENSION,
};
use crate::layout::edge::common::routing_skeleton::{
    finalize_edges, resolve_endpoints, RoutingContext,
};
use crate::layout::edge::visibility;
use std::collections::HashMap;

const APPLICABLE_TYPES: &[DiagramType] = &[
    DiagramType::Flowchart,
    DiagramType::Architecture,
    DiagramType::State,
    DiagramType::Er,
    DiagramType::Mindmap,
];

pub(crate) const BEZIER_OPTIONS: &[AlgorithmOptionSpec] = &[AlgorithmOptionSpec {
    key: "tension",
    kind: OptionKind::Number {
        min: 0.0,
        max: 2.0,
        exclude_min: true,
    },
    default: DEFAULT_BEZIER_TENSION,
    description: "贝塞尔曲线控制点延伸比例",
}];

/// 障碍物膨胀间距
///
/// 保留供参考：实际膨胀间距取自 [`constants::DEFAULT_NODE_MARGIN`]。
/// `ObstacleIndex::build` 不再接受固定 padding 参数。
#[allow(dead_code)]
const OBSTACLE_PADDING: f64 = 8.0;

/// 穿障检测的曲线采样点数
const OBSTACLE_CHECK_SAMPLES: usize = 16;

/// 贝塞尔路由可调参数
#[derive(Clone, Copy)]
pub struct BezierConfig {
    pub tension: f64,
}

impl Default for BezierConfig {
    fn default() -> Self {
        Self {
            tension: BEZIER_OPTIONS[0].default,
        }
    }
}

/// 贝塞尔边路由策略（构造时注入已解析的 option）
pub struct BezierRouting {
    config: BezierConfig,
}

impl Default for BezierRouting {
    fn default() -> Self {
        Self::from_options(&crate::layout::plan::ResolvedAlgoOptions::from_spec_defaults(
            BEZIER_OPTIONS,
        ))
    }
}

impl BezierRouting {
    pub fn from_options(options: &crate::layout::plan::ResolvedAlgoOptions) -> Self {
        Self {
            config: BezierConfig {
                tension: options.get_or_default(&BEZIER_OPTIONS[0]),
            },
        }
    }
}

impl EdgeRoutingStrategy for BezierRouting {
    fn name(&self) -> &'static str {
        "bezier"
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
        route_edges_bezier(diagram, result, self.config)
    }

    /// bezier 穿障后会退化为 Polyline，需要 refine 检测并兜底。
    fn supports_refine(&self) -> bool {
        true
    }
}

/// 在节点布局完成后，为所有边计算贝塞尔路径与标签位置
pub fn route_edges_bezier(
    diagram: &Diagram,
    result: LayoutResult,
    config: BezierConfig,
) -> LayoutResult {
    let relations = &diagram.relations;
    let tension = config.tension;
    let ctx = RoutingContext::new(diagram, &result);

    // 构建障碍索引（用于穿障检测与退化绕行）
    // 膨胀间距取自 constants::DEFAULT_NODE_MARGIN。
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

        let control_points = compute_bezier_controls(
            ep.start.x, ep.start.y, ep.end.x, ep.end.y,
            ep.from_port, ep.to_port, tension,
        );

        // 标签位于曲线 t 处（由 label_position 锚点决定）
        let cp0 = control_points[0];
        let cp1 = control_points[1];
        let bez_start = ep.start;
        let bez_end = ep.end;
        let middle_t = parse_label_t(rel);
        let labels = build_edge_labels(rel, middle_t, Point::new(label_off.ox, label_off.oy), |t| {
            cubic_bezier_point(bez_start, cp0, cp1, bez_end, t)
        });

        let geometry = PathGeometry::Bezier {
            start: ep.start,
            end: ep.end,
            controls: control_points,
        };

        let mut edge = EdgeLayout {
            geometry,
            labels,
            from_port: ep.from_port,
            to_port: ep.to_port,
        };

        // ── 穿障检测：采样曲线，若穿过非端点节点则退化到 spline 绕行 ──
        let from_idx = node_id_to_idx.get(ep.from_id.as_str()).copied().unwrap_or(usize::MAX);
        let to_idx = node_id_to_idx.get(ep.to_id.as_str()).copied().unwrap_or(usize::MAX);
        let skip = [from_idx, to_idx];

        if curve_intersects_obstacles(&edge, &obstacle_index, &skip) {
            // 退化到 spline 绕行
            let detour = obstacle_index.shortest_path(ep.start, ep.end, &skip);
            if !detour.is_empty() {
                // 用绕行折线替换几何（保留标签位置与端口）
                edge.geometry = PathGeometry::Polyline { points: detour };
            }
        }

        edges.push(edge);
    }

    finalize_edges(result, edges, diagram)
}

/// 检测贝塞尔曲线采样后是否穿过任何非 skip 障碍物
fn curve_intersects_obstacles(
    edge: &EdgeLayout,
    obstacles: &visibility::ObstacleIndex,
    skip: &[usize],
) -> bool {
    let sampled = edge.sampled_path(OBSTACLE_CHECK_SAMPLES);
    for window in sampled.windows(2) {
        if obstacles.segment_hits_any(window[0], window[1], skip) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::edge::common::test_fixtures::make_diagram_with_layout;

    #[test]
    fn bezier_edge_has_control_points() {
        let (diagram, result) = make_diagram_with_layout(
            vec![("a", 0.0, 0.0), ("b", 200.0, 100.0)],
            vec![("a", "b", None)],
        );

        let routed = route_edges_bezier(&diagram, result, BezierConfig::default());
        assert_eq!(routed.edges.len(), 1);
        assert!(routed.edges[0].is_bezier());
        assert!(routed.edges[0].bezier_controls().is_some());
    }

    #[test]
    fn bezier_horizontal_edge_extends_control_points() {
        let (diagram, result) = make_diagram_with_layout(
            vec![("a", 40.0, 40.0), ("b", 260.0, 40.0)],
            vec![("a", "b", None)],
        );

        let routed = route_edges_bezier(&diagram, result, BezierConfig::default());
        let cp = routed.edges[0].bezier_controls().unwrap();
        let start = routed.edges[0].path_start().unwrap();
        assert!(cp[0].x > start.x);
    }

    #[test]
    fn bezier_bidirectional_edges_offset_controls() {
        let (diagram, result) = make_diagram_with_layout(
            vec![("a", 40.0, 40.0), ("b", 40.0, 200.0)],
            vec![("a", "b", None), ("b", "a", None)],
        );

        let routed = route_edges_bezier(&diagram, result, BezierConfig::default());
        assert_eq!(routed.edges.len(), 2);
        let cp_a = routed.edges[0].bezier_controls().unwrap();
        let cp_b = routed.edges[1].bezier_controls().unwrap();
        assert!((cp_a[0].x - cp_b[0].x).abs() > 0.1 || (cp_a[0].y - cp_b[0].y).abs() > 0.1);
    }

    #[test]
    fn bezier_detours_around_obstacle() {
        // 三个垂直对齐节点，a→c 的 bezier 会穿过 b
        let (diagram, result) = make_diagram_with_layout(
            vec![("a", 120.0, 40.0), ("b", 120.0, 150.0), ("c", 120.0, 300.0)],
            vec![("a", "c", None)],
        );

        let routed = route_edges_bezier(&diagram, result, BezierConfig::default());
        assert_eq!(routed.edges.len(), 1);
        // 穿障后应退化为 Polyline（绕行折线）
        assert!(
            routed.edges[0].is_polyline(),
            "bezier edge through obstacle should degrade to polyline, got {:?}",
            routed.edges[0].geometry
        );
    }

    #[test]
    fn bezier_keeps_bezier_when_no_obstacle() {
        // 两个节点无中间障碍，应保持 bezier
        let (diagram, result) = make_diagram_with_layout(
            vec![("a", 40.0, 40.0), ("b", 260.0, 40.0)],
            vec![("a", "b", None)],
        );

        let routed = route_edges_bezier(&diagram, result, BezierConfig::default());
        assert_eq!(routed.edges.len(), 1);
        assert!(
            routed.edges[0].is_bezier(),
            "bezier edge without obstacle should stay bezier"
        );
    }


}
