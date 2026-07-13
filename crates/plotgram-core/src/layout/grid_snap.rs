//! 节点对齐（NodeAlign）与边像素量化（EdgeSnap）。
//!
//! 本模块职责拆分为两个独立阶段：
//! - **节点对齐**（[`align_nodes`]）：rank 轴同层对齐 + layer 轴重叠消除（保持层重心）。
//!   属于结构修正，**改坐标、影响路由输入**，在路由前执行。由 `align` 属性控制。
//! - **边像素量化**（[`snap_edge_waypoints`]）：正交折线通道轴坐标量化到网格。
//!   属于视觉优化，**不改拓扑**，在路由后执行。由 `snap` 属性控制。
//!
//! rank 轴与 layer 轴相互独立，各有开关；layer 轴不再从 padding 原点重排槽位，
//! 以避免破坏 Sugiyama barycenter 对称分布。

use crate::ast::{AttributeValue, Diagram};
use crate::layout::constants::{
    self, GRID_SNAP_LAYER_TOLERANCE, GRID_SNAP_MAX_DISTANCE, GRID_SNAP_NODE_GAP_ARCH,
    GRID_SNAP_NODE_GAP_SUGIYAMA, GRID_SNAP_STEP,
};
use crate::types::attr_constants;
use crate::layout::geometry::Point;
use crate::layout::group::constants::{GROUP_BORDER_SHELL_PAD, PORT_STUB_CLEARANCE};
use crate::layout::{EdgeLayout, GroupLayout, LayoutResult, NodeLayout};
use std::collections::{HashMap, HashSet};

/// 共线判定容差（严格共线）
const COLLINEAR_EPS: f64 = 0.1;

/// 量化后简化容差（放宽以消除量化产生的微小折点）
const POST_QUANTIZE_SIMPLIFY_EPS: f64 = 2.0;

/// 默认分组边框排斥轮数
const DEFAULT_REPULSE_MAX_ROUNDS: usize = 2;

// ─── 配置结构 ────────────────────────────────────────────

/// Layer 轴（同 rank 层内的垂直于流向轴）对齐模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerAxisAlign {
    /// 不调整 layer 轴，完整保留主布局算法的 barycenter 分布。
    Off,
    /// 仅在同层节点重叠或间距不足时分离，并保持层重心不变。
    OverlapOnly,
    /// 在需要分离时均匀化至 `node_gap`，并保持层重心不变。
    Centroid,
}

/// diagram 顶层 `align` 属性覆盖模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagramAlignOverride {
    /// `align: false` — 完全禁用节点对齐。
    Off,
    /// `align: true` — 使用布局算法声明的默认配置。
    Default,
    /// `align: rank` — 仅 rank 轴对齐。
    RankOnly,
    /// `align: layer` — 仅 layer 轴对齐。
    LayerOnly,
    /// `align: full` — rank + layer 轴均开启。
    Full,
}

/// 节点结构对齐配置（由 [`LayoutStrategy::node_align_config()`] 声明）。
///
/// rank 轴与 layer 轴独立控制；layer 轴只做重叠消除/间距修正，不从 padding 重排槽位。
#[derive(Debug, Clone)]
pub struct NodeAlignConfig {
    pub enabled: bool,
    /// rank 轴（流向轴上的层内对齐）：同层节点中心线对齐到中位数。
    pub rank_axis: bool,
    /// layer 轴（垂直于流向的分布修正）。
    pub layer_axis: LayerAxisAlign,
    pub node_gap: f64,
    pub max_snap_distance: f64,
    pub layer_tolerance: f64,
    pub padding: f64,
}

impl NodeAlignConfig {
    /// 构造一个禁用节点对齐的配置。
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Self::default_sugiyama()
        }
    }

    /// Sugiyama 系算法（sugiyama-v2）的默认配置。
    pub fn default_sugiyama() -> Self {
        Self {
            enabled: true,
            rank_axis: true,
            layer_axis: LayerAxisAlign::OverlapOnly,
            node_gap: GRID_SNAP_NODE_GAP_SUGIYAMA,
            max_snap_distance: GRID_SNAP_MAX_DISTANCE,
            layer_tolerance: GRID_SNAP_LAYER_TOLERANCE,
            padding: constants::DEFAULT_PADDING,
        }
    }

    /// 流程图的默认配置：rank 轴对齐 + layer 轴仅消除重叠。
    pub fn default_flowchart() -> Self {
        Self::default_sugiyama()
    }

    /// ER 图默认：仅 rank 轴对齐，layer 轴保持 Sugiyama 原始分布。
    pub fn default_er() -> Self {
        Self {
            layer_axis: LayerAxisAlign::Off,
            ..Self::default_sugiyama()
        }
    }

    /// 架构图的默认配置（node_gap 不同，layer 轴做间距均匀化）。
    pub fn default_architecture() -> Self {
        Self {
            node_gap: GRID_SNAP_NODE_GAP_ARCH,
            layer_axis: LayerAxisAlign::Centroid,
            ..Self::default_sugiyama()
        }
    }

    pub fn with_layer_axis(mut self, layer_axis: LayerAxisAlign) -> Self {
        self.layer_axis = layer_axis;
        self
    }

    /// 应用 diagram 顶层 `align` 属性覆盖。
    pub fn apply_diagram_override(&mut self, mode: DiagramAlignOverride) {
        match mode {
            DiagramAlignOverride::Off => self.enabled = false,
            DiagramAlignOverride::Default => {}
            DiagramAlignOverride::RankOnly => {
                self.enabled = true;
                self.rank_axis = true;
                self.layer_axis = LayerAxisAlign::Off;
            }
            DiagramAlignOverride::LayerOnly => {
                self.enabled = true;
                self.rank_axis = false;
                self.layer_axis = LayerAxisAlign::OverlapOnly;
            }
            DiagramAlignOverride::Full => {
                self.enabled = true;
                self.rank_axis = true;
                self.layer_axis = LayerAxisAlign::OverlapOnly;
            }
        }
    }
}

