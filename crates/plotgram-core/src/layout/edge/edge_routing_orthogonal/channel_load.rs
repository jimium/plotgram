//! Phase 3: 通道负载感知 Reroute 增强。
//!
//! 在 X-1 reroute 的 scorer 中加入通道负载惩罚，使重路由优先选择低负载通道，
//! 从源头减少拥堵。
//!
//! 设计：
//! - `ChannelLoadMap`：key = (轴, 量化层坐标)，value = 段数
//! - 在 reroute 每轮开始时从当前所有边路径构建
//! - `channel_load_penalty`：负载 > 阈值时按多余边数惩罚
//! - 通过 `RoutingContext.channel_load` 传入 scorer
//!
//! 确定性（AGENTS.md §2）：使用 HashMap 但 key 为 (Axis, i64)，
//! 查询时量化 layer 后查表，不依赖迭代顺序。

use crate::layout::geometry::{Axis, Point};
use crate::layout::EdgeLayout;
use std::collections::HashMap;

/// 通道负载阈值——负载超过此值才开始惩罚
const CHANNEL_LOAD_THRESHOLD: usize = 3;

/// 每条多余边的惩罚值（介于 BEND_PENALTY=16 和 EDGE_OVERLAP_PENALTY=1200 之间）
const CHANNEL_LOAD_PENALTY: f64 = 200.0;

/// 通道负载图：key = (轴, 量化层坐标)，value = 该通道上的段数。
///
/// 量化步长 = `GRID_SNAP_STEP`（8px），将相近的 layer 坐标归并到同一通道，
/// 避免浮点误差导致漏检。
#[derive(Debug, Clone, Default)]
pub struct ChannelLoadMap {
    loads: HashMap<(Axis, i64), usize>,
    step: f64,
}

impl ChannelLoadMap {
    /// 从所有边的路径构建通道负载图。
    ///
    /// 遍历每条边的每段，按段方向（H/V）和层坐标（H段=y, V段=x）量化后计数。
    pub fn build(edges: &[EdgeLayout], step: f64) -> Self {
        let mut loads: HashMap<(Axis, i64), usize> = HashMap::new();
        if step <= 0.0 {
            return Self { loads, step: 8.0 };
        }
        for ei in 0..edges.len() {
            if edges[ei].path_is_empty() {
                continue;
            }
            let points = edges[ei].path_points();
            for w in points.windows(2) {
                let dx = (w[1].x - w[0].x).abs();
                let dy = (w[1].y - w[0].y).abs();
                // V 段：dx≈0, dy>0, layer = x
                if dx < crate::layout::geometry::EPS && dy > crate::layout::geometry::EPS {
                    let key = (Axis::Vertical, (w[0].x / step).round() as i64);
                    *loads.entry(key).or_insert(0) += 1;
                } else if dy < crate::layout::geometry::EPS && dx > crate::layout::geometry::EPS {
                    // H 段：dy≈0, dx>0, layer = y
                    let key = (Axis::Horizontal, (w[0].y / step).round() as i64);
                    *loads.entry(key).or_insert(0) += 1;
                }
            }
        }
        Self { loads, step }
    }

    /// 查询指定通道的负载数。量化 layer 后查表，未命中返回 0。
    pub fn load(&self, axis: Axis, layer: f64) -> usize {
        let key = (axis, (layer / self.step).round() as i64);
        *self.loads.get(&key).unwrap_or(&0)
    }

    /// 返回图中最大负载值（用于统计/调试）
    pub fn max_load(&self) -> usize {
        *self.loads.values().max().unwrap_or(&0)
    }
}

/// 计算路径的通道负载惩罚。
///
/// 对路径中每段查询其所在通道的负载，负载 > CHANNEL_LOAD_THRESHOLD 时
/// 按多余边数 × CHANNEL_LOAD_PENALTY 累加惩罚。
pub fn channel_load_penalty(path: &[Point], load_map: &ChannelLoadMap) -> f64 {
    let mut penalty = 0.0;
    for w in path.windows(2) {
        let dx = (w[1].x - w[0].x).abs();
        let dy = (w[1].y - w[0].y).abs();
        let (axis, layer) = if dx < crate::layout::geometry::EPS && dy > crate::layout::geometry::EPS {
            (Axis::Vertical, w[0].x)
        } else if dy < crate::layout::geometry::EPS && dx > crate::layout::geometry::EPS {
            (Axis::Horizontal, w[0].y)
        } else {
            continue;
        };
        let load = load_map.load(axis, layer);
        if load > CHANNEL_LOAD_THRESHOLD {
            penalty += (load - CHANNEL_LOAD_THRESHOLD) as f64 * CHANNEL_LOAD_PENALTY;
        }
    }
    penalty
}

