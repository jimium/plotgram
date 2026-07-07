//! Mindmap（思维导图）专属布局算法
//!
//! 支持三种展开方向：
//! - `radial` / `from_center`（默认）：中心主题居中，一级分支左右交替辐射
//! - `top-to-bottom`：中心主题在上方，树形向下展开
//! - `left-to-right`：中心主题在左侧，树形向右展开

use crate::ast::Diagram;
use crate::types::DiagramType;
use crate::layout::algorithm_config::{MindmapLayoutConfig, MINDMAP_LAYOUT_OPTIONS};
use crate::layout::plan::ResolvedAlgoOptions;
use crate::layout::{AlgorithmOptionSpec, LayoutResult, LayoutStrategy, NodeLayout};
use std::collections::HashMap;
use unicode_width::UnicodeWidthStr;

const APPLICABLE_TYPES: &[DiagramType] = &[DiagramType::Mindmap];

/// 思维导图节点种类
///
/// 从 entity 的 `type` 属性推导，驱动节点尺寸估算。
/// 取代旧版 `node_size` 中散落的字符串匹配 + 魔法数字。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MindmapNodeKind {
    /// 中心主题（`type: root`）
    Root,
    /// 一级分支（`type: main`）
    Main,
    /// 叶子节点（`type: leaf`）
    Leaf,
    /// 普通分支（默认/未知 type）
    Branch,
}

impl MindmapNodeKind {
    /// 从 entity 的 `type` 属性字符串推导种类
    fn from_entity(entity: &crate::ast::Entity) -> Self {
        let ty = entity
            .attributes
            .standard
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("branch");
        match ty {
            "root" => MindmapNodeKind::Root,
            "main" => MindmapNodeKind::Main,
            "leaf" => MindmapNodeKind::Leaf,
            _ => MindmapNodeKind::Branch,
        }
    }

    /// 节点尺寸参数：(min_w, max_w, height, label_padding, char_width)
    const fn size_params(self) -> (f64, f64, f64, f64, f64) {
        match self {
            MindmapNodeKind::Root => (160.0, 180.0, 140.0, 48.0, 16.0),
            MindmapNodeKind::Main => (132.0, 200.0, 54.0, 36.0, 13.5),
            MindmapNodeKind::Leaf => (108.0, 176.0, 46.0, 30.0, 12.0),
            MindmapNodeKind::Branch => (120.0, 188.0, 50.0, 32.0, 12.5),
        }
    }

    /// 是否为中心主题（用于 `horizontal_depth_x` 识别 root 宽度）
    const fn is_root(self) -> bool {
        matches!(self, MindmapNodeKind::Root)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MindmapMode {
    /// 水平双向布局（左右交替，旧版 radial）
    Radial,
    /// 真正的极坐标径向布局（root 居中，分支按角度辐射）
    TrueRadial,
    TopToBottom,
    LeftToRight,
}

/// 思维导图专属布局
pub struct MindmapLayout {
    config: MindmapLayoutConfig,
}

impl MindmapLayout {
    pub fn new(config: MindmapLayoutConfig) -> Self {
        Self { config }
    }

    pub fn from_options(options: &ResolvedAlgoOptions) -> Self {
        Self::new(MindmapLayoutConfig::from_options(options))
    }
}

impl Default for MindmapLayout {
    fn default() -> Self {
        Self::new(MindmapLayoutConfig::default())
    }
}

impl LayoutStrategy for MindmapLayout {
    fn name(&self) -> &'static str {
        "mindmap"
    }

    fn applicable_diagram_types(&self) -> &'static [DiagramType] {
        APPLICABLE_TYPES
    }

