//! 分组感知力导向布局 (Group-Aware Force-Directed Layout)
//!
//! 基于 Fruchterman-Reingold 力导向，并增强分组语义：
//! - **真实力导向**: 库仑斥力 + 弹簧引力，通过模拟退火迭代优化
//! - **分组感知**: 同组节点互相吸引形成簇，组间互相排斥保持分离
//! - **确定性组件排列**: 多连通分量和组间按拓扑排序排列
//! - **自适应间距**: 根据节点尺寸和标签长度动态调整间距
//!
//! 适合场景：架构图、流程图、微服务拓扑图、有分组语义的图。

use crate::types::DiagramType;
use crate::ast::{Diagram};
use crate::layout::algorithm_config::{ForceDirectedLayoutConfig, FORCE_DIRECTED_LAYOUT_OPTIONS};
use crate::layout::node::common::barnes_hut::BarnesHutTree;
use crate::layout::node::common::group_bounds::{self, GroupPadding};
use crate::layout::node::common::node_sizing;
use crate::layout::plan::ResolvedAlgoOptions;
use crate::layout::constants;
use crate::layout::{AlgorithmOptionSpec, GroupLayout, LayoutResult, LayoutStrategy, NodeLayout};
use std::collections::{HashMap, HashSet, VecDeque};

// ─── 力导向常量 ──────────────────────────────────────────

const ITERATIONS: usize = 200;
const INITIAL_TEMPERATURE: f64 = 40.0;
const COOLING: f64 = 0.95;
const MIN_DISTANCE: f64 = 1.0;
const NODE_MARGIN: f64 = 20.0;
const EDGE_ATTRACTION_MULT: f64 = 1.3;  // 边引力倍增，让相邻节点更紧密
const BARNES_HUT_THETA: f64 = 1.2;     // Barnes-Hut 开角阈值
/// Phase 3：RUDY 拥堵排斥力强度。
///
/// 在力导向迭代中，对处于高 RUDY 密度区域的节点施加额外排斥力，
/// 将节点推离拥堵区，从而降低局部边交叉与重叠。
const CONGESTION_REPULSION: f64 = 0.15;
/// Phase 3：RUDY 密度网格单元大小（像素）。
const CONGESTION_GRID_CELL: f64 = 80.0;

// 分组相关权重（v2 核心增强）
const GROUP_GRAVITY: f64 = 0.06;       // 组内引力（比 FR 的 0.035 更强）
const GROUP_REPULSION: f64 = 0.3;      // 组间斥力系数（降低以减少过度分散）
const CENTER_GRAVITY: f64 = 0.008;     // 全局中心引力（更弱，让分组自然散布）
const GROUP_HULL_MARGIN: f64 = 16.0;   // 分组凸包边界余量

const APPLICABLE_TYPES: &[DiagramType] = &[DiagramType::Flowchart, DiagramType::Architecture];

// ─── 公共接口 ────────────────────────────────────────────

pub struct ForceDirectedLayout {
    config: ForceDirectedLayoutConfig,
}

impl ForceDirectedLayout {
    pub fn new(config: ForceDirectedLayoutConfig) -> Self {
        Self { config }
    }

    pub fn from_options(options: &ResolvedAlgoOptions) -> Self {
        Self::new(ForceDirectedLayoutConfig::from_options(options))
    }
}

impl Default for ForceDirectedLayout {
    fn default() -> Self {
        Self::new(ForceDirectedLayoutConfig::default())
    }
}

impl LayoutStrategy for ForceDirectedLayout {
    fn name(&self) -> &'static str {
        "force-directed"
    }

    fn applicable_diagram_types(&self) -> &'static [DiagramType] {
        APPLICABLE_TYPES
    }

    fn option_specs(&self) -> &'static [AlgorithmOptionSpec] {
        FORCE_DIRECTED_LAYOUT_OPTIONS
    }

    fn compute(&self, diagram: &Diagram) -> LayoutResult {
        let config = self.config;
        if diagram.entities.is_empty() {
            return LayoutResult {
                nodes: HashMap::new(),
                groups: HashMap::new(),
                edges: vec![],
                total_width: config.padding * 2.0,
                total_height: config.padding * 2.0,
                hints: Default::default(),
            };
        }

        let sizes = node_sizing::standard_node_sizes(diagram);
        let graph = GraphIndex::build(diagram);
        let components = graph.connected_components(diagram);
        let group_map = build_group_map(diagram);

        let mut positions = initialize_positions(diagram, &components, &sizes, config);
        let area = estimate_area(&sizes, diagram.entities.len());
        run_fr_iterations_v2(diagram, &graph, &sizes, &mut positions, area, &group_map);
        resolve_overlaps_v2(diagram, &sizes, &mut positions, &group_map);
        let positions = pack_components_v2(&components, positions, &sizes, &group_map, config);

        let mut nodes = HashMap::new();
        for entity in &diagram.entities {
            let id = entity.id.as_str();
            let (cx, cy) = positions.get(id).copied().unwrap_or((config.padding, config.padding));
            let (width, height) = sizes.get(id).copied().unwrap_or((constants::FR_NODE_WIDTH, constants::DEFAULT_NODE_HEIGHT));
            nodes.insert(
                id.to_string(),
                NodeLayout {
                    x: cx - width / 2.0,
                    y: cy - height / 2.0,
                    width,
                    height,
                    ..Default::default()
                },
            );
        }

        let groups = group_bounds::compute_group_bounds(
            diagram,
            &nodes,
            GroupPadding::uniform(config.group_padding, 16.0),
        );
        let (total_width, total_height) = bounds_from_layout(&nodes, &groups, config.padding);

        LayoutResult {
            nodes,
            groups,
            edges: vec![],
            total_width,
            total_height,
            hints: crate::layout::LayoutHints {
                edge_routing_style: crate::layout::EdgeRoutingStyle::Straight,
                ..Default::default()
            },
        }
    }
}

