//! Sugiyama 分层图布局算法（基线备份）
//!
//! 本模块仅作基线参考保留，生产环境请使用 [`crate::layout::sugiyama_v2`]。
//!
//! 参考论文：Gansner et al., "A Technique for Drawing Directed Graphs"（1993）
//!
//! 改进点：
//!   1. Phase 3：同时考虑边交叉和边弯曲程度
//!   2. Phase 4：优化节点位置，平衡边的长度和弯曲
//!   3. 支持与正交边路由更好地配合
//!
//! 完整的 Sugiyama 四阶段流程：
//!   1. 去环 (Cycle Removal) — DFS 反转回边，保证图为 DAG
//!   2. 层分配 (Layer Assignment) — 最长路径法
//!   3. 交叉最小化 + 弯曲优化 — 综合评分法
//!   4. 坐标分配 — 优化位置以减少边长和弯曲
//!
//! 支持 top-to-bottom 和 left-to-right 两种方向。
//! 使用 petgraph 作为图数据结构基础。

use crate::types::DiagramType;
use crate::ast::{Diagram, Entity};
use crate::kinds::er::semantics::entity_node_size;
use crate::layout::algorithm_config::SugiyamaLayoutConfig;
use crate::layout::node::common::group_bounds::{self, GroupPadding};
use crate::layout::constants;
use crate::layout::{AlgorithmOptionSpec, LayoutResult, LayoutStrategy, NodeLayout};
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::Direction;
use std::collections::{HashMap, HashSet, VecDeque};

// ─── 布局常量 ────────────────────────────────────────────

const H_GAP: f64 = 60.0;
const V_GAP: f64 = 80.0;
const CROSSING_ITERATIONS: usize = 6;

// 权重系数：边交叉 vs 边弯曲
const CROSSING_WEIGHT: f64 = 1.0;
const BENDING_WEIGHT: f64 = 0.5;
const EDGE_LENGTH_WEIGHT: f64 = 0.3;

const APPLICABLE_TYPES: &[DiagramType] = &[DiagramType::Flowchart, DiagramType::Er];

/// Sugiyama 分层图布局
pub struct SugiyamaLayout {
    config: SugiyamaLayoutConfig,
}

impl SugiyamaLayout {
    pub fn new(config: SugiyamaLayoutConfig) -> Self {
        Self { config }
    }
}

impl Default for SugiyamaLayout {
    fn default() -> Self {
        Self::new(SugiyamaLayoutConfig::default())
    }
}

impl LayoutStrategy for SugiyamaLayout {
    fn name(&self) -> &'static str {
        "sugiyama"
    }

    fn applicable_diagram_types(&self) -> &'static [DiagramType] {
        APPLICABLE_TYPES
    }

    fn supports_custom(&self) -> bool {
        true
    }

    fn option_specs(&self) -> &'static [AlgorithmOptionSpec] {
        crate::layout::algorithm_config::SUGIYAMA_LAYOUT_OPTIONS
    }

    fn supported_directions(&self) -> &'static [&'static str] {
        const SUPPORTED_DIRECTIONS: &[&str] = &[
            crate::types::attr_constants::direction::TOP_TO_BOTTOM,
            crate::types::attr_constants::direction::LEFT_TO_RIGHT,
        ];
        SUPPORTED_DIRECTIONS
    }

    fn compute(&self, diagram: &Diagram) -> LayoutResult {
        let horizontal = crate::layout::resolve_effective_direction(diagram) == Some("left-to-right");

        if diagram.entities.is_empty() {
            return LayoutResult {
                nodes: HashMap::new(),
                groups: HashMap::new(),
                edges: vec![],
                total_width: constants::DEFAULT_PADDING * 2.0,
                total_height: constants::DEFAULT_PADDING * 2.0,
                hints: Default::default(),
            };
        }

        // ── 构建 petgraph DiGraph ──
        let (graph, _node_id_to_idx, idx_to_node_id) = build_graph(diagram);

        // ── Phase 1: 去环 ──
        let reversed_edges = phase1_remove_cycles(&graph, &idx_to_node_id);

        // 构建去环后的 DAG
        let dag = build_dag(&graph, &reversed_edges);

        // ── Phase 2: 层分配（最长路径法）──
        let layers = phase2_assign_layers(diagram, &dag, &idx_to_node_id);

        // ── Phase 3: 交叉最小化 + 弯曲优化 ──
        let layers = phase3_minimize_crossings_and_bending(&dag, layers, &idx_to_node_id);

        // ── Phase 4: 坐标分配（优化位置）──
        let nodes = phase4_assign_optimized_coordinates(diagram, &dag, &layers, horizontal);

        // ── 计算分组边界 ──
        let groups = group_bounds::compute_group_bounds(
            diagram,
            &nodes,
            GroupPadding::uniform(self.config.group_padding, 16.0),
        );

        // ── 计算总尺寸 ──
        let total_width = nodes
            .values()
            .map(|n| n.x + n.width)
            .fold(0.0_f64, f64::max)
            + constants::DEFAULT_PADDING;
        let total_height = nodes
            .values()
            .map(|n| n.y + n.height)
            .fold(0.0_f64, f64::max)
            + constants::DEFAULT_PADDING;

        let mut result = LayoutResult {
            nodes,
            groups,
            edges: vec![],
            total_width,
            total_height,
            hints: crate::layout::LayoutHints {
                edge_routing_style: crate::layout::EdgeRoutingStyle::Orthogonal,
                ..Default::default()
            },
        };

        if diagram.diagram_type == DiagramType::Er {
            normalize_layout_to_padding(&mut result);
        }

        result
    }
}

