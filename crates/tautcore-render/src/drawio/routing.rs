//! draw.io 边路由：拐点规划、端口坐标、标签相对位置。

use tautcore_model::geometry::{Point, Rect};
use tautcore_model::result::EdgePath;

/// 一条边的 drawio 路由计划（坐标为画布坐标，页边距由编码器追加）。
pub(crate) struct EdgeRouting {
    pub style_parts: Vec<String>,
    /// 拐点 waypoint（画布坐标，不含首尾端点）。
    pub waypoints: Vec<Point>,
}

/// 根据 `EdgePath` 规划 drawio 边路由（优先可编辑性，不导出采样点）。
///
/// - Polyline 0 拐点 → `edgeStyle=none`
/// - Polyline 1 拐点 → `edgeStyle=elbowEdgeStyle`（无 waypoint）
/// - Polyline 2+ 拐点 → `edgeStyle=segmentEdgeStyle` + 全量拐点 waypoint
/// - Cubic → `edgeStyle=none;curved=1`（近似，非精确采样）
/// - 路径无效 → 空 style（draw.io 自动路由）
pub(crate) fn plan_edge_routing(path: &EdgePath) -> EdgeRouting {
    match path {
        EdgePath::Cubic { .. } => EdgeRouting {
            style_parts: vec!["edgeStyle=none".to_string(), "curved=1".to_string()],
            waypoints: Vec::new(),
        },
        EdgePath::Polyline { points } => {
            if points.len() < 2 {
                return EdgeRouting {
                    style_parts: Vec::new(),
                    waypoints: Vec::new(),
                };
            }
            let corners = extract_polyline_corners(points);
            match corners.len() {
                0 => EdgeRouting {
                    style_parts: vec!["edgeStyle=none".to_string()],
                    waypoints: Vec::new(),
                },
                1 => EdgeRouting {
                    style_parts: vec!["edgeStyle=elbowEdgeStyle".to_string()],
                    waypoints: Vec::new(),
                },
                _ => EdgeRouting {
                    style_parts: vec!["edgeStyle=segmentEdgeStyle".to_string()],
                    waypoints: corners,
                },
            }
        }
    }
}

/// 从折线提取方向变化拐点（去共线冗余点，不含首尾端点）。
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

/// 路径锚点 → 节点框上的 drawio 连接点相对坐标 (0..1)。
///
/// 锚点位于节点框边界：沿边偏移由几何直接归一化得到（平行边 / 反向边
/// 不再共用中心 0.5）。框退化时回退侧中点。
pub(crate) fn port_coords(point: Point, frame: Rect) -> (f64, f64) {
    if frame.width <= 0.0 || frame.height <= 0.0 {
        return (0.5, 0.5);
    }
    (
        ((point.x - frame.x) / frame.width).clamp(0.0, 1.0),
        ((point.y - frame.y) / frame.height).clamp(0.0, 1.0),
    )
}

pub(crate) fn fmt_port_coord(v: f64) -> String {
    format!("{:.4}", v.clamp(0.0, 1.0))
}

/// 标签中心 → drawio edge geometry 相对位置 `(x, y)`。
///
/// draw.io edge label 坐标系（geometry relative=1）：
/// - `x = -1` source 端；`x = 0` 中点；`x = 1` target 端
/// - `y`：垂直像素偏移（draw.io 向下为正；标签在线上方 → 负值）
pub(crate) fn compute_label_rel_pos(label_center: Point, path: &EdgePath) -> (f64, f64) {
    match path {
        EdgePath::Cubic { start, end, .. } => {
            // 简化：以起止点直线近似
            project_on_segment(label_center, *start, *end)
        }
        EdgePath::Polyline { points } => {
            if points.len() < 2 {
                return (0.0, 0.0);
            }
            let total_len: f64 = points
                .windows(2)
                .map(|w| dist(w[0], w[1]))
                .sum();
            if total_len < 1e-6 {
                return (0.0, 0.0);
            }

            // 找距标签中心最近的线段，投影得到全局 t 与带符号垂直偏移
            let mut best_t = 0.5_f64;
            let mut best_y_off = 0.0_f64;
            let mut best_dist_sq = f64::MAX;
            let mut cum_len = 0.0;
            for w in points.windows(2) {
                let (a, b) = (w[0], w[1]);
                let seg_len = dist(a, b);
                let (t_local, px, py) = project_point_on_segment(label_center, a, b);
                let dist_sq = dist_sq(label_center, Point { x: px, y: py });
                if dist_sq < best_dist_sq {
                    best_dist_sq = dist_sq;
                    best_t = (cum_len + t_local * seg_len) / total_len;
                    // 带符号垂直偏移：标签在线上方 → 负值（draw.io 向下为正）
                    let cross =
                        (label_center.x - px) * (b.y - a.y) - (label_center.y - py) * (b.x - a.x);
                    let sign = if cross >= 0.0 { -1.0 } else { 1.0 };
                    best_y_off = sign * dist_sq.sqrt();
                }
                cum_len += seg_len;
            }
            ((2.0 * best_t - 1.0).clamp(-1.0, 1.0), best_y_off)
        }
    }
}

