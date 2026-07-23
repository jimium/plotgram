//! 通用形状边界锚点吸附（ISS-003 / 根因簇 B）。
//!
//! **设计原则**：不为每种形状写定制锚点逻辑。所有形状统一表达为轮廓多边形
//! （`Vec<Point>`），锚点吸附归结为**最近边界点投影**——通用、确定性、
//! 无图名/形状名特判。
//!
//! - 锚点已在轮廓上（Stadium 直边段、Diamond 顶点等）→ 距离≈0，不移动。
//! - 锚点在 bbox 角（轮廓外，如 Diamond 四角）→ 吸附到最近轮廓边。
//!
//! - Rect / RoundedRect / Subprocess：bbox 即边界，返回 `None`（不吸附，零回归）。
//! - Diamond / Hexagon / Parallelogram / Document / Cloud：复用已有顶点生成器。
//! - Circle / Stadium / Cylinder：以 ≥24 顶点多边形逼近曲线（误差 < 0.5px）。
//!
//! 集成点：`phase_port_slot` 计算 `slot_anchor` 后、`stub_fix` 创建新端点时，
//! 调用 [`snap_anchor_to_boundary`] 将 bbox 锚点吸附到真实轮廓。

use crate::layout::geometry::Point;
use crate::layout::{NodeLayout, Port};
use crate::render::visual::NodeShape;
use std::collections::HashMap;

/// 从 diagram entities + 节点布局构建「非矩形形状」的轮廓多边形映射。
///
/// 仅对需要吸附的形状（Diamond/Hexagon/Circle 等）生成多边形；
/// Rect/RoundedRect/Subprocess 不进入映射（调用方查不到 → 不吸附）。
pub fn build_shape_polygons(
    entities: &[crate::ast::Entity],
    nodes: &HashMap<String, NodeLayout>,
) -> HashMap<String, Vec<Point>> {
    let mut map: HashMap<String, Vec<Point>> = HashMap::new();
    let debug = std::env::var("PLOTGRAM_SHAPE_DEBUG").map(|v| v == "1").unwrap_or(false);
    for entity in entities {
        let shape = crate::icons::resolve::node_shape_from_entity(entity);
        if debug {
            eprintln!("[shape_dbg] entity={} shape={:?}", entity.id.as_str(), shape);
        }
        if !needs_shape_snap(&shape) {
            continue;
        }
        let id = entity.id.as_str();
        let Some(nl) = nodes.get(id) else {
            continue;
        };
        if let Some(poly) = shape_outline_polygon(&shape, nl) {
            if debug {
                eprintln!("[shape_dbg]   -> polygon {} pts for {}", poly.len(), id);
            }
            map.insert(id.to_string(), poly);
        }
    }
    map
}

/// 判断形状是否需要锚点吸附（bbox ≠ 真实轮廓的形状）。
fn needs_shape_snap(shape: &NodeShape) -> bool {
    !matches!(
        shape,
        NodeShape::Rect | NodeShape::RoundedRect | NodeShape::Subprocess
    )
}

/// 生成形状的轮廓多边形（闭合，首尾不重复）。
///
/// 返回 `None` 表示 bbox 即边界（无需吸附）。
fn shape_outline_polygon(shape: &NodeShape, nl: &NodeLayout) -> Option<Vec<Point>> {
    let (x, y, w, h) = (nl.x, nl.y, nl.width, nl.height);
    if w < 1.0 || h < 1.0 {
        return None;
    }
    match shape {
        // bbox 即边界，不吸附
        NodeShape::Rect | NodeShape::RoundedRect | NodeShape::Subprocess => None,

        // 已有顶点生成器（graphic_style::common::Point → layout::geometry::Point）
        NodeShape::Diamond => {
            Some(convert_points(&crate::graphic_style::common::diamond_points(x, y, w, h)))
        }
        NodeShape::Hexagon => {
            Some(convert_points(&crate::graphic_style::common::hexagon_points(x, y, w, h)))
        }
        NodeShape::Parallelogram => {
            Some(convert_points(&crate::graphic_style::common::parallelogram_points(x, y, w, h)))
        }
        NodeShape::Document => {
            Some(convert_points(&crate::graphic_style::common::document_points(x, y, w, h)))
        }
        NodeShape::Cloud => {
            Some(convert_points(&crate::graphic_style::common::cloud_points(x, y, w, h)))
        }

        // 曲线形状：多边形逼近
        NodeShape::Circle => Some(ellipse_polygon(x, y, w, h, 32)),
        NodeShape::Stadium => Some(stadium_polygon(x, y, w, h, 24)),
        NodeShape::Cylinder => Some(cylinder_polygon(x, y, w, h, 16)),

        // Person 形状复杂（头+肩+体），暂用 bbox（不吸附）
        NodeShape::Person => None,
    }
}

/// graphic_style::common::Point → layout::geometry::Point 批量转换。
fn convert_points(pts: &[crate::graphic_style::common::Point]) -> Vec<Point> {
    pts.iter().map(|p| Point::new(p.x, p.y)).collect()
}