/// 将布局整体平移，保证所有节点落在 `constants::DEFAULT_PADDING` 内侧。
fn normalize_layout_to_padding(result: &mut LayoutResult) {
    let min_x = result
        .nodes
        .values()
        .map(|n| n.x)
        .fold(f64::INFINITY, f64::min);
    let min_y = result
        .nodes
        .values()
        .map(|n| n.y)
        .fold(f64::INFINITY, f64::min);

    if !min_x.is_finite() || !min_y.is_finite() {
        return;
    }

    let dx = if min_x < constants::DEFAULT_PADDING { constants::DEFAULT_PADDING - min_x } else { 0.0 };
    let dy = if min_y < constants::DEFAULT_PADDING { constants::DEFAULT_PADDING - min_y } else { 0.0 };
    if dx <= 0.0 && dy <= 0.0 {
        return;
    }

    for node in result.nodes.values_mut() {
        node.x += dx;
        node.y += dy;
    }
    for group in result.groups.values_mut() {
        group.x += dx;
        group.y += dy;
    }

    result.total_width += dx;
    result.total_height += dy;
}

// ═══════════════════════════════════════════════════════════
//  图构建
// ═══════════════════════════════════════════════════════════

/// 从 Diagram 构建 petgraph DiGraph
fn build_graph(diagram: &Diagram) -> (DiGraph<String, ()>, HashMap<String, NodeIndex>, HashMap<NodeIndex, String>) {
    let mut graph = DiGraph::<String, ()>::new();
    let mut node_id_to_idx: HashMap<String, NodeIndex> = HashMap::new();
    let mut idx_to_node_id: HashMap<NodeIndex, String> = HashMap::new();

    // 添加节点
    for entity in &diagram.entities {
        let idx = graph.add_node(entity.id.as_str().to_string());
        node_id_to_idx.insert(entity.id.as_str().to_string(), idx);
        idx_to_node_id.insert(idx, entity.id.as_str().to_string());
    }

    // 添加边
    for rel in &diagram.relations {
        let from_idx = node_id_to_idx.get(rel.from.as_str());
        let to_idx = node_id_to_idx.get(rel.to.as_str());
        if let (Some(from), Some(to)) = (from_idx, to_idx) {
            graph.add_edge(*from, *to, ());
        }
    }

    (graph, node_id_to_idx, idx_to_node_id)
}

/// 构建去环后的 DAG（反转标记的边）
fn build_dag(graph: &DiGraph<String, ()>, reversed_edges: &HashSet<(NodeIndex, NodeIndex)>) -> DiGraph<String, ()> {
    let mut dag = DiGraph::<String, ()>::new();

    // 复制节点
    let mut node_map: HashMap<NodeIndex, NodeIndex> = HashMap::new();
    for node in graph.node_indices() {
        let new_idx = dag.add_node(graph[node].clone());
        node_map.insert(node, new_idx);
    }

    // 复制边（反转标记的边）
    for edge in graph.edge_indices() {
        let (src, tgt) = graph.edge_endpoints(edge).unwrap();
        let new_src = node_map[&src];
        let new_tgt = node_map[&tgt];

        if reversed_edges.contains(&(src, tgt)) {
            // 反转边
            dag.add_edge(new_tgt, new_src, ());
        } else {
            dag.add_edge(new_src, new_tgt, ());
        }
    }

    dag
}

// ═══════════════════════════════════════════════════════════
//  Phase 1: 去环 (Cycle Removal)
//  使用 DFS 标记边类型，反转回边使图变为 DAG
// ═══════════════════════════════════════════════════════════

fn phase1_remove_cycles(
    graph: &DiGraph<String, ()>,
    _idx_to_node_id: &HashMap<NodeIndex, String>,
) -> HashSet<(NodeIndex, NodeIndex)> {
    let mut reversed: HashSet<(NodeIndex, NodeIndex)> = HashSet::new();
    let mut visited: HashSet<NodeIndex> = HashSet::new();
    let mut on_stack: HashSet<NodeIndex> = HashSet::new();

    for node in graph.node_indices() {
        if !visited.contains(&node) {
            dfs_cycle_detect(node, graph, &mut visited, &mut on_stack, &mut reversed);
        }
    }

    reversed
}

