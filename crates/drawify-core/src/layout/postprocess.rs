//! 布局后处理：统一画布尺寸计算与内容居中
//!
//! 在所有布局算法和边路由完成后，统一执行：
//! 1. 计算全局包围框（节点 + 分组 + 边路径 + 边标签 + 引线）
//! 2. 平移所有内容使视觉包围框起点落在 (padding, padding)
//! 3. 设定 total_width / total_height 使四边留白对称
//!
//! 此步骤取代各算法各自的 finalize_bounds 逻辑，保证所有布局算法行为一致。

use crate::layout::LayoutResult;

/// 边路径采样步数（Bezier 曲线离散化为折线时的段数）
const EDGE_SAMPLE_STEPS: usize = 20;

/// 统一后处理入口：计算全局包围框 → 平移 → 设定画布尺寸。
///
/// 应在 `compute_layout_with_plan` 的最后调用（边路由 + refine + grid_snap 之后）。
pub fn finalize_canvas_bounds(result: &mut LayoutResult, padding: f64) {
    // 空图保护
    if result.nodes.is_empty() && result.edges.is_empty() {
        result.total_width = padding * 2.0;
        result.total_height = padding * 2.0;
        return;
    }

    let Some((min_x, min_y, max_x, max_y)) = compute_global_bbox(result) else {
        result.total_width = padding * 2.0;
        result.total_height = padding * 2.0;
        return;
    };

    let content_width = (max_x - min_x).max(0.0);
    let content_height = (max_y - min_y).max(0.0);
    let canvas_width = content_width + padding * 2.0;
    let canvas_height = content_height + padding * 2.0;

    // 平移量：使内容包围框左上角 (min_x, min_y) 移动到 (padding, padding)
    let dx = padding - min_x;
    let dy = padding - min_y;

    // 仅在需要时平移（避免浮点微扰）
    if dx.abs() > 1e-9 || dy.abs() > 1e-9 {
        translate_all(result, dx, dy);
    }

    result.total_width = canvas_width;
    result.total_height = canvas_height;
}

/// 计算全局包围框，涵盖节点、分组、边路径采样点、边标签包围框与引线终点。
///
/// 返回 `(min_x, min_y, max_x, max_y)`。若没有任何视觉元素则返回 `None`。
fn compute_global_bbox(result: &LayoutResult) -> Option<(f64, f64, f64, f64)> {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    let mut extend = |x: f64, y: f64| {
        if x < min_x { min_x = x; }
        if y < min_y { min_y = y; }
        if x > max_x { max_x = x; }
        if y > max_y { max_y = y; }
    };

    let mut has_any = false;

    // 节点
    for nl in result.nodes.values() {
        extend(nl.x, nl.y);
        extend(nl.x + nl.width, nl.y + nl.height);
        has_any = true;
    }

    // 分组
    for gl in result.groups.values() {
        extend(gl.x, gl.y);
        extend(gl.x + gl.width, gl.y + gl.height);
        has_any = true;
    }

    // 边路径 + 标签
    for edge in &result.edges {
        // 路径采样点
        for pt in edge.geometry.sample(EDGE_SAMPLE_STEPS) {
            extend(pt.x, pt.y);
            has_any = true;
        }

        // 标签包围框 + 引线
        for label in &edge.labels {
            let (l, t, r, b) = label.bbox();
            extend(l, t);
            extend(r, b);
            has_any = true;

            if let Some(pt) = label.leader_to {
                extend(pt.x, pt.y);
            }
        }
    }

    if has_any {
        Some((min_x, min_y, max_x, max_y))
    } else {
        None
    }
}

