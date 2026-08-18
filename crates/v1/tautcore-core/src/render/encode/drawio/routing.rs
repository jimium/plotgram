//! draw.io 边路由规划、端口坐标与标签几何。

use crate::layout::geometry::Point;
use crate::layout::{NodeLayout, PathGeometry, Port};
use crate::render::scene::{ExportEdge, ExportScene};

use super::report::{DegradeTier, DrawioExportOptions, ExportReport, ExportWarning};

/// `Port` → drawio 连接点坐标 (x, y) 默认值（侧中点），无布局信息时回退。
pub(crate) fn port_to_xy(port: &Port) -> (f64, f64) {
    match port {
        Port::Top => (0.5, 0.0),
        Port::Bottom => (0.5, 1.0),
        Port::Left => (0.0, 0.5),
        Port::Right => (1.0, 0.5),
    }
}

/// 将布局路径锚点映射为 draw.io 端口相对坐标 (0..1)。
pub(crate) fn anchor_to_drawio_port(point: Point, node: &NodeLayout, port: Port) -> (f64, f64) {
    if node.width <= 0.0 || node.height <= 0.0 {
        return port_to_xy(&port);
    }
    match port {
        Port::Top => (
            ((point.x - node.x) / node.width).clamp(0.0, 1.0),
            0.0,
        ),
        Port::Bottom => (
            ((point.x - node.x) / node.width).clamp(0.0, 1.0),
            1.0,
        ),
        Port::Left => (
            0.0,
            ((point.y - node.y) / node.height).clamp(0.0, 1.0),
        ),
        Port::Right => (
            1.0,
            ((point.y - node.y) / node.height).clamp(0.0, 1.0),
        ),
    }
}

fn find_node_layout<'a>(scene: &'a ExportScene<'_>, entity_id: &str) -> Option<&'a NodeLayout> {
    scene
        .nodes
        .iter()
        .find(|n| n.entity.id.as_str() == entity_id)
        .map(|n| &n.layout)
}

/// 从路径几何 + 节点布局解析 draw.io 的 exit/entry 端口坐标。
pub(crate) fn resolve_edge_ports(edge: &ExportEdge<'_>, scene: &ExportScene<'_>) -> (f64, f64, f64, f64) {
    let start = edge.layout.geometry.start();
    let end = edge.layout.geometry.end();
    let from_id = edge.relation.from.as_str();
    let to_id = edge.relation.to.as_str();

    let (exit_x, exit_y) = find_node_layout(scene, from_id)
        .map(|nl| anchor_to_drawio_port(start, nl, edge.layout.from_port))
        .unwrap_or_else(|| port_to_xy(&edge.layout.from_port));
    let (entry_x, entry_y) = find_node_layout(scene, to_id)
        .map(|nl| anchor_to_drawio_port(end, nl, edge.layout.to_port))
        .unwrap_or_else(|| port_to_xy(&edge.layout.to_port));

    (exit_x, exit_y, entry_x, entry_y)
}

pub(crate) fn fmt_port_coord(v: f64) -> String {
    format!("{:.4}", v.clamp(0.0, 1.0))
}

/// draw.io 边路由计划：用 style 字符串表达曲线类型，必要时辅以少量拐点。
pub(crate) struct DrawioEdgeRouting {
    pub(crate) style_parts: Vec<String>,
    pub(crate) waypoints: Vec<(f64, f64)>,
    pub(crate) tier: DegradeTier,
}