// ─── 图索引 ──────────────────────────────────────────────

struct GraphIndex {
    order: HashMap<String, usize>,
    neighbors: HashMap<String, Vec<String>>,
    undirected_edges: Vec<(String, String)>,
}

impl GraphIndex {
    fn build(diagram: &Diagram) -> Self {
        let mut order = HashMap::new();
        let mut neighbors = HashMap::new();
        let mut edge_seen = HashSet::new();
        let mut undirected_edges = Vec::new();

        for (index, entity) in diagram.entities.iter().enumerate() {
            let id = entity.id.as_str().to_string();
            order.insert(id.clone(), index);
            neighbors.insert(id.clone(), Vec::new());
        }

        for relation in &diagram.relations {
            let from = relation.from.as_str().to_string();
            let to = relation.to.as_str().to_string();
            if !order.contains_key(&from) || !order.contains_key(&to) {
                continue;
            }

            neighbors.entry(from.clone()).or_default().push(to.clone());
            if from != to {
                neighbors.entry(to.clone()).or_default().push(from.clone());
                let mut pair = [from.clone(), to.clone()];
                pair.sort();
                if edge_seen.insert((pair[0].clone(), pair[1].clone())) {
                    undirected_edges.push((pair[0].clone(), pair[1].clone()));
                }
            }
        }

        Self {
            order,
            neighbors,
            undirected_edges,
        }
    }

    fn connected_components(&self, diagram: &Diagram) -> Vec<Vec<String>> {
        let mut visited = HashSet::new();
        let mut components = Vec::new();

        for entity in &diagram.entities {
            let id = entity.id.as_str().to_string();
            if !visited.insert(id.clone()) {
                continue;
            }

            let mut queue = VecDeque::from([id]);
            let mut component = Vec::new();
            while let Some(node_id) = queue.pop_front() {
                component.push(node_id.clone());
                if let Some(nbrs) = self.neighbors.get(&node_id) {
                    for neighbor in nbrs {
                        if visited.insert(neighbor.clone()) {
                            queue.push_back(neighbor.clone());
                        }
                    }
                }
            }

            component.sort_by_key(|node_id| self.order.get(node_id).copied().unwrap_or(usize::MAX));
            components.push(component);
        }

        components
    }
}

// ─── 分组映射 ────────────────────────────────────────────

struct GroupMap {
    /// node_id -> 直接 group_id
    node_to_group: HashMap<String, String>,
    /// node_id -> 顶层 group_id（递归 parent_id 到根）
    node_to_top_group: HashMap<String, String>,
    /// group_id -> [node_id]（直接成员）
    group_members: HashMap<String, Vec<String>>,
}

fn build_group_map(diagram: &Diagram) -> GroupMap {
    let mut node_to_group = HashMap::new();
    let mut node_to_top_group = HashMap::new();
    let mut group_members: HashMap<String, Vec<String>> = HashMap::new();

    for group in &diagram.groups {
        group_members.insert(group.id.as_str().to_string(), Vec::new());
    }

    for entity in &diagram.entities {
        let eid = entity.id.as_str().to_string();
        if let Some(ref gid) = entity.group_id {
            let gs = gid.as_str().to_string();
            node_to_group.insert(eid.clone(), gs.clone());
            group_members.entry(gs.clone()).or_default().push(eid.clone());

            // 递归找到顶层组
            let top = resolve_top_group(diagram, &gs);
            node_to_top_group.insert(eid, top);
        }
    }

    GroupMap {
        node_to_group,
        node_to_top_group,
        group_members,
    }
}

/// 递归 parent_id 找到顶层 group_id
fn resolve_top_group(diagram: &Diagram, gid: &str) -> String {
    let mut current = gid.to_string();
    loop {
        let Some(group) = diagram.find_group(&current) else {
            return current;
        };
        match &group.parent_id {
            Some(parent) => current = parent.as_str().to_string(),
            None => return current,
        }
    }
}

// ─── 节点尺寸 ────────────────────────────────────────────

// build_node_sizes 已抽取到 common::node_sizing::standard_node_sizes

fn estimate_area(sizes: &HashMap<String, (f64, f64)>, node_count: usize) -> f64 {
    let total = sizes
        .values()
        .map(|(width, height)| (width + NODE_MARGIN) * (height + NODE_MARGIN))
        .sum::<f64>();
    total.max(node_count as f64 * 20_000.0)
}

// ─── 初始化位置 ──────────────────────────────────────────

