//! Routing context and endpoint pair for orthogonal edge routing.
//!
//! These types bundle the per-edge and per-diagram state that `select_best_path`
//! and the candidate scorer need, so that path building functions can take a
//! single context reference instead of a long parameter list.

use crate::layout::geometry::Point;
use crate::layout::group::GroupRoutingContext;
use crate::layout::NodeLayout;
use std::collections::HashMap;

use crate::layout::demand::CorridorModel;
use crate::layout::routing::common::spatial_grid::SpatialGrid;
use super::{OrthoConfig, OrthoRoutingProfile, RoutedSegment};
use super::slot::Endpoint;
use super::visibility_graph::OrthogonalVisibilityGraph;

/// Shared, read-only routing context for a single `route_edges_orthogonal` call.
///
/// Holds references to the diagram-level node/group maps, the already-routed
/// segments (for overlap detection), and the resolved config. All per-edge
/// path-building functions receive `&OrthoRoutingContext` instead of repeating
/// these parameters.
pub struct OrthoRoutingContext<'a> {
    pub nodes: &'a HashMap<String, NodeLayout>,
    pub group_ctx: &'a GroupRoutingContext,
    pub grid: &'a SegmentGrid,
    pub cfg: &'a OrthoConfig,
    pub profile: &'a OrthoRoutingProfile,
    pub obstacles: &'a PreparedObstacles,
    /// P2：预路由廊模型（只读）；None 时不做廊 OVER soft
    pub corridor_model: Option<&'a CorridorModel>,
    /// 该边是否强制拒绝穿无关组内部（`path_avoids_group_interiors` 硬过滤）。
    ///
    /// 由 `should_strict_group_transit` 按边判定：存在 corridor chain 时为 true。
    /// 无走廊时保持 false，允许 nodes-only 软降级，避免大图无通道时路径塌缩。
    pub strict_group_transit: bool,
    /// 空间契约：0 候选升档后加大外框绕行垫（S2）。
    pub corridor_boost: bool,
    /// S4：优先评估外环绕行候选（feedback / 监控枢纽边）。Legacy 轨使用。
    pub prefer_outer_ring: bool,
    /// Phase 2：外围通道偏好（TransitIntent.prefer_periphery → LexA* Q5）。
    pub prefer_periphery: bool,
    /// Phase B: 正交可见性图（OVG），用于 degraded 边的路径搜索。
    pub ovg: Option<&'a OrthogonalVisibilityGraph>,
    /// Phase B3: 全局通道规划分配的通道坐标（cross-axis coord）。
    /// 注入 build_channel_detours 作为优先候选。
    pub planned_channel: Option<f64>,
    /// P3-1: 第一轮粗路由模式——跳过 crossing_penalty（尚无全局拥堵信息）
    pub first_pass: bool,
}

impl<'a> OrthoRoutingContext<'a> {
    pub fn new(
        nodes: &'a HashMap<String, NodeLayout>,
        group_ctx: &'a GroupRoutingContext,
        grid: &'a SegmentGrid,
        cfg: &'a OrthoConfig,
        profile: &'a OrthoRoutingProfile,
        obstacles: &'a PreparedObstacles,
    ) -> Self {
        Self {
            nodes,
            group_ctx,
            grid,
            cfg,
            profile,
            obstacles,
            corridor_model: None,
            // 默认 false，由调用方按边调用 should_strict_group_transit 覆盖
            strict_group_transit: false,
            corridor_boost: false,
            prefer_outer_ring: false,
            prefer_periphery: false,
            ovg: None,
            planned_channel: None,
            first_pass: false,
        }
    }

    /// 设置该边是否强制拒绝穿无关组（见 `should_strict_group_transit`）。
    pub fn with_strict_group_transit(mut self, strict: bool) -> Self {
        self.strict_group_transit = strict;
        self
    }

    pub fn with_corridor_boost(mut self, boost: bool) -> Self {
        self.corridor_boost = boost;
        self
    }

    /// P2：注入预计算廊模型（不改 `new` 签名）。
    pub fn with_corridor_demands(mut self, model: &'a CorridorModel) -> Self {
        self.corridor_model = Some(model);
        self
    }