/// 椭圆多边形逼近（Circle）。
fn ellipse_polygon(x: f64, y: f64, w: f64, h: f64, n: usize) -> Vec<Point> {
    let cx = x + w / 2.0;
    let cy = y + h / 2.0;
    let rx = w / 2.0;
    let ry = h / 2.0;
    (0..n)
        .map(|i| {
            let t = std::f64::consts::TAU * i as f64 / n as f64;
            Point::new(cx + rx * t.cos(), cy + ry * t.sin())
        })
        .collect()
}

/// Stadium（圆角矩形 = 两端半圆 + 中间直段）多边形逼近。
fn stadium_polygon(x: f64, y: f64, w: f64, h: f64, n: usize) -> Vec<Point> {
    // Stadium = 水平方向两端半圆 + 中间矩形
    let r = h / 2.0; // 半圆半径 = 高度一半
    let cx_left = x + r;
    let cx_right = x + w - r;
    let cy = y + h / 2.0;
    let mut pts = Vec::with_capacity(n + 4);
    // 右半圆（从 -90° 到 +90°）
    let half_n = n / 2;
    for i in 0..=half_n {
        let t = -std::f64::consts::FRAC_PI_2 + std::f64::consts::PI * i as f64 / half_n as f64;
        pts.push(Point::new(cx_right + r * t.cos(), cy + r * t.sin()));
    }
    // 左半圆（从 +90° 到 +270°）
    for i in 0..=half_n {
        let t = std::f64::consts::FRAC_PI_2 + std::f64::consts::PI * i as f64 / half_n as f64;
        pts.push(Point::new(cx_left + r * t.cos(), cy + r * t.sin()));
    }
    pts
}

/// Cylinder（数据库）多边形逼近：顶椭圆弧 + 右侧 + 底椭圆弧 + 左侧。
fn cylinder_polygon(x: f64, y: f64, w: f64, h: f64, arc_pts: usize) -> Vec<Point> {
    // 顶椭圆：中心 (cx, y + ry)，半轴 rx, ry
    let rx = w / 2.0;
    let ry = (h * 0.12).max(4.0); // 顶椭圆高度约为总高 12%
    let cx = x + w / 2.0;
    let top_cy = y + ry;
    let bot_cy = y + h - ry;
    let mut pts = Vec::with_capacity(arc_pts * 2 + 4);
    // 顶椭圆上半弧（从 180° 到 0°，即从左到右经过顶部）
    for i in 0..=arc_pts {
        let t = std::f64::consts::PI - std::f64::consts::PI * i as f64 / arc_pts as f64;
        pts.push(Point::new(cx + rx * t.cos(), top_cy - ry * t.sin()));
    }
    // 右侧下行
    pts.push(Point::new(x + w, bot_cy));
    // 底椭圆下半弧（从 0° 到 -180°，即从右到左经过底部）
    for i in 0..=arc_pts {
        let t = -std::f64::consts::PI * i as f64 / arc_pts as f64;
        pts.push(Point::new(cx + rx * t.cos(), bot_cy - ry * t.sin()));
    }
    // 左侧上行（闭合）
    pts.push(Point::new(x, top_cy));
    pts
}

// ─── 核心算法：最近边界点投影 ─────────────────────────────────────

/// 将 bbox 锚点吸附到形状真实轮廓。
///
/// 对轮廓多边形的每条边求最近点，取全局距离最小的投影点。
/// - 锚点已在轮廓上（如 Stadium 直边段、Diamond 顶点）→ 距离≈0，不移动。
/// - 锚点在 bbox 角（轮廓外，如 Diamond 四角）→ 吸附到最近轮廓边。
///
/// 确定性：纯几何计算，不依赖任何迭代顺序。
pub fn snap_anchor_to_boundary(anchor: Point, _side: Port, polygon: &[Point]) -> Point {
    if polygon.len() < 3 {
        return anchor;
    }

    let mut best: Option<(f64, Point)> = None; // (dist², closest_point)

    let n = polygon.len();
    for i in 0..n {
        let a = polygon[i];
        let b = polygon[(i + 1) % n];
        let cp = closest_point_on_segment(anchor, a, b);
        let dx = cp.x - anchor.x;
        let dy = cp.y - anchor.y;
        let dist_sq = dx * dx + dy * dy;
        if best.map_or(true, |(bd, _)| dist_sq < bd) {
            best = Some((dist_sq, cp));
        }
    }

    best.map(|(_, p)| p).unwrap_or(anchor)
}

/// 点 P 到线段 A→B 的最近点。
fn closest_point_on_segment(p: Point, a: Point, b: Point) -> Point {
    let abx = b.x - a.x;
    let aby = b.y - a.y;
    let len_sq = abx * abx + aby * aby;
    if len_sq < 1e-12 {
        return a; // 退化线段
    }
    let t = ((p.x - a.x) * abx + (p.y - a.y) * aby) / len_sq;
    let t = t.clamp(0.0, 1.0);
    Point::new(a.x + t * abx, a.y + t * aby)
}