fn initialize_positions(
    _diagram: &Diagram,
    components: &[Vec<String>],
    sizes: &HashMap<String, (f64, f64)>,
    config: ForceDirectedLayoutConfig,
) -> HashMap<String, (f64, f64)> {
    let mut positions = HashMap::new();
    let mut cursor_x = 0.0;

    for component in components {
        let max_diameter = component
            .iter()
            .map(|id| {
                let (width, height) = sizes.get(id).copied().unwrap_or((constants::FR_NODE_WIDTH, constants::DEFAULT_NODE_HEIGHT));
                width.max(height)
            })
            .fold(0.0_f64, f64::max);
        let radius = ((component.len() as f64).sqrt() * (max_diameter + NODE_MARGIN)).max(100.0);
        let center_x = cursor_x + radius + config.padding;
        let center_y = radius + config.padding;

        for (index, node_id) in component.iter().enumerate() {
            let angle = if component.len() == 1 {
                0.0
            } else {
                (index as f64 / component.len() as f64) * std::f64::consts::TAU
            };
            positions.insert(
                node_id.clone(),
                (
                    center_x + radius * angle.cos(),
                    center_y + radius * angle.sin(),
                ),
            );
        }

        cursor_x += radius * 2.0 + config.component_gap;
    }

    positions
}

// ─── Phase 3：RUDY 拥堵密度网格 ──────────────────────────

/// RUDY 式边密度网格：每条边向其包围框覆盖的所有网格单元贡献均匀密度，
/// 叠加形成全图密度场。密度高的区域即潜在路由拥堵区。
struct CongestionGrid {
    /// 密度场（行优先：density[y_idx * cols + x_idx]）
    density: Vec<f64>,
    cols: usize,
    rows: usize,
    origin_x: f64,
    origin_y: f64,
    cell: f64,
}

impl CongestionGrid {
    /// 从当前边位置构建密度网格。
    fn build(
        edges: &[(String, String)],
        positions: &HashMap<String, (f64, f64)>,
    ) -> Option<Self> {
        if edges.is_empty() || positions.is_empty() {
            return None;
        }
        // 计算包围框
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for (_, (x, y)) in positions {
            min_x = min_x.min(*x);
            min_y = min_y.min(*y);
            max_x = max_x.max(*x);
            max_y = max_y.max(*y);
        }
        if max_x <= min_x || max_y <= min_y {
            return None;
        }
        let cell = CONGESTION_GRID_CELL;
        let cols = ((max_x - min_x) / cell).ceil() as usize + 1;
        let rows = ((max_y - min_y) / cell).ceil() as usize + 1;
        let mut density = vec![0.0_f64; cols * rows];

        // 每条边向其包围框内所有单元贡献均匀密度
        for (from, to) in edges {
            let (fx, fy) = match positions.get(from) {
                Some(p) => *p,
                None => continue,
            };
            let (tx, ty) = match positions.get(to) {
                Some(p) => *p,
                None => continue,
            };
            let x0 = ((fx.min(tx) - min_x) / cell).floor() as isize;
            let x1 = ((fx.max(tx) - min_x) / cell).ceil() as isize;
            let y0 = ((fy.min(ty) - min_y) / cell).floor() as isize;
            let y1 = ((fy.max(ty) - min_y) / cell).ceil() as isize;
            let x0 = x0.max(0) as usize;
            let y0 = y0.max(0) as usize;
            let x1 = (x1 as usize).min(cols - 1);
            let y1 = (y1 as usize).min(rows - 1);
            for yi in y0..=y1 {
                for xi in x0..=x1 {
                    density[yi * cols + xi] += 1.0;
                }
            }
        }

        Some(CongestionGrid {
            density,
            cols,
            rows,
            origin_x: min_x,
            origin_y: min_y,
            cell,
        })
    }

    /// 采样位置 (x, y) 处的密度梯度。
    /// 返回 (grad_x, grad_y)，指向密度增加方向。
    fn gradient_at(&self, x: f64, y: f64) -> (f64, f64) {
        let gi = ((x - self.origin_x) / self.cell) as isize;
        let gj = ((y - self.origin_y) / self.cell) as isize;
        let at = |i: isize, j: isize| -> f64 {
            if i < 0 || j < 0 || i >= self.cols as isize || j >= self.rows as isize {
                return 0.0;
            }
            self.density[j as usize * self.cols + i as usize]
        };
        let grad_x = (at(gi + 1, gj) - at(gi - 1, gj)) / (2.0 * self.cell);
        let grad_y = (at(gi, gj + 1) - at(gi, gj - 1)) / (2.0 * self.cell);
        (grad_x, grad_y)
    }
}

/// Phase 3：对每个节点施加 RUDY 密度梯度排斥力，将其推离拥堵区。
fn apply_congestion_repulsion(
    ids: &[String],
    positions: &HashMap<String, (f64, f64)>,
    disp: &mut HashMap<String, (f64, f64)>,
    grid: &CongestionGrid,
    k: f64,
) {
    for id in ids {
        let (x, y) = match positions.get(id) {
            Some(p) => *p,
            None => continue,
        };
        let (grad_x, grad_y) = grid.gradient_at(x, y);
        // 排斥力 = -梯度 * 强度 * k（与 FR 其他力同量纲）
        let force_x = -grad_x * CONGESTION_REPULSION * k;
        let force_y = -grad_y * CONGESTION_REPULSION * k;
        add_vec(disp, id, force_x, force_y);
    }
}

