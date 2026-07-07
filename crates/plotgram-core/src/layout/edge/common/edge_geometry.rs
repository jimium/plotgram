//! 边几何辅助函数
//!
//! 节点中心、端口选择、贝塞尔控制点等在多个边路由模块中
//! 重复定义的函数统一收归于此。

use crate::layout::geometry::Point;
use crate::layout::{EdgeLabelLayout, NodeLayout, Port};

pub use crate::layout::geometry::node_center;

/// 默认贝塞尔张力（0.0 = 直线，1.0 = 最大弧度）
pub const DEFAULT_BEZIER_TENSION: f64 = 0.55;

/// 最小控制点延伸距离（避免曲线太"平"）
const MIN_CONTROL_EXTENSION: f64 = 20.0;

/// 无向节点对的唯一键（用于分组平行边）
pub fn undirected_pair_key(a: &str, b: &str) -> String {
    if a < b {
        format!("{a}|{b}")
    } else {
        format!("{b}|{a}")
    }
}

/// 规范化的有序节点对（用于确定正向方向）
pub fn canonical_pair<'a>(a: &'a str, b: &'a str) -> (&'a str, &'a str) {
    if a < b { (a, b) } else { (b, a) }
}

/// 基于规范方向计算垂直单位向量
///
/// 确保同一对节点无论边方向如何，法线方向一致，
/// 从而使正/反向边的偏移落在法线的两侧。
pub fn canonical_perpendicular(
    from_id: &str,
    to_id: &str,
    cx1: f64,
    cy1: f64,
    cx2: f64,
    cy2: f64,
) -> Point {
    let dx = cx2 - cx1;
    let dy = cy2 - cy1;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.01 {
        return Point::new(0.0, 1.0);
    }

    let (can_from, can_to) = canonical_pair(from_id, to_id);
    let sign = if from_id == can_from && to_id == can_to { 1.0 } else { -1.0 };

    Point::new(-dy / len * sign, dx / len * sign)
}

/// 箭头类型的稳定标签，用于并线分组键（`ArrowType` 未派生 `Hash`）。
///
/// 返回值相同的边在「箭头方向」维度上一致，才允许并线（并线原则 1）。
pub fn arrow_type_tag(arrow: &crate::ast::ArrowType) -> &'static str {
    match arrow {
        crate::ast::ArrowType::Active => "active",
        crate::ast::ArrowType::Passive => "passive",
        crate::ast::ArrowType::Bidirectional => "bidi",
    }
}

/// 计算边的线型签名，用于并线分组时区分不同视觉类型的边。
///
/// 签名规则：
/// - 若设置了非空 `stroke_dasharray`，返回 `dash:<pattern>`
/// - 否则若 `dashed == true`，返回 `dashed`
/// - 否则返回 `solid`
///
/// 同签名的边在视觉上线型一致，才允许并线（并线原则 2）。
pub fn edge_line_style_signature(rel: &crate::ast::Relation) -> String {
    use crate::types::style_attr_keys::{DASHED, STROKE_DASHARRAY};
    if let Some(v) = rel.attributes.style.get(STROKE_DASHARRAY) {
        let s = match v {
            crate::ast::AttributeValue::String(s) => Some(s.as_str()),
            _ => None,
        };
        if let Some(non_empty) = s.filter(|x| !x.is_empty()) {
            return format!("dash:{non_empty}");
        }
    }
    if let Some(crate::ast::AttributeValue::Boolean(true)) = rel.attributes.style.get(DASHED) {
        return "dashed".to_string();
    }
    "solid".to_string()
}

/// 计算边的描边颜色签名，用于并线分组时区分不同颜色的边。
///
/// 签名规则：
/// - 若设置了 `stroke`，返回 `stroke:<color>`（trim 后小写）
/// - 否则返回 `default`
///
/// 同签名的边在视觉上颜色一致，才允许并线。
pub fn edge_stroke_color_signature(rel: &crate::ast::Relation) -> String {
    use crate::types::style_attr_keys::STROKE;
    let v = rel.attributes.style.get(STROKE);
    match v {
        Some(crate::ast::AttributeValue::String(s)) => {
            let s = s.trim().to_lowercase();
            if s.is_empty() { "default".to_string() } else { format!("stroke:{s}") }
        }
        _ => "default".to_string(),
    }
}

/// 计算边的描边宽度签名，用于并线分组时区分不同粗细的边。
///
/// 签名规则：
/// - 若设置了 `stroke_width`，返回 `width:<value>`（数值）
/// - 否则返回 `default`
///
/// 同签名的边在视觉上粗细一致，才允许并线。
pub fn edge_stroke_width_signature(rel: &crate::ast::Relation) -> String {
    use crate::types::style_attr_keys::STROKE_WIDTH;
    let v = rel.attributes.style.get(STROKE_WIDTH);
    match v {
        Some(crate::ast::AttributeValue::Number(n)) => format!("width:{n}"),
        Some(crate::ast::AttributeValue::String(s)) => {
            let s = s.trim();
            if let Ok(n) = s.parse::<f64>() { format!("width:{n}") } else { "default".to_string() }
        }
        _ => "default".to_string(),
    }
}

/// 根据连接点在节点上的位置选择端口
pub fn select_port(px: f64, py: f64, nl: &NodeLayout) -> Port {
    let cx = nl.x + nl.width / 2.0;
    let cy = nl.y + nl.height / 2.0;
    let dx = px - cx;
    let dy = py - cy;

    if dx.abs() > dy.abs() {
        if dx > 0.0 { Port::Right } else { Port::Left }
    } else if dy > 0.0 {
        Port::Bottom
    } else {
        Port::Top
    }
}