    pub fn with_prefer_outer_ring(mut self, prefer: bool) -> Self {
        self.prefer_outer_ring = prefer;
        self.prefer_periphery = prefer;
        self
    }

    pub fn with_prefer_periphery(mut self, prefer: bool) -> Self {
        self.prefer_periphery = prefer;
        self
    }

    /// Phase B: 注入正交可见性图（OVG）。
    pub fn with_ovg(mut self, ovg: &'a OrthogonalVisibilityGraph) -> Self {
        self.ovg = Some(ovg);
        self
    }

    /// Phase B3: 注入全局通道规划分配的通道坐标。
    pub fn with_planned_channel(mut self, coord: Option<f64>) -> Self {
        self.planned_channel = coord;
        self
    }

    /// P3-1: 设置第一轮粗路由模式。
    pub fn with_first_pass(mut self, v: bool) -> Self {
        self.first_pass = v;
        self
    }
}

/// 路由前预排序的障碍物索引，避免每次调用重复 `nodes.keys().collect() + sort()`。
///
/// 节点/分组集合在路由期间不变，排序一次即可。原先 `obstacle_penalty`、
/// `path_is_clean`、`collect_obstacle_boundaries_on_axis` 等函数每次调用
/// 都重新排序，累计 O(E·C·N log N) 开销。
pub struct PreparedObstacles {
    /// 按 id 排序的节点 ID 列表
    pub sorted_node_ids: Vec<String>,
    /// 按 id 排序的分组 ID 列表
    pub sorted_group_ids: Vec<String>,
}

impl PreparedObstacles {
    pub fn build(
        nodes: &HashMap<String, NodeLayout>,
        group_ctx: &GroupRoutingContext,
    ) -> Self {
        let mut sorted_node_ids: Vec<String> = nodes.keys().cloned().collect();
        sorted_node_ids.sort();
        let mut sorted_group_ids: Vec<String> = group_ctx.groups.keys().cloned().collect();
        sorted_group_ids.sort();
        Self { sorted_node_ids, sorted_group_ids }
    }
}

/// 均匀网格空间索引，用于加速 `edge_overlap_penalty` 中的段-段重叠检测。
///
/// 将画布按固定大小的 cell 划分，每条已路由段插入其 bbox 覆盖的 cells。
/// 查询时只需扫描查询段 bbox 覆盖的 cells，将 O(R) 线性扫描降为 O(k)
/// （k 为命中数，通常 ≤ 10）。
///
/// 所有段均为轴对齐（水平/垂直），cell 大小 64px 是 BBOX_EXPAND(10) 的
/// 合理倍数，保证绝大多数段仅覆盖 1-2 个 cell。
/// Shared spatial grid of routed segments — pub(crate) for Phase 2 group repair from refine.
pub struct SegmentGrid {
    grid: SpatialGrid<usize>,
    segments: Vec<RoutedSegment>,
}

impl Default for SegmentGrid {
    fn default() -> Self {
        Self::new()
    }
}

impl SegmentGrid {
    const CELL_SIZE: f64 = 64.0;

    pub fn new() -> Self {
        Self {
            grid: SpatialGrid::new(Self::CELL_SIZE),
            segments: Vec::new(),
        }
    }

    /// 将一条边的所有段插入网格。
    pub fn insert_path(&mut self, path: &[Point], edge_index: usize) {
        for window in path.windows(2) {
            let seg = RoutedSegment {
                x1: window[0].x,
                y1: window[0].y,
                x2: window[1].x,
                y2: window[1].y,
                edge_index,
            };
            let idx = self.segments.len();
            self.segments.push(seg);
            self.insert_into_cells(idx, &seg);
        }
    }

    /// 批量移除多条边的段，只重建一次网格（P0-C）。
    pub fn remove_by_edges(&mut self, edge_indices: &[usize]) {
        if edge_indices.is_empty() {
            return;
        }
        let remove: std::collections::HashSet<usize> = edge_indices.iter().copied().collect();
        self.segments.retain(|seg| !remove.contains(&seg.edge_index));
        self.rebuild();
    }