// ═══════════════════════════════════════════════════════════
//  单元测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::PathGeometry;

    fn mk_edge(points: &[Point]) -> EdgeLayout {
        let mut e = EdgeLayout {
            geometry: PathGeometry::Polyline { points: Vec::new() },
            labels: vec![],
            from_port: crate::layout::Port::Bottom,
            to_port: crate::layout::Port::Top,
        };
        e.set_polyline_points(points.to_vec());
        e
    }

    fn pt(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn test_build_load_map_counts_segments() {
        // 2 条 V 段在 x=200，1 条 H 段在 y=100
        let p0 = vec![pt(200.0, 100.0), pt(200.0, 300.0)]; // V 段
        let p1 = vec![pt(200.0, 100.0), pt(200.0, 300.0)]; // V 段
        let p2 = vec![pt(100.0, 100.0), pt(300.0, 100.0)]; // H 段
        let edges = vec![mk_edge(&p0), mk_edge(&p1), mk_edge(&p2)];

        let map = ChannelLoadMap::build(&edges, 8.0);
        // x=200 量化为 200/8=25
        assert_eq!(map.load(Axis::Vertical, 200.0), 2, "x=200 应有 2 条 V 段");
        // y=100 量化为 100/8=12.5 → round=13
        assert_eq!(map.load(Axis::Horizontal, 100.0), 1, "y=100 应有 1 条 H 段");
    }

    #[test]
    fn test_load_returns_zero_for_empty_channel() {
        let edges: Vec<EdgeLayout> = Vec::new();
        let map = ChannelLoadMap::build(&edges, 8.0);
        assert_eq!(map.load(Axis::Vertical, 200.0), 0, "空图应返回 0");
        assert_eq!(map.load(Axis::Horizontal, 100.0), 0, "空图应返回 0");
    }

    #[test]
    fn test_load_quantizes_layer() {
        // x=199.9 和 x=200.1 应量化到同一通道
        let p0 = vec![pt(199.9, 100.0), pt(199.9, 300.0)];
        let p1 = vec![pt(200.1, 100.0), pt(200.1, 300.0)];
        let edges = vec![mk_edge(&p0), mk_edge(&p1)];

        let map = ChannelLoadMap::build(&edges, 8.0);
        // 199.9/8=24.9875 → round=25, 200.1/8=25.0125 → round=25
        let load = map.load(Axis::Vertical, 200.0);
        assert_eq!(load, 2, "相近 layer 应量化到同一通道，load={}", load);
    }

    #[test]
    fn test_channel_load_penalty_zero_for_low_load() {
        // 3 条 V 段在 x=200 → load=3 = 阈值，不惩罚
        let p0 = vec![pt(200.0, 100.0), pt(200.0, 300.0)];
        let p1 = vec![pt(200.0, 100.0), pt(200.0, 300.0)];
        let p2 = vec![pt(200.0, 100.0), pt(200.0, 300.0)];
        let edges = vec![mk_edge(&p0), mk_edge(&p1), mk_edge(&p2)];
        let map = ChannelLoadMap::build(&edges, 8.0);

        let test_path = vec![pt(200.0, 100.0), pt(200.0, 300.0)];
        let penalty = channel_load_penalty(&test_path, &map);
        assert_eq!(penalty, 0.0, "load=3=阈值不应有惩罚");
    }

    #[test]
    fn test_channel_load_penalty_scales_with_load() {
        // 5 条 V 段在 x=200 → load=5, 多余 2 条 → 2×200=400
        let p = vec![pt(200.0, 100.0), pt(200.0, 300.0)];
        let edges = vec![mk_edge(&p); 5];
        let map = ChannelLoadMap::build(&edges, 8.0);

        let test_path = vec![pt(200.0, 100.0), pt(200.0, 300.0)];
        let penalty = channel_load_penalty(&test_path, &map);
        assert!(
            (penalty - 400.0).abs() < 0.01,
            "load=5 应惩罚 2×200=400，实际 {}",
            penalty
        );
    }

    #[test]
    fn test_scorer_prefers_low_load_channel() {
        // 构造两条候选路径：
        // 路径 A 经过 load=5 通道 → 高惩罚
        // 路径 B 经过 load=0 通道 → 无惩罚
        // 验证 channel_load_penalty 对 A 的惩罚 > B
        let p = vec![pt(200.0, 100.0), pt(200.0, 300.0)];
        let edges = vec![mk_edge(&p); 5];
        let map = ChannelLoadMap::build(&edges, 8.0);

        let congested_path = vec![pt(200.0, 100.0), pt(200.0, 300.0)];
        let free_path = vec![pt(400.0, 100.0), pt(400.0, 300.0)];

        let penalty_a = channel_load_penalty(&congested_path, &map);
        let penalty_b = channel_load_penalty(&free_path, &map);

        assert!(
            penalty_a > penalty_b,
            "拥堵通道惩罚 ({}) 应大于空闲通道 ({})",
            penalty_a,
            penalty_b
        );
        assert_eq!(penalty_b, 0.0, "空闲通道不应有惩罚");
        assert!(penalty_a > 0.0, "拥堵通道应有惩罚");
    }
}