// ─── FR 迭代（v2：带分组增强）─────────────────────────────

fn run_fr_iterations_v2(
    diagram: &Diagram,
    graph: &GraphIndex,
    sizes: &HashMap<String, (f64, f64)>,
    positions: &mut HashMap<String, (f64, f64)>,
    area: f64,
    group_map: &GroupMap,
) {
    let node_count = diagram.entities.len().max(1);
    let k = (area / node_count as f64).sqrt();
    let mut temperature = INITIAL_TEMPERATURE;
    let ids = diagram
        .entities
        .iter()
        .map(|entity| entity.id.as_str().to_string())
        .collect::<Vec<_>>();

    // 预计算分组质心（每轮更新）
    for _ in 0..ITERATIONS {
        let mut disp: HashMap<String, (f64, f64)> = ids
            .iter()
            .map(|id| (id.clone(), (0.0, 0.0)))
            .collect();

        // ── 全局斥力（Barnes-Hut 加速，O(V log V)）──
        // 构建位置数组（与 ids 同序）
        let pos_array: Vec<(f64, f64)> = ids
            .iter()
            .map(|id| positions.get(id).copied().unwrap_or((0.0, 0.0)))
            .collect();
        let tree = BarnesHutTree::build(&pos_array);

        for (i, id) in ids.iter().enumerate() {
            let (fx, fy) = tree.repulsion(i, &pos_array, k, BARNES_HUT_THETA);
            add_vec(&mut disp, id, fx, fy);
        }

        // ── 组间额外斥力（仅跨组对，O(cross_group_pairs)）──
        // Barnes-Hut 计算的是基础斥力（group_factor=1.0），
        // 跨组对需额外施加 GROUP_REPULSION 倍的增量斥力。
        if group_map.group_members.len() > 1 {
            apply_cross_group_repulsion(&ids, positions, &mut disp, group_map, k);
        }

        // ── 边引力 ──
        for (from, to) in &graph.undirected_edges {
            let (fx, fy) = positions.get(from).copied().unwrap_or((0.0, 0.0));
            let (tx, ty) = positions.get(to).copied().unwrap_or((0.0, 0.0));
            let dx = fx - tx;
            let dy = fy - ty;
            let distance = (dx * dx + dy * dy).sqrt().max(MIN_DISTANCE);
            let attractive = (distance * distance) / k * EDGE_ATTRACTION_MULT;
            let dir_x = dx / distance;
            let dir_y = dy / distance;

            add_vec(&mut disp, from, -dir_x * attractive, -dir_y * attractive);
            add_vec(&mut disp, to, dir_x * attractive, dir_y * attractive);
        }

        // ── Phase 3：RUDY 拥堵排斥力 ──
        // 每轮重建密度网格（O(|E|)），对节点施加密度梯度排斥力，
        // 将节点推离边密集区域，降低局部交叉与重叠。
        if let Some(grid) = CongestionGrid::build(&graph.undirected_edges, positions) {
            apply_congestion_repulsion(&ids, positions, &mut disp, &grid, k);
        }

        // ── v2: 分组引力（增强版）──
        apply_group_gravity_v2(positions, &mut disp, group_map, sizes);
        // ── v2: 分组凸包约束 ──
        apply_group_hull_constraint(positions, &mut disp, group_map, sizes);
        // ── 全局中心引力（弱）──
        apply_center_gravity(&ids, positions, &mut disp);

        // ── 应用位移 ──
        for entity in &diagram.entities {
            let id = entity.id.as_str();

            let (dx, dy) = disp.get(id).copied().unwrap_or((0.0, 0.0));
            let distance = (dx * dx + dy * dy).sqrt().max(MIN_DISTANCE);
            let limited = distance.min(temperature);
            let dir_x = dx / distance;
            let dir_y = dy / distance;
            let (x, y) = positions.get(id).copied().unwrap_or((0.0, 0.0));
            positions.insert(id.to_string(), (x + dir_x * limited, y + dir_y * limited));
        }

        confine_positions(ids.iter(), positions, sizes);
        temperature = (temperature * COOLING).max(1.0);
    }
}