/// 计算贝塞尔控制点（端口感知 + 自适应张力）
///
/// 控制点沿端口射出方向延伸，并根据节点间距、端口对齐与转角关系
/// 自适应调整，使曲线自然地从节点端口出发/到达。
pub fn compute_bezier_controls(
    sx: f64,
    sy: f64,
    ex: f64,
    ey: f64,
    from_port: Port,
    to_port: Port,
    tension: f64,
) -> [Point; 2] {
    let from_dir = port_direction(from_port);
    let to_dir = port_direction(to_port);

    let dx = ex - sx;
    let dy = ey - sy;
    let dist = (dx * dx + dy * dy).sqrt().max(1.0);

    let base_extension = (dist * tension).max(MIN_CONTROL_EXTENSION);

    let to_vector = Point::new(dx / dist, dy / dist);

    let alignment = from_dir.x * to_vector.x + from_dir.y * to_vector.y;

    let cp1 = if alignment > 0.7 {
        Point::new(
            sx + from_dir.x * base_extension,
            sy + from_dir.y * base_extension,
        )
    } else if alignment < -0.7 {
        let nx = -from_dir.y;
        let ny = from_dir.x;
        let blend = 0.6;
        Point::new(
            sx + from_dir.x * base_extension * 0.3 + nx * base_extension * blend,
            sy + from_dir.y * base_extension * 0.3 + ny * base_extension * blend,
        )
    } else {
        let blend = (1.0 - alignment.abs()) * 0.5 + 0.2;
        Point::new(
            sx + from_dir.x * base_extension * (1.0 - blend) + to_vector.x * base_extension * blend,
            sy + from_dir.y * base_extension * (1.0 - blend) + to_vector.y * base_extension * blend,
        )
    };

    let from_vector = Point::new(-dx / dist, -dy / dist);

    let alignment2 = to_dir.x * from_vector.x + to_dir.y * from_vector.y;

    let cp2 = if alignment2 > 0.7 {
        Point::new(
            ex + to_dir.x * base_extension,
            ey + to_dir.y * base_extension,
        )
    } else if alignment2 < -0.7 {
        let nx = -to_dir.y;
        let ny = to_dir.x;
        let blend = 0.6;
        Point::new(
            ex + to_dir.x * base_extension * 0.3 + nx * base_extension * blend,
            ey + to_dir.y * base_extension * 0.3 + ny * base_extension * blend,
        )
    } else {
        let blend = (1.0 - alignment2.abs()) * 0.5 + 0.2;
        Point::new(
            ex + to_dir.x * base_extension * (1.0 - blend) + from_vector.x * base_extension * blend,
            ey + to_dir.y * base_extension * (1.0 - blend) + from_vector.y * base_extension * blend,
        )
    };

    [cp1, cp2]
}

/// 默认肩长比例（沿端口方向伸出的长度占连线距离的比例）
pub const DEFAULT_SHOULDER_RATIO: f64 = 0.35;

/// 计算有机贝塞尔控制点（适合 MindMap 等树形结构）
///
/// 采用「肘形 S 曲线」设计，类似 XMind / MindManager 等主流产品风格：
/// - 控制点沿端口方向伸出一段「肩」，再平滑过渡到目标方向
/// - 两端控制点向相反方向偏移，形成自然的 S 形弧线
/// - 曲线从节点侧面水平/垂直伸出，连接处更自然
///
/// 起点和终点应已位于节点边界上（如通过 `edge_point` 计算）。
///
/// # 参数
/// - `tension`: 整体弧度大小（0.0 = 近直线，1.0 = 最大弧度）
/// - `shoulder_ratio`: 肩长占连线距离的比例，控制水平伸出段的长度
pub fn compute_bezier_controls_organic(
    sx: f64,
    sy: f64,
    ex: f64,
    ey: f64,
    from_port: Port,
    to_port: Port,
    tension: f64,
    shoulder_ratio: f64,
) -> [Point; 2] {
    let dx = ex - sx;
    let dy = ey - sy;
    let dist = (dx * dx + dy * dy).sqrt().max(1.0);

    let dir = Point::new(dx / dist, dy / dist);

    let from_dir = port_direction(from_port);
    let to_dir = port_direction(to_port);

    let shoulder_len = (dist * shoulder_ratio * tension).max(MIN_CONTROL_EXTENSION * 0.5);

    let axial_extension = (dist * tension * 0.2).max(MIN_CONTROL_EXTENSION * 0.3);

    let cp1_x = sx + from_dir.x * shoulder_len + dir.x * axial_extension;
    let cp1_y = sy + from_dir.y * shoulder_len + dir.y * axial_extension;

    let cp2_x = ex + to_dir.x * shoulder_len - dir.x * axial_extension;
    let cp2_y = ey + to_dir.y * shoulder_len - dir.y * axial_extension;

    let perp_x = dir.y;
    let perp_y = -dir.x;
    let s_offset = dist * 0.06 * tension;

    let s_sign = if from_dir.x.abs() > from_dir.y.abs() {
        from_dir.x.signum()
    } else {
        from_dir.y.signum()
    };

    let cp1 = Point::new(
        cp1_x + perp_x * s_offset * s_sign,
        cp1_y + perp_y * s_offset * s_sign,
    );
    let cp2 = Point::new(
        cp2_x - perp_x * s_offset * s_sign,
        cp2_y - perp_y * s_offset * s_sign,
    );

    [cp1, cp2]
}