    fn option_specs(&self) -> &'static [AlgorithmOptionSpec] {
        MINDMAP_LAYOUT_OPTIONS
    }

    fn supported_directions(&self) -> &'static [&'static str] {
        const SUPPORTED_DIRECTIONS: &[&str] = &[
            crate::types::attr_constants::direction::RADIAL,
            crate::types::attr_constants::direction::TOP_TO_BOTTOM,
            crate::types::attr_constants::direction::LEFT_TO_RIGHT,
        ];
        SUPPORTED_DIRECTIONS
    }

    fn compute(&self, diagram: &Diagram) -> LayoutResult {
        let config = self.config;
        if diagram.entities.is_empty() {
            return empty_result(config);
        }

        let children = build_children_map(diagram);
        let root_id = find_root_id(diagram, &children);
        let mode = layout_mode(diagram);
        // 径向模式下，root 子节点数 >= 3 时启用真正的极坐标径向布局
        let mode = match mode {
            MindmapMode::Radial => {
                let root_branch_count = children.get(&root_id).map(|v| v.len()).unwrap_or(0);
                if root_branch_count >= 3 {
                    MindmapMode::TrueRadial
                } else {
                    MindmapMode::Radial
                }
            }
            other => other,
        };
        let mut centers: HashMap<String, (f64, f64)> = HashMap::new();
        let mut sizes: HashMap<String, (f64, f64)> = HashMap::new();

        for entity in &diagram.entities {
            sizes.insert(entity.id.as_str().to_string(), node_size(diagram, entity));
        }

        match mode {
            MindmapMode::Radial => {
                layout_radial(diagram, &root_id, &children, &sizes, &mut centers, config)
            }
            MindmapMode::TrueRadial => {
                layout_true_radial(&root_id, &children, &sizes, &mut centers, config)
            }
            MindmapMode::TopToBottom => layout_directional_tree(
                diagram, &root_id, &children, &sizes, &mut centers, false, config,
            ),
            MindmapMode::LeftToRight => layout_directional_tree(
                diagram, &root_id, &children, &sizes, &mut centers, true, config,
            ),
        }

        place_disconnected_nodes(diagram, &children, &root_id, &sizes, &mut centers, mode, config);

        // 重叠检测与消除（安全网，所有模式通用）
        detect_and_fix_overlaps(&mut centers, &sizes, config.node_gap);

        // TrueRadial 模式：最终重新居中，确保 root 在画布几何中心
        if mode == MindmapMode::TrueRadial {
            recenter_root(&root_id, &mut centers, &sizes, config);
        }

        let nodes = centers
            .iter()
            .map(|(id, (cx, cy))| {
                let (w, h) = sizes.get(id).copied().unwrap_or((150.0, 48.0));
                (
                    id.clone(),
                    NodeLayout {
                        x: cx - w / 2.0,
                        y: cy - h / 2.0,
                        width: w,
                        height: h,
                        ..Default::default()
                    },
                )
            })
            .collect::<HashMap<_, _>>();

        // 计算节点深度，供边路由层级感知使用
        let node_depths = compute_node_depths(&root_id, &children);

        // TrueRadial 模式：对称画布尺寸（root 在中心）；其他模式用实际边界
        let (total_width, total_height) = if mode == MindmapMode::TrueRadial {
            let (root_cx, root_cy) = centers[&root_id];
            let mut max_dx: f64 = 0.0;
            let mut max_dy: f64 = 0.0;
            for (id, (cx, cy)) in centers.iter() {
                let (w, h) = sizes.get(id).copied().unwrap_or((150.0, 48.0));
                max_dx = max_dx.max((cx - root_cx).abs() + w / 2.0);
                max_dy = max_dy.max((cy - root_cy).abs() + h / 2.0);
            }
            (2.0 * (max_dx + config.padding), 2.0 * (max_dy + config.padding))
        } else {
            bounds_from_nodes(&nodes, config)
        };
        LayoutResult {
            nodes,
            groups: HashMap::new(),
            edges: vec![],
            total_width,
            total_height,
            hints: crate::layout::LayoutHints {
                edge_routing_style: crate::layout::EdgeRoutingStyle::Curved,
                mindmap_depths: Some(node_depths),
                ..Default::default()
            },
        }
    }
}

fn empty_result(config: MindmapLayoutConfig) -> LayoutResult {
    LayoutResult {
        nodes: HashMap::new(),
        groups: HashMap::new(),
        edges: vec![],
        total_width: config.padding * 2.0,
        total_height: config.padding * 2.0,
        hints: crate::layout::LayoutHints {
            edge_routing_style: crate::layout::EdgeRoutingStyle::Curved,
            ..Default::default()
        },
    }
}

fn layout_mode(diagram: &Diagram) -> MindmapMode {
    // 统一通过 resolve_effective_direction 获取有效方向
    match crate::layout::resolve_effective_direction(diagram) {
        Some("left-to-right") => MindmapMode::LeftToRight,
        Some("top-to-bottom") => MindmapMode::TopToBottom,
        // radial / None（不应发生，mindmap profile 有默认值）/ 未知值均回退 radial
        _ => MindmapMode::Radial,
    }
}

fn node_size(_diagram: &Diagram, entity: &crate::ast::Entity) -> (f64, f64) {
    let kind = MindmapNodeKind::from_entity(entity);
    let (min_w, max_w, h, padding, char_w) = kind.size_params();

    let label_width = entity.label.width() as f64;
    let w = (label_width * char_w + padding).clamp(min_w, max_w);
    let (w, h) = crate::layout::styled_node_size(entity, w, h);

    if kind.is_root() {
        let side = w.max(h);
        (side, side)
    } else {
        (w, h)
    }
}

fn build_children_map(diagram: &Diagram) -> HashMap<String, Vec<String>> {
    let mut children: HashMap<String, Vec<String>> = HashMap::new();
    for entity in &diagram.entities {
        children.entry(entity.id.as_str().to_string()).or_default();
    }
    for rel in &diagram.relations {
        children
            .entry(rel.from.as_str().to_string())
            .or_default()
            .push(rel.to.as_str().to_string());
    }
    children
}