/// 平移所有布局元素 (dx, dy)。
fn translate_all(result: &mut LayoutResult, dx: f64, dy: f64) {
    // 节点
    for nl in result.nodes.values_mut() {
        nl.x += dx;
        nl.y += dy;
    }

    // 分组
    for gl in result.groups.values_mut() {
        gl.x += dx;
        gl.y += dy;
    }

    // 边（路径 + 标签 + 引线）
    for edge in &mut result.edges {
        edge.translate(dx, dy);
    }

    // Circular hints: 圆环中心
    if let Some(circular) = result.hints.circular.as_mut() {
        for circle in &mut circular.circles {
            circle.center.0 += dx;
            circle.center.1 += dy;
        }
    }

    // Sequence hints: 生命线缺口 y 坐标
    if let Some(seq) = result.hints.sequence.as_mut() {
        for gaps in seq.lifeline_gaps.values_mut() {
            for y in gaps.iter_mut() {
                *y += dy;
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════
//  单元测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{
        EdgeLabelLayout, EdgeLayout, LayoutHints, NodeLayout, PathGeometry, Port,
    };
    use crate::layout::geometry::Point;
    use std::collections::HashMap;

    fn empty_result() -> LayoutResult {
        LayoutResult {
            nodes: HashMap::new(),
            groups: HashMap::new(),
            edges: vec![],
            total_width: 0.0,
            total_height: 0.0,
            hints: LayoutHints::default(),
        }
    }

    #[test]
    fn empty_diagram_gets_padding_only() {
        let mut result = empty_result();
        finalize_canvas_bounds(&mut result, 70.0);
        assert!((result.total_width - 140.0).abs() < 1e-9);
        assert!((result.total_height - 140.0).abs() < 1e-9);
    }

    #[test]
    fn single_node_centered_with_padding() {
        let mut result = empty_result();
        result.nodes.insert(
            "a".to_string(),
            NodeLayout {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 50.0,
                ..Default::default()
            },
        );
        finalize_canvas_bounds(&mut result, 70.0);
        let nl = &result.nodes["a"];
        // 节点应被平移到 (70, 70)
        assert!((nl.x - 70.0).abs() < 1e-9);
        assert!((nl.y - 70.0).abs() < 1e-9);
        // 画布尺寸 = 100 + 70*2 = 240, 50 + 70*2 = 190
        assert!((result.total_width - 240.0).abs() < 1e-9);
        assert!((result.total_height - 190.0).abs() < 1e-9);
    }

    #[test]
    fn edge_label_included_in_bounds() {
        let mut result = empty_result();
        result.nodes.insert(
            "a".to_string(),
            NodeLayout {
                x: 100.0,
                y: 100.0,
                width: 80.0,
                height: 40.0,
                ..Default::default()
            },
        );
        // 标签中心在 (50, 50)，size (60, 20) → bbox: (20, 40, 80, 60)
        let edge = EdgeLayout {
            geometry: PathGeometry::Straight {
                start: Point::new(140.0, 100.0),
                end: Point::new(140.0, 200.0),
            },
            labels: vec![EdgeLabelLayout::with_size(
                "lbl",
                Point::new(50.0, 50.0),
                (60.0, 20.0),
            )],
            from_port: Port::Top,
            to_port: Port::Bottom,
        };
        result.edges.push(edge);

        finalize_canvas_bounds(&mut result, 70.0);

        // 全局包围框应包含标签的 left=20, top=40
        // 平移后标签 left = 20 + dx，应 >= 70（即 padding）
        let lbl = &result.edges[0].labels[0];
        let (l, t, _, _) = lbl.bbox();
        assert!(l >= 70.0 - 1e-9, "label left {} should be >= padding 70", l);
        assert!(t >= 70.0 - 1e-9, "label top {} should be >= padding 70", t);
    }

    #[test]
    fn already_centered_no_shift() {
        let mut result = empty_result();
        result.nodes.insert(
            "a".to_string(),
            NodeLayout {
                x: 70.0,
                y: 70.0,
                width: 100.0,
                height: 50.0,
                ..Default::default()
            },
        );
        finalize_canvas_bounds(&mut result, 70.0);
        let nl = &result.nodes["a"];
        // 已在正确位置，不应移动
        assert!((nl.x - 70.0).abs() < 1e-9);
        assert!((nl.y - 70.0).abs() < 1e-9);
        assert!((result.total_width - 240.0).abs() < 1e-9);
        assert!((result.total_height - 190.0).abs() < 1e-9);
    }

    #[test]
    fn negative_coordinates_shifted_to_padding() {
        let mut result = empty_result();
        result.nodes.insert(
            "a".to_string(),
            NodeLayout {
                x: -30.0,
                y: -20.0,
                width: 60.0,
                height: 40.0,
                ..Default::default()
            },
        );
        finalize_canvas_bounds(&mut result, 70.0);
        let nl = &result.nodes["a"];
        // -30 + dx = 70 → dx = 100; -20 + dy = 70 → dy = 90
        assert!((nl.x - 70.0).abs() < 1e-9);
        assert!((nl.y - 70.0).abs() < 1e-9);
        // content: 60x40, canvas: 60+140=200, 40+140=180
        assert!((result.total_width - 200.0).abs() < 1e-9);
        assert!((result.total_height - 180.0).abs() < 1e-9);
    }
}