fn port_direction(port: Port) -> Point {
    match port {
        Port::Top => Point::new(0.0, -1.0),
        Port::Bottom => Point::new(0.0, 1.0),
        Port::Left => Point::new(-1.0, 0.0),
        Port::Right => Point::new(1.0, 0.0),
    }
}

/// 三次贝塞尔曲线上的点
pub fn cubic_bezier_point(
    p0: Point,
    p1: Point,
    p2: Point,
    p3: Point,
    t: f64,
) -> Point {
    let u = 1.0 - t;
    let x = u * u * u * p0.x + 3.0 * u * u * t * p1.x + 3.0 * u * t * t * p2.x + t * t * t * p3.x;
    let y = u * u * u * p0.y + 3.0 * u * u * t * p1.y + 3.0 * u * t * t * p2.y + t * t * t * p3.y;
    Point::new(x, y)
}

/// 沿折线路径在参数 t∈[0,1] 处取点（按弧长均匀分布）。
///
/// t=0 返回起点，t=1 返回终点，t=0.5 返回弧长中点。
/// 适用于 Straight（2 点）和 Polyline（n 点）路径。
/// 对于 Bezier 路径，应直接使用 [`cubic_bezier_point`]。
pub fn point_at_path_t(path: &[Point], t: f64) -> Point {
    if path.is_empty() {
        return Point::new(0.0, 0.0);
    }
    if path.len() == 1 {
        return path[0];
    }

    let seg_lengths: Vec<f64> = path.windows(2)
        .map(|w| {
            let dx = w[1].x - w[0].x;
            let dy = w[1].y - w[0].y;
            (dx * dx + dy * dy).sqrt()
        })
        .collect();
    let total_len: f64 = seg_lengths.iter().sum();

    if total_len < 1e-9 {
        return path[0];
    }

    let target_dist = total_len * t.clamp(0.0, 1.0);
    let mut accum = 0.0;
    for (i, &seg_len) in seg_lengths.iter().enumerate() {
        if accum + seg_len >= target_dist {
            let local_t = if seg_len > 1e-9 {
                (target_dist - accum) / seg_len
            } else {
                0.0
            };
            return Point::new(
                path[i].x + (path[i + 1].x - path[i].x) * local_t,
                path[i].y + (path[i + 1].y - path[i].y) * local_t,
            );
        }
        accum += seg_len;
    }
    *path.last().unwrap()
}

/// 在折线路径上找到离给定点最近的点。
///
/// 逐段计算点到线段的最近点，返回全局最近点及其距离。
/// 空路径返回 ((0,0), +∞)；单点路径返回 (该点, 到该点距离)。
pub fn closest_point_on_path(path: &[Point], p: Point) -> (Point, f64) {
    if path.is_empty() {
        return (Point::new(0.0, 0.0), f64::INFINITY);
    }
    if path.len() == 1 {
        let dx = p.x - path[0].x;
        let dy = p.y - path[0].y;
        return (path[0], (dx * dx + dy * dy).sqrt());
    }

    let mut best_pt = path[0];
    let mut best_dist_sq = f64::INFINITY;
    for w in path.windows(2) {
        let (cp, dist_sq) = closest_point_on_segment(w[0], w[1], p);
        if dist_sq < best_dist_sq {
            best_dist_sq = dist_sq;
            best_pt = cp;
        }
    }
    (best_pt, best_dist_sq.sqrt())
}

/// 点到线段的最近点（含距离平方）。
fn closest_point_on_segment(
    a: Point,
    b: Point,
    p: Point,
) -> (Point, f64) {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len_sq = dx * dx + dy * dy;
    if len_sq < 1e-18 {
        let ddx = p.x - a.x;
        let ddy = p.y - a.y;
        return (a, ddx * ddx + ddy * ddy);
    }
    let t = ((p.x - a.x) * dx + (p.y - a.y) * dy) / len_sq;
    let t = t.clamp(0.0, 1.0);
    let cx = a.x + dx * t;
    let cy = a.y + dy * t;
    let ddx = p.x - cx;
    let ddy = p.y - cy;
    (Point::new(cx, cy), ddx * ddx + ddy * ddy)
}

/// 从 Relation 的 `attributes.style` 解析 label_position 为 t 值。
///
/// 支持格式：
/// - "middle" / 未设置 → 0.5
/// - "start" → 0.15
/// - "end" → 0.85
/// - "t:0.25" → 0.25（自定义 t 值）
pub fn parse_label_t(rel: &crate::ast::Relation) -> f64 {
    match rel.attributes.style.get("label_position") {
        Some(crate::ast::AttributeValue::String(s)) => parse_label_position_str(s),
        _ => 0.5,
    }
}

/// 解析 label_position 字符串为 t 值
fn parse_label_position_str(s: &str) -> f64 {
    let s = s.trim();
    if s.eq_ignore_ascii_case("middle") {
        0.5
    } else if s.eq_ignore_ascii_case("start") {
        0.15
    } else if s.eq_ignore_ascii_case("end") {
        0.85
    } else if let Some(rest) = s.strip_prefix("t:") {
        rest.trim().parse::<f64>().unwrap_or(0.5).clamp(0.0, 1.0)
    } else {
        0.5
    }
}

/// head_label 默认 t 值（靠近箭头头部 / `to` 端）
pub const HEAD_LABEL_T: f64 = 0.85;