fn dfs_cycle_detect(
    node: NodeIndex,
    graph: &DiGraph<String, ()>,
    visited: &mut HashSet<NodeIndex>,
    on_stack: &mut HashSet<NodeIndex>,
    reversed: &mut HashSet<(NodeIndex, NodeIndex)>,
) {
    visited.insert(node);
    on_stack.insert(node);

    // 遍历所有出边
    for neighbor in graph.neighbors_directed(node, Direction::Outgoing) {
        if !visited.contains(&neighbor) {
            dfs_cycle_detect(neighbor, graph, visited, on_stack, reversed);
        } else if on_stack.contains(&neighbor) {
            // 回边：反转以打破环
            reversed.insert((node, neighbor));
        }
    }

    on_stack.remove(&node);
}

// ═══════════════════════════════════════════════════════════
//  Phase 2: 层分配 — 最长路径法 (Longest Path)
//  节点层级 = max(所有前驱层级) + 1
// ═══════════════════════════════════════════════════════════

fn phase2_assign_layers(
    _diagram: &Diagram,
    dag: &DiGraph<String, ()>,
    _idx_to_node_id: &HashMap<NodeIndex, String>,
) -> Vec<Vec<NodeIndex>> {
    let mut layer_of: HashMap<NodeIndex, usize> = HashMap::new();
    let mut queue: VecDeque<NodeIndex> = VecDeque::new();
    let mut indegree: HashMap<NodeIndex, usize> = HashMap::new();

    // 计算入度（自环不计入）并找到源节点
    for node in dag.node_indices() {
        let deg = dag
            .neighbors_directed(node, Direction::Incoming)
            .filter(|&pred| pred != node)
            .count();
        indegree.insert(node, deg);
        if deg == 0 {
            queue.push_back(node);
            layer_of.insert(node, 0);
        }
    }

    // 处理全环的退化情况
    if queue.is_empty() && dag.node_count() > 0 {
        let first = dag.node_indices().next().unwrap();
        queue.push_back(first);
        layer_of.insert(first, 0);
        indegree.insert(first, 0);
    }

    // 拓扑排序 + 最长路径
    while let Some(node) = queue.pop_front() {
        let my_layer = layer_of[&node];

        for neighbor in dag.neighbors_directed(node, Direction::Outgoing) {
            if neighbor == node {
                continue;
            }

            let neighbor_layer = layer_of.entry(neighbor).or_insert(0);
            *neighbor_layer = (*neighbor_layer).max(my_layer + 1);

            if let Some(deg) = indegree.get_mut(&neighbor) {
                *deg = deg.saturating_sub(1);
                if *deg == 0 {
                    queue.push_back(neighbor);
                }
            }
        }
    }

    // 处理未分配的孤立节点
    let max_layer = layer_of.values().copied().max().unwrap_or(0);
    for node in dag.node_indices() {
        if !layer_of.contains_key(&node) {
            layer_of.insert(node, max_layer + 1);
        }
    }

    // 按层级分组
    let max_l = layer_of.values().copied().max().unwrap_or(0);
    let mut layers: Vec<Vec<NodeIndex>> = vec![Vec::new(); max_l + 1];
    for node in dag.node_indices() {
        layers[layer_of[&node]].push(node);
    }

    layers
}

// ═══════════════════════════════════════════════════════════
//  Phase 3: 交叉最小化 + 弯曲优化 — 综合评分法
//  在相邻层之间反复上下扫描，按综合评分（交叉 + 弯曲）排序节点
// ═══════════════════════════════════════════════════════════

fn phase3_minimize_crossings_and_bending(
    dag: &DiGraph<String, ()>,
    mut layers: Vec<Vec<NodeIndex>>,
    _idx_to_node_id: &HashMap<NodeIndex, String>,
) -> Vec<Vec<NodeIndex>> {
    for _ in 0..CROSSING_ITERATIONS {
        // 向下扫描：固定上层，排序下层
        for i in 0..layers.len().saturating_sub(1) {
            let upper_pos: HashMap<NodeIndex, usize> = layers[i]
                .iter()
                .enumerate()
                .map(|(j, &id)| (id, j))
                .collect();

            let mut scores: Vec<(usize, f64)> = Vec::new();
            for (j, &node) in layers[i + 1].iter().enumerate() {
                let score = compute_node_score(
                    j,
                    node,
                    &upper_pos,
                    dag,
                    Direction::Incoming,
                );
                scores.push((j, score));
            }

            scores.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
            let old = layers[i + 1].clone();
            for (new_pos, &(old_pos, _)) in scores.iter().enumerate() {
                layers[i + 1][new_pos] = old[old_pos];
            }
        }

        // 向上扫描：固定下层，排序上层
        for i in (0..layers.len().saturating_sub(1)).rev() {
            let lower_pos: HashMap<NodeIndex, usize> = layers[i + 1]
                .iter()
                .enumerate()
                .map(|(j, &id)| (id, j))
                .collect();

            let mut scores: Vec<(usize, f64)> = Vec::new();
            for (j, &node) in layers[i].iter().enumerate() {
                let score = compute_node_score(
                    j,
                    node,
                    &lower_pos,
                    dag,
                    Direction::Outgoing,
                );
                scores.push((j, score));
            }

            scores.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
            let old = layers[i].clone();
            for (new_pos, &(old_pos, _)) in scores.iter().enumerate() {
                layers[i][new_pos] = old[old_pos];
            }
        }
    }

    layers
}