fn dist(a: Point, b: Point) -> f64 {
    dist_sq(a, b).sqrt()
}

fn dist_sq(a: Point, b: Point) -> f64 {
    (a.x - b.x).powi(2) + (a.y - b.y).powi(2)
}

/// 点投影到线段：返回 (t∈[0,1], 投影点)。
fn project_point_on_segment(p: Point, a: Point, b: Point) -> (f64, f64, f64) {
    let len_sq = dist_sq(a, b);
    if len_sq < 1e-12 {
        return (0.5, a.x, a.y);
    }
    let t = (((p.x - a.x) * (b.x - a.x) + (p.y - a.y) * (b.y - a.y)) / len_sq).clamp(0.0, 1.0);
    (t, a.x + t * (b.x - a.x), a.y + t * (b.y - a.y))
}

fn project_on_segment(p: Point, a: Point, b: Point) -> (f64, f64) {
    let (t, px, py) = project_point_on_segment(p, a, b);
    let cross = (p.x - px) * (b.y - a.y) - (p.y - py) * (b.x - a.x);
    let sign = if cross >= 0.0 { -1.0 } else { 1.0 };
    ((2.0 * t - 1.0).clamp(-1.0, 1.0), sign * dist(p, Point { x: px, y: py }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64) -> Point {
        Point { x, y }
    }

    #[test]
    fn routing_plan_by_corner_count() {
        // (points, expect_style_joined, expect_waypoints)
        let cases: Vec<(Vec<Point>, &str, usize)> = vec![
            (vec![p(0.0, 0.0), p(10.0, 0.0)], "edgeStyle=none", 0),
            // 1 拐点 L 形 → elbow，无 waypoint
            (
                vec![p(0.0, 0.0), p(0.0, 10.0), p(10.0, 10.0)],
                "edgeStyle=elbowEdgeStyle",
                0,
            ),
            // 共线冗余点被去掉后仍是直线
            (
                vec![p(0.0, 0.0), p(5.0, 0.0), p(10.0, 0.0)],
                "edgeStyle=none",
                0,
            ),
            // 2+ 拐点 → segment + 全量 waypoint（V-H-V-H 共 3 个拐点）
            (
                vec![
                    p(0.0, 0.0),
                    p(0.0, 10.0),
                    p(10.0, 10.0),
                    p(10.0, 20.0),
                    p(20.0, 20.0),
                ],
                "edgeStyle=segmentEdgeStyle",
                3,
            ),
        ];
        for (points, want_style, want_wp) in cases {
            let r = plan_edge_routing(&EdgePath::polyline(points));
            assert_eq!(r.style_parts.join(";"), want_style);
            assert_eq!(r.waypoints.len(), want_wp);
        }

        // 无效路径 → 空 style
        let r = plan_edge_routing(&EdgePath::polyline(vec![p(1.0, 2.0)]));
        assert!(r.style_parts.is_empty());
        assert!(r.waypoints.is_empty());

        // Cubic → curved
        let r = plan_edge_routing(&EdgePath::cubic(
            p(0.0, 0.0),
            p(10.0, 0.0),
            [p(2.5, 5.0), p(7.5, 5.0)],
        ));
        assert!(r.style_parts.contains(&"curved=1".to_string()));
        assert!(r.waypoints.is_empty());
    }

    #[test]
    fn port_coords_normalize_along_the_frame() {
        let frame = Rect::new(200.0, 100.0, 160.0, 50.0);
        // 下边界偏左 0.375
        let (x, y) = port_coords(p(260.0, 150.0), frame);
        assert!((x - 0.375).abs() < 1e-9);
        assert_eq!(y, 1.0);
        // 左边界
        let (x, y) = port_coords(p(200.0, 125.0), frame);
        assert_eq!(x, 0.0);
        assert_eq!(y, 0.5);
        // 退化框回退中心
        let (x, y) = port_coords(p(10.0, 10.0), Rect::new(0.0, 0.0, 0.0, 0.0));
        assert_eq!((x, y), (0.5, 0.5));
    }

    #[test]
    fn label_rel_pos_on_polyline() {
        // Z 形路径，标签在中段水平段上方
        let path = EdgePath::polyline(vec![
            p(0.0, 0.0),
            p(0.0, 10.0),
            p(20.0, 10.0),
            p(20.0, 20.0),
        ]);
        // 中点 (10, 10) 正上方 4px → drawio 偏移为负（向下为正）
        let (x, y) = compute_label_rel_pos(p(10.0, 6.0), &path);
        assert!((x - 0.0).abs() < 1e-6, "mid label x≈0, got {x}");
        assert!((y - -4.0).abs() < 1e-6, "offset above segment, got {y}");
    }
}
