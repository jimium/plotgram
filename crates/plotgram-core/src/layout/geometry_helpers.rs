use crate::ast::{Entity, AttributeValue};
use super::types::NodeLayout;

// ─── 样式感知的节点尺寸 ────────────────────────────────────

/// 从 entity 的 `attributes.style` 读取尺寸覆盖，回退到默认值。
///
/// prepare 已将 theme cascade 物化到 `attributes.style`，其中可能包含
/// `width`、`height` 等布局相关属性。布局算法应优先使用这些值，
/// 仅在 style 中未指定时才使用算法自身的默认尺寸。
///
/// # 参数
/// - `entity`: AST 实体
/// - `default_width`: 算法默认宽度
/// - `default_height`: 算法默认高度
///
/// # 返回
/// `(width, height)` — style 覆盖值（如有），否则为默认值
pub fn styled_node_size(entity: &Entity, default_width: f64, default_height: f64) -> (f64, f64) {
    let width = entity
        .attributes
        .style
        .get("width")
        .and_then(|v| match v {
            AttributeValue::Number(n) if *n > 0.0 => Some(*n),
            _ => None,
        })
        .unwrap_or(default_width);

    let height = entity
        .attributes
        .style
        .get("height")
        .and_then(|v| match v {
            AttributeValue::Number(n) if *n > 0.0 => Some(*n),
            _ => None,
        })
        .unwrap_or(default_height);

    let (width, height) = (width, height);
    crate::icons::layout::apply_icon_to_node_size(entity, width, height, &crate::icons::ResolveOptions::default())
}

// ─── 几何工具函数 ────────────────────────────────────────

/// 计算从节点中心到目标点的射线与节点边界的交点。
///
/// 近似正方形的节点（思维导图 root 等圆形）按椭圆/圆求交，
/// 避免矩形包围盒在斜角方向把连接点推到圆外。
pub fn edge_point(nl: &NodeLayout, tx: f64, ty: f64) -> (f64, f64) {
    let aspect = nl.width / nl.height.max(1e-6);
    if (aspect - 1.0).abs() < 0.08 {
        return ellipse_edge_point(nl, tx, ty);
    }
    rect_edge_point(nl, tx, ty)
}

/// 矩形包围盒边界交点
fn rect_edge_point(nl: &NodeLayout, tx: f64, ty: f64) -> (f64, f64) {
    let cx = nl.x + nl.width / 2.0;
    let cy = nl.y + nl.height / 2.0;
    let dx = tx - cx;
    let dy = ty - cy;

    if dx.abs() < 0.01 && dy.abs() < 0.01 {
        return (cx, cy);
    }

    let hw = nl.width / 2.0;
    let hh = nl.height / 2.0;

    let scale_x = if dx.abs() > 0.01 {
        hw / dx.abs()
    } else {
        f64::MAX
    };
    let scale_y = if dy.abs() > 0.01 {
        hh / dy.abs()
    } else {
        f64::MAX
    };
    let scale = scale_x.min(scale_y);

    (cx + dx * scale, cy + dy * scale)
}

/// 椭圆（含圆）边界交点：射线从中心指向目标，落在椭圆周上。
pub fn ellipse_edge_point(nl: &NodeLayout, tx: f64, ty: f64) -> (f64, f64) {
    let cx = nl.x + nl.width / 2.0;
    let cy = nl.y + nl.height / 2.0;
    let dx = tx - cx;
    let dy = ty - cy;

    if dx.abs() < 0.01 && dy.abs() < 0.01 {
        return (cx + nl.width / 2.0, cy);
    }

    let a = nl.width / 2.0;
    let b = nl.height / 2.0;
    // 椭圆参数方程：点 = (a cos θ, b sin θ)，θ 由方向决定
    // 规范化方向后求与椭圆的交：t = 1 / sqrt((dx/a)² + (dy/b)²)
    let t = 1.0 / ((dx / a).powi(2) + (dy / b).powi(2)).sqrt();
    (cx + dx * t, cy + dy * t)
}