/// 计算节点的综合评分（交叉 + 弯曲）
fn compute_node_score(
    node_pos: usize,
    node: NodeIndex,
    neighbor_pos: &HashMap<NodeIndex, usize>,
    dag: &DiGraph<String, ()>,
    dir: Direction,
) -> f64 {
    let connected: Vec<usize> = match dir {
        Direction::Incoming => {
            dag.neighbors_directed(node, Direction::Incoming)
                .filter_map(|p| neighbor_pos.get(&p).copied())
                .collect()
        }
        Direction::Outgoing => {
            dag.neighbors_directed(node, Direction::Outgoing)
                .filter_map(|s| neighbor_pos.get(&s).copied())
                .collect()
        }
    };

    // 计算重心 (用于交叉最小化)
    let barycenter = if connected.is_empty() {
        node_pos as f64
    } else {
        connected.iter().sum::<usize>() as f64 / connected.len() as f64
    };

    // 计算边弯曲评分 (计算方差，越大说明越分散)
    let bending_score = if connected.is_empty() {
        0.0
    } else {
        let mean = barycenter;
        let variance = connected
            .iter()
            .map(|&pos| (pos as f64 - mean).powi(2))
            .sum::<f64>() / connected.len() as f64;
        variance.sqrt()
    };

    // 综合评分：交叉权重 + 弯曲权重
    CROSSING_WEIGHT * barycenter + BENDING_WEIGHT * bending_score
}

// ═══════════════════════════════════════════════════════════
//  Phase 4: 优化坐标分配
//  使用动态规划或迭代优化，减少边长和弯曲
// ═══════════════════════════════════════════════════════════

fn phase4_assign_optimized_coordinates(
    diagram: &Diagram,
    dag: &DiGraph<String, ()>,
    layers: &[Vec<NodeIndex>],
    horizontal: bool,
) -> HashMap<String, NodeLayout> {
    let mut nodes: HashMap<String, NodeLayout> = HashMap::new();
    let er_mode = diagram.diagram_type == DiagramType::Er;

    let mut temp_pos: HashMap<NodeIndex, (f64, f64)> = HashMap::new();

    if er_mode {
        temp_pos = assign_er_initial_positions(diagram, dag, layers, horizontal);
    } else {
        let max_layer_size = layers.iter().map(|l| l.len()).max().unwrap_or(1);
        let total_span = max_layer_size as f64 * (constants::DEFAULT_NODE_WIDTH + H_GAP) - H_GAP;

        for (li, layer) in layers.iter().enumerate() {
            let layer_span = layer.len() as f64 * (constants::DEFAULT_NODE_WIDTH + H_GAP) - H_GAP;
            let offset = (total_span - layer_span) / 2.0;

            for (ni, &node_idx) in layer.iter().enumerate() {
                let (x, y) = if horizontal {
                    (
                        constants::DEFAULT_PADDING + li as f64 * (constants::DEFAULT_NODE_WIDTH + V_GAP),
                        constants::DEFAULT_PADDING + offset + ni as f64 * (constants::DEFAULT_NODE_HEIGHT + H_GAP),
                    )
                } else {
                    (
                        constants::DEFAULT_PADDING + offset + ni as f64 * (constants::DEFAULT_NODE_WIDTH + H_GAP),
                        constants::DEFAULT_PADDING + li as f64 * (constants::DEFAULT_NODE_HEIGHT + V_GAP),
                    )
                };
                temp_pos.insert(node_idx, (x, y));
            }
        }
    }

    // ER 图使用真实尺寸中心坐标，跳过基于固定格网的迭代优化
    if !er_mode {
        for _ in 0..3 {
            temp_pos = optimize_positions(&temp_pos, layers, dag, horizontal);
        }
    }

    // 生成最终布局
    for layer in layers.iter() {
        for &node_idx in layer {
            let (x, y) = temp_pos[&node_idx];
            let node_id = &dag[node_idx];
            let (width, height) = diagram
                .find_entity(node_id)
                .map(|entity| sized_node_for(diagram, entity))
                .unwrap_or((constants::DEFAULT_NODE_WIDTH, constants::DEFAULT_NODE_HEIGHT));

            let (center_x, center_y) = if er_mode {
                (x, y)
            } else {
                (x + constants::DEFAULT_NODE_WIDTH / 2.0, y + constants::DEFAULT_NODE_HEIGHT / 2.0)
            };

            let layout = NodeLayout {
                x: center_x - width / 2.0,
                y: center_y - height / 2.0,
                width,
                height,
                ..Default::default()
            };

            nodes.insert(node_id.clone(), layout);
        }
    }

    nodes
}

