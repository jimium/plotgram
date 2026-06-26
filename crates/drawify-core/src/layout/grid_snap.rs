//! L3 Node Frame — 节点对齐与像素量化（Grid Snap Refinement）
//!
//! 对应 Group Frame 三层模型中的 **L3 Node Frame**（见
//! `docs/architecture/布局优化/group-frame-spec.md` §3.3）。组间宏观几何（L1）
//! 已迁入 `layout::group_frame`；本模块只管 **节点** rank/layer 对齐与量化：
//! - rank 轴（TB 为 y，LR 为 x）：同层节点对齐到层中心线
//! - layer 轴（TB 为 x，LR 为 y）：层内槽位吸附（ER 图跳过）
//!
//! 边路由完成后对通道轴 snap（保护磁吸点/stub）。

use crate::types::DiagramType;
use crate::ast::{AttributeValue, Diagram};
use crate::layout::constants::{
    self, GRID_SNAP_LAYER_TOLERANCE, GRID_SNAP_MAX_DISTANCE, GRID_SNAP_NODE_GAP_ARCH,
    GRID_SNAP_NODE_GAP_SUGIYAMA, GRID_SNAP_STEP,
};
use crate::layout::geometry::Point;
use crate::layout::intent::PinSet;
use crate::layout::{EdgeLayout, GroupLayout, LayoutResult, NodeLayout};
use std::collections::{HashMap, HashSet};

const COLLINEAR_EPS: f64 = 0.1;

/// 网格吸附配置
#[derive(Debug, Clone)]
pub struct GridSnapConfig {
    pub enabled: bool,
    pub grid_step: f64,
    pub node_gap: f64,
    pub max_snap_distance: f64,
    pub layer_tolerance: f64,
    pub padding: f64,
    /// ER 等图类型仅做 rank 轴对齐，跳过层内槽位吸附
    pub rank_axis_only: bool,
}

impl GridSnapConfig {
    /// 从 diagram 顶层属性 `snap` 与布局算法构建配置；缺省为开启。
    pub fn for_diagram(algo: &str, diagram: &Diagram) -> Self {
        let mut config = config_for(algo, diagram);
        config.enabled = snap_enabled_for_diagram(diagram, algo);
        config
    }
}

impl Default for GridSnapConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            grid_step: GRID_SNAP_STEP,
            node_gap: GRID_SNAP_NODE_GAP_SUGIYAMA,
            max_snap_distance: GRID_SNAP_MAX_DISTANCE,
            layer_tolerance: GRID_SNAP_LAYER_TOLERANCE,
            padding: constants::DEFAULT_PADDING,
            rank_axis_only: false,
        }
    }
}

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

/// 是否对该布局算法启用 grid snap
pub fn should_snap(algo: &str) -> bool {
    matches!(
        algo,
        "flowchart" | "er" | "sugiyama-v2" | "architecture"
    )
}

/// 按布局算法与图类型构建配置（`enabled` 由 [`snap_enabled_for_diagram`] 决定）
pub fn config_for(algo: &str, diagram: &Diagram) -> GridSnapConfig {
    let node_gap = match algo {
        "architecture" => GRID_SNAP_NODE_GAP_ARCH,
        _ => GRID_SNAP_NODE_GAP_SUGIYAMA,
    };
    GridSnapConfig {
        node_gap,
        rank_axis_only: algo == "er"
            || (algo == "sugiyama-v2" && diagram.diagram_type == DiagramType::Er),
        ..Default::default()
    }
}

/// 读取 diagram 顶层 `snap: true | false`；未声明时默认 `true`。
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

/// 当前 diagram 是否应执行 grid snap（算法白名单 + `snap` 属性）。
pub fn snap_enabled_for_diagram(diagram: &Diagram, algo: &str) -> bool {
    if !should_snap(algo) {
        return false;
    }
    diagram_snap_attribute(diagram).unwrap_or(true)
}

/// 对布局结果执行网格吸附。
///
/// `pinned` 中的节点在对应轴上跳过 snap（由 `Pin` / `Align*` 意图保护）。
pub fn snap_layout_to_grid(
    layout: &mut LayoutResult,
    config: &GridSnapConfig,
    horizontal: bool,
    pinned: &PinSet,
) -> SnapReport {
    if !config.enabled || layout.nodes.is_empty() {
        return SnapReport::default();
    }

    let mut node_ids: Vec<String> = layout.nodes.keys().cloned().collect();
    // 排序保证迭代顺序确定（HashMap 迭代顺序随机）
    node_ids.sort();
    let layers = cluster_by_rank_axis(&layout.nodes, &node_ids, horizontal, config.layer_tolerance);

    let mut report = SnapReport::default();

    for layer in &layers {
        snap_rank_axis_centers(layout, layer, horizontal, config, &mut report, pinned);
    }

    if !config.rank_axis_only {
        for layer in &layers {
            snap_layer_axis_slots(layout, layer, horizontal, config, &mut report, pinned);
            resolve_layer_axis_overlaps(layout, layer, horizontal, config, pinned);
        }
    }

    report
}