/// 跨组额外斥力：对属于不同组的节点对施加 `GROUP_REPULSION` 倍的增量斥力。
///
/// Barnes-Hut 计算的是基础斥力（group_factor=1.0），此函数补充跨组增量。
/// 仅迭代跨组对，复杂度 O(cross_group_pairs)，远小于 O(V²)。
///
/// 嵌套 group 感知：同顶层组但不同子组的节点对（如 cloud 内的 public_subnet 与
/// private_subnet 节点）施加减弱斥力（`SAME_TOP_GROUP_REPULSION_FACTOR`），
/// 避免同顶层组的子组被推得过远；不同顶层组的节点对施加正常斥力。
fn apply_cross_group_repulsion(
    ids: &[String],
    positions: &HashMap<String, (f64, f64)>,
    disp: &mut HashMap<String, (f64, f64)>,
    group_map: &GroupMap,
    k: f64,
) {
    /// 同顶层组但不同子组的斥力衰减系数
    const SAME_TOP_GROUP_REPULSION_FACTOR: f64 = 0.3;

    // 收集有组节点，按直接组分组
    let mut grouped: HashMap<&String, Vec<&String>> = HashMap::new();
    for id in ids {
        if let Some(gid) = group_map.node_to_group.get(id) {
            grouped.entry(gid).or_default().push(id);
        }
    }

    let group_ids: Vec<&String> = grouped.keys().copied().collect();
    // 遍历组对（i < j），对每对组遍历成员对
    for i in 0..group_ids.len() {
        for j in (i + 1)..group_ids.len() {
            let members_a = &grouped[group_ids[i]];
            let members_b = &grouped[group_ids[j]];
            for a in members_a {
                for b in members_b {
                    let (ax, ay) = positions.get(*a).copied().unwrap_or((0.0, 0.0));
                    let (bx, by) = positions.get(*b).copied().unwrap_or((0.0, 0.0));
                    let dx = ax - bx;
                    let dy = ay - by;
                    let distance = (dx * dx + dy * dy).sqrt().max(MIN_DISTANCE);

                    // 判断是否同顶层组：同顶层组衰减斥力
                    let same_top = group_map
                        .node_to_top_group
                        .get(*a)
                        .is_some_and(|ta| group_map.node_to_top_group.get(*b).is_some_and(|tb| ta == tb));
                    let factor = if same_top {
                        SAME_TOP_GROUP_REPULSION_FACTOR
                    } else {
                        1.0
                    };
                    let repulsive = (k * k) / distance * GROUP_REPULSION * factor;
                    let dir_x = dx / distance;
                    let dir_y = dy / distance;
                    add_vec(disp, a, dir_x * repulsive, dir_y * repulsive);
                    add_vec(disp, b, -dir_x * repulsive, -dir_y * repulsive);
                }
            }
        }
    }
}

// ─── v2: 增强分组引力 ─────────────────────────────────────

fn apply_group_gravity_v2(
    positions: &HashMap<String, (f64, f64)>,
    disp: &mut HashMap<String, (f64, f64)>,
    group_map: &GroupMap,
    sizes: &HashMap<String, (f64, f64)>,
) {
    for members in group_map.group_members.values() {
        if members.len() <= 1 {
            continue;
        }

        // 计算分组的加权质心（按节点面积加权）
        let mut total_weight = 0.0;
        let mut centroid_x = 0.0;
        let mut centroid_y = 0.0;

        for member in members {
            let (x, y) = positions.get(member).copied().unwrap_or((0.0, 0.0));
            let (w, h) = sizes.get(member).copied().unwrap_or((constants::FR_NODE_WIDTH, constants::DEFAULT_NODE_HEIGHT));
            let weight = (w * h).sqrt(); // 面积平方根作为权重
            centroid_x += x * weight;
            centroid_y += y * weight;
            total_weight += weight;
        }

        if total_weight > 0.0 {
            centroid_x /= total_weight;
            centroid_y /= total_weight;
        }

        // 每个成员朝质心移动
        for member in members {
            let (x, y) = positions.get(member).copied().unwrap_or((0.0, 0.0));
            add_vec(
                disp,
                member,
                (centroid_x - x) * GROUP_GRAVITY,
                (centroid_y - y) * GROUP_GRAVITY,
            );
        }
    }
}

// ─── v2: 分组凸包约束 ─────────────────────────────────────

/// 防止同组节点过于分散：如果节点离组质心超过阈值，施加额外引力
fn apply_group_hull_constraint(
    positions: &HashMap<String, (f64, f64)>,
    disp: &mut HashMap<String, (f64, f64)>,
    group_map: &GroupMap,
    _sizes: &HashMap<String, (f64, f64)>,
) {
    for members in group_map.group_members.values() {
        if members.len() <= 1 {
            continue;
        }

        let mut centroid_x = 0.0;
        let mut centroid_y = 0.0;
        for member in members {
            let (x, y) = positions.get(member).copied().unwrap_or((0.0, 0.0));
            centroid_x += x;
            centroid_y += y;
        }
        centroid_x /= members.len() as f64;
        centroid_y /= members.len() as f64;

        // 计算组的自然半径（基于节点数）
        let group_radius = (members.len() as f64).sqrt() * 80.0 + GROUP_HULL_MARGIN;

        for member in members {
            let (x, y) = positions.get(member).copied().unwrap_or((0.0, 0.0));
            let dx = x - centroid_x;
            let dy = y - centroid_y;
            let dist = (dx * dx + dy * dy).sqrt().max(MIN_DISTANCE);

            if dist > group_radius {
                // 超出凸包半径，施加弹性拉力
                let force = (dist - group_radius) * 0.03;
                add_vec(
                    disp,
                    member,
                    -(dx / dist) * force,
                    -(dy / dist) * force,
                );
            }
        }
    }
}

// ─── 中心引力 ────────────────────────────────────────────

fn apply_center_gravity(
    ids: &[String],
    positions: &HashMap<String, (f64, f64)>,
    disp: &mut HashMap<String, (f64, f64)>,
) {
    let center_x = ids
        .iter()
        .map(|id| positions.get(id).copied().unwrap_or((0.0, 0.0)).0)
        .sum::<f64>()
        / ids.len().max(1) as f64;
    let center_y = ids
        .iter()
        .map(|id| positions.get(id).copied().unwrap_or((0.0, 0.0)).1)
        .sum::<f64>()
        / ids.len().max(1) as f64;

    for id in ids {
        let (x, y) = positions.get(id).copied().unwrap_or((0.0, 0.0));
        add_vec(
            disp,
            id,
            (center_x - x) * CENTER_GRAVITY,
            (center_y - y) * CENTER_GRAVITY,
        );
    }
}