/// 边像素量化配置（由 [`EdgeRoutingStrategy::edge_snap_config()`] 声明）。
///
/// 控制正交折线路径的通道轴坐标量化，以及分组边框排斥的几何参数。
/// 属于像素量化阶段，在路由完成后执行，**不改拓扑**。
/// 仅输出折线路径（Polyline）的路由算法应启用此配置。
#[derive(Debug, Clone)]
pub struct EdgeSnapConfig {
    pub enabled: bool,
    pub grid_step: f64,
    pub shell_pad: f64,
    pub stub_clearance: f64,
    pub repulse_max_rounds: usize,
}

impl EdgeSnapConfig {
    /// 构造一个禁用边量化的配置（默认值）。
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Self::default_orthogonal()
        }
    }

    /// 正交路由的默认配置。
    pub fn default_orthogonal() -> Self {
        Self {
            enabled: true,
            grid_step: GRID_SNAP_STEP,
            shell_pad: GROUP_BORDER_SHELL_PAD,
            stub_clearance: PORT_STUB_CLEARANCE,
            repulse_max_rounds: DEFAULT_REPULSE_MAX_ROUNDS,
        }
    }
}

// ─── 工具函数 ────────────────────────────────────────────

/// 读取 diagram 顶层 `snap: true | false`（控制像素量化）；未声明时返回 `None`（默认开启）。
pub fn diagram_snap_attribute(diagram: &Diagram) -> Option<bool> {
    diagram
        .attributes
        .iter()
        .find(|attr| attr.key == "snap")
        .and_then(|attr| match attr.value {
            AttributeValue::Boolean(value) => Some(value),
            _ => None,
        })
}

/// 读取 diagram 顶层 `align` 属性覆盖。
///
/// 支持：`false` / `true` / `"rank"` / `"layer"` / `"full"` / `"off"`。
/// 未声明时返回 `None`（使用布局算法默认配置）。
pub fn diagram_align_override(diagram: &Diagram) -> Option<DiagramAlignOverride> {
    let attr = diagram.attributes.iter().find(|attr| attr.key == "align")?;
    parse_align_override(&attr.value)
}

fn parse_align_override(value: &AttributeValue) -> Option<DiagramAlignOverride> {
    match value {
        AttributeValue::Boolean(false) => Some(DiagramAlignOverride::Off),
        AttributeValue::Boolean(true) => Some(DiagramAlignOverride::Default),
        AttributeValue::String(s) => match s.as_str().to_ascii_lowercase().as_str() {
            attr_constants::align::OFF
            | attr_constants::align::NONE => Some(DiagramAlignOverride::Off),
            attr_constants::align::DEFAULT => Some(DiagramAlignOverride::Default),
            attr_constants::align::RANK => Some(DiagramAlignOverride::RankOnly),
            attr_constants::align::LAYER => Some(DiagramAlignOverride::LayerOnly),
            attr_constants::align::FULL
            | attr_constants::align::ALL
            | attr_constants::align::BOTH => Some(DiagramAlignOverride::Full),
            _ => None,
        },
        _ => None,
    }
}

/// 根据节点数量自适应选择网格步长（P5）。
///
/// - 少于 20 个节点：4px（小图精细）
/// - 20-50 个节点：8px（默认）
/// - 多于 50 个节点：16px（大图粗放，减少密集区视觉碎片）
pub fn adaptive_grid_step(node_count: usize) -> f64 {
    if node_count < 20 {
        4.0
    } else if node_count <= 50 {
        GRID_SNAP_STEP
    } else {
        16.0
    }
}

/// 将 value 量化到最近的网格点（四舍五入）
pub fn snap_to_grid(value: f64, step: f64) -> f64 {
    if step <= f64::EPSILON {
        return value;
    }
    (value / step).round() * step
}

/// 将 value 量化到不大于它的最近网格点（floor）
pub fn snap_floor(value: f64, step: f64) -> f64 {
    if step <= f64::EPSILON {
        return value;
    }
    (value / step).floor() * step
}

/// 将 value 量化到不小于它的最近网格点（ceil）
pub fn snap_ceil(value: f64, step: f64) -> f64 {
    if step <= f64::EPSILON {
        return value;
    }
    (value / step).ceil() * step
}

// ─── 报告 ────────────────────────────────────────────────

/// snap 执行报告（内部使用，供测试断言）
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SnapReport {
    pub snapped_nodes: usize,
    pub skipped_nodes: usize,
    pub total_displacement: f64,
    pub max_displacement: f64,
    pub snapped_groups: usize,
    pub snapped_waypoints: usize,
}

// ─── 节点对齐 ───────────────────────────────────────────

/// 对布局结果执行节点结构对齐（rank 轴中心线 + layer 轴重叠消除）。
///
/// 属于结构对齐阶段，在边路由之前执行。
pub fn align_nodes(
    layout: &mut LayoutResult,
    config: &NodeAlignConfig,
    horizontal: bool,
) -> SnapReport {
    if !config.enabled || layout.nodes.is_empty() {
        return SnapReport::default();
    }

    let mut node_ids: Vec<String> = layout.nodes.keys().cloned().collect();
    node_ids.sort();
    let layers = cluster_by_rank_axis(&layout.nodes, &node_ids, horizontal, config.layer_tolerance);

    let mut report = SnapReport::default();

    if config.rank_axis {
        for layer in &layers {
            snap_rank_axis_centers(layout, layer, horizontal, config, &mut report);
        }
    }

    if config.layer_axis != LayerAxisAlign::Off {
        for layer in &layers {
            apply_layer_axis_align(layout, layer, horizontal, config, &mut report);
        }
    }

    report
}

// ─── 边 snap ─────────────────────────────────────────────