/// tail_label 默认 t 值（靠近箭头尾部 / `from` 端）
pub const TAIL_LABEL_T: f64 = 0.15;

/// 数值微分步长（用于计算路径切线方向）
const TANGENT_DT: f64 = 0.01;

/// 计算路径在参数 t 处的切线角度（度）。
///
/// 通过对 `point_at_t` 做数值微分求方向向量，再转为角度。
/// 返回值范围 (-180, 180]，0 = 水平向右。
fn tangent_angle_at_t<F>(t: f64, point_at_t: &F) -> f64
where
    F: Fn(f64) -> Point,
{
    let t_lo = (t - TANGENT_DT).max(0.0);
    let t_hi = (t + TANGENT_DT).min(1.0);
    let p_lo = point_at_t(t_lo);
    let p_hi = point_at_t(t_hi);
    let dx = p_hi.x - p_lo.x;
    let dy = p_hi.y - p_lo.y;
    if dx.abs() < 1e-9 && dy.abs() < 1e-9 {
        return 0.0;
    }
    dy.atan2(dx).to_degrees()
}

/// 根据 Relation 的 label / head_label / tail_label 构建标签列表。
///
/// `middle_t` 是中段标签在路径上的参数位置（通常由 `parse_label_t(rel)` 得到，
/// circular 等特殊布局可传入自定义值）。head_label 固定在 `HEAD_LABEL_T`，
/// tail_label 固定在 `TAIL_LABEL_T`。
/// `point_at_t` 是一个闭包，接收 t∈[0,1] 返回路径上该参数处的点。
/// `offset` 是标签相对路径点的偏移（法线方向等）。
///
/// 每个标签的 `rotation` 字段会被预计算为路径在该 t 处的切线角度，
/// 供渲染层在 `LabelRotation::AlongEdge` 模式下使用。
pub fn build_edge_labels<F>(
    rel: &crate::ast::Relation,
    middle_t: f64,
    offset: Point,
    point_at_t: F,
) -> Vec<EdgeLabelLayout>
where
    F: Fn(f64) -> Point,
{
    let mut labels = Vec::new();

    if let Some(text) = &rel.label {
        let base = point_at_t(middle_t);
        let center = Point::new(base.x + offset.x, base.y + offset.y);
        let angle = tangent_angle_at_t(middle_t, &point_at_t);
        let mut lbl = EdgeLabelLayout::new(text, center);
        lbl.rotation = angle;
        labels.push(lbl);
    }

    if let Some(text) = &rel.tail_label {
        let base = point_at_t(TAIL_LABEL_T);
        let center = Point::new(base.x + offset.x, base.y + offset.y);
        let angle = tangent_angle_at_t(TAIL_LABEL_T, &point_at_t);
        let mut lbl = EdgeLabelLayout::new(text, center);
        lbl.rotation = angle;
        labels.push(lbl);
    }

    if let Some(text) = &rel.head_label {
        let base = point_at_t(HEAD_LABEL_T);
        let center = Point::new(base.x + offset.x, base.y + offset.y);
        let angle = tangent_angle_at_t(HEAD_LABEL_T, &point_at_t);
        let mut lbl = EdgeLabelLayout::new(text, center);
        lbl.rotation = angle;
        labels.push(lbl);
    }

    labels
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{ArrowType, AttributeMap, AttributeValue, Identifier, Relation, Span, TextValue};

    #[test]
    fn bezier_controls_vertical_ports() {
        let controls = compute_bezier_controls(
            100.0, 0.0, 100.0, 100.0, Port::Bottom, Port::Top, DEFAULT_BEZIER_TENSION,
        );
        assert!(controls[0].y > 0.0);
        assert!(controls[1].y < 100.0);
    }

    #[test]
    fn bezier_controls_corner_ports() {
        let controls = compute_bezier_controls(
            0.0, 0.0, 100.0, 100.0, Port::Bottom, Port::Left, DEFAULT_BEZIER_TENSION,
        );
        assert!(controls[0].x.abs() < 200.0);
        assert!(controls[0].y.abs() < 200.0);
        assert!(controls[1].x.abs() < 200.0);
        assert!(controls[1].y.abs() < 200.0);
    }

    #[test]
    fn point_at_path_t_empty_path_returns_origin() {
        let p = point_at_path_t(&[], 0.5);
        assert_eq!(p, Point::new(0.0, 0.0));
    }

    #[test]
    fn point_at_path_t_single_point_returns_that_point() {
        let path = [Point::new(10.0, 20.0)];
        assert_eq!(point_at_path_t(&path, 0.0), Point::new(10.0, 20.0));
        assert_eq!(point_at_path_t(&path, 1.0), Point::new(10.0, 20.0));
        assert_eq!(point_at_path_t(&path, 0.5), Point::new(10.0, 20.0));
    }

    #[test]
    fn point_at_path_t_two_points_endpoints() {
        let path = [Point::new(0.0, 0.0), Point::new(10.0, 0.0)];
        assert_eq!(point_at_path_t(&path, 0.0), Point::new(0.0, 0.0));
        assert_eq!(point_at_path_t(&path, 1.0), Point::new(10.0, 0.0));
        assert_eq!(point_at_path_t(&path, 0.5), Point::new(5.0, 0.0));
    }

    #[test]
    fn point_at_path_t_clamps_out_of_range_t() {
        let path = [Point::new(0.0, 0.0), Point::new(10.0, 0.0)];
        assert_eq!(point_at_path_t(&path, -1.0), Point::new(0.0, 0.0));
        assert_eq!(point_at_path_t(&path, 2.0), Point::new(10.0, 0.0));
    }

    #[test]
    fn point_at_path_t_polyline_by_arc_length() {
        let path = [Point::new(0.0, 0.0), Point::new(10.0, 0.0), Point::new(10.0, 10.0)];
        let mid = point_at_path_t(&path, 0.5);
        assert!((mid.x - 10.0).abs() < 1e-9);
        assert!((mid.y - 0.0).abs() < 1e-9);
        let q1 = point_at_path_t(&path, 0.25);
        assert!((q1.x - 5.0).abs() < 1e-9);
        assert!((q1.y - 0.0).abs() < 1e-9);
        let q3 = point_at_path_t(&path, 0.75);
        assert!((q3.x - 10.0).abs() < 1e-9);
        assert!((q3.y - 5.0).abs() < 1e-9);
    }

    #[test]
    fn point_at_path_t_unequal_segments() {
        let path = [Point::new(0.0, 0.0), Point::new(10.0, 0.0), Point::new(10.0, 30.0)];
        let p25 = point_at_path_t(&path, 0.25);
        assert!((p25.x - 10.0).abs() < 1e-9);
        assert!((p25.y - 0.0).abs() < 1e-9);
        let p50 = point_at_path_t(&path, 0.5);
        assert!((p50.x - 10.0).abs() < 1e-9);
        assert!((p50.y - 10.0).abs() < 1e-9);
    }

    #[test]
    fn point_at_path_t_zero_length_path_returns_first() {
        let path = [Point::new(5.0, 5.0), Point::new(5.0, 5.0)];
        assert_eq!(point_at_path_t(&path, 0.5), Point::new(5.0, 5.0));
    }

    fn relation_with_label_position(value: Option<AttributeValue>) -> Relation {
        let mut attrs = AttributeMap::default();
        if let Some(v) = value {
            attrs.style.insert("label_position".to_string(), v);
        }
        Relation {
            from: Identifier::new_unchecked("a"),
            to: Identifier::new_unchecked("b"),
            arrow: ArrowType::Active,
            label: Some("lbl".to_string()),
            head_label: None,
            tail_label: None,
            attributes: attrs,
            span: Span::dummy(),
        }
    }

    #[test]
    fn parse_label_t_default_is_middle() {
        let rel = relation_with_label_position(None);
        assert!((parse_label_t(&rel) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn parse_label_t_middle_string() {
        let rel = relation_with_label_position(Some(AttributeValue::String(TextValue::quoted("middle"))));
        assert!((parse_label_t(&rel) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn parse_label_t_start_string() {
        let rel = relation_with_label_position(Some(AttributeValue::String(TextValue::quoted("start"))));
        assert!((parse_label_t(&rel) - 0.15).abs() < 1e-9);
    }

    #[test]
    fn parse_label_t_end_atom() {
        let rel = relation_with_label_position(Some(AttributeValue::String(TextValue::unquoted("end"))));
        assert!((parse_label_t(&rel) - 0.85).abs() < 1e-9);
    }

    #[test]
    fn parse_label_t_custom_t_value() {
        let rel = relation_with_label_position(Some(AttributeValue::String(TextValue::quoted("t:0.25"))));
        assert!((parse_label_t(&rel) - 0.25).abs() < 1e-9);
    }

    #[test]
    fn parse_label_t_custom_t_clamps_out_of_range() {
        let rel = relation_with_label_position(Some(AttributeValue::String(TextValue::quoted("t:1.5"))));
        assert!((parse_label_t(&rel) - 1.0).abs() < 1e-9);
        let rel2 = relation_with_label_position(Some(AttributeValue::String(TextValue::quoted("t:-0.3"))));
        assert!((parse_label_t(&rel2) - 0.0).abs() < 1e-9);
    }

    #[test]
    fn parse_label_t_invalid_t_falls_back_to_middle() {
        let rel = relation_with_label_position(Some(AttributeValue::String(TextValue::quoted("t:abc"))));
        assert!((parse_label_t(&rel) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn parse_label_t_unknown_keyword_falls_back_to_middle() {
        let rel = relation_with_label_position(Some(AttributeValue::String(TextValue::quoted("weird"))));
        assert!((parse_label_t(&rel) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn parse_label_t_case_insensitive() {
        let rel = relation_with_label_position(Some(AttributeValue::String(TextValue::unquoted("START"))));
        assert!((parse_label_t(&rel) - 0.15).abs() < 1e-9);
    }

    #[test]
    fn arrow_type_tag_distinct_per_variant() {
        assert_eq!(arrow_type_tag(&ArrowType::Active), "active");
        assert_eq!(arrow_type_tag(&ArrowType::Passive), "passive");
        assert_eq!(arrow_type_tag(&ArrowType::Bidirectional), "bidi");
        assert_ne!(arrow_type_tag(&ArrowType::Active), arrow_type_tag(&ArrowType::Passive));
        assert_ne!(arrow_type_tag(&ArrowType::Active), arrow_type_tag(&ArrowType::Bidirectional));
    }

    fn relation_with_style(style: &[(&str, AttributeValue)]) -> Relation {
        let mut attrs = AttributeMap::default();
        for (k, v) in style {
            attrs.style.insert(k.to_string(), v.clone());
        }
        Relation {
            from: Identifier::new_unchecked("a"),
            to: Identifier::new_unchecked("b"),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: attrs,
            span: Span::dummy(),
        }
    }

    #[test]
    fn line_style_signature_default_is_solid() {
        let rel = relation_with_style(&[]);
        assert_eq!(edge_line_style_signature(&rel), "solid");
    }

    #[test]
    fn line_style_signature_dashed_boolean() {
        let rel = relation_with_style(&[("dashed", AttributeValue::Boolean(true))]);
        assert_eq!(edge_line_style_signature(&rel), "dashed");
    }

    #[test]
    fn line_style_signature_dashed_false_is_solid() {
        let rel = relation_with_style(&[("dashed", AttributeValue::Boolean(false))]);
        assert_eq!(edge_line_style_signature(&rel), "solid");
    }

    #[test]
    fn line_style_signature_stroke_dasharray_string() {
        let rel = relation_with_style(&[("stroke_dasharray", AttributeValue::String(TextValue::quoted("8,4")))]);
        assert_eq!(edge_line_style_signature(&rel), "dash:8,4");
    }

    #[test]
    fn line_style_signature_stroke_dasharray_atom() {
        let rel = relation_with_style(&[("stroke_dasharray", AttributeValue::String(TextValue::unquoted("2,4")))]);
        assert_eq!(edge_line_style_signature(&rel), "dash:2,4");
    }

    #[test]
    fn line_style_signature_empty_dasharray_falls_back_to_dashed_flag() {
        let rel = relation_with_style(&[
            ("stroke_dasharray", AttributeValue::String(TextValue::quoted(""))),
            ("dashed", AttributeValue::Boolean(true)),
        ]);
        assert_eq!(edge_line_style_signature(&rel), "dashed");
    }

    #[test]
    fn line_style_signature_dasharray_takes_priority_over_dashed() {
        let rel = relation_with_style(&[
            ("stroke_dasharray", AttributeValue::String(TextValue::quoted("8,4"))),
            ("dashed", AttributeValue::Boolean(true)),
        ]);
        assert_eq!(edge_line_style_signature(&rel), "dash:8,4");
    }

    #[test]
    fn line_style_signature_different_patterns_differ() {
        let a = relation_with_style(&[("stroke_dasharray", AttributeValue::String(TextValue::quoted("8,4")))]);
        let b = relation_with_style(&[("stroke_dasharray", AttributeValue::String(TextValue::quoted("2,4")))]);
        assert_ne!(edge_line_style_signature(&a), edge_line_style_signature(&b));
    }

    #[test]
    fn closest_point_on_path_empty_returns_infinity() {
        let (pt, dist) = closest_point_on_path(&[], Point::new(5.0, 5.0));
        assert_eq!(pt, Point::new(0.0, 0.0));
        assert!(dist.is_infinite());
    }

    #[test]
    fn closest_point_on_path_single_point() {
        let path = [Point::new(3.0, 4.0)];
        let (pt, dist) = closest_point_on_path(&path, Point::new(0.0, 0.0));
        assert_eq!(pt, Point::new(3.0, 4.0));
        assert!((dist - 5.0).abs() < 1e-9);
    }

    #[test]
    fn closest_point_on_path_point_on_segment() {
        let path = [Point::new(0.0, 0.0), Point::new(10.0, 0.0)];
        let (pt, dist) = closest_point_on_path(&path, Point::new(5.0, 0.0));
        assert_eq!(pt, Point::new(5.0, 0.0));
        assert!(dist.abs() < 1e-9);
    }

    #[test]
    fn closest_point_on_path_perpendicular() {
        let path = [Point::new(0.0, 0.0), Point::new(10.0, 0.0)];
        let (pt, dist) = closest_point_on_path(&path, Point::new(5.0, 3.0));
        assert!((pt.x - 5.0).abs() < 1e-9);
        assert!((pt.y - 0.0).abs() < 1e-9);
        assert!((dist - 3.0).abs() < 1e-9);
    }

    #[test]
    fn closest_point_on_path_clamps_to_endpoint() {
        let path = [Point::new(0.0, 0.0), Point::new(10.0, 0.0)];
        let (pt, dist) = closest_point_on_path(&path, Point::new(-5.0, 5.0));
        assert_eq!(pt, Point::new(0.0, 0.0));
        assert!((dist - (50.0_f64).sqrt()).abs() < 1e-9);
    }

    #[test]
    fn closest_point_on_path_polyline_corner() {
        let path = [Point::new(0.0, 0.0), Point::new(10.0, 0.0), Point::new(10.0, 10.0)];
        let (pt, dist) = closest_point_on_path(&path, Point::new(5.0, 5.0));
        assert!((dist - 5.0).abs() < 1e-9);
        let on_seg1 = pt.y.abs() < 1e-9 && pt.x >= 0.0 && pt.x <= 10.0;
        let on_seg2 = (pt.x - 10.0).abs() < 1e-9 && pt.y >= 0.0 && pt.y <= 10.0;
        assert!(on_seg1 || on_seg2, "pt={:?}", pt);
    }

    #[test]
    fn closest_point_on_path_polyline_nearest_segment() {
        let path = [Point::new(0.0, 0.0), Point::new(10.0, 0.0), Point::new(10.0, 10.0)];
        let (pt, dist) = closest_point_on_path(&path, Point::new(12.0, 5.0));
        assert!((pt.x - 10.0).abs() < 1e-9);
        assert!((pt.y - 5.0).abs() < 1e-9);
        assert!((dist - 2.0).abs() < 1e-9);
    }

    fn relation_with_labels(
        label: Option<&str>,
        head: Option<&str>,
        tail: Option<&str>,
    ) -> Relation {
        Relation {
            from: Identifier::new_unchecked("a"),
            to: Identifier::new_unchecked("b"),
            arrow: ArrowType::Active,
            label: label.map(|s| s.to_string()),
            head_label: head.map(|s| s.to_string()),
            tail_label: tail.map(|s| s.to_string()),
            attributes: AttributeMap::default(),
            span: Span::dummy(),
        }
    }

    #[test]
    fn build_edge_labels_no_labels_returns_empty() {
        let rel = relation_with_labels(None, None, None);
        let labels = build_edge_labels(&rel, 0.5, Point::new(0.0, 0.0), |t| {
            point_at_path_t(&[Point::new(0.0, 0.0), Point::new(100.0, 0.0)], t)
        });
        assert!(labels.is_empty());
    }

    #[test]
    fn build_edge_labels_only_middle() {
        let rel = relation_with_labels(Some("mid"), None, None);
        let labels = build_edge_labels(&rel, 0.5, Point::new(0.0, 5.0), |t| {
            point_at_path_t(&[Point::new(0.0, 0.0), Point::new(100.0, 0.0)], t)
        });
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].text, "mid");
        assert!((labels[0].center.x - 50.0).abs() < 1e-9);
        assert!((labels[0].center.y - 5.0).abs() < 1e-9);
    }

    #[test]
    fn build_edge_labels_all_three() {
        let rel = relation_with_labels(Some("mid"), Some("head"), Some("tail"));
        let labels = build_edge_labels(&rel, 0.5, Point::new(0.0, 0.0), |t| {
            point_at_path_t(&[Point::new(0.0, 0.0), Point::new(100.0, 0.0)], t)
        });
        assert_eq!(labels.len(), 3);
        assert_eq!(labels[0].text, "mid");
        assert_eq!(labels[1].text, "tail");
        assert_eq!(labels[2].text, "head");

        assert!((labels[0].center.x - 50.0).abs() < 1e-9);
        assert!((labels[1].center.x - 15.0).abs() < 1e-9);
        assert!((labels[2].center.x - 85.0).abs() < 1e-9);
    }

    #[test]
    fn build_edge_labels_head_and_tail_only() {
        let rel = relation_with_labels(None, Some("H"), Some("T"));
        let labels = build_edge_labels(&rel, 0.5, Point::new(0.0, 0.0), |t| {
            point_at_path_t(&[Point::new(0.0, 0.0), Point::new(100.0, 0.0)], t)
        });
        assert_eq!(labels.len(), 2);
        assert_eq!(labels[0].text, "T");
        assert_eq!(labels[1].text, "H");
    }

    #[test]
    fn build_edge_labels_custom_middle_t() {
        let rel = relation_with_labels(Some("mid"), None, None);
        let labels = build_edge_labels(&rel, 0.25, Point::new(0.0, 0.0), |t| {
            point_at_path_t(&[Point::new(0.0, 0.0), Point::new(100.0, 0.0)], t)
        });
        assert_eq!(labels.len(), 1);
        assert!((labels[0].center.x - 25.0).abs() < 1e-9);
    }

    #[test]
    fn build_edge_labels_offset_applied_to_all() {
        let rel = relation_with_labels(Some("mid"), Some("head"), Some("tail"));
        let labels = build_edge_labels(&rel, 0.5, Point::new(10.0, -5.0), |t| {
            point_at_path_t(&[Point::new(0.0, 0.0), Point::new(100.0, 0.0)], t)
        });
        assert_eq!(labels.len(), 3);
        assert!((labels[0].center.x - 60.0).abs() < 1e-9);
        assert!((labels[0].center.y - (-5.0)).abs() < 1e-9);
        assert!((labels[1].center.x - 25.0).abs() < 1e-9);
        assert!((labels[2].center.x - 95.0).abs() < 1e-9);
    }

    #[test]
    fn build_edge_labels_with_bezier_closure() {
        let rel = relation_with_labels(Some("mid"), Some("head"), Some("tail"));
        let labels = build_edge_labels(&rel, 0.5, Point::new(0.0, 0.0), |t| {
            Point::new(
                (1.0 - t) * 0.0 + t * 100.0,
                (1.0 - t) * 0.0 + t * 100.0,
            )
        });
        assert_eq!(labels.len(), 3);
        assert!((labels[0].center.x - 50.0).abs() < 1e-9);
        assert!((labels[0].center.y - 50.0).abs() < 1e-9);
        assert!((labels[1].center.x - 15.0).abs() < 1e-9);
        assert!((labels[1].center.y - 15.0).abs() < 1e-9);
        assert!((labels[2].center.x - 85.0).abs() < 1e-9);
        assert!((labels[2].center.y - 85.0).abs() < 1e-9);
    }

    #[test]
    fn build_edge_labels_rotation_horizontal_edge() {
        let rel = relation_with_labels(Some("mid"), Some("head"), Some("tail"));
        let labels = build_edge_labels(&rel, 0.5, Point::new(0.0, 0.0), |t| {
            point_at_path_t(&[Point::new(0.0, 0.0), Point::new(100.0, 0.0)], t)
        });
        assert_eq!(labels.len(), 3);
        for lbl in &labels {
            assert!(
                lbl.rotation.abs() < 1e-6,
                "horizontal edge rotation should be ~0°, got {}",
                lbl.rotation
            );
        }
    }

    #[test]
    fn build_edge_labels_rotation_vertical_edge() {
        let rel = relation_with_labels(Some("mid"), None, None);
        let labels = build_edge_labels(&rel, 0.5, Point::new(0.0, 0.0), |t| {
            point_at_path_t(&[Point::new(0.0, 0.0), Point::new(0.0, 100.0)], t)
        });
        assert_eq!(labels.len(), 1);
        assert!(
            (labels[0].rotation - 90.0).abs() < 1e-6,
            "vertical edge rotation should be ~90°, got {}",
            labels[0].rotation
        );
    }

    #[test]
    fn build_edge_labels_rotation_diagonal_edge() {
        let rel = relation_with_labels(Some("mid"), None, None);
        let labels = build_edge_labels(&rel, 0.5, Point::new(0.0, 0.0), |t| {
            point_at_path_t(&[Point::new(0.0, 0.0), Point::new(100.0, 100.0)], t)
        });
        assert_eq!(labels.len(), 1);
        assert!(
            (labels[0].rotation - 45.0).abs() < 1e-6,
            "diagonal edge rotation should be ~45°, got {}",
            labels[0].rotation
        );
    }

    #[test]
    fn build_edge_labels_rotation_default_zero() {
        let lbl = EdgeLabelLayout::new("test", Point::new(50.0, 50.0));
        assert!(lbl.rotation.abs() < 1e-9);
    }
}

#[cfg(test)]
mod organic_tests {
    use super::*;

    #[test]
    fn organic_horizontal_rightward_curves_smoothly() {
        let [cp1, cp2] = compute_bezier_controls_organic(
            0.0, 0.0, 100.0, 0.0,
            Port::Right, Port::Left,
            0.55, DEFAULT_SHOULDER_RATIO,
        );
        assert!(cp1.x > 0.0, "cp1.x should be right of start");
        assert!(cp2.x < 100.0, "cp2.x should be left of end");
        assert!(cp1.x < 100.0, "cp1.x should be between start and end");
        assert!(cp2.x > 0.0, "cp2.x should be between start and end");
    }

    #[test]
    fn organic_diagonal_produces_smooth_arc() {
        let [cp1, cp2] = compute_bezier_controls_organic(
            0.0, 0.0, 100.0, 100.0,
            Port::Right, Port::Left,
            0.55, DEFAULT_SHOULDER_RATIO,
        );
        let dist = 141.42;
        assert!(cp1.x >= 0.0);
        assert!(cp1.y >= -dist * 0.5);
        assert!(cp2.x <= 100.0 + dist * 0.5);
        assert!(cp2.y <= 100.0 + dist * 0.5);
        let cp1_ext = ((cp1.x - 0.0).powi(2) + (cp1.y - 0.0).powi(2)).sqrt();
        assert!(cp1_ext > 0.0, "cp1 should have some extension");
    }

    #[test]
    fn organic_zero_tension_makes_straightish() {
        let [cp1, _cp2] = compute_bezier_controls_organic(
            0.0, 0.0, 100.0, 0.0,
            Port::Right, Port::Left,
            0.01, DEFAULT_SHOULDER_RATIO,
        );
        let cp1_ext = ((cp1.x - 0.0).powi(2) + (cp1.y - 0.0).powi(2)).sqrt();
        assert!(cp1_ext < 30.0, "low tension should produce small extension");
    }

    #[test]
    fn organic_high_tension_increases_extension() {
        let [cp1_lo, _] = compute_bezier_controls_organic(
            0.0, 0.0, 100.0, 0.0,
            Port::Right, Port::Left,
            0.3, DEFAULT_SHOULDER_RATIO,
        );
        let [cp1_hi, _] = compute_bezier_controls_organic(
            0.0, 0.0, 100.0, 0.0,
            Port::Right, Port::Left,
            0.9, DEFAULT_SHOULDER_RATIO,
        );
        assert!(cp1_hi.x > cp1_lo.x, "higher tension should extend control point further horizontally");
    }

    #[test]
    fn organic_short_edge_still_has_min_extension() {
        let [cp1, _] = compute_bezier_controls_organic(
            0.0, 0.0, 5.0, 0.0,
            Port::Right, Port::Left,
            0.1, DEFAULT_SHOULDER_RATIO,
        );
        let cp1_ext = ((cp1.x - 0.0).powi(2) + (cp1.y - 0.0).powi(2)).sqrt();
        assert!(cp1_ext >= 5.0, "short edge should still have min control extension");
    }

    #[test]
    fn organic_s_shape_controls_offset_opposite() {
        let [cp1, cp2] = compute_bezier_controls_organic(
            0.0, 0.0, 100.0, 0.0,
            Port::Right, Port::Left,
            0.8, DEFAULT_SHOULDER_RATIO,
        );
        let dy1 = cp1.y - 0.0;
        let dy2 = cp2.y - 0.0;
        assert!(
            dy1 * dy2 <= 0.0 || (dy1 - dy2).abs() > 0.1,
            "organic curve should have S-shape characteristic, dy1={}, dy2={}", dy1, dy2
        );
    }

    #[test]
    fn organic_shoulder_ratio_affects_extension() {
        let [cp1_small, _] = compute_bezier_controls_organic(
            0.0, 0.0, 100.0, 0.0,
            Port::Right, Port::Left,
            0.55, 0.1,
        );
        let [cp1_large, _] = compute_bezier_controls_organic(
            0.0, 0.0, 100.0, 0.0,
            Port::Right, Port::Left,
            0.55, 0.6,
        );
        assert!(
            cp1_large.x > cp1_small.x,
            "larger shoulder_ratio should extend cp1 further right"
        );
    }
}