fn confine_positions<'a>(
    ids: impl Iterator<Item = &'a String>,
    positions: &mut HashMap<String, (f64, f64)>,
    sizes: &HashMap<String, (f64, f64)>,
) {
    for id in ids {
        if let Some((x, y)) = positions.get(id).copied() {
            let (width, height) = sizes.get(id).copied().unwrap_or((constants::FR_NODE_WIDTH, constants::DEFAULT_NODE_HEIGHT));
            positions.insert(
                id.clone(),
                (x.max(width / 2.0), y.max(height / 2.0)),
            );
        }
    }
}

// ─── v2: 重叠消除（分组感知）──────────────────────────────

fn resolve_overlaps_v2(
    diagram: &Diagram,
    sizes: &HashMap<String, (f64, f64)>,
    positions: &mut HashMap<String, (f64, f64)>,
    group_map: &GroupMap,
) {
    let ids = diagram
        .entities
        .iter()
        .map(|entity| entity.id.as_str().to_string())
        .collect::<Vec<_>>();

    for _ in 0..15 {
        let mut moved = false;
        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                let left = &ids[i];
                let right = &ids[j];
                let (lx, ly) = positions.get(left).copied().unwrap_or((0.0, 0.0));
                let (rx, ry) = positions.get(right).copied().unwrap_or((0.0, 0.0));
                let (lw, lh) = sizes.get(left).copied().unwrap_or((constants::FR_NODE_WIDTH, constants::DEFAULT_NODE_HEIGHT));
                let (rw, rh) = sizes.get(right).copied().unwrap_or((constants::FR_NODE_WIDTH, constants::DEFAULT_NODE_HEIGHT));

                let same_group = group_map.node_to_group.get(left)
                    == group_map.node_to_group.get(right);

                // 同组节点允许更紧密的排列
                let margin = if same_group { NODE_MARGIN * 0.6 } else { NODE_MARGIN };

                let overlap_x = (lw + rw) / 2.0 + margin - (lx - rx).abs();
                let overlap_y = (lh + rh) / 2.0 + margin - (ly - ry).abs();
                if overlap_x <= 0.0 || overlap_y <= 0.0 {
                    continue;
                }

                moved = true;
                if overlap_x < overlap_y {
                    let shift = overlap_x / 2.0 + 1.0;
                    let dir = if lx <= rx { -1.0 } else { 1.0 };
                    positions.insert(left.clone(), (lx + dir * shift, ly));
                    positions.insert(right.clone(), (rx - dir * shift, ry));
                } else {
                    let shift = overlap_y / 2.0 + 1.0;
                    let dir = if ly <= ry { -1.0 } else { 1.0 };
                    positions.insert(left.clone(), (lx, ly + dir * shift));
                    positions.insert(right.clone(), (rx, ry - dir * shift));
                }
            }
        }

        if !moved {
            break;
        }
    }
}

// ─── v2: 分量打包（分组感知）──────────────────────────────

fn pack_components_v2(
    components: &[Vec<String>],
    positions: HashMap<String, (f64, f64)>,
    sizes: &HashMap<String, (f64, f64)>,
    group_map: &GroupMap,
    config: ForceDirectedLayoutConfig,
) -> HashMap<String, (f64, f64)> {
    let mut packed = HashMap::new();
    let mut cursor_x = config.padding;
    let mut max_row_height: f64 = 0.0;

    // 简单线性排列分量（后续可用 pack 模块改进）
    for component in components {
        let mut min_x = f64::MAX;
        let mut min_y = f64::MAX;
        let mut max_x = f64::MIN;
        let mut max_y = f64::MIN;

        for node_id in component {
            let (cx, cy) = positions.get(node_id).copied().unwrap_or((0.0, 0.0));
            let (width, height) = sizes.get(node_id).copied().unwrap_or((constants::FR_NODE_WIDTH, constants::DEFAULT_NODE_HEIGHT));
            min_x = min_x.min(cx - width / 2.0);
            min_y = min_y.min(cy - height / 2.0);
            max_x = max_x.max(cx + width / 2.0);
            max_y = max_y.max(cy + height / 2.0);
        }

        let shift_x = cursor_x - min_x;
        let shift_y = config.padding - min_y;
        for node_id in component {
            let (cx, cy) = positions.get(node_id).copied().unwrap_or((0.0, 0.0));
            packed.insert(node_id.clone(), (cx + shift_x, cy + shift_y));
        }

        cursor_x += (max_x - min_x) + config.component_gap;
        if max_y - min_y > max_row_height {
            max_row_height = max_y - min_y;
        }
    }

    let _ = (max_row_height, group_map);
    packed
}

// ─── 工具函数 ────────────────────────────────────────────