    fn rebuild(&mut self) {
        self.grid.clear();
        // 先收集 (idx, bbox cell 范围)，避免在迭代 segments 时可变借用 self
        let entries: Vec<(usize, i32, i32, i32, i32)> = self
            .segments
            .iter()
            .enumerate()
            .map(|(idx, seg)| {
                let xmin = seg.x1.min(seg.x2);
                let xmax = seg.x1.max(seg.x2);
                let ymin = seg.y1.min(seg.y2);
                let ymax = seg.y1.max(seg.y2);
                let (cx0, cx1, cy0, cy1) = self.grid.cell_range(xmin, xmax, ymin, ymax);
                (idx, cx0, cx1, cy0, cy1)
            })
            .collect();
        for (idx, cx0, cx1, cy0, cy1) in entries {
            self.grid.insert_range(cx0, cx1, cy0, cy1, idx);
        }
    }

    fn insert_into_cells(&mut self, idx: usize, seg: &RoutedSegment) {
        let xmin = seg.x1.min(seg.x2);
        let xmax = seg.x1.max(seg.x2);
        let ymin = seg.y1.min(seg.y2);
        let ymax = seg.y1.max(seg.y2);
        let (cx0, cx1, cy0, cy1) = self.grid.cell_range(xmin, xmax, ymin, ymax);
        self.grid.insert_range(cx0, cx1, cy0, cy1, idx);
    }

    /// 查询与给定段 bbox（扩张 `expand`）重叠的所有已路由段（去重）。
    pub fn query_overlapping(&self, seg: &RoutedSegment, expand: f64) -> Vec<&RoutedSegment> {
        let xmin = seg.x1.min(seg.x2) - expand;
        let xmax = seg.x1.max(seg.x2) + expand;
        let ymin = seg.y1.min(seg.y2) - expand;
        let ymax = seg.y1.max(seg.y2) + expand;
        let (cx0, cx1, cy0, cy1) = self.grid.cell_range(xmin, xmax, ymin, ymax);

        // 结果集通常很小（< 20），用 Vec 线性查找比 HashSet 更快
        let mut seen: Vec<usize> = Vec::new();
        let mut result = Vec::new();
        for cx in cx0..=cx1 {
            for cy in cy0..=cy1 {
                if let Some(indices) = self.grid.cells().get(&(cx, cy)) {
                    for &idx in indices {
                        if !seen.contains(&idx) {
                            seen.push(idx);
                            result.push(&self.segments[idx]);
                        }
                    }
                }
            }
        }
        result.sort_by_key(|s| (s.edge_index, s.x1.to_bits(), s.y1.to_bits(), s.x2.to_bits(), s.y2.to_bits()));
        result
    }

    /// 查询 bbox 范围内的所有已路由段（用于 OVG 重叠惩罚）。
    pub fn query_bbox(&self, x_lo: f64, y_lo: f64, x_hi: f64, y_hi: f64) -> Vec<&RoutedSegment> {
        let (cx0, cx1, cy0, cy1) = self.grid.cell_range(x_lo, x_hi, y_lo, y_hi);
        let mut seen: Vec<usize> = Vec::new();
        let mut result = Vec::new();
        for cx in cx0..=cx1 {
            for cy in cy0..=cy1 {
                if let Some(indices) = self.grid.cells().get(&(cx, cy)) {
                    for &idx in indices {
                        if !seen.contains(&idx) {
                            seen.push(idx);
                            result.push(&self.segments[idx]);
                        }
                    }
                }
            }
        }
        result
    }

    /// 返回所有已路由段的只读切片（用于全局统计/调试）。
    #[inline]
    #[allow(dead_code)]
    pub fn all_segments(&self) -> &[RoutedSegment] {
        &self.segments
    }
}

/// A pair of resolved endpoints (from / to) for a single edge.
///
/// Each `Endpoint` carries its anchor coordinates, connection side, and node id,
/// so `select_best_path` can reconstruct everything it needs from `&EndpointPair`.
pub struct EndpointPair {
    pub from: Endpoint,
    pub to: Endpoint,
}

impl EndpointPair {
    #[inline]
    pub fn from_id(&self) -> &str {
        &self.from.node_id
    }

    #[inline]
    pub fn to_id(&self) -> &str {
        &self.to.node_id
    }

    #[inline]
    pub fn from_anchor(&self) -> Point {
        self.from.anchor
    }

    #[inline]
    pub fn to_anchor(&self) -> Point {
        self.to.anchor
    }
}