/// 边路由完成后，对正交折线路径做通道轴 snap（保护磁吸点与 stub，不逐点双轴 snap）。
///
/// - 端点 `path[0]` / `path[last]`：磁吸锚点，不修改
/// - stub `path[1]` / `path[last-1]`（len ≥ 4）：端口 clearance，不修改
/// - 仅对通道段量化：竖线段对齐 x、横线段对齐 y；邻接 protected 的段只 snap 可动端的主轴坐标
/// - Phase B：量化后将贴边通道段投影到分组边框壳层外的合法格点
pub fn snap_edge_waypoints(
    edges: &mut [EdgeLayout],
    groups: &HashMap<String, GroupLayout>,
    config: &EdgeSnapConfig,
) -> usize {
    if !config.enabled {
        return 0;
    }

    let mut snapped = 0usize;
    for edge in edges.iter_mut() {
        if edge.is_bezier() || edge.path_len() <= 2 {
            continue;
        }

        let Some(points) = edge.polyline_points_mut() else {
            continue;
        };
        let before_len = points.len().saturating_sub(2);
        if before_len == 0 {
            continue;
        }

        snapped += snap_edge_path_channels(points, config.grid_step);
        crate::layout::group::project_path_off_group_borders_with_stub(
            points,
            groups,
            config.shell_pad,
            config.grid_step,
            config.stub_clearance,
        );
        let simplified = simplify_polyline_path_preserving_stubs(points);
        edge.set_polyline_points(simplified);
    }

    snapped
}

// ─── 画布尺寸 ────────────────────────────────────────────

/// 根据 nodes / groups 更新画布 total 尺寸
pub fn update_canvas_bounds(layout: &mut LayoutResult, padding: f64) {
    let (total_width, total_height) =
        crate::layout::node::common::canvas_bounds::canvas_size(&layout.nodes, &layout.groups, padding);
    layout.total_width = total_width;
    layout.total_height = total_height;
}

// ─── 内部实现 ────────────────────────────────────────────

fn protected_path_indices(len: usize) -> HashSet<usize> {
    let mut protected = HashSet::from([0, len.saturating_sub(1)]);
    if len >= 3 {
        protected.insert(1);
    }
    if len >= 5 {
        protected.insert(len - 2);
    }
    protected
}

fn is_vertical_segment(a: Point, b: Point) -> bool {
    (a.x - b.x).abs() < COLLINEAR_EPS && (a.y - b.y).abs() >= COLLINEAR_EPS
}

fn is_horizontal_segment(a: Point, b: Point) -> bool {
    (a.y - b.y).abs() < COLLINEAR_EPS && (a.x - b.x).abs() >= COLLINEAR_EPS
}

/// P2+P3: 将一对近等值通道坐标量化到同一格点，优先端点锚定 + 位移最小化。
///
/// - P2: 若某一端点已接近格点（< 0.5px），直接使用该格点，避免不必要的位移。
/// - P3: 否则选择让两端点最大位移最小化的格点（minimax），而非简单 round 中点。
fn snap_channel_value(v1: f64, v2: f64, step: f64) -> f64 {
    if step <= f64::EPSILON {
        return (v1 + v2) * 0.5;
    }

    let g1 = snap_to_grid(v1, step);
    let g2 = snap_to_grid(v2, step);

    // P2: 若某一端点已接近格点（< 0.5px），优先使用它
    let on_grid_tol = 0.5;
    if (v1 - g1).abs() < on_grid_tol {
        return g1;
    }
    if (v2 - g2).abs() < on_grid_tol {
        return g2;
    }

    // P3: 两端都偏离格点，选 minimax 格点（最大位移更小者）
    let max_disp_1 = (v1 - g1).abs().max((v2 - g1).abs());
    let max_disp_2 = (v1 - g2).abs().max((v2 - g2).abs());

    if max_disp_1 <= max_disp_2 {
        g1
    } else {
        g2
    }
}

fn snap_edge_path_channels(path: &mut [Point], step: f64) -> usize {
    let n = path.len();
    if n <= 2 {
        return 0;
    }

    let protected = protected_path_indices(n);
    let mut count = 0usize;

    for i in 0..n - 1 {
        if protected.contains(&i) || protected.contains(&(i + 1)) {
            continue;
        }
        let a = path[i];
        let b = path[i + 1];
        if is_vertical_segment(a, b) {
            // P2+P3: 端点锚定 + 位移最小化（替代中点 round）
            let sx = snap_channel_value(a.x, b.x, step);
            if (path[i].x - sx).abs() > f64::EPSILON {
                path[i].x = sx;
                count += 1;
            }
            if (path[i + 1].x - sx).abs() > f64::EPSILON {
                path[i + 1].x = sx;
                count += 1;
            }
        } else if is_horizontal_segment(a, b) {
            let sy = snap_channel_value(a.y, b.y, step);
            if (path[i].y - sy).abs() > f64::EPSILON {
                path[i].y = sy;
                count += 1;
            }
            if (path[i + 1].y - sy).abs() > f64::EPSILON {
                path[i + 1].y = sy;
                count += 1;
            }
        }
    }

    for i in 0..n - 1 {
        let p0 = protected.contains(&i);
        let p1 = protected.contains(&(i + 1));
        if p0 == p1 {
            continue;
        }
        let (prot_idx, mod_idx) = if p0 { (i, i + 1) } else { (i + 1, i) };

        let other_neighbor = if mod_idx > prot_idx {
            mod_idx + 1
        } else {
            mod_idx.saturating_sub(1)
        };
        if other_neighbor < n && protected.contains(&other_neighbor) {
            continue;
        }

        let prot_pt = path[prot_idx];
        let seg = (path[i], path[i + 1]);

        let new_pt = if is_vertical_segment(seg.0, seg.1) {
            Point::new(prot_pt.x, snap_to_grid(path[mod_idx].y, step))
        } else if is_horizontal_segment(seg.0, seg.1) {
            Point::new(snap_to_grid(path[mod_idx].x, step), prot_pt.y)
        } else {
            continue;
        };

        if (path[mod_idx].x - new_pt.x).abs() > f64::EPSILON
            || (path[mod_idx].y - new_pt.y).abs() > f64::EPSILON
        {
            path[mod_idx] = new_pt;
            count += 1;
        }
    }

    for i in 1..n - 1 {
        if protected.contains(&i) {
            continue;
        }
        if protected.contains(&(i - 1)) || protected.contains(&(i + 1)) {
            continue;
        }
        let prev = path[i - 1];
        let curr = path[i];
        let next = path[i + 1];
        let prev_h = is_horizontal_segment(prev, curr);
        let prev_v = is_vertical_segment(prev, curr);
        let next_h = is_horizontal_segment(curr, next);
        let next_v = is_vertical_segment(curr, next);

        let is_corner = (prev_h && next_v) || (prev_v && next_h);
        if !is_corner {
            continue;
        }

        let (x, y) = if prev_h && next_v {
            (next.x, prev.y)
        } else if prev_v && next_h {
            (prev.x, next.y)
        } else {
            continue;
        };

        if (curr.x - x).abs() > f64::EPSILON || (curr.y - y).abs() > f64::EPSILON {
            path[i] = Point::new(x, y);
            count += 1;
        }
    }

    count
}