/// 根据 PathGeometry 规划 draw.io 边路由。
///
/// 策略（优先可编辑性，避免大量采样点）：
/// - `Straight` → `edgeStyle=none`，无拐点
/// - `Bezier` → `edgeStyle=none;curved=1`，无拐点（draw.io 原生曲线）
/// - `Polyline` → 0 拐点直线；1 拐点用 `elbowEdgeStyle`（无 waypoint）；2+ 拐点用
///   `segmentEdgeStyle;rounded=1`，最多保留 `max_edge_waypoints` 个（超出时取首尾拐点，L1 降级）
pub(crate) fn plan_drawio_edge_routing(
    edge: &ExportEdge<'_>,
    options: &DrawioExportOptions,
    report: &mut ExportReport,
) -> DrawioEdgeRouting {
    let pad = options.page_padding;
    let max_wp = options.max_edge_waypoints as usize;

    if edge.layout.path_len() < 2 {
        report.warnings.push(ExportWarning {
            code: "EDGE_NO_PATH".to_string(),
            entity_id: None,
            edge_index: Some(edge.index),
            tier: DegradeTier::L2,
            message: format!("边 #{} 路径无效，由 draw.io 自动路由", edge.index),
        });
        return DrawioEdgeRouting {
            style_parts: Vec::new(),
            waypoints: Vec::new(),
            tier: DegradeTier::L2,
        };
    }

    match &edge.layout.geometry {
        PathGeometry::Straight { .. } => DrawioEdgeRouting {
            style_parts: vec!["edgeStyle=none".to_string()],
            waypoints: Vec::new(),
            tier: DegradeTier::L0,
        },
        PathGeometry::Bezier { .. } => {
            report.warnings.push(ExportWarning {
                code: "BEZIER_CURVED".to_string(),
                entity_id: None,
                edge_index: Some(edge.index),
                tier: DegradeTier::L1,
                message: format!(
                    "边 #{} Bezier 曲线映射为 draw.io curved=1（非精确采样）",
                    edge.index
                ),
            });
            DrawioEdgeRouting {
                style_parts: vec!["edgeStyle=none".to_string(), "curved=1".to_string()],
                waypoints: Vec::new(),
                tier: DegradeTier::L1,
            }
        }
        PathGeometry::Polyline { points } => {
            let corners = extract_polyline_corners(points);
            if corners.is_empty() {
                return DrawioEdgeRouting {
                    style_parts: vec!["edgeStyle=none".to_string()],
                    waypoints: Vec::new(),
                    tier: DegradeTier::L0,
                };
            }

            let mut tier = DegradeTier::L0;
            if corners.len() == 1 {
                return DrawioEdgeRouting {
                    style_parts: vec!["edgeStyle=elbowEdgeStyle".to_string()],
                    waypoints: Vec::new(),
                    tier: DegradeTier::L0,
                };
            }

            if corners.len() > max_wp {
                report.warnings.push(ExportWarning {
                    code: "EDGE_WAYPOINTS_SIMPLIFIED".to_string(),
                    entity_id: None,
                    edge_index: Some(edge.index),
                    tier: DegradeTier::L1,
                    message: format!(
                        "边 #{} 有 {} 个拐点，draw.io 导出仅保留 {} 个",
                        edge.index,
                        corners.len(),
                        max_wp
                    ),
                });
                tier = DegradeTier::L1;
            }

            let selected = select_waypoints(&corners, max_wp);
            let waypoints: Vec<(f64, f64)> = selected
                .iter()
                .map(|p| (p.x + pad, p.y + pad))
                .collect();

            DrawioEdgeRouting {
                style_parts: vec![
                    "edgeStyle=segmentEdgeStyle".to_string(),
                    "rounded=1".to_string(),
                ],
                waypoints,
                tier,
            }
        }
    }
}

/// 从折线中提取方向变化拐点（去掉共线冗余点，不含首尾端点）。
pub(crate) fn extract_polyline_corners(points: &[Point]) -> Vec<Point> {
    if points.len() < 3 {
        return Vec::new();
    }
    let mut corners = Vec::new();
    for i in 1..points.len() - 1 {
        let d1 = segment_axis(points[i - 1], points[i]);
        let d2 = segment_axis(points[i], points[i + 1]);
        if d1 != d2 {
            corners.push(points[i]);
        }
    }
    corners
}

/// 线段主方向：Horizontal / Vertical / Other（含零长度）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SegmentAxis {
    Horizontal,
    Vertical,
    Other,
}

const SEGMENT_EPS: f64 = 1e-3;

fn segment_axis(a: Point, b: Point) -> SegmentAxis {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    if dx.abs() < SEGMENT_EPS && dy.abs() < SEGMENT_EPS {
        return SegmentAxis::Other;
    }
    if dy.abs() < SEGMENT_EPS {
        SegmentAxis::Horizontal
    } else if dx.abs() < SEGMENT_EPS {
        SegmentAxis::Vertical
    } else {
        SegmentAxis::Other
    }
}

/// 从拐点列表中选取最多 `max` 个用于 draw.io waypoint（优先保留首尾拐点）。
fn select_waypoints(corners: &[Point], max: usize) -> Vec<Point> {
    if max == 0 || corners.is_empty() {
        return Vec::new();
    }
    if corners.len() <= max {
        return corners.to_vec();
    }
    if max == 1 {
        return vec![corners[corners.len() / 2]];
    }
    vec![corners[0], corners[corners.len() - 1]]
}