/// 边路由完成后，对正交折线路径做通道轴 snap（保护磁吸点与 stub，不逐点双轴 snap）。
///
/// - 端点 `path[0]` / `path[last]`：磁吸锚点，不修改
/// - stub `path[1]` / `path[last-1]`（len ≥ 4）：端口 clearance，不修改
/// - 仅对通道段量化：竖线段对齐 x、横线段对齐 y；邻接 protected 的段只 snap 可动端的主轴坐标
/// - Phase B：量化后将贴边通道段投影到分组边框壳层外的合法格点
pub fn snap_edge_waypoints(
    edges: &mut [EdgeLayout],
    groups: &HashMap<String, GroupLayout>,
    config: &GridSnapConfig,
    shell_pad: f64,
    stub_clearance: f64,
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
            shell_pad,
            config.grid_step,
            stub_clearance,
        );
        let simplified = simplify_polyline_path_preserving_stubs(points);
        // set_polyline_points 自动根据点数选择 Straight（≤2 点）/ Polyline（>2 点）
        edge.set_polyline_points(simplified);
    }

    snapped
}

/// 根据 nodes / groups 更新画布 total 尺寸
pub fn update_canvas_bounds(layout: &mut LayoutResult, padding: f64) {
    let (total_width, total_height) = bounds_from_layout(&layout.nodes, &layout.groups, padding);
    layout.total_width = total_width;
    layout.total_height = total_height;
}


