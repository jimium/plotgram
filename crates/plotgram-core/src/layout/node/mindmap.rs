//! Mindmap（思维导图）专属布局算法
//!
//! 支持三种展开方向：
//! - `left-to-right`（默认）：中心主题在左侧，树形向右展开
//! - `radial`：中心主题居中，一级分支左右交替辐射
//! - `top-to-bottom`：中心主题在上方，树形向下展开

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
            MindmapNodeKind::Root => (64.0, 100.0, 64.0, 24.0, 13.5),
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
    /// 水平双向布局（左右交替）
    Radial,
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
        let mut centers: HashMap<String, (f64, f64)> = HashMap::new();
        let mut sizes: HashMap<String, (f64, f64)> = HashMap::new();

        for entity in &diagram.entities {
            sizes.insert(entity.id.as_str().to_string(), node_size(diagram, entity));
        }

        match mode {
            MindmapMode::Radial => {
                layout_radial(diagram, &root_id, &children, &sizes, &mut centers, config)
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

        // 推开后可能越出画布，再次归一化到 padding
        normalize_to_padding(&mut centers, &sizes, config);

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

        let (total_width, total_height) = bounds_from_nodes(&nodes, config);
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
        // radial / 未知值均回退 radial
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
    // 按各层最大节点宽累加 x，避免 level_gap < 半宽之和时父子水平重叠
    let depth_xs = compute_radial_depth_xs(root_id, children, sizes, root_w, config);

    // 按子树权重贪心分配左右，两侧独立纵向堆叠后再对齐到 root
    let directions = assign_radial_sides(&root_children, children);
    let mut right_ids = Vec::new();
    let mut left_ids = Vec::new();
    for (child_id, dir) in root_children.iter().zip(directions.iter()) {
        if *dir > 0.0 {
            right_ids.push(child_id.clone());
        } else {
            left_ids.push(child_id.clone());
        }
    }

    let right_span = layout_radial_side(
        &right_ids, 1.0, children, sizes, centers, config, &depth_xs,
    );
    let left_span = layout_radial_side(
        &left_ids, -1.0, children, sizes, centers, config, &depth_xs,
    );

    // 两侧各自居中到 y=0，root 落在原点
    recenter_side_vertical(&right_ids, children, centers, right_span);
    recenter_side_vertical(&left_ids, children, centers, left_span);
    centers.insert(root_id.to_string(), (0.0, 0.0));

    normalize_to_padding(centers, sizes, config);
}

/// 计算径向布局各深度的 |x| 中心坐标（从 root 向外累加）。
///
/// depth 0 = 0；depth d 的中心 = 上一层右缘 + level_gap + 本层半宽。
fn compute_radial_depth_xs(
    root_id: &str,
    children: &HashMap<String, Vec<String>>,
    sizes: &HashMap<String, (f64, f64)>,
    root_w: f64,
    config: MindmapLayoutConfig,
) -> Vec<f64> {
    let level_widths = compute_level_max_sizes(root_id, children, sizes, true);
    if level_widths.is_empty() {
        return vec![0.0];
    }

    let mut xs = vec![0.0]; // depth 0 = root
    // depth 1：root 半宽 + center_gap + 本层半宽
    if level_widths.len() > 1 {
        xs.push(root_w / 2.0 + config.center_gap + level_widths[1] / 2.0);
    }
    for depth in 2..level_widths.len() {
        let prev = xs[depth - 1];
        let prev_half = level_widths[depth - 1] / 2.0;
        let cur_half = level_widths[depth] / 2.0;
        xs.push(prev + prev_half + config.level_gap + cur_half);
    }
    xs
}

/// 按子树叶子数贪心分配左右侧，保持声明顺序在同侧内的相对次序。
///
/// 每次把下一个分支放到当前累计权重更轻的一侧；平局时优先右侧（与经典
/// 思维导图「先右后左」阅读习惯一致）。
fn assign_radial_sides(
    root_children: &[String],
    children: &HashMap<String, Vec<String>>,
) -> Vec<f64> {
    let mut directions = Vec::with_capacity(root_children.len());
    let mut right_weight = 0.0_f64;
    let mut left_weight = 0.0_f64;

    for child_id in root_children {
        let w = subtree_leaf_count(child_id, children) as f64;
        if right_weight <= left_weight {
            directions.push(1.0);
            right_weight += w;
        } else {
            directions.push(-1.0);
            left_weight += w;
        }
    }
    directions
}

fn subtree_leaf_count(node_id: &str, children: &HashMap<String, Vec<String>>) -> usize {
    match children.get(node_id) {
        Some(kids) if !kids.is_empty() => {
            kids.iter().map(|kid| subtree_leaf_count(kid, children)).sum()
        }
        _ => 1,
    }
}

/// 在单侧独立堆叠一级分支及其子树，返回该侧总高度跨度。
fn layout_radial_side(
    branch_ids: &[String],
    direction: f64,
    children: &HashMap<String, Vec<String>>,
    sizes: &HashMap<String, (f64, f64)>,
    centers: &mut HashMap<String, (f64, f64)>,
    config: MindmapLayoutConfig,
    depth_xs: &[f64],
) -> f64 {
    if branch_ids.is_empty() {
        return 0.0;
    }

    let mut y_cursor = 0.0;
    let cluster_gap = config.node_gap * 1.6;
    for (i, child_id) in branch_ids.iter().enumerate() {
        layout_horizontal_subtree(
            child_id, 1, direction, &mut y_cursor, children, sizes, centers, config,
            depth_xs,
        );
        if i + 1 < branch_ids.len() {
            y_cursor += cluster_gap;
        }
    }
    y_cursor
}

/// 将一侧已放置的节点整体平移，使该侧包围盒垂直居中于 y=0。
fn recenter_side_vertical(
    branch_ids: &[String],
    children: &HashMap<String, Vec<String>>,
    centers: &mut HashMap<String, (f64, f64)>,
    side_span: f64,
) {
    if branch_ids.is_empty() || side_span <= 0.0 {
        return;
    }
    let shift_y = -side_span / 2.0;
    let mut stack: Vec<String> = branch_ids.to_vec();
    let mut visited = std::collections::HashSet::new();
    while let Some(id) = stack.pop() {
        if !visited.insert(id.clone()) {
            continue;
        }
        if let Some((_, cy)) = centers.get_mut(&id) {
            *cy += shift_y;
        }
        if let Some(kids) = children.get(&id) {
            stack.extend(kids.iter().cloned());
        }
    }
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

fn layout_horizontal_subtree(
    node_id: &str,
    depth: usize,
    direction: f64,
    y_cursor: &mut f64,
    children: &HashMap<String, Vec<String>>,
    sizes: &HashMap<String, (f64, f64)>,
    centers: &mut HashMap<String, (f64, f64)>,
    config: MindmapLayoutConfig,
    depth_xs: &[f64],
) -> f64 {
    let (_, h) = sizes.get(node_id).copied().unwrap_or((150.0, 48.0));
    let kids = children.get(node_id).map(|v| v.as_slice()).unwrap_or(&[]);
    let cx = direction * depth_xs.get(depth).copied().unwrap_or(0.0);

    if kids.is_empty() {
        let cy = *y_cursor + h / 2.0;
        centers.insert(node_id.to_string(), (cx, cy));
        *y_cursor += h;
        return h;
    }

    let start_y = *y_cursor;
    let mut subtree_height = 0.0;
    for (i, kid) in kids.iter().enumerate() {
        let kid_h = layout_horizontal_subtree(
            kid, depth + 1, direction, y_cursor, children, sizes, centers, config,
            depth_xs,
        );
        subtree_height += kid_h;
        if i + 1 < kids.len() {
            *y_cursor += config.node_gap;
            subtree_height += config.node_gap;
        }
    }

    let cy = start_y + subtree_height / 2.0;
    centers.insert(node_id.to_string(), (cx, cy));
    subtree_height
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
        *cursor += span;
        return span;
    }

    let start = *cursor;
    let mut subtree_span = 0.0;
    for (i, kid) in kids.iter().enumerate() {
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
        subtree_span += kid_span;
        if i + 1 < kids.len() {
            // 根下一级分支之间略加大间距，形成视觉分组
            let gap = if depth == 0 {
                config.branch_gap * 1.75
            } else {
                config.branch_gap
            };
            *cursor += gap;
            subtree_span += gap;
        }
    }

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
            constraints: vec![],
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
            Some("radial"),
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
    fn radial_alternates_three_branches_horizontally() {
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
            Some("radial"),
        );

        let result = MindmapLayout::default().compute(&diagram);
        let root = result.nodes.get("root").unwrap();
        let a = result.nodes.get("a").unwrap();
        let b = result.nodes.get("b").unwrap();
        let c = result.nodes.get("c").unwrap();

        let root_cx = root.x + root.width / 2.0;
        let a_cx = a.x + a.width / 2.0;
        let b_cx = b.x + b.width / 2.0;
        let c_cx = c.x + c.width / 2.0;

        assert!(a_cx > root_cx, "first branch should be on the right");
        assert!(b_cx < root_cx, "second branch should be on the left");
        assert!(c_cx > root_cx, "third branch should be on the right");
        assert_eq!(result.nodes.len(), 4);
    }

    #[test]
    fn default_direction_is_left_to_right() {
        let diagram = mindmap_diagram(
            vec![
                entity("root", "root"),
                entity("a", "main"),
                entity("b", "main"),
            ],
            vec![relation("root", "a"), relation("root", "b")],
            None,
        );

        let result = MindmapLayout::default().compute(&diagram);
        let root = result.nodes.get("root").unwrap();
        let a = result.nodes.get("a").unwrap();

        assert!(root.x + root.width <= a.x + 1.0);
    }

    #[test]
    fn root_node_is_compact_circle() {
        let diagram = mindmap_diagram(
            vec![entity("root", "root")],
            vec![],
            None,
        );

        let result = MindmapLayout::default().compute(&diagram);
        let root = result.nodes.get("root").unwrap();

        assert!(root.width <= 100.0, "root width should stay compact");
        assert!(root.height <= 100.0, "root height should stay compact");
        assert!((root.width - root.height).abs() < 0.1, "root should be square");
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

    #[test]
    fn radial_balances_left_and_right_independently() {
        // 4 个等权分支：应左右各 2，且两侧都跨越 root 的上下（独立堆叠后居中）
        let diagram = mindmap_diagram(
            vec![
                entity("root", "root"),
                entity("a", "main"),
                entity("b", "main"),
                entity("c", "main"),
                entity("d", "main"),
                entity("a1", "leaf"),
                entity("b1", "leaf"),
                entity("c1", "leaf"),
                entity("d1", "leaf"),
            ],
            vec![
                relation("root", "a"),
                relation("root", "b"),
                relation("root", "c"),
                relation("root", "d"),
                relation("a", "a1"),
                relation("b", "b1"),
                relation("c", "c1"),
                relation("d", "d1"),
            ],
            Some("radial"),
        );

        let result = MindmapLayout::default().compute(&diagram);
        let root = result.nodes.get("root").unwrap();
        let root_cx = root.x + root.width / 2.0;
        let root_cy = root.y + root.height / 2.0;

        let mut left = 0;
        let mut right = 0;
        for id in ["a", "b", "c", "d"] {
            let n = result.nodes.get(id).unwrap();
            let cx = n.x + n.width / 2.0;
            if cx < root_cx {
                left += 1;
            } else {
                right += 1;
            }
        }
        assert_eq!(left, 2, "should place two branches on the left");
        assert_eq!(right, 2, "should place two branches on the right");

        let main_cys: Vec<f64> = ["a", "b", "c", "d"]
            .iter()
            .map(|id| {
                let n = result.nodes.get(*id).unwrap();
                n.y + n.height / 2.0
            })
            .collect();
        let above = main_cys.iter().filter(|&&y| y < root_cy - 1.0).count();
        let below = main_cys.iter().filter(|&&y| y > root_cy + 1.0).count();
        assert!(above >= 1, "expected at least one branch above root, got {:?}", main_cys);
        assert!(below >= 1, "expected at least one branch below root, got {:?}", main_cys);
    }

    #[test]
    fn radial_parent_child_do_not_horizontally_overlap() {
        let diagram = mindmap_diagram(
            vec![
                entity("root", "root"),
                entity("main", "main"),
                entity("leaf1", "leaf"),
                entity("leaf2", "leaf"),
            ],
            vec![
                relation("root", "main"),
                relation("main", "leaf1"),
                relation("main", "leaf2"),
            ],
            Some("radial"),
        );

        let result = MindmapLayout::default().compute(&diagram);
        let main = result.nodes.get("main").unwrap();
        let leaf1 = result.nodes.get("leaf1").unwrap();

        let main_right = main.x + main.width;
        let main_left = main.x;
        let leaf_right = leaf1.x + leaf1.width;
        let leaf_left = leaf1.x;

        let overlap = main_left < leaf_right && main_right > leaf_left;
        assert!(
            !overlap,
            "main [{:.1},{:.1}] should not overlap leaf [{:.1},{:.1}]",
            main_left, main_right, leaf_left, leaf_right
        );
    }
}