fn find_root_id(diagram: &Diagram, children: &HashMap<String, Vec<String>>) -> String {
    for entity in &diagram.entities {
        if MindmapNodeKind::from_entity(entity).is_root() {
            return entity.id.as_str().to_string();
        }
    }

    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    for entity in &diagram.entities {
        in_degree.insert(entity.id.as_str(), 0);
    }
    for rel in &diagram.relations {
        *in_degree.entry(rel.to.as_str()).or_insert(0) += 1;
    }

    if let Some((id, _)) = in_degree.iter().find(|(_, deg)| **deg == 0) {
        return id.to_string();
    }

    children
        .keys()
        .next()
        .cloned()
        .unwrap_or_else(|| diagram.entities[0].id.as_str().to_string())
}

/// 计算每个节点的深度（BFS 从 root 开始）
///
/// root 节点 depth = 0，一级分支 depth = 1，依此类推。
/// 孤立节点 depth = 0。
fn compute_node_depths(
    root_id: &str,
    children: &HashMap<String, Vec<String>>,
) -> HashMap<String, usize> {
    use std::collections::VecDeque;

    let mut depths: HashMap<String, usize> = HashMap::new();
    let mut queue: VecDeque<(String, usize)> = VecDeque::new();

    queue.push_back((root_id.to_string(), 0));
    depths.insert(root_id.to_string(), 0);

    while let Some((node_id, depth)) = queue.pop_front() {
        if let Some(child_list) = children.get(&node_id) {
            for child_id in child_list {
                if !depths.contains_key(child_id) {
                    depths.insert(child_id.clone(), depth + 1);
                    queue.push_back((child_id.clone(), depth + 1));
                }
            }
        }
    }

    // 处理孤立节点（不在树中的节点）
    for id in children.keys() {
        if !depths.contains_key(id) {
            depths.insert(id.clone(), 0);
        }
    }

    depths
}

/// 真正的极坐标径向布局：root 居中，一级分支按角度均匀辐射，子树沿父方向扇形展开。
///
/// 角度分配按子树叶子数加权（Reingold-Tilford 思想），避免大子树与小子树角度相同导致重叠。
/// 半径随深度递增，确保层级清晰。
fn layout_true_radial(
    root_id: &str,
    children: &HashMap<String, Vec<String>>,
    sizes: &HashMap<String, (f64, f64)>,
    centers: &mut HashMap<String, (f64, f64)>,
    config: MindmapLayoutConfig,
) {
    // root 在原点
    centers.insert(root_id.to_string(), (0.0, 0.0));

    let root_children = children.get(root_id).cloned().unwrap_or_default();
    if root_children.is_empty() {
        normalize_to_padding(centers, sizes, config);
        return;
    }

    // 按子树叶子数加权分配角度扇区
    let weights: Vec<f64> = root_children
        .iter()
        .map(|cid| subtree_leaf_count(cid, children) as f64)
        .collect();
    let total_weight: f64 = weights.iter().sum();

    // 从正上方（-90°）开始顺时针分配
    let start_angle = -std::f64::consts::PI / 2.0;
    let mut angle_cursor = start_angle;

    let (root_w, root_h) = sizes.get(root_id).copied().unwrap_or((120.0, 120.0));
    let root_extent = root_w.max(root_h) / 2.0;
    let base_radius = root_extent + config.center_gap;

    for (i, child_id) in root_children.iter().enumerate() {
        let sector = 2.0 * std::f64::consts::PI * weights[i] / total_weight;
        let child_angle = angle_cursor + sector / 2.0;

        // 一级分支的半径
        let (child_w, child_h) = sizes.get(child_id).copied().unwrap_or((132.0, 54.0));
        let child_extent = child_w.max(child_h) / 2.0;
        let radius = base_radius + child_extent;
        let cx = radius * child_angle.cos();
        let cy = radius * child_angle.sin();
        centers.insert(child_id.to_string(), (cx, cy));

        // 递归布局子树到扇区内
        layout_radial_subtree(
            child_id,
            child_angle,
            sector,
            radius,
            children,
            sizes,
            centers,
            config,
        );

        angle_cursor += sector;
    }

    normalize_to_padding(centers, sizes, config);
}

