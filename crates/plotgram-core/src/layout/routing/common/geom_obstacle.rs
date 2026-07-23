//! 边路由 / lint 共享的穿障几何 primitive。
//!
//! ## 节点穿障（A1）
//! 统一 router 侧「线段是否穿过节点内部」的判定，收敛此前散落在
//! `scoring::segment_intersects_node` / `lane_assignment::segment_hits_node`
//! 的重复实现（两者内层完全等价）。
//!
//! 语义契约：
//! - `pad`：节点障碍物膨胀间距，由调用点显式传入（端点节点常传 0.0，第三方节点传
//!   `NODE_OBSTACLE_PAD`）。不同 pad 是**有意的**，不得在此处偷偷统一。
//! - 穿内部容差固定为 `geometry::EPS`（= 0.1），与既有 scoring / lane 一致。
//!
//! 注意：lint 门禁节点真值仍走 `refine::segment_intersects_node`（pad=0.5、eps=0.5），
//! 与 router pad 口径不同（有意偏置，见 A1 一致率笔记）。
//!
//! ## 分组穿内部（分组穿越语义统一）
//! lint 与 router **必须**调用 [`segment_pierces_group_interior`]：线段是否穿越
//! group 矩形的严格内部（四边内缩 [`GROUP_INTERIOR_EPS`]）。废止 lint 旧的
//! 「段中点落域」启发式——那与 router 的线段穿越是真算法分歧。

use crate::layout::geometry::{Point, Rect, EPS};
use crate::layout::{GroupLayout, NodeLayout};

/// 分组严格内部收缩量（px）。
///
/// 与 router 历史 `path_avoids_group_interiors` 所用 `geometry::EPS`（0.1）对齐，
/// 保证统一到本 primitive 后 **router 硬过滤语义不变**。lint 从「段中点落域」
/// 升级为同一线段穿越判定（可能多报真实穿组，属门禁收紧）。
pub const GROUP_INTERIOR_EPS: f64 = EPS;

/// router 避障语义：节点按 `pad` 膨胀后，判定线段 `a→b` 是否穿其严格内部。
///
/// 等价于原 `Rect::from(nl).expanded(pad).segment_crosses_interior(a, b, EPS)`。
#[inline]
pub fn segment_pierces_node(a: Point, b: Point, nl: &NodeLayout, pad: f64) -> bool {
    Rect::from(nl).expanded(pad).segment_crosses_interior(a, b, EPS)
}

/// 分组穿内部：线段 `a→b` 是否穿越 group 矩形的严格内部。
///
/// lint `edge_crosses_group_interior` 与 router `path_avoids_group_interiors`
/// 的共同真值。宽高非正的 group 视为无内部，返回 `false`。
#[inline]
pub fn segment_pierces_group_interior(a: Point, b: Point, gl: &GroupLayout) -> bool {
    if gl.width <= 0.0 || gl.height <= 0.0 {
        return false;
    }
    Rect::from(gl).segment_crosses_interior(a, b, GROUP_INTERIOR_EPS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::GroupLayout;

    fn group(x: f64, y: f64, w: f64, h: f64) -> GroupLayout {
        GroupLayout {
            x,
            y,
            width: w,
            height: h,
            ..Default::default()
        }
    }

    #[test]
    fn group_interior_segment_cross_vs_midpoint_outside() {
        // 水平段切过组框中部：中点在组内，线段亦穿内部。
        let gl = group(0.0, 0.0, 100.0, 100.0);
        let a = Point::new(-10.0, 50.0);
        let b = Point::new(110.0, 50.0);
        assert!(segment_pierces_group_interior(a, b, &gl));
    }

    #[test]
    fn group_interior_skimming_corner_caught_by_segment() {
        // 斜切角：中点可能在框外，但线段穿严格内部——统一后应判穿。
        let gl = group(0.0, 0.0, 100.0, 100.0);
        let a = Point::new(-5.0, 10.0);
        let b = Point::new(10.0, -5.0);
        assert!(segment_pierces_group_interior(a, b, &gl));
    }

    #[test]
    fn group_border_shell_not_interior() {
        // 完全在框外平行的边不穿内部。
        let gl = group(0.0, 0.0, 100.0, 100.0);
        let a = Point::new(-20.0, -20.0);
        let b = Point::new(120.0, -20.0);
        assert!(!segment_pierces_group_interior(a, b, &gl));
    }
}