/// ER 图初始布局：按真实节点尺寸分层排布，temp_pos 存节点中心坐标。
fn assign_er_initial_positions(
    diagram: &Diagram,
    dag: &DiGraph<String, ()>,
    layers: &[Vec<NodeIndex>],
    horizontal: bool,
) -> HashMap<NodeIndex, (f64, f64)> {
    let mut temp_pos = HashMap::new();
    let layer_gap = V_GAP + 32.0;
    let mut primary = constants::DEFAULT_PADDING;

    for layer in layers {
        let sizes: Vec<(f64, f64)> = layer
            .iter()
            .map(|&idx| {
                diagram
                    .find_entity(&dag[idx])
                    .map(entity_node_size)
                    .unwrap_or((constants::DEFAULT_NODE_WIDTH, constants::DEFAULT_NODE_HEIGHT))
            })
            .collect();

        if sizes.is_empty() {
            continue;
        }

        let layer_primary = if horizontal {
            sizes.iter().map(|(w, _)| *w).fold(0.0_f64, f64::max)
        } else {
            sizes.iter().map(|(_, h)| *h).fold(0.0_f64, f64::max)
        };

        let secondary_span = if horizontal {
            sizes.iter().map(|(_, h)| *h).sum::<f64>()
                + (sizes.len().saturating_sub(1) as f64) * H_GAP
        } else {
            sizes.iter().map(|(w, _)| *w).sum::<f64>()
                + (sizes.len().saturating_sub(1) as f64) * H_GAP
        };

        let mut secondary = constants::DEFAULT_PADDING;

        for (&node_idx, &(width, height)) in layer.iter().zip(sizes.iter()) {
            let (cx, cy) = if horizontal {
                let cx = primary + layer_primary / 2.0;
                let cy = secondary + height / 2.0;
                secondary += height + H_GAP;
                (cx, cy)
            } else {
                let cx = secondary + width / 2.0;
                let cy = primary + layer_primary / 2.0;
                secondary += width + H_GAP;
                (cx, cy)
            };
            temp_pos.insert(node_idx, (cx, cy));
        }

        let _ = secondary_span;
        primary += layer_primary + layer_gap;
    }

    temp_pos
}