/// 递归将子树布局到父节点的角度扇区内。
#[allow(clippy::too_many_arguments)]
fn layout_radial_subtree(
    node_id: &str,
    node_angle: f64,
    sector: f64,
    node_radius: f64,
    children: &HashMap<String, Vec<String>>,
    sizes: &HashMap<String, (f64, f64)>,
    centers: &mut HashMap<String, (f64, f64)>,
    config: MindmapLayoutConfig,
) {
    let kids = match children.get(node_id) {
        Some(k) if !k.is_empty() => k,
        _ => return,
    };

    // 子节点的半径 = 当前半径 + 当前节点 extent + level_gap
    let (node_w, node_h) = sizes.get(node_id).copied().unwrap_or((132.0, 54.0));
    let node_extent = node_w.max(node_h) / 2.0;
    let child_radius = node_radius + node_extent + config.level_gap;

    // 按叶子数分配子扇区
    let weights: Vec<f64> = kids
        .iter()
        .map(|kid| subtree_leaf_count(kid, children) as f64)
        .collect();
    let total_weight: f64 = weights.iter().sum();

    let mut angle_cursor = node_angle - sector / 2.0;

    for (i, kid) in kids.iter().enumerate() {
        let child_sector = if total_weight > 0.0 {
            sector * weights[i] / total_weight
        } else {
            sector / kids.len() as f64
        };
        let child_angle = angle_cursor + child_sector / 2.0;

        let cx = child_radius * child_angle.cos();
        let cy = child_radius * child_angle.sin();
        centers.insert(kid.to_string(), (cx, cy));

        layout_radial_subtree(
            kid,
            child_angle,
            child_sector,
            child_radius,
            children,
            sizes,
            centers,
            config,
        );

        angle_cursor += child_sector;
    }
}

/// 计算子树的叶子节点数（用于角度加权分配）。
fn subtree_leaf_count(node_id: &str, children: &HashMap<String, Vec<String>>) -> usize {
    match children.get(node_id) {
        Some(kids) if !kids.is_empty() => {
            kids.iter().map(|kid| subtree_leaf_count(kid, children)).sum()
        }
        _ => 1,
    }
}

/// 节点重叠检测与消除：迭代式推开重叠节点。
///
/// 作为布局安全网，检测所有节点对的包围盒重叠，沿重叠较小的轴推开。
/// 适用于所有布局模式（radial / directional）。
fn detect_and_fix_overlaps(
    centers: &mut HashMap<String, (f64, f64)>,
    sizes: &HashMap<String, (f64, f64)>,
    min_gap: f64,
) {
    let max_iterations = 30;
    let ids: Vec<String> = centers.keys().cloned().collect();
    let n = ids.len();
    if n < 2 {
        return;
    }

    for _ in 0..max_iterations {
        let mut moved = false;

        for i in 0..n {
            for j in (i + 1)..n {
                let id_a = &ids[i];
                let id_b = &ids[j];
                let (ax, ay) = centers[id_a];
                let (bx, by) = centers[id_b];
                let (aw, ah) = sizes.get(id_a).copied().unwrap_or((150.0, 48.0));
                let (bw, bh) = sizes.get(id_b).copied().unwrap_or((150.0, 48.0));

                // 包围盒重叠检测（含最小间距）
                let min_dx = (aw + bw) / 2.0 + min_gap;
                let min_dy = (ah + bh) / 2.0 + min_gap;
                let dx = bx - ax;
                let dy = by - ay;
                let abs_dx = dx.abs();
                let abs_dy = dy.abs();

                if abs_dx < min_dx && abs_dy < min_dy {
                    let overlap_x = min_dx - abs_dx;
                    let overlap_y = min_dy - abs_dy;

                    // 沿重叠较小的轴推开（减少总位移）
                    if overlap_x < overlap_y {
                        let push = overlap_x / 2.0 + 0.5;
                        let sign = if dx >= 0.0 { 1.0 } else { -1.0 };
                        if let Some((x, _)) = centers.get_mut(id_a) {
                            *x -= sign * push;
                        }
                        if let Some((x, _)) = centers.get_mut(id_b) {
                            *x += sign * push;
                        }
                    } else {
                        let push = overlap_y / 2.0 + 0.5;
                        let sign = if dy >= 0.0 { 1.0 } else { -1.0 };
                        if let Some((_, y)) = centers.get_mut(id_a) {
                            *y -= sign * push;
                        }
                        if let Some((_, y)) = centers.get_mut(id_b) {
                            *y += sign * push;
                        }
                    }
                    moved = true;
                }
            }
        }

        if !moved {
            break;
        }
    }
}

fn layout_radial(
    _diagram: &Diagram,
    root_id: &str,
    children: &HashMap<String, Vec<String>>,
    sizes: &HashMap<String, (f64, f64)>,
    centers: &mut HashMap<String, (f64, f64)>,
    config: MindmapLayoutConfig,
) {
    let root_children = children.get(root_id).cloned().unwrap_or_default();
    if root_children.is_empty() {
        let (w, h) = sizes.get(root_id).copied().unwrap_or((120.0, 120.0));
        centers.insert(
            root_id.to_string(),
            (config.padding + w / 2.0, config.padding + h / 2.0),
        );
        return;
    }

    let root_w = sizes.get(root_id).map(|(w, _)| *w).unwrap_or(120.0);

    let mut y_cursor = 0.0;
    for (i, child_id) in root_children.iter().enumerate() {
        let direction = if i % 2 == 0 { 1.0 } else { -1.0 };
        layout_horizontal_subtree(
            child_id, 1, direction, &mut y_cursor, children, sizes, centers, config,
            root_w,
        );
        y_cursor += config.node_gap;
    }
    if y_cursor > 0.0 {
        y_cursor -= config.node_gap;
    }

    let root_cy = y_cursor / 2.0;
    centers.insert(root_id.to_string(), (0.0, root_cy));

    normalize_to_padding(centers, sizes, config);
}

