//! 贝塞尔边路由模块
//!
//! 在节点布局完成后为每条边计算三次贝塞尔曲线路径。
//! 控制点沿端口方向自适应延伸，可通过 `edge_routing: bezier { tension: … }` 调节弧度。
//!
//! 障碍避让：路由完成后采样曲线检测穿障，穿障的边退化到 spline 绕行折线。

use std::collections::HashMap;

use crate::types::DiagramType;
use crate::ast::{Diagram};
use crate::layout::algorithm_config::{AlgorithmOptionSpec, OptionKind};
use crate::layout::geometry::Point;
use crate::layout::{EdgeLayout, LayoutResult, PathGeometry};
use crate::layout::routing::common::edge_geometry::{
    build_edge_labels, compute_bezier_controls,
    cubic_bezier_point, parse_label_t, point_at_path_t, DEFAULT_BEZIER_TENSION,
};
use crate::layout::routing::common::routing_skeleton::{
    finalize_edges, resolve_endpoints, RoutingContext,
};
use crate::layout::routing::common::self_loop::{self_loop_indices, route_self_loop, SelfLoopStyle};

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
        Self::from_options(&crate::layout::pipeline::plan::ResolvedAlgoOptions::from_spec_defaults(
            BEZIER_OPTIONS,
        ))
    }
}

impl BezierRouting {
    pub fn from_options(options: &crate::layout::pipeline::plan::ResolvedAlgoOptions) -> Self {
        Self {
            config: BezierConfig {
                tension: options.get_or_default(&BEZIER_OPTIONS[0]),
            },
        }
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
    // 4.2: 懒构建——快速预检无边可能穿障时跳过 O(n²) 构建
    let (node_id_to_idx, obstacle_index) = if crate::layout::routing::common::routing_skeleton::quick_check_need_obstacle_index(&result, relations) {
        let (idx, obs) = crate::layout::routing::common::routing_skeleton::build_obstacle_context(&result);
        (idx, Some(obs))
    } else {
        (HashMap::new(), None)
    };

    let self_loop_idx = self_loop_indices(relations);
    let mut edges: Vec<EdgeLayout> = Vec::with_capacity(relations.len());

    for (i, rel) in relations.iter().enumerate() {
        if rel.from.as_str() == rel.to.as_str() {
            if let Some(nl) = ctx.nodes.get(rel.from.as_str()) {
                let loop_idx = self_loop_idx.get(&i).copied().unwrap_or(0);
                edges.push(route_self_loop(rel, nl, loop_idx, SelfLoopStyle::Curved));
            } else {
                edges.push(EdgeLayout::empty());
            }
            continue;
        }

        let Some((ep, label_off)) = resolve_endpoints(&ctx, rel, i) else {
            edges.push(EdgeLayout::empty());
            continue;
        };

        let control_points = compute_bezier_controls(
            ep.start.x, ep.start.y, ep.end.x, ep.end.y,
            ep.from_port, ep.to_port, tension,
        );
        let control_points = [
            Point::new(control_points[0].x + ep.mid_ox, control_points[0].y + ep.mid_oy),
            Point::new(control_points[1].x + ep.mid_ox, control_points[1].y + ep.mid_oy),
        ];

        let mut geometry = PathGeometry::Bezier {
            start: ep.start,
            end: ep.end,
            controls: control_points,
        };

        // ── 穿障检测：采样曲线，若穿过非端点节点则退化到 spline 绕行 ──
        let from_idx = node_id_to_idx.get(ep.from_id.as_str()).copied().unwrap_or(usize::MAX);
        let to_idx = node_id_to_idx.get(ep.to_id.as_str()).copied().unwrap_or(usize::MAX);
        let skip = [from_idx, to_idx];

        let probe = EdgeLayout {
            geometry: geometry.clone(),
            labels: Vec::new(),
            from_port: ep.from_port,
            to_port: ep.to_port,
        };
        if let Some(ref obstacle_index) = obstacle_index {
            if crate::layout::routing::common::obstacle_check::curve_intersects_obstacles(&probe, obstacle_index, &skip) {
                let detour = obstacle_index.shortest_path(ep.start, ep.end, &skip);
                if !detour.is_empty() {
                    geometry = PathGeometry::Polyline { points: detour };
                }
            }
        }

        // 先定最终几何，再采样建标签（避免穿障改折线后标签仍挂旧曲线）
        let middle_t = parse_label_t(rel);
        let label_off_pt = Point::new(label_off.ox, label_off.oy);
        let labels = match &geometry {
            PathGeometry::Polyline { points } => {
                build_edge_labels(rel, middle_t, label_off_pt, |t| point_at_path_t(points, t))
            }
            PathGeometry::Bezier { start, end, controls } => {
                let cp0 = controls[0];
                let cp1 = controls[1];
                let s = *start;
                let e = *end;
                build_edge_labels(rel, middle_t, label_off_pt, |t| {
                    cubic_bezier_point(s, cp0, cp1, e, t)
                })
            }
            _ => build_edge_labels(rel, middle_t, label_off_pt, |_| {
                Point::new((ep.start.x + ep.end.x) * 0.5, (ep.start.y + ep.end.y) * 0.5)
            }),
        };

        edges.push(EdgeLayout {
            geometry,
            labels,
            from_port: ep.from_port,
            to_port: ep.to_port,
        });
    }

    finalize_edges(result, edges, diagram)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::routing::common::test_fixtures::make_diagram_with_layout;

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