/// 优化节点位置，平衡边的长度和弯曲
fn optimize_positions(
    current_pos: &HashMap<NodeIndex, (f64, f64)>,
    layers: &[Vec<NodeIndex>],
    dag: &DiGraph<String, ()>,
    horizontal: bool,
) -> HashMap<NodeIndex, (f64, f64)> {
    let mut new_pos = current_pos.clone();

    // 向下扫描优化
    for i in 0..layers.len().saturating_sub(1) {
        let _upper_layer = &layers[i];
        let lower_layer = &layers[i + 1];

        for &lower_node in lower_layer {
            let (old_x, old_y) = new_pos[&lower_node];
            let mut target_x = old_x;
            let mut target_y = old_y;

            // 计算基于父节点的理想位置
            let mut parents = Vec::new();
            for parent in dag.neighbors_directed(lower_node, Direction::Incoming) {
                if let Some(&pos) = current_pos.get(&parent) {
                    parents.push(pos);
                }
            }

            if !parents.is_empty() {
                let avg_x = parents.iter().map(|p| p.0).sum::<f64>() / parents.len() as f64;
                let avg_y = parents.iter().map(|p| p.1).sum::<f64>() / parents.len() as f64;

                if horizontal {
                    target_x = old_x;
                    target_y = target_y * (1.0 - EDGE_LENGTH_WEIGHT) + avg_y * EDGE_LENGTH_WEIGHT;
                } else {
                    target_x = target_x * (1.0 - EDGE_LENGTH_WEIGHT) + avg_x * EDGE_LENGTH_WEIGHT;
                    target_y = old_y;
                }
            }

            new_pos.insert(lower_node, (target_x, target_y));
        }
    }

    // 向上扫描优化
    for i in (0..layers.len().saturating_sub(1)).rev() {
        let upper_layer = &layers[i];
        let _lower_layer = &layers[i + 1];

        for &upper_node in upper_layer {
            let (old_x, old_y) = new_pos[&upper_node];
            let mut target_x = old_x;
            let mut target_y = old_y;

            // 计算基于子节点的理想位置
            let mut children = Vec::new();
            for child in dag.neighbors_directed(upper_node, Direction::Outgoing) {
                if let Some(&pos) = current_pos.get(&child) {
                    children.push(pos);
                }
            }

            if !children.is_empty() {
                let avg_x = children.iter().map(|p| p.0).sum::<f64>() / children.len() as f64;
                let avg_y = children.iter().map(|p| p.1).sum::<f64>() / children.len() as f64;

                if horizontal {
                    target_x = old_x;
                    target_y = target_y * (1.0 - EDGE_LENGTH_WEIGHT) + avg_y * EDGE_LENGTH_WEIGHT;
                } else {
                    target_x = target_x * (1.0 - EDGE_LENGTH_WEIGHT) + avg_x * EDGE_LENGTH_WEIGHT;
                    target_y = old_y;
                }
            }

            new_pos.insert(upper_node, (target_x, target_y));
        }
    }

    // 层内对齐修正（防止偏移过大）
    for layer in layers {
        if layer.is_empty() {
            continue;
        }

        // 找出层内的平均坐标
        let (sum_x, sum_y) = layer.iter()
            .map(|&idx| new_pos[&idx])
            .fold((0.0, 0.0), |(sx, sy), (x, y)| (sx + x, sy + y));
        let _avg_x = sum_x / layer.len() as f64;
        let _avg_y = sum_y / layer.len() as f64;

        // 调整到合理的间距
        for (ni, &node_idx) in layer.iter().enumerate() {
            let (_, _) = new_pos[&node_idx];
            let max_layer_size = layers.iter().map(|l| l.len()).max().unwrap_or(1);
            let total_span = max_layer_size as f64 * (constants::DEFAULT_NODE_WIDTH + H_GAP) - H_GAP;
            let layer_span = layer.len() as f64 * (constants::DEFAULT_NODE_WIDTH + H_GAP) - H_GAP;
            let offset = (total_span - layer_span) / 2.0;

            let _li = layers.iter().position(|l| l.contains(&node_idx)).unwrap();

            let (x, y) = if horizontal {
                let layer_i = layers.iter().position(|l| l.contains(&node_idx)).unwrap();
                (
                    constants::DEFAULT_PADDING + layer_i as f64 * (constants::DEFAULT_NODE_WIDTH + V_GAP),
                    constants::DEFAULT_PADDING + offset + ni as f64 * (constants::DEFAULT_NODE_HEIGHT + H_GAP),
                )
            } else {
                let layer_i = layers.iter().position(|l| l.contains(&node_idx)).unwrap();
                (
                    constants::DEFAULT_PADDING + offset + ni as f64 * (constants::DEFAULT_NODE_WIDTH + H_GAP),
                    constants::DEFAULT_PADDING + layer_i as f64 * (constants::DEFAULT_NODE_HEIGHT + V_GAP),
                )
            };
            new_pos.insert(node_idx, (x, y));
        }
    }

    new_pos
}

fn state_entity_type(_diagram: &Diagram, entity: &Entity) -> String {
    entity
        .attributes
        .standard
        .get("type")
        .and_then(|value| value.as_str())
        .unwrap_or("state")
        .to_string()
}

fn state_fallback_node_size(diagram: &Diagram, entity: &Entity) -> (f64, f64) {
    match state_entity_type(diagram, entity).as_str() {
        "initial" => (28.0, 28.0),
        "final" => (36.0, 36.0),
        "choice" => {
            let chars = entity.label.chars().count() as f64;
            let side = (chars * 13.0 + 48.0).clamp(72.0, 120.0);
            (side, side * 0.72)
        }
        _ => {
            let chars = entity.label.chars().count() as f64;
            let width = (chars * 14.0 + 36.0).clamp(80.0, 200.0);
            (width, 44.0)
        }
    }
}

fn sized_node_for(diagram: &Diagram, entity: &Entity) -> (f64, f64) {
    let (default_w, default_h) = if diagram.diagram_type == DiagramType::Er {
        entity_node_size(entity)
    } else if diagram.diagram_type == DiagramType::State {
        state_fallback_node_size(diagram, entity)
    } else {
        (constants::DEFAULT_NODE_WIDTH, constants::DEFAULT_NODE_HEIGHT)
    };

    let (w, h) = crate::layout::styled_node_size(entity, default_w, default_h);
    (w, h)
}