fn normalize_to_padding(
    centers: &mut HashMap<String, (f64, f64)>,
    sizes: &HashMap<String, (f64, f64)>,
    config: MindmapLayoutConfig,
) {
    let (min_x, min_y, _, _) = center_bounds(centers, sizes);
    let shift_x = config.padding - min_x;
    let shift_y = config.padding - min_y;
    let placed: Vec<(String, (f64, f64))> = centers.drain().collect();
    for (id, (cx, cy)) in placed {
        centers.insert(id, (cx + shift_x, cy + shift_y));
    }
}

/// 以 root 为中心重新平移所有节点，确保 root 位于画布几何中心。
///
/// 用于 TrueRadial 模式的最终居中：计算所有节点相对 root 的最大偏移（含节点尺寸），
/// 取对称半径，平移使 root 落在 (padding + radius, padding + radius) 即画布中心。
fn recenter_root(
    root_id: &str,
    centers: &mut HashMap<String, (f64, f64)>,
    sizes: &HashMap<String, (f64, f64)>,
    config: MindmapLayoutConfig,
) {
    let (root_cx, root_cy) = centers.get(root_id).copied().unwrap_or((0.0, 0.0));
    let mut max_dx: f64 = 0.0;
    let mut max_dy: f64 = 0.0;
    for (id, (cx, cy)) in centers.iter() {
        let (w, h) = sizes.get(id).copied().unwrap_or((150.0, 48.0));
        max_dx = max_dx.max((cx - root_cx).abs() + w / 2.0);
        max_dy = max_dy.max((cy - root_cy).abs() + h / 2.0);
    }
    let shift_x = config.padding + max_dx - root_cx;
    let shift_y = config.padding + max_dy - root_cy;
    for (_, (cx, cy)) in centers.iter_mut() {
        *cx += shift_x;
        *cy += shift_y;
    }
}

fn layout_horizontal_subtree(
    node_id: &str,
    depth: usize,
    direction: f64,
    y_cursor: &mut f64,
    children: &HashMap<String, Vec<String>>,
    sizes: &HashMap<String, (f64, f64)>,
    centers: &mut HashMap<String, (f64, f64)>,
    config: MindmapLayoutConfig,
    root_w: f64,
) -> f64 {
    let (_, h) = sizes.get(node_id).copied().unwrap_or((150.0, 48.0));
    let kids = children.get(node_id).map(|v| v.as_slice()).unwrap_or(&[]);

    if kids.is_empty() {
        let cy = *y_cursor + h / 2.0;
        let cx = direction * horizontal_depth_x(depth, root_w, config);
        centers.insert(node_id.to_string(), (cx, cy));
        *y_cursor += h + config.node_gap;
        return h;
    }

    let start_y = *y_cursor;
    let mut subtree_height = 0.0;
    for kid in kids {
        let kid_h = layout_horizontal_subtree(
            kid, depth + 1, direction, y_cursor, children, sizes, centers, config,
            root_w,
        );
        subtree_height += kid_h + config.node_gap;
    }
    subtree_height -= config.node_gap;

    let cy = start_y + subtree_height / 2.0;
    let cx = direction * horizontal_depth_x(depth, root_w, config);
    centers.insert(node_id.to_string(), (cx, cy));
    subtree_height
}

/// 计算水平径向布局中指定深度的 x 坐标偏移
fn horizontal_depth_x(depth: usize, root_w: f64, config: MindmapLayoutConfig) -> f64 {
    config.center_gap
        + root_w / 2.0
        + (depth as f64 - 1.0) * config.level_gap
        + config.level_gap / 2.0
}

fn layout_directional_tree(
    diagram: &Diagram,
    root_id: &str,
    children: &HashMap<String, Vec<String>>,
    sizes: &HashMap<String, (f64, f64)>,
    centers: &mut HashMap<String, (f64, f64)>,
    horizontal: bool,
    config: MindmapLayoutConfig,
) {
    let level_sizes = compute_level_max_sizes(root_id, children, sizes, horizontal);
    let level_centers = compute_level_center_offsets(&level_sizes, config);

    let mut cursor = 0.0;
    layout_tree_subtree(
        root_id,
        0,
        &mut cursor,
        children,
        sizes,
        centers,
        horizontal,
        &level_centers,
        config,
    );

    normalize_to_padding(centers, sizes, config);

    let _ = diagram;
}