fn is_collinear_eps(a: Point, b: Point, c: Point, eps: f64) -> bool {
    let cross = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    cross.abs() < eps
}

fn simplify_polyline_path_preserving_stubs(path: &[Point]) -> Vec<Point> {
    // P4: 量化后使用放大容差（POST_QUANTIZE_SIMPLIFY_EPS）消除量化产生的微小折点
    simplify_polyline_path_with_eps(path, POST_QUANTIZE_SIMPLIFY_EPS)
}

fn simplify_polyline_path_with_eps(path: &[Point], eps: f64) -> Vec<Point> {
    if path.len() <= 2 {
        return path.to_vec();
    }

    let mut deduped = path.to_vec();
    deduped.dedup_by(|a, b| (a.x - b.x).abs() < eps && (a.y - b.y).abs() < eps);
    if deduped.len() <= 2 {
        return deduped;
    }
    if deduped.len() <= 4 {
        return deduped;
    }

    let first_stub_index = 1;
    let last_stub_index = deduped.len() - 2;
    let mut simplified = vec![deduped[0]];

    for i in 1..deduped.len() - 1 {
        let prev = *simplified.last().unwrap();
        let curr = deduped[i];
        let next = deduped[i + 1];
        let preserves_stub = i == first_stub_index || i == last_stub_index;
        if preserves_stub || !is_collinear_eps(prev, curr, next, eps) {
            simplified.push(curr);
        }
    }

    simplified.push(*deduped.last().unwrap());
    simplified
}

#[cfg(test)]
fn is_on_grid(value: f64, step: f64) -> bool {
    if step <= f64::EPSILON {
        return true;
    }
    let ratio = value / step;
    (ratio - ratio.round()).abs() < 1e-6
}

fn rank_center(layout: &NodeLayout, horizontal: bool) -> f64 {
    if horizontal {
        layout.x + layout.width / 2.0
    } else {
        layout.y + layout.height / 2.0
    }
}

fn layer_center(layout: &NodeLayout, horizontal: bool) -> f64 {
    if horizontal {
        layout.y + layout.height / 2.0
    } else {
        layout.x + layout.width / 2.0
    }
}

fn layer_size(layout: &NodeLayout, horizontal: bool) -> f64 {
    if horizontal {
        layout.height
    } else {
        layout.width
    }
}

fn set_rank_center(layout: &mut NodeLayout, center: f64, horizontal: bool) {
    if horizontal {
        layout.x = center - layout.width / 2.0;
    } else {
        layout.y = center - layout.height / 2.0;
    }
}

fn set_layer_center(layout: &mut NodeLayout, center: f64, horizontal: bool) {
    if horizontal {
        layout.y = center - layout.height / 2.0;
    } else {
        layout.x = center - layout.width / 2.0;
    }
}

fn record_snap(report: &mut SnapReport, before: f64, after: f64) {
    let displacement = (after - before).abs();
    if displacement < f64::EPSILON {
        report.skipped_nodes += 1;
        return;
    }
    report.snapped_nodes += 1;
    report.total_displacement += displacement;
    if displacement > report.max_displacement {
        report.max_displacement = displacement;
    }
}