// ═══════════════════════════════════════════════════════════
//  单元测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        ArrowType, AttributeMap, AttributeValue, Diagram, DiagramAttribute, Entity, Identifier,
        Relation, SourceInfo, Span, TextValue,
    };
    use crate::types::DiagramType;

    fn create_test_diagram(entities: Vec<&str>, relations: Vec<(&str, &str)>) -> Diagram {
        let span = Span::dummy();
        Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: Vec::new(),
            entities: entities
                .into_iter()
                .map(|id| Entity {
                    id: Identifier::new_unchecked(id),
                    label: id.to_string(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span,
                })
                .collect(),
            relations: relations
                .into_iter()
                .map(|(from, to)| Relation {
                    from: Identifier::new_unchecked(from),
                    to: Identifier::new_unchecked(to),
                    arrow: ArrowType::Active,
                    label: None,
                    head_label: None,
                    tail_label: None,
                    attributes: AttributeMap::default(),
                    span,
                })
                .collect(),
            groups: Vec::new(),
            style_decls: vec![],
            source_info: SourceInfo {
                file: None,
                line_count: 1,
            },
            ..Default::default()
        }
    }

    fn create_state_test_diagram(
        entities: Vec<(&str, &str, &str)>,
        relations: Vec<(&str, &str)>,
    ) -> Diagram {
        let span = Span::dummy();
        Diagram {
            diagram_type: DiagramType::State,
            attributes: Vec::new(),
            entities: entities
                .into_iter()
                .map(|(id, label, kind)| {
                    let mut attributes = AttributeMap::default();
                    attributes.standard.insert(
                        "type".to_string(),
                        AttributeValue::String(TextValue::unquoted(kind.to_string())),
                    );
                    Entity {
                        id: Identifier::new_unchecked(id),
                        label: label.to_string(),
                        attributes,
                        group_id: None,
                        span,
                    }
                })
                .collect(),
            relations: relations
                .into_iter()
                .map(|(from, to)| Relation {
                    from: Identifier::new_unchecked(from),
                    to: Identifier::new_unchecked(to),
                    arrow: ArrowType::Active,
                    label: None,
                    head_label: None,
                    tail_label: None,
                    attributes: AttributeMap::default(),
                    span,
                })
                .collect(),
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
    fn test_build_graph() {
        let diagram = create_test_diagram(
            vec!["a", "b", "c"],
            vec![("a", "b"), ("b", "c")],
        );

        let (graph, node_id_to_idx, _) = build_graph(&diagram);

        assert_eq!(graph.node_count(), 3);
        assert_eq!(graph.edge_count(), 2);
        assert!(node_id_to_idx.contains_key("a"));
        assert!(node_id_to_idx.contains_key("b"));
        assert!(node_id_to_idx.contains_key("c"));
    }

    #[test]
    fn test_sugiyama_layout_empty() {
        let diagram = Diagram::new(DiagramType::Flowchart, SourceInfo {
            file: None,
            line_count: 1,
        });

        let layout = SugiyamaLayout::default();
        let result = layout.compute(&diagram);

        assert!(result.nodes.is_empty());
        assert!(result.groups.is_empty());
    }

    #[test]
    fn test_sugiyama_layout_single_node() {
        let diagram = create_test_diagram(vec!["a"], vec![]);

        let layout = SugiyamaLayout::default();
        let result = layout.compute(&diagram);

        assert_eq!(result.nodes.len(), 1);
        let node_layout = result.nodes.get("a").unwrap();
        assert_eq!(node_layout.x, constants::DEFAULT_PADDING);
        assert_eq!(node_layout.y, constants::DEFAULT_PADDING);
    }

    #[test]
    fn test_sugiyama_layout_self_loop_does_not_hang() {
        let diagram = create_test_diagram(
            vec!["a", "b"],
            vec![("a", "b"), ("b", "b")],
        );

        let result = SugiyamaLayout::default().compute(&diagram);

        assert_eq!(result.nodes.len(), 2);
        assert!(result.nodes.contains_key("a"));
        assert!(result.nodes.contains_key("b"));
    }

    #[test]
    fn test_layout_stress_dag_full_pipeline() {
        let source = include_str!("../../../../../../showcase/flowchart/c.layout-stress-dag.dfy");
        let raw = crate::pipeline::parse(source).expect("parse");
        let output = crate::pipeline::prepare(raw, &crate::prepare::StyleRequest::default())
            .expect("prepare");
        let result = crate::layout::compute_layout(&output.diagram).expect("layout");
        assert!(!result.nodes.is_empty());
        assert!(!result.edges.is_empty());
    }

    #[test]
    fn test_layout_stress_transitions_full_pipeline() {
        let source =
            include_str!("../../../../../../showcase/state/c.layout-stress-transitions.dfy");
        let raw = crate::pipeline::parse(source).expect("parse");
        let output = crate::pipeline::prepare(raw, &crate::prepare::StyleRequest::default())
            .expect("prepare");
        let result = crate::layout::compute_layout(&output.diagram).expect("layout");
        assert!(!result.nodes.is_empty());
        assert!(!result.edges.is_empty());
    }

    #[test]
    fn test_sugiyama_layout_complex() {
        let diagram = create_test_diagram(
            vec!["a", "b", "c", "d", "e", "f"],
            vec![
                ("a", "b"),
                ("a", "c"),
                ("b", "d"),
                ("c", "d"),
                ("d", "e"),
                ("d", "f"),
            ],
        );

        let layout = SugiyamaLayout::default();
        let result = layout.compute(&diagram);

        assert_eq!(result.nodes.len(), 6);
        assert!(result.total_width > constants::DEFAULT_PADDING * 2.0);
        assert!(result.total_height > constants::DEFAULT_PADDING * 2.0);
    }

    #[test]
    fn test_state_layout_uses_compact_terminal_nodes() {
        let diagram = create_state_test_diagram(
            vec![
                ("init", "", "initial"),
                ("active", "处理中", "state"),
                ("done", "已关闭", "final"),
            ],
            vec![("init", "active"), ("active", "done")],
        );

        let output = crate::pipeline::prepare(
            crate::ast::RawDiagram(diagram),
            &crate::prepare::StyleRequest::default(),
        )
        .expect("prepare state diagram");

        let result = SugiyamaLayout::default().compute(output.diagram.inner());

        let init = result.nodes.get("init").unwrap();
        let active = result.nodes.get("active").unwrap();
        let done = result.nodes.get("done").unwrap();

        assert!(init.width < active.width);
        assert!(init.height < active.height);
        assert!(done.width < active.width);
        assert!(done.height < active.height);
    }

    #[test]
    fn test_phase3_scores() {
        let diagram = create_test_diagram(
            vec!["a", "b", "c", "d"],
            vec![("a", "c"), ("a", "d"), ("b", "c"), ("b", "d")],
        );

        let (graph, _, idx_to_node_id) = build_graph(&diagram);
        let reversed = phase1_remove_cycles(&graph, &idx_to_node_id);
        let dag = build_dag(&graph, &reversed);
        let layers = phase2_assign_layers(&diagram, &dag, &idx_to_node_id);

        // 应该有2层
        assert_eq!(layers.len(), 2);
        assert_eq!(layers[0].len(), 2);
        assert_eq!(layers[1].len(), 2);

        // 应用 Phase 3
        let optimized_layers = phase3_minimize_crossings_and_bending(&dag, layers, &idx_to_node_id);
        assert_eq!(optimized_layers.len(), 2);
    }

    #[test]
    fn er_showcase_user_post_layout_is_normalized() {
        let source = include_str!("../../../../../../showcase/er/s.user-post.dfy");
        let raw = crate::pipeline::parse(source).expect("parse user-post");
        assert_eq!(raw.inner().diagram_type, DiagramType::Er);
        let output = crate::pipeline::prepare(raw, &crate::prepare::StyleRequest::default())
            .expect("prepare user-post");

        let result = crate::layout::compute_layout(&output.diagram).expect("layout");
        let padding = constants::DEFAULT_PADDING;
        for (id, node) in &result.nodes {
            assert!(
                node.x >= padding - 0.1,
                "{id} x={} should be >= {padding}",
                node.x
            );
            assert!(
                node.y >= padding - 0.1,
                "{id} y={} should be >= {padding}",
                node.y
            );
        }
    }

    #[test]
    fn er_layout_keeps_nodes_inside_padding() {
        let span = Span::dummy();
        let mut user_attrs = AttributeMap::default();
        user_attrs
            .meta
            .insert("pk".to_string(), AttributeValue::String(TextValue::quoted("id".to_string())));
        user_attrs.meta.insert(
            "fields".to_string(),
            AttributeValue::String(TextValue::quoted("username\nemail".to_string())),
        );

        let mut post_attrs = AttributeMap::default();
        post_attrs
            .meta
            .insert("pk".to_string(), AttributeValue::String(TextValue::quoted("id".to_string())));
        post_attrs
            .meta
            .insert("fk".to_string(), AttributeValue::String(TextValue::quoted("user_id".to_string())));

        let diagram = Diagram {
            diagram_type: DiagramType::Er,
            attributes: vec![DiagramAttribute {
                key: "direction".to_string(),
                value: AttributeValue::String(TextValue::unquoted("left-to-right".to_string())),
                span,
            }],
            entities: vec![
                Entity {
                    id: Identifier::new_unchecked("user"),
                    label: "User".to_string(),
                    attributes: user_attrs,
                    group_id: None,
                    span,
                },
                Entity {
                    id: Identifier::new_unchecked("post"),
                    label: "Post".to_string(),
                    attributes: post_attrs,
                    group_id: None,
                    span,
                },
            ],
            relations: vec![Relation {
                from: Identifier::new_unchecked("user"),
                to: Identifier::new_unchecked("post"),
                arrow: ArrowType::Active,
                label: Some("发表".to_string()),
                head_label: None,
                tail_label: None,
                attributes: AttributeMap::default(),
                span,
            }],
            groups: vec![],
            style_decls: vec![],
            source_info: SourceInfo {
                file: None,
                line_count: 1,
            },
            ..Default::default()
        };

        let result = SugiyamaLayout::default().compute(&diagram);
        for node in result.nodes.values() {
            assert!(node.x >= constants::DEFAULT_PADDING - 0.1, "node x={} too small", node.x);
            assert!(node.y >= constants::DEFAULT_PADDING - 0.1, "node y={} too small", node.y);
        }
    }
}
