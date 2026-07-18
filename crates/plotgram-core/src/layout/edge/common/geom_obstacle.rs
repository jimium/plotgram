//! 边路由共享的穿障几何 primitive（A1）。
//!
//! 统一 router 侧「线段是否穿过节点内部」的判定，收敛此前散落在
//! `scoring::segment_intersects_node` / `lane_assignment::segment_hits_node`
//! 的重复实现（两者内层完全等价）。
//!
//! 语义契约：
//! - `pad`：节点障碍物膨胀间距，由调用点显式传入（端点节点常传 0.0，第三方节点传
//!   `NODE_OBSTACLE_PAD`）。不同 pad 是**有意的**，不得在此处偷偷统一。
//! - 穿内部容差固定为 `geometry::EPS`（= 0.1），与既有 scoring / lane 一致。
//!
//! 注意：lint 门禁真值走 `refine::segment_intersects_node`（pad=0.5、eps=0.5，
//! 语义不同）；分组穿越判定 lint 用「段中点落域」而 router 用「线段穿内部」，
//! 属真算法分歧，均**不**在本 primitive 收敛（见重构方案 TD-2 / B1）。

use crate::layout::geometry::{Point, Rect, EPS};
use crate::layout::NodeLayout;

/// router 避障语义：节点按 `pad` 膨胀后，判定线段 `a→b` 是否穿其严格内部。
///
/// 等价于原 `Rect::from(nl).expanded(pad).segment_crosses_interior(a, b, EPS)`。
#[inline]
pub fn segment_pierces_node(a: Point, b: Point, nl: &NodeLayout, pad: f64) -> bool {
    Rect::from(nl).expanded(pad).segment_crosses_interior(a, b, EPS)
}