fn cluster_by_rank_axis(
    nodes: &HashMap<String, NodeLayout>,
    node_ids: &[String],
    horizontal: bool,
    tolerance: f64,
) -> Vec<Vec<String>> {
    let mut sorted: Vec<(String, f64)> = node_ids
        .iter()
        .filter_map(|id| nodes.get(id).map(|nl| (id.clone(), rank_center(nl, horizontal))))
        .collect();
    if sorted.is_empty() {
        return vec![];
    }
    sorted.sort_by(|a, b| {
        a.1.partial_cmp(&b.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });

    let mut layers: Vec<Vec<String>> = Vec::new();
    let mut layer_centers: Vec<f64> = Vec::new();

    for (id, center) in sorted {
        if let Some(last_center) = layer_centers.last() {
            if (center - last_center).abs() <= tolerance {
                let idx = layers.len() - 1;
                layers[idx].push(id);
                let sum: f64 = layers[idx]
                    .iter()
                    .filter_map(|node_id| nodes.get(node_id))
                    .map(|nl| rank_center(nl, horizontal))
                    .sum();
                layer_centers[idx] = sum / layers[idx].len() as f64;
            } else {
                layers.push(vec![id]);
                layer_centers.push(center);
            }
        } else {
            layers.push(vec![id]);
            layer_centers.push(center);
        }
    }

    layers
}

fn snap_rank_axis_centers(
    layout: &mut LayoutResult,
    layer: &[String],
    horizontal: bool,
    config: &NodeAlignConfig,
    report: &mut SnapReport,
) {
    if layer.is_empty() {
        return;
    }

    let centers: Vec<f64> = layer
        .iter()
        .filter_map(|id| layout.nodes.get(id))
        .map(|nl| rank_center(nl, horizontal))
        .collect();
    if centers.is_empty() {
        return;
    }

    let mut sorted = centers.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let target = sorted[sorted.len() / 2];

    for id in layer {
        let Some(node) = layout.nodes.get_mut(id) else {
            continue;
        };
        let before = rank_center(node, horizontal);
        if (before - target).abs() <= config.max_snap_distance {
            set_rank_center(node, target, horizontal);
            record_snap(report, before, target);
        } else {
            report.skipped_nodes += 1;
        }
    }
}

fn layer_axis_needs_adjustment(
    centers: &[f64],
    sizes: &[f64],
    gap: f64,
    mode: LayerAxisAlign,
) -> bool {
    if centers.len() <= 1 {
        return false;
    }
    for i in 1..centers.len() {
        let min_center = centers[i - 1] + sizes[i - 1] / 2.0 + gap + sizes[i] / 2.0;
        match mode {
            LayerAxisAlign::Off => return false,
            LayerAxisAlign::OverlapOnly => {
                if centers[i] < min_center - f64::EPSILON {
                    return true;
                }
            }
            LayerAxisAlign::Centroid => {
                let actual_gap = centers[i] - centers[i - 1] - sizes[i - 1] / 2.0 - sizes[i] / 2.0;
                if actual_gap < gap - f64::EPSILON {
                    return true;
                }
            }
        }
    }
    false
}

/// layer 轴对齐：仅在重叠或间距不足时分离，保持层重心不变。
fn apply_layer_axis_align(
    layout: &mut LayoutResult,
    layer: &[String],
    horizontal: bool,
    config: &NodeAlignConfig,
    report: &mut SnapReport,
) {
    if layer.len() <= 1 || config.layer_axis == LayerAxisAlign::Off {
        return;
    }

    let mut ordered: Vec<String> = layer.to_vec();
    ordered.sort_by(|a, b| {
        let ca = layout
            .nodes
            .get(a)
            .map(|nl| layer_center(nl, horizontal))
            .unwrap_or(0.0);
        let cb = layout
            .nodes
            .get(b)
            .map(|nl| layer_center(nl, horizontal))
            .unwrap_or(0.0);
        ca.partial_cmp(&cb)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.cmp(b))
    });

    let mut centers: Vec<f64> = ordered
        .iter()
        .filter_map(|id| layout.nodes.get(id))
        .map(|nl| layer_center(nl, horizontal))
        .collect();
    if centers.len() != ordered.len() {
        return;
    }

    let sizes: Vec<f64> = ordered
        .iter()
        .filter_map(|id| layout.nodes.get(id))
        .map(|nl| layer_size(nl, horizontal))
        .collect();

    if !layer_axis_needs_adjustment(&centers, &sizes, config.node_gap, config.layer_axis) {
        return;
    }

    let original_centroid = centers.iter().sum::<f64>() / centers.len() as f64;

    for i in 1..centers.len() {
        let min_center = centers[i - 1] + sizes[i - 1] / 2.0 + config.node_gap + sizes[i] / 2.0;
        if centers[i] < min_center {
            centers[i] = min_center;
        }
    }

    for i in (0..centers.len().saturating_sub(1)).rev() {
        let max_center = centers[i + 1] - sizes[i + 1] / 2.0 - config.node_gap - sizes[i] / 2.0;
        if centers[i] > max_center {
            centers[i] = max_center;
        }
    }

    let new_centroid = centers.iter().sum::<f64>() / centers.len() as f64;
    let shift = original_centroid - new_centroid;
    for center in &mut centers {
        *center += shift;
    }

    let left_extent = centers[0] - sizes[0] / 2.0;
    if left_extent < config.padding {
        let delta = config.padding - left_extent;
        for center in &mut centers {
            *center += delta;
        }
    }

    for (i, id) in ordered.iter().enumerate() {
        let Some(node) = layout.nodes.get_mut(id) else {
            continue;
        };
        let before = layer_center(node, horizontal);
        set_layer_center(node, centers[i], horizontal);
        record_snap(report, before, centers[i]);
    }
}