/// 生成边的 `mxGeometry` XML（waypoints + 标签相对位置）。
///
/// draw.io 原生做法：标签文字写在 edge cell 的 `value`，位置由 geometry 的
/// `x`/`y`（relative=1）与 `<mxPoint as="offset"/>` 控制，标签随边移动/重连。
pub(crate) fn format_edge_geometry(
    waypoints: &[(f64, f64)],
    label_center: Option<Point>,
    geometry: &PathGeometry,
) -> String {
    let waypoint_xml = if waypoints.is_empty() {
        String::new()
    } else {
        let pts: Vec<String> = waypoints
            .iter()
            .map(|(x, y)| format!(r#"<mxPoint x="{x}" y="{y}" />"#))
            .collect();
        format!(r#"<Array as="points">{}</Array>"#, pts.join(""))
    };

    match label_center {
        None if waypoint_xml.is_empty() => {
            r#"<mxGeometry relative="1" as="geometry" />"#.to_string()
        }
        None => format!(
            r#"<mxGeometry relative="1" as="geometry">
            {waypoint_xml}
          </mxGeometry>"#
        ),
        Some(center) => {
            let (x, y) = compute_label_rel_pos(center, geometry);
            let inner = if waypoint_xml.is_empty() {
                r#"<mxPoint as="offset" />"#.to_string()
            } else {
                format!("{waypoint_xml}\n            <mxPoint as=\"offset\" />")
            };
            format!(
                r#"<mxGeometry x="{x:.4}" y="{y:.2}" relative="1" as="geometry">
            {inner}
          </mxGeometry>"#
            )
        }
    }
}

/// 计算标签在边路径上的相对位置 (`x`) 和垂直偏移 (`y`)。
///
/// draw.io edge label 坐标系（geometry relative=1）：
/// - `x = -1`：source 端；`x = 0`：中点；`x = 1`：target 端
/// - `y`：垂直偏移（正方向朝上），像素单位
pub(crate) fn compute_label_rel_pos(
    label_center: Point,
    geometry: &PathGeometry,
) -> (f64, f64) {
    let lx = label_center.x;
    let ly = label_center.y;

    match geometry {
        PathGeometry::Straight { start, end } => {
            project_on_segment(lx, ly, start.x, start.y, end.x, end.y)
        }
        PathGeometry::Polyline { points } => {
            if points.len() < 2 {
                return (0.0, 0.0);
            }
            let total_len: f64 = points
                .windows(2)
                .map(|w| {
                    let dx = w[1].x - w[0].x;
                    let dy = w[1].y - w[0].y;
                    (dx * dx + dy * dy).sqrt()
                })
                .sum();

            if total_len < 1e-6 {
                return (0.0, 0.0);
            }

            // 找到距标签中心最近的线段
            let mut best_t = 0.5_f64;
            let mut best_y_off = 0.0_f64;
            let mut best_dist_sq = f64::MAX;
            let mut cum_len = 0.0;

            for w in points.windows(2) {
                let a = w[0];
                let b = w[1];
                let seg_len = ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
                let (t_local, px, py) = project_point_on_segment(lx, ly, a.x, a.y, b.x, b.y);
                let dist_sq = (lx - px).powi(2) + (ly - py).powi(2);

                if dist_sq < best_dist_sq {
                    best_dist_sq = dist_sq;
                    let t_global = if total_len > 0.0 {
                        (cum_len + t_local * seg_len) / total_len
                    } else {
                        0.5
                    };
                    best_t = t_global;
                    // 带符号垂直偏移：正 = 屏幕上方
                    let edge_dx = b.x - a.x;
                    let edge_dy = b.y - a.y;
                    let cross = (lx - px) * edge_dy - (ly - py) * edge_dx;
                    let sign = if cross >= 0.0 { -1.0 } else { 1.0 };
                    best_y_off = sign * dist_sq.sqrt();
                }
                cum_len += seg_len;
            }

            let x_pos = (2.0 * best_t - 1.0).clamp(-1.0, 1.0);
            (x_pos, best_y_off)
        }
        PathGeometry::Bezier { start, end, .. } => {
            // 简化：以起止点直线近似
            project_on_segment(lx, ly, start.x, start.y, end.x, end.y)
        }
    }
}

/// 将点投影到线段上，返回 (参数 t∈[0,1], 投影点 x, 投影点 y)。
fn project_point_on_segment(
    px: f64, py: f64,
    ax: f64, ay: f64,
    bx: f64, by: f64,
) -> (f64, f64, f64) {
    let dx = bx - ax;
    let dy = by - ay;
    let len_sq = dx * dx + dy * dy;
    if len_sq < 1e-12 {
        return (0.5, ax, ay);
    }
    let t = ((px - ax) * dx + (py - ay) * dy) / len_sq;
    let t = t.clamp(0.0, 1.0);
    let proj_x = ax + t * dx;
    let proj_y = ay + t * dy;
    (t, proj_x, proj_y)
}

/// 便捷封装：投影并直接返回 draw.io 相对坐标 (x, y)。
fn project_on_segment(
    lx: f64, ly: f64,
    ax: f64, ay: f64,
    bx: f64, by: f64,
) -> (f64, f64) {
    let (t, proj_x, proj_y) = project_point_on_segment(lx, ly, ax, ay, bx, by);
    let x_pos = (2.0 * t - 1.0).clamp(-1.0, 1.0);
    // 带符号垂直偏移
    let edge_dx = bx - ax;
    let edge_dy = by - ay;
    let cross = (lx - proj_x) * edge_dy - (ly - proj_y) * edge_dx;
    let sign = if cross >= 0.0 { -1.0 } else { 1.0 };
    let dist = ((lx - proj_x).powi(2) + (ly - proj_y).powi(2)).sqrt();
    let y_off = sign * dist;
    (x_pos, y_off)
}

/// 取 PathGeometry 中点坐标（用于无 layout label 时的回退位置）。
pub(crate) fn geometry_midpoint(geometry: &PathGeometry) -> Point {
    match geometry {
        PathGeometry::Straight { start, end } => {
            Point::new((start.x + end.x) / 2.0, (start.y + end.y) / 2.0)
        }
        PathGeometry::Polyline { points } => {
            if points.is_empty() {
                return Point::new(0.0, 0.0);
            }
            let mid = points.len() / 2;
            points[mid]
        }
        PathGeometry::Bezier { start, end, controls } => {
            // 三次 Bezier 在 t=0.5 处的值
            let c0 = start;
            let c1 = controls[0];
            let c2 = controls[1];
            let c3 = end;
            let x = 0.125 * c0.x + 0.375 * c1.x + 0.375 * c2.x + 0.125 * c3.x;
            let y = 0.125 * c0.y + 0.375 * c1.y + 0.375 * c2.y + 0.125 * c3.y;
            Point::new(x, y)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        ArrowType, AttributeMap, Identifier, Relation, Span,
    };
    use crate::layout::{EdgeLayout, NodeLayout, PathGeometry, Port};
    use crate::render::scene::ExportEdge;
    use crate::render::visual::EdgeStyle;
    use crate::types::DiagramType;

    fn make_synthetic_edge<'a>(index: usize, relation: &'a Relation, geometry: PathGeometry) -> ExportEdge<'a> {
        ExportEdge {
            index,
            relation,
            layout: EdgeLayout {
                geometry,
                labels: Vec::new(),
                from_port: Port::Right,
                to_port: Port::Left,
            },
            style: EdgeStyle::default(),
        }
    }

    #[test]
    fn test_plan_drawio_edge_routing_empty_path() {
        let span = Span::dummy();
        let relation = Relation {
            from: Identifier::new_unchecked("a"),
            to: Identifier::new_unchecked("b"),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span,
        };
        let edge = make_synthetic_edge(
            0,
            &relation,
            PathGeometry::Polyline {
                points: vec![Point::new(1.0, 2.0)],
            },
        );
        let mut report = ExportReport::new(&DiagramType::Flowchart);
        let routing =
            plan_drawio_edge_routing(&edge, &DrawioExportOptions::default(), &mut report);
        assert!(routing.waypoints.is_empty(), "空路径应无拐点");
        assert!(routing.style_parts.is_empty());
        assert_eq!(routing.tier, DegradeTier::L2);
        assert!(report.warnings.iter().any(|w| w.code == "EDGE_NO_PATH"));
    }

    #[test]
    fn test_plan_drawio_edge_routing_straight() {
        let span = Span::dummy();
        let relation = Relation {
            from: Identifier::new_unchecked("a"),
            to: Identifier::new_unchecked("b"),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span,
        };
        let edge = make_synthetic_edge(
            0,
            &relation,
            PathGeometry::Straight {
                start: Point::new(0.0, 0.0),
                end: Point::new(10.0, 10.0),
            },
        );
        let mut report = ExportReport::new(&DiagramType::Flowchart);
        let routing =
            plan_drawio_edge_routing(&edge, &DrawioExportOptions::default(), &mut report);
        assert!(routing.waypoints.is_empty());
        assert!(routing.style_parts.contains(&"edgeStyle=none".to_string()));
        assert_eq!(routing.tier, DegradeTier::L0);
    }

    #[test]
    fn test_plan_drawio_edge_routing_bezier_uses_curved() {
        let span = Span::dummy();
        let relation = Relation {
            from: Identifier::new_unchecked("a"),
            to: Identifier::new_unchecked("b"),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span,
        };
        let edge = make_synthetic_edge(
            2,
            &relation,
            PathGeometry::Bezier {
                start: Point::new(0.0, 0.0),
                end: Point::new(10.0, 0.0),
                controls: [Point::new(2.5, 5.0), Point::new(7.5, 5.0)],
            },
        );
        let mut report = ExportReport::new(&DiagramType::Flowchart);
        let routing =
            plan_drawio_edge_routing(&edge, &DrawioExportOptions::default(), &mut report);
        assert!(routing.waypoints.is_empty(), "Bezier 不应导出采样拐点");
        assert!(routing.style_parts.contains(&"curved=1".to_string()));
        assert_eq!(routing.tier, DegradeTier::L1);
        assert!(report.warnings.iter().any(|w| w.code == "BEZIER_CURVED"));
    }

    #[test]
    fn test_plan_drawio_edge_routing_polyline_corners() {
        let span = Span::dummy();
        let relation = Relation {
            from: Identifier::new_unchecked("a"),
            to: Identifier::new_unchecked("b"),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span,
        };
        let edge = make_synthetic_edge(
            0,
            &relation,
            PathGeometry::Polyline {
                points: vec![Point::new(0.0, 0.0), Point::new(0.0, 10.0), Point::new(10.0, 10.0)],
            },
        );
        let mut report = ExportReport::new(&DiagramType::Flowchart);
        let routing =
            plan_drawio_edge_routing(&edge, &DrawioExportOptions::default(), &mut report);
        assert_eq!(routing.waypoints.len(), 0, "L 形应使用 elbowEdgeStyle，无 waypoint");
        assert!(routing
            .style_parts
            .contains(&"edgeStyle=elbowEdgeStyle".to_string()));
        assert_eq!(routing.tier, DegradeTier::L0);
    }

    #[test]
    fn test_plan_drawio_edge_routing_polyline_max_two_waypoints() {
        let span = Span::dummy();
        let relation = Relation {
            from: Identifier::new_unchecked("a"),
            to: Identifier::new_unchecked("b"),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span,
        };
        let edge = make_synthetic_edge(
            0,
            &relation,
            PathGeometry::Polyline {
                points: vec![
                    Point::new(0.0, 0.0),
                    Point::new(0.0, 10.0),
                    Point::new(10.0, 10.0),
                    Point::new(10.0, 20.0),
                    Point::new(20.0, 20.0),
                ],
            },
        );
        let mut report = ExportReport::new(&DiagramType::Flowchart);
        let routing =
            plan_drawio_edge_routing(&edge, &DrawioExportOptions::default(), &mut report);
        assert_eq!(routing.waypoints.len(), 2);
        assert_eq!(routing.tier, DegradeTier::L1);
        assert!(report
            .warnings
            .iter()
            .any(|w| w.code == "EDGE_WAYPOINTS_SIMPLIFIED"));
    }

    #[test]
    fn test_extract_polyline_corners_skips_collinear() {
        let points = vec![
            Point::new(0.0, 0.0),
            Point::new(0.0, 5.0),
            Point::new(0.0, 10.0),
            Point::new(10.0, 10.0),
            Point::new(10.0, 20.0),
        ];
        let corners = extract_polyline_corners(&points);
        assert_eq!(corners.len(), 2);
        assert_eq!(corners[0], Point::new(0.0, 10.0));
        assert_eq!(corners[1], Point::new(10.0, 10.0));
    }

    #[test]
    fn test_port_to_xy() {
        assert_eq!(port_to_xy(&Port::Top), (0.5, 0.0));
        assert_eq!(port_to_xy(&Port::Bottom), (0.5, 1.0));
        assert_eq!(port_to_xy(&Port::Left), (0.0, 0.5));
        assert_eq!(port_to_xy(&Port::Right), (1.0, 0.5));
    }

    #[test]
    fn test_anchor_to_drawio_port_parallel_edges() {
        let client = NodeLayout {
            x: 202.0,
            y: 70.0,
            width: 160.0,
            height: 50.0,
        };
        let (ex, ey) = anchor_to_drawio_port(Point::new(262.0, 120.0), &client, Port::Bottom);
        assert!((ex - 0.375).abs() < 1e-3);
        assert_eq!(ey, 1.0);

        let gateway = NodeLayout {
            x: 202.0,
            y: 204.0,
            width: 160.0,
            height: 50.0,
        };
        let (enx, eny) = anchor_to_drawio_port(Point::new(302.0, 204.0), &gateway, Port::Top);
        assert!((enx - 0.625).abs() < 1e-3);
        assert_eq!(eny, 0.0);
    }
}
