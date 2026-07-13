//! 曲线路由器共享的穿障检测逻辑。
//!
//! 抽取自 bezier / circular / organic 三处完全相同的 `curve_intersects_obstacles`
//! 和 `OBSTACLE_CHECK_SAMPLES` 常量。spline 路由器无此函数(走可见性图避障),
//! 但共享 `OBSTACLE_CHECK_SAMPLES` 语义。

use crate::layout::edge::visibility;
use crate::layout::EdgeLayout;

/// 穿障检测的曲线采样点数
pub const OBSTACLE_CHECK_SAMPLES: usize = 16;

/// 检测曲线采样后是否穿过任何非 skip 障碍物。
///
/// 对边路径采样 `OBSTACLE_CHECK_SAMPLES` 个点,逐段检测是否命中障碍物索引中
/// 非端点(skip)的节点。用于 bezier / circular / organic 路由器的穿障判定。
pub fn curve_intersects_obstacles(
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