// ─── Tests ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{EdgeLayout, LayoutHints, PathGeometry};

    fn node(x: f64, y: f64, width: f64, height: f64) -> NodeLayout {
        NodeLayout { x, y, width, height, ..Default::default() }
    }

    fn sample_layout(nodes: HashMap<String, NodeLayout>) -> LayoutResult {
        LayoutResult {
            nodes,
            groups: HashMap::new(),
            edges: Vec::<EdgeLayout>::new(),
            total_width: 400.0,
            total_height: 300.0,
            hints: LayoutHints::default(),
        }
    }

    fn rank_centers(layout: &LayoutResult, ids: &[&str], horizontal: bool) -> Vec<f64> {
        ids.iter()
            .map(|id| rank_center(layout.nodes.get(*id).unwrap(), horizontal))
            .collect()
    }

    #[test]
    fn cluster_groups_nearby_rank_centers() {
        let mut nodes = HashMap::new();
        nodes.insert("a".into(), node(10.0, 100.0, 80.0, 40.0));
        nodes.insert("b".into(), node(120.0, 102.0, 80.0, 40.0));
        nodes.insert("c".into(), node(40.0, 200.0, 80.0, 40.0));

        let layers = cluster_by_rank_axis(&nodes, &nodes.keys().cloned().collect::<Vec<_>>(), false, 4.0);
        assert_eq!(layers.len(), 2);
        assert_eq!(layers[0].len(), 2);
        assert!(layers[0].contains(&"a".to_string()));
        assert!(layers[0].contains(&"b".to_string()));
    }

    #[test]
    fn snap_aligns_same_layer_rank_axis_tb() {
        let mut nodes = HashMap::new();
        nodes.insert("a".into(), node(10.0, 100.0, 80.0, 40.0));
        nodes.insert("b".into(), node(120.0, 103.0, 80.0, 40.0));
        nodes.insert("c".into(), node(40.0, 200.0, 80.0, 40.0));

        let mut layout = sample_layout(nodes);
        let config = NodeAlignConfig {
            max_snap_distance: 24.0,
            node_gap: 48.0,
            ..NodeAlignConfig::default_sugiyama()
        };
        align_nodes(&mut layout, &config, false);

        let top = rank_centers(&layout, &["a", "b"], false);
        assert!((top[0] - top[1]).abs() < f64::EPSILON, "same layer y must match");
    }

    #[test]
    fn snap_aligns_same_layer_rank_axis_lr() {
        let mut nodes = HashMap::new();
        nodes.insert("a".into(), node(100.0, 10.0, 40.0, 80.0));
        nodes.insert("b".into(), node(103.0, 120.0, 40.0, 80.0));

        let mut layout = sample_layout(nodes);
        let config = NodeAlignConfig::default_sugiyama();
        align_nodes(&mut layout, &config, true);

        let xs = rank_centers(&layout, &["a", "b"], true);
        assert!((xs[0] - xs[1]).abs() < f64::EPSILON, "same layer x must match for LR");
    }

    #[test]
    fn overlap_only_separates_overlapping_siblings() {
        let mut nodes = HashMap::new();
        nodes.insert("a".into(), node(41.0, 100.0, 80.0, 40.0));
        nodes.insert("b".into(), node(90.0, 100.0, 80.0, 40.0));

        let mut layout = sample_layout(nodes);
        let config = NodeAlignConfig {
            node_gap: 48.0,
            layer_axis: LayerAxisAlign::OverlapOnly,
            ..NodeAlignConfig::default_sugiyama()
        };
        align_nodes(&mut layout, &config, false);

        let a = &layout.nodes["a"];
        let b = &layout.nodes["b"];
        let gap = b.x - (a.x + a.width);
        assert!(gap >= 48.0 - 0.1, "gap={gap}");
    }

    #[test]
    fn overlap_only_preserves_well_spaced_parallel_branches() {
        let mut nodes = HashMap::new();
        nodes.insert("left".into(), node(70.0, 600.0, 160.0, 50.0));
        nodes.insert("right".into(), node(294.0, 600.0, 160.0, 50.0));

        let mut layout = sample_layout(nodes.clone());
        let config = NodeAlignConfig::default_flowchart();
        align_nodes(&mut layout, &config, false);

        assert!((layout.nodes["left"].x - nodes["left"].x).abs() < 0.1);
        assert!((layout.nodes["right"].x - nodes["right"].x).abs() < 0.1);
    }

    #[test]
    fn layer_axis_off_preserves_layer_positions() {
        let mut nodes = HashMap::new();
        nodes.insert("a".into(), node(41.0, 100.0, 80.0, 40.0));
        nodes.insert("b".into(), node(200.0, 100.0, 80.0, 40.0));

        let mut layout = sample_layout(nodes.clone());
        let config = NodeAlignConfig::default_er();
        align_nodes(&mut layout, &config, false);

        assert!((layout.nodes["b"].x - nodes["b"].x).abs() < 0.1);
    }

    #[test]
    fn layer_align_preserves_centroid_when_separating() {
        let mut nodes = HashMap::new();
        nodes.insert("a".into(), node(200.0, 100.0, 80.0, 40.0));
        nodes.insert("b".into(), node(230.0, 100.0, 80.0, 40.0));

        let before_centroid =
            (nodes["a"].x + nodes["a"].width / 2.0 + nodes["b"].x + nodes["b"].width / 2.0) / 2.0;

        let mut layout = sample_layout(nodes);
        let config = NodeAlignConfig {
            node_gap: 48.0,
            padding: 0.0,
            layer_axis: LayerAxisAlign::OverlapOnly,
            ..NodeAlignConfig::default_sugiyama()
        };
        align_nodes(&mut layout, &config, false);

        let a_cx = layout.nodes["a"].x + layout.nodes["a"].width / 2.0;
        let b_cx = layout.nodes["b"].x + layout.nodes["b"].width / 2.0;
        let after_centroid = (a_cx + b_cx) / 2.0;
        assert!(
            (after_centroid - before_centroid).abs() < 1.0,
            "centroid should be preserved: before={before_centroid}, after={after_centroid}"
        );
    }

    #[test]
    fn snap_is_deterministic() {
        let mut nodes = HashMap::new();
        nodes.insert("a".into(), node(10.0, 100.0, 80.0, 40.0));
        nodes.insert("b".into(), node(130.0, 103.0, 80.0, 40.0));
        nodes.insert("c".into(), node(40.0, 200.0, 80.0, 40.0));

        let mut layout1 = sample_layout(nodes.clone());
        let mut layout2 = sample_layout(nodes);
        let config = NodeAlignConfig::default_sugiyama();
        align_nodes(&mut layout1, &config, false);
        align_nodes(&mut layout2, &config, false);

        for id in ["a", "b", "c"] {
            let n1 = &layout1.nodes[id];
            let n2 = &layout2.nodes[id];
            assert_eq!(n1.x, n2.x);
            assert_eq!(n1.y, n2.y);
        }
    }

    #[test]
    fn nodes_do_not_overlap_after_snap() {
        let mut nodes = HashMap::new();
        nodes.insert("a".into(), node(41.0, 100.0, 80.0, 40.0));
        nodes.insert("b".into(), node(90.0, 100.0, 80.0, 40.0));
        nodes.insert("c".into(), node(140.0, 100.0, 80.0, 40.0));

        let mut layout = sample_layout(nodes);
        let config = NodeAlignConfig {
            max_snap_distance: 48.0,
            ..NodeAlignConfig::default_sugiyama()
        };
        align_nodes(&mut layout, &config, false);

        let ids: Vec<_> = layout.nodes.keys().cloned().collect();
        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                let a = &layout.nodes[&ids[i]];
                let b = &layout.nodes[&ids[j]];
                let overlap_x = a.x < b.x + b.width && b.x < a.x + a.width;
                let overlap_y = a.y < b.y + b.height && b.y < a.y + b.height;
                assert!(!(overlap_x && overlap_y), "nodes {} and {} overlap", ids[i], ids[j]);
            }
        }
    }

    #[test]
    fn skips_nodes_beyond_max_snap_distance() {
        let mut nodes = HashMap::new();
        nodes.insert("a".into(), node(10.0, 100.0, 80.0, 40.0));
        nodes.insert("far".into(), node(300.0, 140.0, 80.0, 40.0));

        let mut layout = sample_layout(nodes);
        let config = NodeAlignConfig {
            max_snap_distance: 10.0,
            ..NodeAlignConfig::default_sugiyama()
        };
        let report = align_nodes(&mut layout, &config, false);

        assert!(report.skipped_nodes >= 1);
        assert!((layout.nodes["far"].y - 140.0).abs() < 0.1);
    }

    #[test]
    fn disabled_config_skips_all_snap() {
        let mut nodes = HashMap::new();
        nodes.insert("a".into(), node(10.0, 100.0, 80.0, 40.0));
        nodes.insert("b".into(), node(120.0, 103.0, 80.0, 40.0));

        let original_positions: Vec<(f64, f64)> = nodes.values().map(|n| (n.x, n.y)).collect();
        let mut layout = sample_layout(nodes);
        let config = NodeAlignConfig::disabled();
        align_nodes(&mut layout, &config, false);

        for (i, (_, n)) in layout.nodes.iter().enumerate() {
            assert!((n.x - original_positions[i].0).abs() < 0.01);
            assert!((n.y - original_positions[i].1).abs() < 0.01);
        }
    }

    #[test]
    fn snap_edge_waypoints_snaps_middle_not_endpoints() {
        let mut edges = vec![EdgeLayout {
            geometry: PathGeometry::Polyline {
                points: vec![
                    Point::new(40.0, 40.0),
                    Point::new(40.0, 67.3),
                    Point::new(40.0, 89.3),
                    Point::new(89.3, 89.3),
                    Point::new(89.3, 96.0),
                    Point::new(89.3, 120.0),
                ],
            },
            labels: vec![],
            from_port: crate::layout::Port::Bottom,
            to_port: crate::layout::Port::Bottom,
        }];

        let config = EdgeSnapConfig::default_orthogonal();
        let count = snap_edge_waypoints(&mut edges, &HashMap::new(), &config);

        assert!(count >= 1);
        let path = edges[0].path_points();
        assert!((path[0].x - 40.0).abs() < f64::EPSILON);
        assert!((path[0].y - 40.0).abs() < f64::EPSILON);
        assert!((path.last().unwrap().x - 89.3).abs() < f64::EPSILON);
        assert!((path.last().unwrap().y - 120.0).abs() < f64::EPSILON);
        assert!((path[1].x - 40.0).abs() < f64::EPSILON);
        assert!((path[1].y - 67.3).abs() < f64::EPSILON);
    }

    #[test]
    fn snap_edge_waypoints_preserves_ortho_on_4point_l_path() {
        let mut edges = vec![EdgeLayout {
            geometry: PathGeometry::Polyline {
                points: vec![
                    Point::new(40.0, 40.0),
                    Point::new(40.0, 67.3),
                    Point::new(89.3, 67.3),
                    Point::new(89.3, 96.0),
                ],
            },
            labels: vec![],
            from_port: crate::layout::Port::Bottom,
            to_port: crate::layout::Port::Top,
        }];

        let config = EdgeSnapConfig::default_orthogonal();
        snap_edge_waypoints(&mut edges, &HashMap::new(), &config);

        let path = edges[0].path_points();
        assert!((path[0].x - 40.0).abs() < f64::EPSILON);
        assert!((path[1].y - 67.3).abs() < f64::EPSILON);
        assert!((path[3].x - 89.3).abs() < f64::EPSILON);
        assert!((path[3].y - 96.0).abs() < f64::EPSILON);
        let c = path[2];
        assert!((c.x - 89.3).abs() < f64::EPSILON, "C.x must match E.x for vertical seg");
        assert!((c.y - 67.3).abs() < f64::EPSILON, "C.y must match s1.y for horizontal seg");
        for si in 0..path.len() - 1 {
            let a = path[si];
            let b = path[si + 1];
            let dx = (b.x - a.x).abs();
            let dy = (b.y - a.y).abs();
            assert!(dx < 0.01 || dy < 0.01, "seg[{si}] must be orthogonal: ({},{})->({},{})", a.x, a.y, b.x, b.y);
        }
    }

    #[test]
    fn snap_5point_path_corner_between_stays_ortho() {
        let mut path = vec![
            Point::new(662.0, 601.0),
            Point::new(662.0, 585.0),
            Point::new(662.0, 462.0),
            Point::new(614.0, 462.0),
            Point::new(614.0, 387.0),
        ];
        let count = snap_edge_path_channels(&mut path, 8.0);

        let c = path[2];
        assert!((c.x - 662.0).abs() < f64::EPSILON, "C.x must stay at s1.x=662");
        assert!((c.y - 462.0).abs() < f64::EPSILON, "C.y must stay at s2.y=462");
        for si in 0..path.len() - 1 {
            let a = path[si];
            let b = path[si + 1];
            let dx = (b.x - a.x).abs();
            let dy = (b.y - a.y).abs();
            assert!(dx < 0.01 || dy < 0.01,
                "seg[{si}] must be orthogonal after snap: ({},{})->({},{}) dx={:.1} dy={:.1} count={}",
                a.x, a.y, b.x, b.y, dx, dy, count);
        }
    }

    #[test]
    fn snap_edge_waypoints_preserves_stub_on_long_path() {
        let mut edges = vec![EdgeLayout {
            geometry: PathGeometry::Polyline {
                points: vec![
                    Point::new(156.5, 80.0),
                    Point::new(156.5, 96.0),
                    Point::new(156.5, 120.0),
                    Point::new(203.7, 120.0),
                    Point::new(203.7, 200.0),
                    Point::new(203.7, 216.0),
                    Point::new(156.5, 216.0),
                ],
            },
            labels: vec![],
            from_port: crate::layout::Port::Bottom,
            to_port: crate::layout::Port::Top,
        }];

        snap_edge_waypoints(&mut edges, &HashMap::new(), &EdgeSnapConfig::default_orthogonal());
        let path = edges[0].path_points();
        assert!((path[1].x - 156.5).abs() < f64::EPSILON);
        assert!((path[1].y - 96.0).abs() < f64::EPSILON);
    }

    #[test]
    fn snap_edge_waypoints_preserves_straight_vertical_segment() {
        let mut edges = vec![EdgeLayout {
            geometry: PathGeometry::Polyline {
                points: vec![Point::new(156.5, 80.0), Point::new(156.5, 119.7), Point::new(156.5, 200.0)],
            },
            labels: vec![],
            from_port: crate::layout::Port::Bottom,
            to_port: crate::layout::Port::Top,
        }];

        snap_edge_waypoints(&mut edges, &HashMap::new(), &EdgeSnapConfig::default_orthogonal());
        let path = edges[0].path_points();
        assert!(path.len() >= 2);
        for p in path.iter() {
            assert!(
                (p.x - 156.5).abs() < f64::EPSILON,
                "vertical segment x must stay aligned, got x={}",
                p.x
            );
        }
        if path.len() == 3 {
            assert!((path[1].y - 119.7).abs() < f64::EPSILON);
        }
    }

    #[test]
    fn snap_edge_waypoints_simplifies_collinear_points() {
        let mut edges = vec![EdgeLayout {
            geometry: PathGeometry::Polyline {
                points: vec![Point::new(0.0, 0.0), Point::new(40.0, 0.0), Point::new(41.0, 0.0), Point::new(80.0, 0.0), Point::new(80.0, 80.0)],
            },
            labels: vec![],
            from_port: crate::layout::Port::Bottom,
            to_port: crate::layout::Port::Top,
        }];

        snap_edge_waypoints(&mut edges, &HashMap::new(), &EdgeSnapConfig::default_orthogonal());
        assert!(edges[0].path_len() < 5);
    }

    #[test]
    fn snap_edge_waypoints_skips_bezier() {
        let original = EdgeLayout {
            geometry: PathGeometry::Bezier {
                start: Point::new(0.0, 0.0),
                end: Point::new(100.0, 100.0),
                controls: [Point::new(30.0, 30.0), Point::new(70.0, 70.0)],
            },
            labels: vec![],
            from_port: crate::layout::Port::Bottom,
            to_port: crate::layout::Port::Top,
        };
        let mut edges = vec![original.clone()];
        snap_edge_waypoints(&mut edges, &HashMap::new(), &EdgeSnapConfig::default_orthogonal());
        assert_eq!(edges[0].path_points(), original.path_points());
    }

    #[test]
    fn snap_edge_waypoints_disabled_skips_all() {
        let mut edges = vec![EdgeLayout {
            geometry: PathGeometry::Polyline {
                points: vec![
                    Point::new(40.0, 40.0),
                    Point::new(40.0, 56.0),
                    Point::new(41.3, 56.0),
                    Point::new(41.3, 120.0),
                ],
            },
            labels: vec![],
            from_port: crate::layout::Port::Bottom,
            to_port: crate::layout::Port::Top,
        }];
        let original_points = edges[0].path_points().into_owned();
        snap_edge_waypoints(&mut edges, &HashMap::new(), &EdgeSnapConfig::disabled());
        assert_eq!(edges[0].path_points().as_ref(), original_points.as_slice());
    }

    #[test]
    fn snap_edge_waypoints_phase_c_skips_stub_adjacent_corner() {
        let mut path = vec![
            Point::new(40.0, 40.0),
            Point::new(40.0, 56.0),
            Point::new(89.3, 56.0),
            Point::new(89.3, 120.0),
            Point::new(89.3, 136.0),
            Point::new(120.0, 136.0),
        ];
        let step = 8.0;
        snap_edge_path_channels(&mut path, step);

        assert!((path[2].y - 56.0).abs() < f64::EPSILON);
        assert!(is_on_grid(path[2].x, step));
    }

    #[test]
    fn snap_edge_waypoints_phase_c_still_snaps_interior_corner() {
        let mut path = vec![
            Point::new(0.0, 0.0),
            Point::new(0.0, 16.0),
            Point::new(40.0, 16.0),
            Point::new(40.0, 80.0),
            Point::new(203.7, 80.0),
            Point::new(203.7, 120.0),
            Point::new(203.7, 136.0),
            Point::new(100.0, 136.0),
        ];
        snap_edge_path_channels(&mut path, 8.0);

        assert!(is_on_grid(path[4].x, 8.0));
        assert!(is_on_grid(path[4].y, 8.0));
    }

    #[test]
    fn node_align_config_variants() {
        let disabled = NodeAlignConfig::disabled();
        assert!(!disabled.enabled);

        let arch = NodeAlignConfig::default_architecture();
        assert!(arch.enabled);
        assert!((arch.node_gap - GRID_SNAP_NODE_GAP_ARCH).abs() < f64::EPSILON);
        assert_eq!(arch.layer_axis, LayerAxisAlign::Centroid);

        let sugi = NodeAlignConfig::default_sugiyama();
        assert!(sugi.enabled);
        assert!(sugi.rank_axis);
        assert_eq!(sugi.layer_axis, LayerAxisAlign::OverlapOnly);

        let er = NodeAlignConfig::default_er();
        assert_eq!(er.layer_axis, LayerAxisAlign::Off);

        let flow = NodeAlignConfig::default_flowchart();
        assert_eq!(flow.layer_axis, LayerAxisAlign::OverlapOnly);
    }

    use crate::ast::TextValue;

    #[test]
    fn parse_align_override_values() {
        assert_eq!(
            parse_align_override(&AttributeValue::Boolean(false)),
            Some(DiagramAlignOverride::Off)
        );
        assert_eq!(
            parse_align_override(&AttributeValue::Boolean(true)),
            Some(DiagramAlignOverride::Default)
        );
        assert_eq!(
            parse_align_override(&AttributeValue::String(TextValue::unquoted("rank"))),
            Some(DiagramAlignOverride::RankOnly)
        );
        assert_eq!(
            parse_align_override(&AttributeValue::String(TextValue::unquoted("layer"))),
            Some(DiagramAlignOverride::LayerOnly)
        );
        assert_eq!(
            parse_align_override(&AttributeValue::String(TextValue::unquoted("full"))),
            Some(DiagramAlignOverride::Full)
        );
    }

    #[test]
    fn edge_snap_config_variants() {
        let disabled = EdgeSnapConfig::disabled();
        assert!(!disabled.enabled);

        let ortho = EdgeSnapConfig::default_orthogonal();
        assert!(ortho.enabled);
        assert!((ortho.grid_step - GRID_SNAP_STEP).abs() < f64::EPSILON);
        assert!((ortho.shell_pad - GROUP_BORDER_SHELL_PAD).abs() < f64::EPSILON);
        assert!((ortho.stub_clearance - PORT_STUB_CLEARANCE).abs() < f64::EPSILON);
    }
}