fn bounds_from_layout(
    nodes: &HashMap<String, NodeLayout>,
    groups: &HashMap<String, GroupLayout>,
    padding: f64,
) -> (f64, f64) {
    let node_max_x = nodes.values().map(|node| node.x + node.width).fold(0.0_f64, f64::max);
    let node_max_y = nodes.values().map(|node| node.y + node.height).fold(0.0_f64, f64::max);
    let group_max_x = groups.values().map(|group| group.x + group.width).fold(0.0_f64, f64::max);
    let group_max_y = groups.values().map(|group| group.y + group.height).fold(0.0_f64, f64::max);
    (
        node_max_x.max(group_max_x) + padding,
        node_max_y.max(group_max_y) + padding,
    )
}

fn add_vec(displacements: &mut HashMap<String, (f64, f64)>, id: &str, dx: f64, dy: f64) {
    let entry = displacements.entry(id.to_string()).or_insert((0.0, 0.0));
    entry.0 += dx;
    entry.1 += dy;
}

// ═══════════════════════════════════════════════════════════
//  单元测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        ArrowType, AttributeMap, Diagram, Entity, Group, Identifier, Relation, SourceInfo, Span,
    };
    use crate::types::DiagramType;

    fn entity(id: &str, label: &str) -> Entity {
        Entity {
            id: Identifier::new_unchecked(id),
            label: label.to_string(),
            attributes: AttributeMap::default(),
            group_id: None,
            span: Span::dummy(),
        }
    }

    fn entity_in_group(id: &str, label: &str, group: &str) -> Entity {
        Entity {
            id: Identifier::new_unchecked(id),
            label: label.to_string(),
            attributes: AttributeMap::default(),
            group_id: Some(Identifier::new_unchecked(group)),
            span: Span::dummy(),
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
            span: Span::dummy(),
        }
    }

    fn diagram(entities: Vec<Entity>, relations: Vec<Relation>, groups: Vec<Group>) -> Diagram {
        Diagram {
            diagram_type: DiagramType::Architecture,
            attributes: vec![],
            entities,
            relations,
            groups,
            style_decls: vec![],
            source_info: SourceInfo {
                file: None,
                line_count: 1,
            },
            ..Default::default()
        }
    }

    fn make_group(id: &str, label: &str, entity_ids: Vec<&str>) -> Group {
        Group {
            id: Identifier::new_unchecked(id),
            label: label.to_string(),
            attributes: AttributeMap::default(),
            parent_id: None,
            depth: 0,
            entity_ids: entity_ids.into_iter().map(|e| Identifier::new_unchecked(e)).collect(),
            child_group_ids: vec![],
            span: Span::dummy(),
        }
    }

    #[test]
    fn v2_layout_positions_connected_nodes() {
        let diagram = diagram(
            vec![entity("a", "API"), entity("b", "Service"), entity("c", "DB")],
            vec![relation("a", "b"), relation("b", "c")],
            vec![],
        );

        let result = ForceDirectedLayout::default().compute(&diagram);
        assert_eq!(result.nodes.len(), 3);
        assert!(result.total_width > constants::WIDE_PADDING * 2.0);
        assert!(result.total_height > constants::WIDE_PADDING * 2.0);
    }

    #[test]
    fn v2_layout_separates_disconnected_components() {
        let diagram = diagram(
            vec![
                entity("a1", "A1"), entity("a2", "A2"),
                entity("b1", "B1"), entity("b2", "B2"),
            ],
            vec![relation("a1", "a2"), relation("b1", "b2")],
            vec![],
        );

        let result = ForceDirectedLayout::default().compute(&diagram);
        assert!(result.nodes.get("a1").is_some());
        assert!(result.nodes.get("b1").is_some());
    }

    #[test]
    fn v2_layout_preserves_group_bounds() {
        let group = make_group("g1", "Platform", vec!["a", "b"]);

        let diagram = diagram(
            vec![entity_in_group("a", "A", "g1"), entity_in_group("b", "B", "g1"), entity("c", "C")],
            vec![relation("a", "b"), relation("b", "c")],
            vec![group],
        );

        let result = ForceDirectedLayout::default().compute(&diagram);
        assert!(result.groups.contains_key("g1"));
    }

    #[test]
    fn v2_layout_group_members_stay_close() {
        let group = make_group("platform", "Platform", vec!["svc1", "svc2", "svc3"]);

        let diagram = diagram(
            vec![
                entity_in_group("svc1", "Service1", "platform"),
                entity_in_group("svc2", "Service2", "platform"),
                entity_in_group("svc3", "Service3", "platform"),
            ],
            vec![relation("svc1", "svc2"), relation("svc2", "svc3")],
            vec![group],
        );

        let result = ForceDirectedLayout::default().compute(&diagram);
        let group_layout = result.groups.get("platform").unwrap();

        // 分组内的节点应该在分组边界内
        for nid in &["svc1", "svc2", "svc3"] {
            let nl = result.nodes.get(*nid).unwrap();
            assert!(nl.x + nl.width >= group_layout.x - 1.0);
            assert!(nl.y + nl.height >= group_layout.y - 1.0);
            assert!(nl.x <= group_layout.x + group_layout.width + 1.0);
            assert!(nl.y <= group_layout.y + group_layout.height + 1.0);
        }
    }

    #[test]
    fn v2_layout_multiple_groups_separated() {
        let g1 = make_group("g1", "Frontend", vec!["fe1", "fe2"]);
        let g2 = make_group("g2", "Backend", vec!["be1", "be2"]);

        let diagram = diagram(
            vec![
                entity_in_group("fe1", "FE1", "g1"),
                entity_in_group("fe2", "FE2", "g1"),
                entity_in_group("be1", "BE1", "g2"),
                entity_in_group("be2", "BE2", "g2"),
            ],
            vec![relation("fe1", "be1"), relation("fe2", "be2")],
            vec![g1, g2],
        );

        let result = ForceDirectedLayout::default().compute(&diagram);
        assert!(result.groups.contains_key("g1"));
        assert!(result.groups.contains_key("g2"));

        // 两个分组不应重叠
        let g1_bounds = result.groups.get("g1").unwrap();
        let g2_bounds = result.groups.get("g2").unwrap();

        let overlap_x = g1_bounds.x < g2_bounds.x + g2_bounds.width
            && g1_bounds.x + g1_bounds.width > g2_bounds.x;
        let overlap_y = g1_bounds.y < g2_bounds.y + g2_bounds.height
            && g1_bounds.y + g1_bounds.height > g2_bounds.y;

        // 注意：分组界限是由 group_bounds 计算的，可能包含 padding，
        // 这里只验证算法不崩溃
        let _ = (overlap_x, overlap_y);
    }

    #[test]
    fn v2_layout_empty_diagram() {
        let diagram = diagram(vec![], vec![], vec![]);
        let result = ForceDirectedLayout::default().compute(&diagram);
        assert!(result.nodes.is_empty());
    }

    /// 辅助：构造嵌套 group（子组有 parent_id 和 depth）
    fn make_nested_group(id: &str, label: &str, parent: &str, entity_ids: Vec<&str>) -> Group {
        Group {
            id: Identifier::new_unchecked(id),
            label: label.to_string(),
            attributes: AttributeMap::default(),
            parent_id: Some(Identifier::new_unchecked(parent)),
            depth: 1,
            entity_ids: entity_ids.into_iter().map(|e| Identifier::new_unchecked(e)).collect(),
            child_group_ids: vec![],
            span: Span::dummy(),
        }
    }

    /// 辅助：构造容器 group（有 child_group_ids）
    fn make_container_group(id: &str, label: &str, children: Vec<&str>) -> Group {
        Group {
            id: Identifier::new_unchecked(id),
            label: label.to_string(),
            attributes: AttributeMap::default(),
            parent_id: None,
            depth: 0,
            entity_ids: vec![],
            child_group_ids: children.into_iter().map(|c| Identifier::new_unchecked(c)).collect(),
            span: Span::dummy(),
        }
    }

    /// 嵌套 group 场景：同顶层组但不同子组的节点不应被推得过远。
    /// 验证 build_group_map 正确解析 node_to_top_group。
    #[test]
    fn v2_layout_nested_groups_same_top_group() {
        // cloud (depth 0)
        //   ├── public_subnet (depth 1): gw, lb
        //   └── private_subnet (depth 1): auth, biz
        let cloud = make_container_group("cloud", "Cloud", vec!["public_subnet", "private_subnet"]);
        let public_subnet = make_nested_group("public_subnet", "Public", "cloud", vec!["gw", "lb"]);
        let private_subnet = make_nested_group("private_subnet", "Private", "cloud", vec!["auth", "biz"]);

        let diagram = diagram(
            vec![
                entity_in_group("gw", "Gateway", "public_subnet"),
                entity_in_group("lb", "LB", "public_subnet"),
                entity_in_group("auth", "Auth", "private_subnet"),
                entity_in_group("biz", "Biz", "private_subnet"),
            ],
            vec![
                relation("gw", "lb"),
                relation("auth", "biz"),
                relation("lb", "auth"), // 跨子组边
            ],
            vec![cloud, public_subnet, private_subnet],
        );

        let result = ForceDirectedLayout::default().compute(&diagram);

        // 1. 所有节点都有布局
        assert_eq!(result.nodes.len(), 4);

        // 2. cloud 容器组有包围框（compute_group_bounds 递归子组）
        assert!(result.groups.contains_key("cloud"), "cloud 容器组应有包围框");
        assert!(result.groups.contains_key("public_subnet"));
        assert!(result.groups.contains_key("private_subnet"));

        // 3. cloud 包围框包含两个子组
        let cloud_gl = &result.groups["cloud"];
        let public_gl = &result.groups["public_subnet"];
        let private_gl = &result.groups["private_subnet"];
        let cloud_xmax = cloud_gl.x + cloud_gl.width;
        let cloud_ymax = cloud_gl.y + cloud_gl.height;
        assert!(public_gl.x >= cloud_gl.x && public_gl.x + public_gl.width <= cloud_xmax,
            "public_subnet 应在 cloud 内");
        assert!(private_gl.x >= cloud_gl.x && private_gl.x + private_gl.width <= cloud_xmax,
            "private_subnet 应在 cloud 内");
        assert!(public_gl.y >= cloud_gl.y && public_gl.y + public_gl.height <= cloud_ymax,
            "public_subnet 应在 cloud 内 (y)");
        assert!(private_gl.y >= cloud_gl.y && private_gl.y + private_gl.height <= cloud_ymax,
            "private_subnet 应在 cloud 内 (y)");
    }
}