fn protected_path_indices(len: usize) -> HashSet<usize> {
    let mut protected = HashSet::from([0, len.saturating_sub(1)]);
    if len >= 3 {
        protected.insert(1);
    }
    // 仅 len ≥ 5 时 path[len-2] 才是入口 stub；len = 4 时该点是通道拐角
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

/// 通道轴 snap：只动可修改点，保持 stub/磁吸点处的 H/V 出线方向。
fn snap_edge_path_channels(path: &mut [Point], step: f64) -> usize {
    let n = path.len();
    if n <= 2 {
        return 0;
    }

    let protected = protected_path_indices(n);
    let mut count = 0usize;

    // Phase A：两端均可动的通道段 — 整段对齐到同一 snapped 轴坐标
    for i in 0..n - 1 {
        if protected.contains(&i) || protected.contains(&(i + 1)) {
            continue;
        }
        let a = path[i];
        let b = path[i + 1];
        if is_vertical_segment(a, b) {
            let sx = snap_to_grid((a.x + b.x) * 0.5, step);
            if (path[i].x - sx).abs() > f64::EPSILON {
                path[i].x = sx;
                count += 1;
            }
            if (path[i + 1].x - sx).abs() > f64::EPSILON {
                path[i + 1].x = sx;
                count += 1;
            }
        } else if is_horizontal_segment(a, b) {
            let sy = snap_to_grid((a.y + b.y) * 0.5, step);
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

    // Phase B：一端 protected（stub/磁吸点）— 只 snap 可动端沿通道主轴的坐标
    for i in 0..n - 1 {
        let p0 = protected.contains(&i);
        let p1 = protected.contains(&(i + 1));
        if p0 == p1 {
            continue;
        }
        let (prot_idx, mod_idx) = if p0 { (i, i + 1) } else { (i + 1, i) };
        let prot_pt = path[prot_idx];
        let mod_pt = path[mod_idx];
        let seg = (path[i], path[i + 1]);

        let new_pt = if is_vertical_segment(seg.0, seg.1) {
            Point::new(prot_pt.x, snap_to_grid(mod_pt.y, step))
        } else if is_horizontal_segment(seg.0, seg.1) {
            Point::new(snap_to_grid(mod_pt.x, step), prot_pt.y)
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

    // Phase C：真实通道拐角（可动点，且不与 stub/磁吸点相邻）
    // stub 邻接拐角由 Phase B 处理，此处再 snap 另一轴会破坏 H/V 正交与圆角切线。
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

        let mut x = curr.x;
        let mut y = curr.y;
        if prev_h {
            y = prev.y;
            x = snap_to_grid(curr.x, step);
        } else if prev_v {
            x = prev.x;
            y = snap_to_grid(curr.y, step);
        }
        if next_h {
            y = next.y;
            x = snap_to_grid(curr.x, step);
        } else if next_v {
            x = next.x;
            y = snap_to_grid(curr.y, step);
        }

        if (curr.x - x).abs() > f64::EPSILON || (curr.y - y).abs() > f64::EPSILON {
            path[i] = Point::new(x, y);
            count += 1;
        }
    }

    count
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

fn is_collinear(a: Point, b: Point, c: Point) -> bool {
    let cross = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    cross.abs() < COLLINEAR_EPS
}

/// 共线简化，保留首尾 stub（与正交边路由 `simplify_path_preserving_stubs` 一致）
fn simplify_polyline_path_preserving_stubs(path: &[Point]) -> Vec<Point> {
    if path.len() <= 2 {
        return path.to_vec();
    }

    let mut deduped = path.to_vec();
    deduped.dedup_by(|a, b| (a.x - b.x).abs() < COLLINEAR_EPS && (a.y - b.y).abs() < COLLINEAR_EPS);
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
        if preserves_stub || !is_collinear(prev, curr, next) {
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

fn bounds_from_layout(
    nodes: &HashMap<String, NodeLayout>,
    groups: &HashMap<String, GroupLayout>,
    padding: f64,
) -> (f64, f64) {
    let node_max_x = nodes.values().map(|n| n.x + n.width).fold(0.0_f64, f64::max);
    let node_max_y = nodes.values().map(|n| n.y + n.height).fold(0.0_f64, f64::max);
    let group_max_x = groups.values().map(|g| g.x + g.width).fold(0.0_f64, f64::max);
    let group_max_y = groups.values().map(|g| g.y + g.height).fold(0.0_f64, f64::max);
    (
        node_max_x.max(group_max_x) + padding,
        node_max_y.max(group_max_y) + padding,
    )
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

/// 按 rank 轴坐标聚类推断层
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
    config: &GridSnapConfig,
    report: &mut SnapReport,
    pinned: &PinSet,
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
        // pinned 节点跳过 rank 轴 snap
        if pinned.is_rank_pinned(id, horizontal) {
            continue;
        }
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

fn snap_layer_axis_slots(
    layout: &mut LayoutResult,
    layer: &[String],
    horizontal: bool,
    config: &GridSnapConfig,
    report: &mut SnapReport,
    pinned: &PinSet,
) {
    if layer.len() <= 1 {
        return;
    }

    let mut ordered: Vec<String> = layer.to_vec();
    ordered.sort_by(|a, b| {
        let ca = layout.nodes.get(a).map(|nl| layer_center(nl, horizontal)).unwrap_or(0.0);
        let cb = layout.nodes.get(b).map(|nl| layer_center(nl, horizontal)).unwrap_or(0.0);
        ca.partial_cmp(&cb)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.cmp(b))
    });

    let mut cursor = config.padding;
    for id in &ordered {
        let Some(node) = layout.nodes.get(id) else {
            continue;
        };
        let size = layer_size(node, horizontal);
        let slot_center = cursor + size / 2.0;
        cursor += size + config.node_gap;

        // pinned 节点跳过 layer 轴 snap
        if pinned.is_layer_pinned(id, horizontal) {
            continue;
        }
        let Some(node) = layout.nodes.get_mut(id) else {
            continue;
        };
        let before = layer_center(node, horizontal);
        if (before - slot_center).abs() <= config.max_snap_distance {
            set_layer_center(node, slot_center, horizontal);
            record_snap(report, before, slot_center);
        } else {
            report.skipped_nodes += 1;
        }
    }
}

/// 层内 layer 轴一维重叠消除（前向 + 后向扫描）。
///
/// `pinned` 节点不被移动，但作为障碍物参与重叠计算。
fn resolve_layer_axis_overlaps(
    layout: &mut LayoutResult,
    layer: &[String],
    horizontal: bool,
    config: &GridSnapConfig,
    pinned: &PinSet,
) {
    if layer.len() <= 1 {
        return;
    }

    let mut ordered: Vec<String> = layer.to_vec();
    ordered.sort_by(|a, b| {
        let ca = layout.nodes.get(a).map(|nl| layer_center(nl, horizontal)).unwrap_or(0.0);
        let cb = layout.nodes.get(b).map(|nl| layer_center(nl, horizontal)).unwrap_or(0.0);
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

    for (i, id) in ordered.iter().enumerate() {
        let min_center = config.padding + sizes[i] / 2.0;
        if centers[i] < min_center {
            centers[i] = min_center;
        }
        // pinned 节点不被移动
        if pinned.is_layer_pinned(id, horizontal) {
            continue;
        }
        if let Some(node) = layout.nodes.get_mut(id) {
            set_layer_center(node, centers[i], horizontal);
        }
    }
}

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
        let config = GridSnapConfig {
            max_snap_distance: 24.0,
            node_gap: 48.0,
            ..Default::default()
        };
        snap_layout_to_grid(&mut layout, &config, false, &PinSet::default());

        let top = rank_centers(&layout, &["a", "b"], false);
        assert!((top[0] - top[1]).abs() < f64::EPSILON, "same layer y must match");
    }

    #[test]
    fn snap_aligns_same_layer_rank_axis_lr() {
        let mut nodes = HashMap::new();
        nodes.insert("a".into(), node(100.0, 10.0, 40.0, 80.0));
        nodes.insert("b".into(), node(103.0, 120.0, 40.0, 80.0));

        let mut layout = sample_layout(nodes);
        let config = GridSnapConfig::default();
        snap_layout_to_grid(&mut layout, &config, true, &PinSet::default());

        let xs = rank_centers(&layout, &["a", "b"], true);
        assert!((xs[0] - xs[1]).abs() < f64::EPSILON, "same layer x must match for LR");
    }

    #[test]
    fn slot_snap_respects_node_gap() {
        let mut nodes = HashMap::new();
        nodes.insert("a".into(), node(41.0, 100.0, 80.0, 40.0));
        nodes.insert("b".into(), node(200.0, 100.0, 80.0, 40.0));

        let mut layout = sample_layout(nodes);
        let config = GridSnapConfig {
            node_gap: 48.0,
            max_snap_distance: 24.0,
            ..Default::default()
        };
        snap_layout_to_grid(&mut layout, &config, false, &PinSet::default());

        let a = &layout.nodes["a"];
        let b = &layout.nodes["b"];
        let gap = b.x - (a.x + a.width);
        assert!(gap >= 48.0 - 0.1, "gap={gap}");
    }

    #[test]
    fn rank_axis_only_skips_layer_slot_snap() {
        let mut nodes = HashMap::new();
        nodes.insert("a".into(), node(41.0, 100.0, 80.0, 40.0));
        nodes.insert("b".into(), node(200.0, 100.0, 80.0, 40.0));

        let mut layout = sample_layout(nodes.clone());
        let mut config = GridSnapConfig::default();
        config.rank_axis_only = true;
        snap_layout_to_grid(&mut layout, &config, false, &PinSet::default());

        assert!((layout.nodes["b"].x - nodes["b"].x).abs() < 0.1);
    }

    #[test]
    fn snap_is_deterministic() {
        let mut nodes = HashMap::new();
        nodes.insert("a".into(), node(10.0, 100.0, 80.0, 40.0));
        nodes.insert("b".into(), node(130.0, 103.0, 80.0, 40.0));
        nodes.insert("c".into(), node(40.0, 200.0, 80.0, 40.0));

        let mut layout1 = sample_layout(nodes.clone());
        let mut layout2 = sample_layout(nodes);
        let config = GridSnapConfig::default();
        snap_layout_to_grid(&mut layout1, &config, false, &PinSet::default());
        snap_layout_to_grid(&mut layout2, &config, false, &PinSet::default());

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
        let config = GridSnapConfig {
            max_snap_distance: 48.0,
            ..Default::default()
        };
        snap_layout_to_grid(&mut layout, &config, false, &PinSet::default());

        let ids: Vec<_> = layout.nodes.keys().cloned().collect();
        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                let a = &layout.nodes[&ids[i]];
                let b = &layout.nodes[&ids[j]];
                let overlap_x = a.x < b.x + b.width && b.x < a.x + a.width;
                let overlap_y = a.y < b.y + b.height && b.y < a.y + a.height;
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
        let config = GridSnapConfig {
            max_snap_distance: 10.0,
            ..Default::default()
        };
        let report = snap_layout_to_grid(&mut layout, &config, false, &PinSet::default());

        assert!(report.skipped_nodes >= 1);
        assert!((layout.nodes["far"].y - 140.0).abs() < 0.1);
    }

    #[test]
    fn should_snap_only_whitelisted_algos() {
        assert!(should_snap("sugiyama-v2"));
        assert!(should_snap("architecture"));
        assert!(!should_snap("force-directed"));
        assert!(!should_snap("sugiyama"));
    }

    #[test]
    fn snap_enabled_defaults_to_true() {
        let diagram = Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: vec![],
            entities: vec![],
            relations: vec![],
            groups: vec![],
            style_decls: vec![],
            source_info: crate::ast::SourceInfo {
                file: None,
                line_count: 1,
            },
            ..Default::default()
        };
        assert!(snap_enabled_for_diagram(&diagram, "sugiyama-v2"));
    }

    #[test]
    fn snap_disabled_by_diagram_attribute() {
        use crate::ast::{DiagramAttribute, Span, Position};

        let span = Span::new(Position::new(1, 1), Position::new(1, 1));
        let diagram = Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: vec![DiagramAttribute {
                key: "snap".into(),
                value: AttributeValue::Boolean(false),
                span,
            }],
            entities: vec![],
            relations: vec![],
            groups: vec![],
            style_decls: vec![],
            source_info: crate::ast::SourceInfo {
                file: None,
                line_count: 1,
            },
            ..Default::default()
        };
        assert!(!snap_enabled_for_diagram(&diagram, "sugiyama-v2"));
        assert!(!GridSnapConfig::for_diagram("sugiyama-v2", &diagram).enabled);
    }

    #[test]
    fn snap_edge_waypoints_snaps_middle_not_endpoints() {
        let mut edges = vec![EdgeLayout {
            geometry: PathGeometry::Polyline {
                points: vec![Point::new(40.0, 40.0), Point::new(40.0, 67.3), Point::new(89.3, 67.3), Point::new(89.3, 96.0)],
            },
            labels: vec![],
            from_port: crate::layout::Port::Bottom,
            to_port: crate::layout::Port::Top,
        }];

        let config = GridSnapConfig::default();
        let count = snap_edge_waypoints(&mut edges, &HashMap::new(), &config, 12.0, 16.0);

        assert!(count >= 1);
        let path = edges[0].path_points();
        assert!((path[0].x - 40.0).abs() < f64::EPSILON);
        assert!((path[0].y - 40.0).abs() < f64::EPSILON);
        assert!((path.last().unwrap().x - 89.3).abs() < f64::EPSILON);
        assert!((path.last().unwrap().y - 96.0).abs() < f64::EPSILON);
        // stub 不被 snap
        assert!((path[1].x - 40.0).abs() < f64::EPSILON);
        assert!((path[1].y - 67.3).abs() < f64::EPSILON);
        // 横通道拐点：x 上格点，y 与 stub 对齐
        let corner = path[2];
        assert!(is_on_grid(corner.x, 8.0));
        assert!((corner.y - 67.3).abs() < f64::EPSILON);
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

        snap_edge_waypoints(&mut edges, &HashMap::new(), &GridSnapConfig::default(), 12.0, 16.0);
        let path = edges[0].path_points();
        assert!((path[1].x - 156.5).abs() < f64::EPSILON);
        assert!((path[1].y - 96.0).abs() < f64::EPSILON);
        assert!((path[path.len() - 2].x - 203.7).abs() < f64::EPSILON || is_on_grid(path[path.len() - 2].x, 8.0));
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

        snap_edge_waypoints(&mut edges, &HashMap::new(), &GridSnapConfig::default(), 12.0, 16.0);
        let path = edges[0].path_points();
        assert!(
            path.len() >= 2,
            "straight vertical path should remain at least start/end"
        );
        for p in path.iter() {
            assert!(
                (p.x - 156.5).abs() < f64::EPSILON,
                "vertical segment x must stay aligned, got x={}",
                p.x
            );
        }
        // 3 点竖直链：中间点为 stub，不参与 snap
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

        snap_edge_waypoints(&mut edges, &HashMap::new(), &GridSnapConfig::default(), 12.0, 16.0);
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
        snap_edge_waypoints(&mut edges, &HashMap::new(), &GridSnapConfig::default(), 12.0, 16.0);
        assert_eq!(edges[0].path_points(), original.path_points());
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

        // stub 邻接拐角：y 必须与 stub 对齐，不能被 Phase C snap 到其它格点
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

        // path[4] 为通道内拐角，两侧均非 stub
        assert!(is_on_grid(path[4].x, 8.0));
        assert!(is_on_grid(path[4].y, 8.0));
    }
}