fn compute_level_max_sizes(
    root_id: &str,
    children: &HashMap<String, Vec<String>>,
    sizes: &HashMap<String, (f64, f64)>,
    horizontal: bool,
) -> Vec<f64> {
    let mut level_sizes: Vec<f64> = Vec::new();
    let mut queue = std::collections::VecDeque::new();
    queue.push_back((root_id.to_string(), 0));

    while let Some((node_id, depth)) = queue.pop_front() {
        let (w, h) = sizes.get(&node_id).copied().unwrap_or((150.0, 48.0));
        let size = if horizontal { w } else { h };

        if depth >= level_sizes.len() {
            level_sizes.resize(depth + 1, 0.0);
        }
        level_sizes[depth] = level_sizes[depth].max(size);

        if let Some(kids) = children.get(&node_id) {
            for kid in kids {
                queue.push_back((kid.clone(), depth + 1));
            }
        }
    }

    level_sizes
}

fn compute_level_center_offsets(level_sizes: &[f64], config: MindmapLayoutConfig) -> Vec<f64> {
    let mut offsets = Vec::with_capacity(level_sizes.len());
    let mut cursor = 0.0;

    for &size in level_sizes {
        offsets.push(cursor + size / 2.0);
        cursor += size + config.level_gap;
    }

    offsets
}

fn layout_tree_subtree(
    node_id: &str,
    depth: usize,
    cursor: &mut f64,
    children: &HashMap<String, Vec<String>>,
    sizes: &HashMap<String, (f64, f64)>,
    centers: &mut HashMap<String, (f64, f64)>,
    horizontal: bool,
    level_centers: &[f64],
    config: MindmapLayoutConfig,
) -> f64 {
    let (w, h) = sizes.get(node_id).copied().unwrap_or((150.0, 48.0));
    let kids = children.get(node_id).map(|v| v.as_slice()).unwrap_or(&[]);

    let primary_center = level_centers.get(depth).copied().unwrap_or(0.0);

    if kids.is_empty() {
        let (cx, cy) = if horizontal {
            (primary_center, *cursor + h / 2.0)
        } else {
            (*cursor + w / 2.0, primary_center)
        };
        centers.insert(node_id.to_string(), (cx, cy));
        let span = if horizontal { h } else { w };
        *cursor += span + config.branch_gap;
        return span;
    }

    let start = *cursor;
    let mut subtree_span = 0.0;
    for kid in kids {
        let kid_span = layout_tree_subtree(
            kid,
            depth + 1,
            cursor,
            children,
            sizes,
            centers,
            horizontal,
            level_centers,
            config,
        );
        subtree_span += kid_span + config.branch_gap;
    }
    subtree_span -= config.branch_gap;

    let (cx, cy) = if horizontal {
        (primary_center, start + subtree_span / 2.0)
    } else {
        (start + subtree_span / 2.0, primary_center)
    };
    centers.insert(node_id.to_string(), (cx, cy));
    subtree_span
}

fn place_disconnected_nodes(
    diagram: &Diagram,
    children: &HashMap<String, Vec<String>>,
    root_id: &str,
    sizes: &HashMap<String, (f64, f64)>,
    centers: &mut HashMap<String, (f64, f64)>,
    mode: MindmapMode,
    config: MindmapLayoutConfig,
) {
    let connected: std::collections::HashSet<String> = {
        let mut set = std::collections::HashSet::new();
        set.insert(root_id.to_string());
        collect_descendants(root_id, children, &mut set);
        set
    };

    let mut y = centers
        .values()
        .map(|(_, cy)| *cy)
        .fold(0.0_f64, f64::max)
        + config.level_gap;

    for entity in &diagram.entities {
        let id = entity.id.as_str().to_string();
        if centers.contains_key(&id) || connected.contains(&id) {
            continue;
        }
        let (w, h) = sizes.get(&id).copied().unwrap_or((150.0, 48.0));
        let (cx, cy) = match mode {
            MindmapMode::LeftToRight => (config.padding + w / 2.0, y + h / 2.0),
            _ => (config.padding + w / 2.0, y + h / 2.0),
        };
        centers.insert(id, (cx, cy));
        y = cy + h / 2.0 + config.node_gap;
    }
}

fn collect_descendants(
    node: &str,
    children: &HashMap<String, Vec<String>>,
    visited: &mut std::collections::HashSet<String>,
) {
    if let Some(kids) = children.get(node) {
        for kid in kids {
            if visited.insert(kid.clone()) {
                collect_descendants(kid, children, visited);
            }
        }
    }
}

fn center_bounds(
    centers: &HashMap<String, (f64, f64)>,
    sizes: &HashMap<String, (f64, f64)>,
) -> (f64, f64, f64, f64) {
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;

    for (id, (cx, cy)) in centers {
        let (w, h) = sizes.get(id).copied().unwrap_or((150.0, 48.0));
        min_x = min_x.min(cx - w / 2.0);
        min_y = min_y.min(cy - h / 2.0);
        max_x = max_x.max(cx + w / 2.0);
        max_y = max_y.max(cy + h / 2.0);
    }

    if min_x == f64::MAX {
        (0.0, 0.0, 0.0, 0.0)
    } else {
        (min_x, min_y, max_x, max_y)
    }
}

fn bounds_from_nodes(nodes: &HashMap<String, NodeLayout>, config: MindmapLayoutConfig) -> (f64, f64) {
    let mut max_x = 0.0_f64;
    let mut max_y = 0.0_f64;
    for nl in nodes.values() {
        max_x = max_x.max(nl.x + nl.width);
        max_y = max_y.max(nl.y + nl.height);
    }
    (max_x + config.padding, max_y + config.padding)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        ArrowType, AttributeMap, AttributeValue, Diagram, DiagramAttribute, Entity, Identifier,
        Relation, SourceInfo, Span, TextValue,
    };
    use crate::layout::constants;

    fn span() -> Span {
        Span::dummy()
    }

    fn entity(id: &str, ty: &str) -> Entity {
        let mut attrs = AttributeMap::default();
        attrs
            .standard
            .insert("type".to_string(), AttributeValue::String(TextValue::unquoted(ty.to_string())));
        Entity {
            id: Identifier::new_unchecked(id),
            label: id.to_string(),
            attributes: attrs,
            group_id: None,
            span: span(),
        }
    }

    fn relation(from: &str, to: &str) -> Relation {
        Relation {
            from: Identifier::new_unchecked(from),
            to: Identifier::new_unchecked(to),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span: span(),
        }
    }

    fn mindmap_diagram(
        entities: Vec<Entity>,
        relations: Vec<Relation>,
        layout: Option<&str>,
    ) -> Diagram {
        let mut attributes = Vec::new();
        if let Some(value) = layout {
            attributes.push(DiagramAttribute {
                key: "direction".to_string(),
                value: AttributeValue::String(TextValue::unquoted(value.to_string())),
                span: span(),
            });
        }
        Diagram {
            diagram_type: DiagramType::Mindmap,
            attributes,
            entities,
            relations,
            groups: Vec::new(),
            style_decls: vec![],
            source_info: SourceInfo {
                file: None,
                line_count: 1,
            },
            ..Default::default()
        }
    }

    #[test]
    fn radial_places_root_between_left_and_right_branches() {
        // 2 个分支：使用旧 Radial 水平双向布局（左一个右一个）
        let diagram = mindmap_diagram(
            vec![
                entity("root", "root"),
                entity("a", "main"),
                entity("b", "main"),
            ],
            vec![
                relation("root", "a"),
                relation("root", "b"),
            ],
            None,
        );

        let result = MindmapLayout::default().compute(&diagram);
        let root = result.nodes.get("root").unwrap();
        let a = result.nodes.get("a").unwrap();
        let b = result.nodes.get("b").unwrap();

        let root_cx = root.x + root.width / 2.0;
        let a_cx = a.x + a.width / 2.0;
        let b_cx = b.x + b.width / 2.0;

        assert!(a_cx > root_cx, "first branch should be on the right");
        assert!(b_cx < root_cx, "second branch should be on the left");
        assert_eq!(result.nodes.len(), 3);

        for nl in result.nodes.values() {
            assert!(nl.x >= constants::MINDMAP_PADDING - 0.1, "node x should stay inside canvas");
            assert!(nl.y >= constants::MINDMAP_PADDING - 0.1, "node y should stay inside canvas");
        }
    }

    #[test]
    fn true_radial_centers_root_with_three_branches() {
        // 3 个分支：启用 TrueRadial 极坐标径向布局，root 在画布中心
        let diagram = mindmap_diagram(
            vec![
                entity("root", "root"),
                entity("a", "main"),
                entity("b", "main"),
                entity("c", "main"),
            ],
            vec![
                relation("root", "a"),
                relation("root", "b"),
                relation("root", "c"),
            ],
            None,
        );

        let result = MindmapLayout::default().compute(&diagram);
        let root = result.nodes.get("root").unwrap();
        let root_cx = root.x + root.width / 2.0;
        let root_cy = root.y + root.height / 2.0;

        // root 应在画布几何中心（允许 padding 级别的误差）
        let canvas_cx = result.total_width / 2.0;
        let canvas_cy = result.total_height / 2.0;
        assert!(
            (root_cx - canvas_cx).abs() < 5.0,
            "root cx {root_cx} should be near canvas center {canvas_cx}"
        );
        assert!(
            (root_cy - canvas_cy).abs() < 5.0,
            "root cy {root_cy} should be near canvas center {canvas_cy}"
        );
        assert_eq!(result.nodes.len(), 4);

        for nl in result.nodes.values() {
            assert!(nl.x >= constants::MINDMAP_PADDING - 0.1, "node x should stay inside canvas");
            assert!(nl.y >= constants::MINDMAP_PADDING - 0.1, "node y should stay inside canvas");
        }
    }

    #[test]
    fn top_to_bottom_places_root_above_children() {
        let diagram = mindmap_diagram(
            vec![
                entity("root", "root"),
                entity("a", "main"),
                entity("b", "main"),
            ],
            vec![relation("root", "a"), relation("root", "b")],
            Some("top-to-bottom"),
        );

        let result = MindmapLayout::default().compute(&diagram);
        let root = result.nodes.get("root").unwrap();
        let a = result.nodes.get("a").unwrap();

        assert!(root.y + root.height <= a.y + 1.0);
    }

    #[test]
    fn left_to_right_places_root_before_children() {
        let diagram = mindmap_diagram(
            vec![
                entity("root", "root"),
                entity("a", "main"),
                entity("b", "main"),
            ],
            vec![relation("root", "a"), relation("root", "b")],
            Some("left-to-right"),
        );

        let result = MindmapLayout::default().compute(&diagram);
        let root = result.nodes.get("root").unwrap();
        let a = result.nodes.get("a").unwrap();

        assert!(root.x + root.width <= a.x + 1.0);
    }

    #[test]
    fn left_to_right_same_depth_nodes_are_vertically_aligned() {
        let diagram = mindmap_diagram(
            vec![
                entity("root", "root"),
                entity("frontend", "main"),
                entity("backend", "main"),
                entity("devops", "main"),
                entity("react", "leaf"),
                entity("wasm", "leaf"),
                entity("rust", "leaf"),
                entity("postgres", "leaf"),
                entity("docker", "leaf"),
                entity("kubernetes", "leaf"),
            ],
            vec![
                relation("root", "frontend"),
                relation("root", "backend"),
                relation("root", "devops"),
                relation("frontend", "react"),
                relation("frontend", "wasm"),
                relation("backend", "rust"),
                relation("backend", "postgres"),
                relation("devops", "docker"),
                relation("devops", "kubernetes"),
            ],
            Some("left-to-right"),
        );

        let result = MindmapLayout::default().compute(&diagram);

        let depth1_ids = vec!["frontend", "backend", "devops"];
        let depth1_xs: Vec<f64> = depth1_ids
            .iter()
            .map(|id| {
                let n = result.nodes.get(*id).unwrap();
                n.x + n.width / 2.0
            })
            .collect();

        let x1 = depth1_xs[0];
        for &x in &depth1_xs[1..] {
            assert!(
                (x - x1).abs() < 0.01,
                "depth-1 nodes should have same center x, but got {:?}",
                depth1_xs
            );
        }

        let depth2_ids = vec!["react", "wasm", "rust", "postgres", "docker", "kubernetes"];
        let depth2_xs: Vec<f64> = depth2_ids
            .iter()
            .map(|id| {
                let n = result.nodes.get(*id).unwrap();
                n.x + n.width / 2.0
            })
            .collect();

        let x2 = depth2_xs[0];
        for &x in &depth2_xs[1..] {
            assert!(
                (x - x2).abs() < 0.01,
                "depth-2 nodes should have same center x, but got {:?}",
                depth2_xs
            );
        }
    }

    #[test]
    fn top_to_bottom_same_depth_nodes_are_horizontally_aligned() {
        let diagram = mindmap_diagram(
            vec![
                entity("root", "root"),
                entity("a", "main"),
                entity("b", "main"),
                entity("a1", "leaf"),
                entity("a2", "leaf"),
                entity("b1", "leaf"),
                entity("b2", "leaf"),
            ],
            vec![
                relation("root", "a"),
                relation("root", "b"),
                relation("a", "a1"),
                relation("a", "a2"),
                relation("b", "b1"),
                relation("b", "b2"),
            ],
            Some("top-to-bottom"),
        );

        let result = MindmapLayout::default().compute(&diagram);

        let depth1_ids = vec!["a", "b"];
        let depth1_ys: Vec<f64> = depth1_ids
            .iter()
            .map(|id| {
                let n = result.nodes.get(*id).unwrap();
                n.y + n.height / 2.0
            })
            .collect();

        let y1 = depth1_ys[0];
        for &y in &depth1_ys[1..] {
            assert!(
                (y - y1).abs() < 0.01,
                "depth-1 nodes should have same center y, but got {:?}",
                depth1_ys
            );
        }

        let depth2_ids = vec!["a1", "a2", "b1", "b2"];
        let depth2_ys: Vec<f64> = depth2_ids
            .iter()
            .map(|id| {
                let n = result.nodes.get(*id).unwrap();
                n.y + n.height / 2.0
            })
            .collect();

        let y2 = depth2_ys[0];
        for &y in &depth2_ys[1..] {
            assert!(
                (y - y2).abs() < 0.01,
                "depth-2 nodes should have same center y, but got {:?}",
                depth2_ys
            );
        }
    }
}
