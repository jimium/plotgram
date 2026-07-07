//! 两阶段架构图布局：组内 Sugiyama → 组间宏观定位 → 全局坐标回填
//!
//! 人类画架构图的顺序是「先定语义舞台（group），再在框内摆节点」。
//! 本模块将 group 提升为一等公民，而非节点包围盒的后验产物。
//!
//! # 与通用分治框架的关系
//!
//! 本模块复用 [`crate::layout::node::common::divide_and_conquer`] 的
//! `IntraLayout`、`GroupTree` 数据结构。组内布局的具体实现（含 hub 居中、
//! client 对齐等特化优化）保留在本模块。未来 flowchart 分治布局将实现
//! `IntraGroupLayouter` trait，共用同一套类型基础。

use super::group_sizing::{apply_group_sizing_policy, parse_group_sizing, GroupSizingPolicy};
use super::group_layout_hint::{
    align_nodes_in_column, assign_ranks_for_mode, parse_group_layout_hint,
    resolve_group_layout_mode, GroupLayoutHint, GroupLayoutMode,
};
use super::layout::acyclic::is_effective_edge;
use super::layout::constants::{
    GROUP_GAP_X, GROUP_LABEL_HEIGHT, INTRA_LAYER_GAP, LAYER_GAP, NEIGHBOR_PULL_FACTOR,
};
use super::layout::coordinate::{
    align_client_nodes_to_hubs, center_group_hub_nodes, layer_centers_from_placed,
    pull_toward_neighbors, rebalance_infrastructure_layers, resolve_x_overlaps,
    uniform_initial_positions,
};
use super::layout::order::{build_layers, order_layers_group_aware};
use super::layout::postprocess::{clamp_to_canvas, compute_total_size};
use super::layout::rank::{assign_intra_ranks, assign_super_macro_ranks};
use super::layout::types::{GraphIndex, GroupMap};
use crate::layout::algorithm_config::ArchitectureV2LayoutConfig;
use crate::ast::Diagram;
use crate::layout::constants;
use crate::layout::node::common::divide_and_conquer::{
    GroupTree, IntraGroupLayouter, IntraLayout,
};
use crate::layout::node::common::group_bounds::GroupPadding;
use crate::layout::{GroupLayout, LayoutResult, NodeLayout};
use std::collections::{HashMap, HashSet};

/// 宏观布局块：顶层 group 或无组节点簇
struct MacroBlock {
    id: String,
    is_group: bool,
    width: f64,
    height: f64,
    x: f64,
    y: f64,
    intra: IntraLayout,
}

impl super::group_sizing::GroupWidthBlock for MacroBlock {
    fn block_id(&self) -> &str {
        &self.id
    }

    fn is_group_block(&self) -> bool {
        self.is_group
    }

    fn block_width(&self) -> f64 {
        self.width
    }

    fn set_block_width(&mut self, width: f64) {
        self.width = width;
    }

    fn shift_intra_nodes_x(&mut self, delta: f64) {
        for nl in self.intra.nodes.values_mut() {
            nl.x += delta;
        }
        self.intra.content_width += delta;
    }
}

pub(super) fn compute_two_phase_layout(
    diagram: &Diagram,
    graph: &GraphIndex,
    group_map: &GroupMap,
    sizes: &HashMap<String, (f64, f64)>,
    reversed_edges: &HashSet<(String, String)>,
    layout_config: ArchitectureV2LayoutConfig,
) -> LayoutResult {
    let padding = GroupPadding::uniform(layout_config.group_padding, GROUP_LABEL_HEIGHT);
    let canvas_padding = layout_config.padding;

    // ── Phase A: 组内布局（递归，支持嵌套分组）──
    let group_tree = GroupTree::build(diagram);
    let mut intra_by_group: HashMap<String, IntraLayout> = HashMap::new();
    for gid in &group_map.top_groups {
        intra_by_group.insert(
            gid.clone(),
            layout_intra_group_recursive(
                diagram,
                gid,
                &group_tree,
                graph,
                sizes,
                reversed_edges,
                &padding,
            ),
        );
    }

    // ── Phase B: 宏观超级节点分层 ──
    let (super_members, super_edges, pair_edge_counts) =
        build_super_graph(graph, group_map, reversed_edges);
    let macro_ranks = assign_super_macro_ranks(&super_members, &super_edges);

    let mut blocks = build_macro_blocks(
        diagram,
        group_map,
        sizes,
        &intra_by_group,
        &super_members,
        graph,
        reversed_edges,
        &padding,
    );

    let sizing = parse_group_sizing(diagram);
    apply_group_sizing_policy(sizing, &group_map.top_groups, &mut blocks);

    position_macro_blocks(
        &mut blocks,
        &macro_ranks,
        &super_edges,
        &pair_edge_counts,
        canvas_padding,
    );

    // ── Phase C: 回填全局坐标 ──
    let (mut nodes, groups) = compose_global_layout(&blocks, &padding);

    // ── Phase C+: 两阶段 spacing 微调 ──
    // 组框已定，对涉及跨组边的组内节点朝跨组边方向做小幅 x 微调，
    // 减少跨组边折弯。这是"先定组框再微调组内节点"的反转步骤。
    // uniform sizing 策略下跳过微调，保持组内居中语义。
    if sizing != GroupSizingPolicy::Uniform {
        nudge_intra_nodes_toward_cross_group_edges(
            &mut nodes,
            &groups,
            &super_edges,
            &super_members,
            graph,
            reversed_edges,
        );
    }

    // ── 后处理：基础设施行居中 ──
    // 从元数据重建全局层（替代旧版从 y 坐标反推）
    let layers = rebuild_layers_from_metadata(&blocks, &macro_ranks);
    rebalance_infrastructure_layers(graph, group_map, &layers, sizes, &mut nodes);
    clamp_to_canvas(&mut nodes, sizes);

    let (total_width, total_height) = compute_total_size(&nodes, &groups);

    // 从全局层导出 sugiyama_ranks（entity_id → rank），供拓扑意图满足度评估使用。
    let sugiyama_ranks: HashMap<String, usize> = layers
        .iter()
        .enumerate()
        .flat_map(|(rank, layer)| layer.iter().map(move |id| (id.clone(), rank)))
        .collect();

    LayoutResult {
        nodes,
        groups,
        edges: vec![],
        total_width,
        total_height,
        hints: crate::layout::LayoutHints {
            edge_routing_style: crate::layout::EdgeRoutingStyle::Orthogonal,
            sugiyama_ranks: Some(sugiyama_ranks),
            ..Default::default()
        },
    }
}

// ─── Phase A: 组内布局 ───────────────────────────────────

fn layout_intra_group(
    diagram: &Diagram,
    group_id: &str,
    members: &[String],
    graph: &GraphIndex,
    sizes: &HashMap<String, (f64, f64)>,
    reversed: &HashSet<(String, String)>,
) -> IntraLayout {
    if members.is_empty() {
        return IntraLayout {
            nodes: HashMap::new(),
            content_width: 0.0,
            content_height: 0.0,
            layers: vec![],
        };
    }

    if members.len() == 1 {
        let id = &members[0];
        let (w, h) = sizes
            .get(id)
            .copied()
            .unwrap_or((constants::DEFAULT_NODE_WIDTH, constants::DEFAULT_NODE_HEIGHT));
        return IntraLayout {
            nodes: HashMap::from([(
                id.clone(),
                NodeLayout {
                    x: 0.0,
                    y: 0.0,
                    width: w,
                    height: h,
                    ..Default::default()
                },
            )]),
            content_width: w,
            content_height: h,
            layers: vec![vec![id.clone()]],
        };
    }

    let member_set: HashSet<String> = members.iter().cloned().collect();
    let intra_map = synthetic_group_map(group_id, members);

    let hint = diagram
        .find_group(group_id)
        .map(parse_group_layout_hint)
        .unwrap_or(GroupLayoutHint::Auto);
    let mode = resolve_group_layout_mode(hint, members, graph, reversed);
    let ranks = assign_ranks_for_mode(&mode, members, graph, reversed);
    let layers = build_layers(&ranks);
    let ordered_layers = order_layers_group_aware(
        graph,
        &intra_map,
        &layers,
        reversed,
    );

    let mut nodes = assign_coordinates_intra(
        graph,
        &ordered_layers,
        sizes,
        &member_set,
    );

    center_group_hub_nodes(graph, &intra_map, &ordered_layers, sizes, &mut nodes);
    align_client_nodes_to_hubs(graph, &intra_map, &ordered_layers, sizes, &mut nodes);

    if mode == GroupLayoutMode::Vertical {
        align_nodes_in_column(&mut nodes);
    }

    normalize_to_origin(&mut nodes);
    let (content_width, content_height) = content_bbox(&nodes);

    IntraLayout {
        nodes,
        content_width,
        content_height,
        layers: ordered_layers.clone(),
    }
}

/// 递归版组内布局：支持嵌套分组
///
/// - 叶子组（无子组）：走 `layout_intra_group` 原逻辑
/// - 容器组（有子组）：递归布局每个子组，然后将子组视为宏观块做组间定位
///
/// 容器组的 IntraLayout 包含所有后代节点的局部坐标（相对容器组内容区原点），
/// layers 反映宏观层级（同 macro rank 的子组 intra layer 对齐）。
fn layout_intra_group_recursive(
    diagram: &Diagram,
    group_id: &str,
    group_tree: &GroupTree,
    graph: &GraphIndex,
    sizes: &HashMap<String, (f64, f64)>,
    reversed: &HashSet<(String, String)>,
    padding: &GroupPadding,
) -> IntraLayout {
    let children = group_tree.children_of(group_id);
    let direct_entities = group_tree.entities_of(group_id).to_vec();

    // 叶子组：走原逻辑
    if children.is_empty() {
        let all_members = group_tree.descendant_entities(group_id);
        return layout_intra_group(diagram, group_id, &all_members, graph, sizes, reversed);
    }

    // 容器组：递归布局子组 + 直接实体
    // 1. 递归布局每个子组
    let mut child_intras: HashMap<String, IntraLayout> = HashMap::new();
    for child_id in children {
        let child_intra = layout_intra_group_recursive(
            diagram,
            child_id,
            group_tree,
            graph,
            sizes,
            reversed,
            padding,
        );
        child_intras.insert(child_id.clone(), child_intra);
    }

    // 2. 直接实体作为"无组节点块"布局（若有）
    let direct_intra = if direct_entities.is_empty() {
        None
    } else {
        Some(layout_ungrouped_cluster(
            diagram,
            &direct_entities,
            graph,
            sizes,
            reversed,
        ))
    };

    // 3. 构建宏观块（子组块 + 直接实体块）
    let mut blocks: Vec<IntraMacroBlock> = Vec::new();
    for child_id in children {
        let intra = child_intras.get(child_id).cloned().unwrap_or(IntraLayout {
            nodes: HashMap::new(),
            content_width: 0.0,
            content_height: 0.0,
            layers: vec![],
        });
        blocks.push(IntraMacroBlock {
            id: child_id.clone(),
            is_group: true,
            width: intra.content_width + padding.x_delta,
            height: intra.content_height + padding.y_delta,
            x: 0.0,
            y: 0.0,
            intra,
        });
    }
    if let Some(di) = &direct_intra {
        blocks.push(IntraMacroBlock {
            id: format!("@direct:{group_id}"),
            is_group: false,
            width: di.content_width,
            height: di.content_height,
            x: 0.0,
            y: 0.0,
            intra: di.clone(),
        });
    }

    // 4. 构建超级节点图（基于跨子组边）
    let (super_members, super_edges, pair_edge_counts) =
        build_super_graph_for_group(group_id, group_tree, graph, reversed);
    let macro_ranks = assign_super_macro_ranks(&super_members, &super_edges);

    // 4.5 应用 uniform sizing：同 rank 的子组块拉齐宽度（嵌套层级也生效）
    let sizing = parse_group_sizing(diagram);
    let child_group_ids: Vec<String> = children.to_vec();
    apply_group_sizing_policy(sizing, &child_group_ids, &mut blocks);

    // 5. 宏观定位（复用 position_macro_blocks 逻辑，padding=0 因为容器组内部无画布 padding）
    position_intra_macro_blocks(&mut blocks, &macro_ranks, &super_edges, &pair_edge_counts);

    // 6. 合并为单个 IntraLayout
    compose_intra_layout_recursive(group_id, &blocks, padding, &child_intras, &direct_intra)
}

/// 容器组内部的宏观块（与顶层 MacroBlock 类似，但仅用于组内）
struct IntraMacroBlock {
    id: String,
    is_group: bool,
    width: f64,
    height: f64,
    x: f64,
    y: f64,
    intra: IntraLayout,
}

impl super::group_sizing::GroupWidthBlock for IntraMacroBlock {
    fn block_id(&self) -> &str {
        &self.id
    }

    fn is_group_block(&self) -> bool {
        self.is_group
    }

    fn block_width(&self) -> f64 {
        self.width
    }

    fn set_block_width(&mut self, width: f64) {
        self.width = width;
    }

    fn shift_intra_nodes_x(&mut self, delta: f64) {
        for nl in self.intra.nodes.values_mut() {
            nl.x += delta;
        }
        self.intra.content_width += delta;
    }
}

/// architecture_v2 的组内布局策略（实现 [`IntraGroupLayouter`]）
///
/// 这是 `layout_intra_group_recursive` 的 thin wrapper，将其包装为 trait 实现。
/// 当前 `compute_two_phase_layout` 仍直接调用 `layout_intra_group_recursive`，
/// 未走 trait 调度——此 struct 仅供文档化关系和未来统一调度使用。
///
/// # 为什么不改变实际调度
///
/// `compute_two_phase_layout` 的 Phase A 需要对每个顶层 group 调用一次组内布局，
/// 并在 Phase B 中复用 `group_tree` / `graph` / `sizes` / `reversed` 等上下文。
/// 强行改为 trait 调度会增加间接层而无功能收益。
#[allow(dead_code)]
pub struct ArchitectureV2IntraLayouter<'a> {
    diagram: &'a Diagram,
    group_tree: &'a GroupTree,
    graph: &'a GraphIndex,
    sizes: &'a HashMap<String, (f64, f64)>,
    reversed: &'a HashSet<(String, String)>,
    padding: &'a GroupPadding,
}

#[allow(dead_code)]
impl<'a> ArchitectureV2IntraLayouter<'a> {
    pub fn new(
        diagram: &'a Diagram,
        group_tree: &'a GroupTree,
        graph: &'a GraphIndex,
        sizes: &'a HashMap<String, (f64, f64)>,
        reversed: &'a HashSet<(String, String)>,
        padding: &'a GroupPadding,
    ) -> Self {
        Self {
            diagram,
            group_tree,
            graph,
            sizes,
            reversed,
            padding,
        }
    }
}

impl<'a> IntraGroupLayouter for ArchitectureV2IntraLayouter<'a> {
    fn layout_intra(&self, group_id: &str, _members: &[String]) -> IntraLayout {
        layout_intra_group_recursive(
            self.diagram,
            group_id,
            self.group_tree,
            self.graph,
            self.sizes,
            self.reversed,
            self.padding,
        )
    }
}

/// 为容器组构建超级节点图
///
/// 超级节点 = 子组 + 直接实体块
/// 超级边 = 跨越不同超级节点的有效边
fn build_super_graph_for_group(
    group_id: &str,
    group_tree: &GroupTree,
    graph: &GraphIndex,
    reversed: &HashSet<(String, String)>,
) -> (
    HashMap<String, Vec<String>>,
    HashSet<(String, String)>,
    HashMap<(String, String), usize>,
) {
    let children = group_tree.children_of(group_id);
    let direct_entities = group_tree.entities_of(group_id);

    // 超级节点成员：子组→后代实体，直接实体块→直接实体
    let mut super_members: HashMap<String, Vec<String>> = HashMap::new();
    for child_id in children {
        super_members.insert(child_id.clone(), group_tree.descendant_entities(child_id));
    }
    if !direct_entities.is_empty() {
        super_members.insert(
            format!("@direct:{group_id}"),
            direct_entities.to_vec(),
        );
    }

    // 节点 → 所属超级节点
    let mut node_to_super: HashMap<String, String> = HashMap::new();
    for (super_id, members) in &super_members {
        for m in members {
            node_to_super.insert(m.clone(), super_id.clone());
        }
    }

    // 超级边：跨超级节点的有效边
    let mut super_edges: HashSet<(String, String)> = HashSet::new();
    // Phase 3：per-pair 边数（归一化为无向 pair）
    let mut pair_edge_counts: HashMap<(String, String), usize> = HashMap::new();
    for (super_id, members) in &super_members {
        for node in members {
            if let Some(succs) = graph.out_edges.get(node) {
                for succ in succs {
                    if !is_effective_edge(node, succ, reversed) {
                        continue;
                    }
                    let from_super = super_id.clone();
                    let to_super = match node_to_super.get(succ) {
                        Some(s) => s.clone(),
                        None => continue,
                    };
                    if from_super != to_super {
                        super_edges.insert((from_super.clone(), to_super.clone()));
                        let pair = if from_super <= to_super {
                            (from_super, to_super)
                        } else {
                            (to_super, from_super)
                        };
                        *pair_edge_counts.entry(pair).or_insert(0) += 1;
                    }
                }
            }
        }
    }

    (super_members, super_edges, pair_edge_counts)
}

/// 容器组内宏观块定位（复用顶层 position_macro_blocks 逻辑，但 padding=0）
fn position_intra_macro_blocks(
    blocks: &mut [IntraMacroBlock],
    macro_ranks: &HashMap<String, usize>,
    super_edges: &HashSet<(String, String)>,
    pair_edge_counts: &HashMap<(String, String), usize>,
) {
    if blocks.is_empty() {
        return;
    }

    let max_rank = macro_ranks.values().copied().max().unwrap_or(0);
    let cross_edge_counts = count_cross_edges_per_rank_gap(super_edges, macro_ranks);
    let mut y_cursor = 0.0;

    for rank in 0..=max_rank {
        let mut rank_indices: Vec<usize> = blocks
            .iter()
            .enumerate()
            .filter(|(_, b)| macro_ranks.get(&b.id).copied().unwrap_or(0) == rank)
            .map(|(i, _)| i)
            .collect();
        rank_indices.sort_by(|&a, &b| blocks[a].id.cmp(&blocks[b].id));

        if rank_indices.is_empty() {
            continue;
        }

        let max_height = rank_indices
            .iter()
            .map(|&i| blocks[i].height)
            .fold(0.0_f64, f64::max);

        if rank_indices.len() == 1 {
            let i = rank_indices[0];
            blocks[i].x = 0.0;
            blocks[i].y = y_cursor;
        } else {
            // Phase 3：per-pair 通道间距（同 position_macro_blocks，含无边靠拢）
            let mut x_cursor = 0.0;
            for (pos, &i) in rank_indices.iter().enumerate() {
                blocks[i].x = x_cursor;
                blocks[i].y = y_cursor;
                x_cursor += blocks[i].width;
                if pos + 1 < rank_indices.len() {
                    let next_i = rank_indices[pos + 1];
                    let pair = if blocks[i].id <= blocks[next_i].id {
                        (blocks[i].id.clone(), blocks[next_i].id.clone())
                    } else {
                        (blocks[next_i].id.clone(), blocks[i].id.clone())
                    };
                    let pair_count = pair_edge_counts.get(&pair).copied().unwrap_or(0);
                    x_cursor += adaptive_group_gap(pair_count);
                }
            }
        }

        let extra_layer_gap = cross_edge_counts
            .get(&rank)
            .map(|&c| (c as f64 * CROSS_EDGE_LAYER_GAP_SCALE).min(MAX_EXTRA_LAYER_GAP))
            .unwrap_or(0.0);
        let effective_layer_gap = LAYER_GAP + extra_layer_gap;

        y_cursor += max_height + effective_layer_gap;
    }
}

/// 合并容器组内的宏观块为单个 IntraLayout
///
/// - 节点坐标：block.x + padding.x + local.x（组块）或 block.x + local.x（直接实体块）
/// - layers：按 macro rank 顺序，同 rank 内对齐各 block 的 intra layer
fn compose_intra_layout_recursive(
    _group_id: &str,
    blocks: &[IntraMacroBlock],
    padding: &GroupPadding,
    _child_intras: &HashMap<String, IntraLayout>,
    _direct_intra: &Option<IntraLayout>,
) -> IntraLayout {
    let mut nodes: HashMap<String, NodeLayout> = HashMap::new();
    let mut max_x = 0.0_f64;
    let mut max_y = 0.0_f64;

    for block in blocks {
        let (offset_x, offset_y) = if block.is_group {
            (block.x + padding.x, block.y + padding.y_top)
        } else {
            (block.x, block.y)
        };
        for (nid, local) in &block.intra.nodes {
            let nx = offset_x + local.x;
            let ny = offset_y + local.y;
            max_x = max_x.max(nx + local.width);
            max_y = max_y.max(ny + local.height);
            nodes.insert(
                nid.clone(),
                NodeLayout {
                    x: nx,
                    y: ny,
                    width: local.width,
                    height: local.height,
                    ..Default::default()
                },
            );
        }
    }

    // 重建 layers：按 block 的 y 顺序，合并 y 接近的 block 的 intra layers
    // 简化策略：直接按 block 顺序拼接 intra.layers（宏观定位已保证 y 不重叠）
    let mut layers: Vec<Vec<String>> = Vec::new();
    let mut sorted_blocks: Vec<&IntraMacroBlock> = blocks.iter().collect();
    sorted_blocks.sort_by(|a, b| {
        a.y.partial_cmp(&b.y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
    });

    for block in &sorted_blocks {
        for intra_layer in &block.intra.layers {
            // 过滤掉不属于当前块的节点（防御性）
            let filtered: Vec<String> = intra_layer
                .iter()
                .filter(|n| nodes.contains_key(*n))
                .cloned()
                .collect();
            if !filtered.is_empty() {
                layers.push(filtered);
            }
        }
    }

    IntraLayout {
        nodes,
        content_width: max_x,
        content_height: max_y,
        layers,
    }
}

/// 无组节点簇的局部水平布局（如 db + mq）
fn layout_ungrouped_cluster(
    diagram: &Diagram,
    members: &[String],
    graph: &GraphIndex,
    sizes: &HashMap<String, (f64, f64)>,
    reversed: &HashSet<(String, String)>,
) -> IntraLayout {
    if members.is_empty() {
        return IntraLayout {
            nodes: HashMap::new(),
            content_width: 0.0,
            content_height: 0.0,
            layers: vec![],
        };
    }

    if members.len() == 1 {
        return layout_intra_group(diagram, "@solo", members, graph, sizes, reversed);
    }

    let ranks = assign_intra_ranks(members, graph, reversed);
    let layers = build_layers(&ranks);
    let member_set: HashSet<String> = members.iter().cloned().collect();

    let mut nodes = assign_coordinates_intra(graph, &layers, sizes, &member_set);
    normalize_to_origin(&mut nodes);
    let (content_width, content_height) = content_bbox(&nodes);

    IntraLayout {
        nodes,
        content_width,
        content_height,
        layers,
    }
}

fn synthetic_group_map(group_id: &str, members: &[String]) -> GroupMap {
    let mut node_to_top_group = HashMap::new();
    for member in members {
        node_to_top_group.insert(member.clone(), group_id.to_string());
    }

    GroupMap {
        node_to_top_group,
        top_group_members: HashMap::from([(group_id.to_string(), members.to_vec())]),
        top_groups: vec![group_id.to_string()],
        ungrouped: vec![],
    }
}

/// 组内坐标分配：局部原点，邻接拉力仅限组内成员
fn assign_coordinates_intra(
    graph: &GraphIndex,
    layers: &[Vec<String>],
    sizes: &HashMap<String, (f64, f64)>,
    member_set: &HashSet<String>,
) -> HashMap<String, NodeLayout> {
    let mut nodes = HashMap::new();

    let layer_heights: Vec<f64> = layers
        .iter()
        .map(|layer| {
            layer
                .iter()
                .map(|node| {
                    sizes
                        .get(node)
                        .map(|(_, h)| *h)
                        .unwrap_or(constants::DEFAULT_NODE_HEIGHT)
                })
                .fold(0.0_f64, f64::max)
        })
        .collect();

    let mut layer_y_offsets = vec![0.0];
    for i in 1..layers.len() {
        layer_y_offsets.push(layer_y_offsets[i - 1] + layer_heights[i - 1] + INTRA_LAYER_GAP);
    }

    for (layer_idx, layer) in layers.iter().enumerate() {
        let y_center = layer_y_offsets[layer_idx] + layer_heights[layer_idx] / 2.0;
        let mut positions = uniform_initial_positions(layer, sizes);

        let upper_x = if layer_idx > 0 {
            Some(layer_centers_from_placed(
                &layers[layer_idx - 1],
                &nodes,
                sizes,
            ))
        } else {
            None
        };
        let lower_x = if layer_idx + 1 < layers.len() {
            Some(layer_centers_from_placed(
                &layers[layer_idx + 1],
                &nodes,
                sizes,
            ))
        } else {
            None
        };

        // 组内坐标分配使用 6 轮迭代（比全局 8 轮少，组内图较小收敛更快）
        for _ in 0..6 {
            if let Some(ref upper) = upper_x {
                pull_toward_neighbors(
                    layer,
                    &mut positions,
                    upper,
                    graph,
                    Some(member_set),
                    true,
                    NEIGHBOR_PULL_FACTOR,
                );
            }
            if let Some(ref lower) = lower_x {
                pull_toward_neighbors(
                    layer,
                    &mut positions,
                    lower,
                    graph,
                    Some(member_set),
                    false,
                    NEIGHBOR_PULL_FACTOR,
                );
            }
        }

        let adjusted = resolve_x_overlaps(layer, &positions, sizes);

        for (i, node) in layer.iter().enumerate() {
            let (width, height) = sizes
                .get(node)
                .copied()
                .unwrap_or((constants::DEFAULT_NODE_WIDTH, constants::DEFAULT_NODE_HEIGHT));
            let x_center = adjusted[i];
            nodes.insert(
                node.clone(),
                NodeLayout {
                    x: x_center - width / 2.0,
                    y: y_center - height / 2.0,
                    width,
                    height,
                    ..Default::default()
                },
            );
        }
    }

    nodes
}

fn normalize_to_origin(nodes: &mut HashMap<String, NodeLayout>) {
    if nodes.is_empty() {
        return;
    }
    let min_x = nodes.values().map(|n| n.x).fold(f64::INFINITY, f64::min);
    let min_y = nodes.values().map(|n| n.y).fold(f64::INFINITY, f64::min);
    for nl in nodes.values_mut() {
        nl.x -= min_x;
        nl.y -= min_y;
    }
}

fn content_bbox(nodes: &HashMap<String, NodeLayout>) -> (f64, f64) {
    if nodes.is_empty() {
        return (0.0, 0.0);
    }
    let max_x = nodes.values().map(|n| n.x + n.width).fold(0.0_f64, f64::max);
    let max_y = nodes.values().map(|n| n.y + n.height).fold(0.0_f64, f64::max);
    (max_x, max_y)
}

// ─── Phase B: 宏观组间定位 ───────────────────────────────

fn build_super_graph(
    graph: &GraphIndex,
    group_map: &GroupMap,
    reversed: &HashSet<(String, String)>,
) -> (
    HashMap<String, Vec<String>>,
    HashSet<(String, String)>,
    HashMap<(String, String), usize>,
) {
    let mut super_members: HashMap<String, Vec<String>> = HashMap::new();

    for gid in &group_map.top_groups {
        super_members.insert(
            gid.clone(),
            group_map
                .top_group_members
                .get(gid)
                .cloned()
                .unwrap_or_default(),
        );
    }
    for node in &group_map.ungrouped {
        super_members.insert(format!("@node:{node}"), vec![node.clone()]);
    }

    let mut super_edges: HashSet<(String, String)> = HashSet::new();
    // Phase 3：per-pair 边数（归一化为无向 pair），用于按 pair 计算通道间距
    let mut pair_edge_counts: HashMap<(String, String), usize> = HashMap::new();
    for node in &graph.node_ids {
        if let Some(succs) = graph.out_edges.get(node) {
            for succ in succs {
                if !is_effective_edge(node, succ, reversed) {
                    continue;
                }
                let from_super = super_node_id(node, group_map);
                let to_super = super_node_id(succ, group_map);
                if from_super != to_super {
                    super_edges.insert((from_super.clone(), to_super.clone()));
                    // 归一化为无向 pair (min, max)
                    let pair = if from_super <= to_super {
                        (from_super, to_super)
                    } else {
                        (to_super, from_super)
                    };
                    *pair_edge_counts.entry(pair).or_insert(0) += 1;
                }
            }
        }
    }

    (super_members, super_edges, pair_edge_counts)
}

fn super_node_id(node: &str, group_map: &GroupMap) -> String {
    group_map
        .node_to_top_group
        .get(node)
        .cloned()
        .unwrap_or_else(|| format!("@node:{node}"))
}

fn build_macro_blocks(
    diagram: &Diagram,
    group_map: &GroupMap,
    sizes: &HashMap<String, (f64, f64)>,
    intra_by_group: &HashMap<String, IntraLayout>,
    super_members: &HashMap<String, Vec<String>>,
    graph: &GraphIndex,
    reversed: &HashSet<(String, String)>,
    padding: &GroupPadding,
) -> Vec<MacroBlock> {
    let mut blocks = Vec::new();

    for gid in &group_map.top_groups {
        let intra = intra_by_group
            .get(gid)
            .cloned()
            .unwrap_or(IntraLayout {
                nodes: HashMap::new(),
                content_width: 0.0,
                content_height: 0.0,
                layers: vec![],
            });
        blocks.push(MacroBlock {
            id: gid.clone(),
            is_group: true,
            width: intra.content_width + padding.x_delta,
            height: intra.content_height + padding.y_delta,
            x: 0.0,
            y: 0.0,
            intra,
        });
    }

    // 无组节点：每个超级节点独立成块（同宏观 rank 时水平排列）
    let mut ungrouped_supers: Vec<String> = super_members
        .keys()
        .filter(|id| id.starts_with("@node:"))
        .cloned()
        .collect();
    ungrouped_supers.sort();

    for super_id in ungrouped_supers {
        let members = super_members.get(&super_id).cloned().unwrap_or_default();
        let intra = layout_ungrouped_cluster(diagram, &members, graph, sizes, reversed);
        blocks.push(MacroBlock {
            id: super_id,
            is_group: false,
            width: intra.content_width,
            height: intra.content_height,
            x: 0.0,
            y: 0.0,
            intra,
        });
    }

    blocks
}

/// 每条跨组边为层间距额外增加的像素
const CROSS_EDGE_LAYER_GAP_SCALE: f64 = 8.0;
/// 层间距额外增加的上限
const MAX_EXTRA_LAYER_GAP: f64 = 60.0;
/// 每条同 rank 跨组边为组间距额外增加的像素
const CROSS_EDGE_GROUP_GAP_SCALE: f64 = 6.0;
/// 组间距额外增加的上限
const MAX_EXTRA_GROUP_GAP: f64 = 40.0;
/// 无跨组边的相邻组间距缩减比例（相对 GROUP_GAP_X）
const NO_EDGE_GROUP_GAP_RATIO: f64 = 0.5;

/// 计算相邻组块间的自适应间距
///
/// - 有跨组边：`GROUP_GAP_X + min(edge_count * scale, max_extra)`（边多则间距大）
/// - 无跨组边：`GROUP_GAP_X * NO_EDGE_GROUP_GAP_RATIO`（自动靠拢，节省空间）
fn adaptive_group_gap(pair_edge_count: usize) -> f64 {
    if pair_edge_count == 0 {
        GROUP_GAP_X * NO_EDGE_GROUP_GAP_RATIO
    } else {
        let extra = (pair_edge_count as f64 * CROSS_EDGE_GROUP_GAP_SCALE)
            .min(MAX_EXTRA_GROUP_GAP);
        GROUP_GAP_X + extra
    }
}

/// 统计每对相邻 macro rank 之间的跨组边数
///
/// 返回 `gap_rank -> cross_edge_count`，其中 `gap_rank = min(from_rank, to_rank)`，
/// 表示该 rank 到下一 rank 之间的跨组边密度。
fn count_cross_edges_per_rank_gap(
    super_edges: &HashSet<(String, String)>,
    macro_ranks: &HashMap<String, usize>,
) -> HashMap<usize, usize> {
    let mut counts: HashMap<usize, usize> = HashMap::new();
    for (from, to) in super_edges {
        let from_rank = macro_ranks.get(from).copied().unwrap_or(0);
        let to_rank = macro_ranks.get(to).copied().unwrap_or(0);
        if from_rank == to_rank {
            continue;
        }
        let gap = from_rank.min(to_rank);
        *counts.entry(gap).or_insert(0) += 1;
    }
    counts
}

fn position_macro_blocks(
    blocks: &mut [MacroBlock],
    macro_ranks: &HashMap<String, usize>,
    super_edges: &HashSet<(String, String)>,
    pair_edge_counts: &HashMap<(String, String), usize>,
    canvas_padding: f64,
) {
    if blocks.is_empty() {
        return;
    }

    let max_rank = macro_ranks.values().copied().max().unwrap_or(0);
    let cross_edge_counts = count_cross_edges_per_rank_gap(super_edges, macro_ranks);
    let mut y_cursor = canvas_padding;

    for rank in 0..=max_rank {
        let mut rank_indices: Vec<usize> = blocks
            .iter()
            .enumerate()
            .filter(|(_, b)| macro_ranks.get(&b.id).copied().unwrap_or(0) == rank)
            .map(|(i, _)| i)
            .collect();
        rank_indices.sort_by_key(|&i| blocks[i].id.clone());

        if rank_indices.is_empty() {
            continue;
        }

        let max_height = rank_indices
            .iter()
            .map(|&i| blocks[i].height)
            .fold(0.0_f64, f64::max);

        if rank_indices.len() == 1 {
            let i = rank_indices[0];
            blocks[i].x = canvas_padding;
            blocks[i].y = y_cursor;
        } else {
            // Phase 3：per-pair 通道间距 — 相邻 block 对按其跨组边数计算独立间距，
            // 边数多的 pair 获得更宽通道，无边的 pair 自动靠拢。
            let mut x_cursor = canvas_padding;
            for (pos, &i) in rank_indices.iter().enumerate() {
                blocks[i].x = x_cursor;
                blocks[i].y = y_cursor;
                x_cursor += blocks[i].width;
                // 计算与下一个 block 的 pair 间距
                if pos + 1 < rank_indices.len() {
                    let next_i = rank_indices[pos + 1];
                    let pair = if blocks[i].id <= blocks[next_i].id {
                        (blocks[i].id.clone(), blocks[next_i].id.clone())
                    } else {
                        (blocks[next_i].id.clone(), blocks[i].id.clone())
                    };
                    let pair_count = pair_edge_counts.get(&pair).copied().unwrap_or(0);
                    x_cursor += adaptive_group_gap(pair_count);
                }
            }
        }

        // 跨 rank 跨组边密度 → 放大层间距
        let extra_layer_gap = cross_edge_counts
            .get(&rank)
            .map(|&c| (c as f64 * CROSS_EDGE_LAYER_GAP_SCALE).min(MAX_EXTRA_LAYER_GAP))
            .unwrap_or(0.0);
        let effective_layer_gap = LAYER_GAP + extra_layer_gap;

        y_cursor += max_height + effective_layer_gap;
    }
}

// ─── Phase C: 全局坐标回填 ───────────────────────────────

fn compose_global_layout(
    blocks: &[MacroBlock],
    padding: &GroupPadding,
) -> (HashMap<String, NodeLayout>, HashMap<String, GroupLayout>) {
    let mut nodes = HashMap::new();
    let mut groups = HashMap::new();

    for block in blocks {
        if block.is_group {
            groups.insert(
                block.id.clone(),
                GroupLayout {
                    x: block.x,
                    y: block.y,
                    width: block.width,
                    height: block.height,
                    ..Default::default()
                },
            );
            for (nid, local) in &block.intra.nodes {
                nodes.insert(
                    nid.clone(),
                    NodeLayout {
                        x: block.x + padding.x + local.x,
                        y: block.y + padding.y_top + local.y,
                        width: local.width,
                        height: local.height,
                        ..Default::default()
                    },
                );
            }
        } else {
            for (nid, local) in &block.intra.nodes {
                nodes.insert(
                    nid.clone(),
                    NodeLayout {
                        x: block.x + local.x,
                        y: block.y + local.y,
                        width: local.width,
                        height: local.height,
                        ..Default::default()
                    },
                );
            }
        }
    }

    (nodes, groups)
}

/// 跨组边端口微调的基础位移（像素），作为动态计算的下限
const CROSS_GROUP_NUDGE_BASE: f64 = 16.0;
/// 跨组边端口微调的最大位移（像素）
const CROSS_GROUP_NUDGE_MAX: f64 = 48.0;
/// 跨组边端口微调占组内可用宽度的比例
const CROSS_GROUP_NUDGE_WIDTH_RATIO: f64 = 0.3;
/// 跨组边端口微调占目标距离的比例
const CROSS_GROUP_NUDGE_DIST_RATIO: f64 = 0.3;
/// 跨组边端口 y 对齐的最大位移（像素）
const CROSS_GROUP_Y_ALIGN_MAX: f64 = 20.0;
/// 跨组边端口 y 对齐的比例系数
const CROSS_GROUP_Y_ALIGN_RATIO: f64 = 0.5;

/// Phase C+: 两阶段 spacing 微调
///
/// 组框已定后，对涉及跨组边的组内节点朝跨组边方向做动态 x 微调，
/// 减少跨组边折弯。这是"先定组框再微调组内节点"的反转步骤。
///
/// 算法：
/// 1. P2.1: y 对齐——同 macro rank 内跨组边的两端节点 y 中心对齐
/// 2. 遍历每条跨组边 (from_super → to_super)
/// 3. 找到 from_super / to_super 中实际参与跨组边的节点
/// 4. 计算两端节点 x 中心的方向与距离
/// 5. 将组内节点朝该方向动态移动
/// 6. 对同组同向多条跨组边，按目标 x 排序后按比例分布
fn nudge_intra_nodes_toward_cross_group_edges(
    nodes: &mut HashMap<String, NodeLayout>,
    groups: &HashMap<String, GroupLayout>,
    super_edges: &HashSet<(String, String)>,
    super_members: &HashMap<String, Vec<String>>,
    graph: &GraphIndex,
    reversed: &HashSet<(String, String)>,
) {
    if super_edges.is_empty() {
        return;
    }

    // ── P2.1: y 对齐阶段 ──
    // 同 macro rank 内，跨组边的两端节点 y 中心对齐。
    // 仅对同 macro rank（同 y 行）的跨组边对端节点做 y 微调，
    // 避免不同 macro rank 的节点被错误拉扯。
    nudge_cross_group_y_alignment(nodes, super_edges, super_members, graph, reversed);

    // ── x 微调阶段（原有逻辑） ──

    // 收集每个节点的跨组边目标信息：(node_id → Vec<target_cx>)
    // target_cx 为跨组边对端节点的中心 x
    let mut node_targets: HashMap<String, Vec<f64>> = HashMap::new();

    // 排序保证迭代顺序确定（HashSet 迭代顺序随机），
    // 否则 node_targets 中每个 Vec<f64> 顺序随机，
    // f64 求和非结合性会导致 avg_target 1 ULP 差异 → desired_x 排序 tie → 最终位置抖动
    let mut super_edges_sorted: Vec<&(String, String)> = super_edges.iter().collect();
    super_edges_sorted.sort();

    for (from_super, to_super) in super_edges_sorted {
        let from_members = match super_members.get(from_super) {
            Some(m) if !m.is_empty() => m,
            _ => continue,
        };
        let to_members = match super_members.get(to_super) {
            Some(m) if !m.is_empty() => m,
            _ => continue,
        };

        for from_node in from_members {
            let succs = graph.out_edges.get(from_node).cloned().unwrap_or_default();
            for succ in &succs {
                if !is_effective_edge(from_node, succ, reversed) {
                    continue;
                }
                if !to_members.contains(succ) {
                    continue;
                }
                let Some(to_nl) = nodes.get(succ) else {
                    continue;
                };
                let to_cx = to_nl.x + to_nl.width / 2.0;
                node_targets
                    .entry(from_node.clone())
                    .or_default()
                    .push(to_cx);
                // 反向：succ 也要朝 from_node 方向微调
                let Some(from_nl) = nodes.get(from_node) else {
                    continue;
                };
                let from_cx = from_nl.x + from_nl.width / 2.0;
                node_targets
                    .entry(succ.clone())
                    .or_default()
                    .push(from_cx);
            }
        }
    }

    // 按组收集同组节点，用于同向多边排序分布
    let mut group_node_targets: HashMap<String, Vec<(String, f64, f64)>> = HashMap::new();
    // (node_id, current_cx, avg_target_cx)

    for (node_id, targets) in &node_targets {
        let Some(nl) = nodes.get(node_id) else {
            continue;
        };

        // 跳过组内 hub：有组内后继的节点（如 gateway → services），
        // 它们需要保持居中于组内子节点，不应被跨组边拉开
        let group_id = super_members
            .iter()
            .find(|(_, members)| members.contains(node_id))
            .map(|(gid, _)| gid.clone());
        let Some(ref gid) = group_id else {
            continue;
        };
        let Some(group_members) = super_members.get(gid) else {
            continue;
        };
        let has_intra_successors = graph
            .out_edges
            .get(node_id)
            .map(|succs| {
                succs.iter().any(|s| {
                    group_members.contains(s) && is_effective_edge(node_id, s, reversed)
                })
            })
            .unwrap_or(false);
        if has_intra_successors {
            continue;
        }

        let current_cx = nl.x + nl.width / 2.0;
        let avg_target = targets.iter().sum::<f64>() / targets.len() as f64;

        group_node_targets
            .entry(gid.clone())
            .or_default()
            .push((node_id.clone(), current_cx, avg_target));
    }

    // 对每组：计算动态微调
    for (gid, entries) in group_node_targets {
        let Some(gl) = groups.get(&gid) else {
            continue;
        };
        let pad = constants::ARCH_V2_GROUP_PADDING;
        let group_min_x = gl.x + pad;
        let group_max_x = gl.x + gl.width - pad;
        let available_width = (group_max_x - group_min_x).max(0.0);

        // 动态位移上限：基于组宽和固定上限取小
        let width_based_cap = available_width * CROSS_GROUP_NUDGE_WIDTH_RATIO;
        let dynamic_cap = width_based_cap.min(CROSS_GROUP_NUDGE_MAX).max(CROSS_GROUP_NUDGE_BASE);

        // 计算每个节点的期望新 x（左上角），保持原有顺序
        let mut planned: Vec<(String, f64, f64)> = entries
            .iter()
            .map(|(node_id, current_cx, target_cx)| {
                let nl = nodes.get(node_id).unwrap();
                let direction = target_cx - current_cx;
                let sign = direction.signum();
                let abs_dir = direction.abs();
                let proportional = (abs_dir * CROSS_GROUP_NUDGE_DIST_RATIO).min(dynamic_cap);
                let min_move = CROSS_GROUP_NUDGE_BASE.min(abs_dir);
                let delta = if abs_dir < f64::EPSILON {
                    0.0
                } else {
                    sign * proportional.max(min_move)
                };
                let desired_x = nl.x + delta;
                (node_id.clone(), nl.width, desired_x)
            })
            .collect();

        // 按 desired_x 排序，强制保持最小间距，避免重叠
        // 加 node_id tie-breaker，避免 desired_x 相同时保持 HashMap 迭代顺序（非确定）
        planned.sort_by(|a, b| {
            a.2.partial_cmp(&b.2)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });

        let min_gap = super::layout::constants::NODE_GAP;
        let n = planned.len();
        for i in 1..n {
            let prev_right = planned[i - 1].2 + planned[i - 1].1;
            if planned[i].2 < prev_right + min_gap {
                planned[i].2 = prev_right + min_gap;
            }
        }
        // 反向再扫一次，防止右溢出导致左重叠
        for i in (0..n.saturating_sub(1)).rev() {
            let next_left = planned[i + 1].2;
            if planned[i].2 + planned[i].1 > next_left - min_gap {
                planned[i].2 = next_left - min_gap - planned[i].1;
            }
        }

        // 应用最终位置，clamp 到组框
        for (node_id, width, desired_x) in planned {
            let Some(nl) = nodes.get_mut(&node_id) else {
                continue;
            };
            let node_min_x = group_min_x;
            let node_max_x = group_max_x - width;
            nl.x = desired_x.clamp(node_min_x, node_max_x);
        }
    }
}

/// P2.1: 跨组边端口 y 对齐
///
/// 对同 macro rank 内跨组边的两端节点做 y 中心对齐微调。
/// 当两个节点通过跨组边连接且处于同一 y 行（y 中心差 < LAYER_GAP）
/// 时，将两者 y 中心向中间值靠拢，减少跨组边的折弯数。
///
/// 仅微调，不改变节点所在层——位移上限为 `CROSS_GROUP_Y_ALIGN_MAX`。
fn nudge_cross_group_y_alignment(
    nodes: &mut HashMap<String, NodeLayout>,
    super_edges: &HashSet<(String, String)>,
    super_members: &HashMap<String, Vec<String>>,
    graph: &GraphIndex,
    reversed: &HashSet<(String, String)>,
) {
    // 收集每对跨组边端点的 y 对齐目标：(node_id → Vec<target_cy>)
    let mut node_y_targets: HashMap<String, Vec<f64>> = HashMap::new();

    // 排序保证迭代顺序确定
    let mut super_edges_sorted: Vec<&(String, String)> = super_edges.iter().collect();
    super_edges_sorted.sort();

    for (from_super, to_super) in &super_edges_sorted {
        let from_members = match super_members.get(from_super) {
            Some(m) if !m.is_empty() => m,
            _ => continue,
        };
        let to_members = match super_members.get(to_super) {
            Some(m) if !m.is_empty() => m,
            _ => continue,
        };

        for from_node in from_members {
            let succs = graph.out_edges.get(from_node).cloned().unwrap_or_default();
            for succ in &succs {
                if !is_effective_edge(from_node, succ, reversed) {
                    continue;
                }
                if !to_members.contains(succ) {
                    continue;
                }
                let Some(from_nl) = nodes.get(from_node) else {
                    continue;
                };
                let Some(to_nl) = nodes.get(succ) else {
                    continue;
                };

                let from_cy = from_nl.y + from_nl.height / 2.0;
                let to_cy = to_nl.y + to_nl.height / 2.0;

                // 仅对同 y 行的节点做 y 对齐（y 中心差 < LAYER_GAP）
                if (from_cy - to_cy).abs() < LAYER_GAP {
                    node_y_targets
                        .entry(from_node.clone())
                        .or_default()
                        .push(to_cy);
                    node_y_targets
                        .entry(succ.clone())
                        .or_default()
                        .push(from_cy);
                }
            }
        }
    }

    // 按 node_id 排序保证确定性
    let mut sorted_targets: Vec<_> = node_y_targets.into_iter().collect();
    sorted_targets.sort_by(|a, b| a.0.cmp(&b.0));

    for (node_id, targets) in sorted_targets {
        let Some(nl) = nodes.get_mut(&node_id) else {
            continue;
        };
        let current_cy = nl.y + nl.height / 2.0;
        let avg_target_cy = targets.iter().sum::<f64>() / targets.len() as f64;
        let direction = avg_target_cy - current_cy;
        let abs_dir = direction.abs();
        if abs_dir < f64::EPSILON {
            continue;
        }
        let delta = (abs_dir * CROSS_GROUP_Y_ALIGN_RATIO)
            .min(CROSS_GROUP_Y_ALIGN_MAX)
            .copysign(direction);
        nl.y += delta;
    }
}

/// 从元数据重建全局层列表，供基础设施行居中使用
///
/// 旧版 `rebuild_layers_from_positions` 从 y 坐标反推层（依赖 4px epsilon，
/// 相邻层 y 接近时会误合并）。本版直接从 macro rank + intra layers 元数据
/// 重建，确定性且无 epsilon 依赖。
fn rebuild_layers_from_metadata(
    blocks: &[MacroBlock],
    macro_ranks: &HashMap<String, usize>,
) -> Vec<Vec<String>> {
    if blocks.is_empty() {
        return vec![];
    }

    let max_rank = macro_ranks.values().copied().max().unwrap_or(0);

    // 收集每个 macro rank 下的 block，按 id 排序保证确定性
    let mut rank_blocks: Vec<Vec<usize>> = vec![Vec::new(); max_rank + 1];
    for (i, b) in blocks.iter().enumerate() {
        let r = macro_ranks.get(&b.id).copied().unwrap_or(0);
        rank_blocks[r].push(i);
    }
    for indices in &mut rank_blocks {
        indices.sort_by(|&a, &b| blocks[a].id.cmp(&blocks[b].id));
    }

    // 同一 macro rank 内，各 block 的 intra layer 0 对齐、layer 1 对齐……
    // 不同 macro rank 产出独立的全局层
    let mut global_layers: Vec<Vec<String>> = Vec::new();
    for indices in &rank_blocks {
        if indices.is_empty() {
            continue;
        }
        let max_intra_layers = indices
            .iter()
            .map(|&i| blocks[i].intra.layers.len())
            .max()
            .unwrap_or(0);
        for intra_idx in 0..max_intra_layers {
            let mut layer: Vec<String> = Vec::new();
            for &bi in indices {
                if let Some(intra_layer) = blocks[bi].intra.layers.get(intra_idx) {
                    layer.extend(intra_layer.iter().cloned());
                }
            }
            if !layer.is_empty() {
                global_layers.push(layer);
            }
        }
    }

    global_layers
}

// ─── 测试 ────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        ArrowType, AttributeMap, AttributeValue, Diagram, DiagramAttribute, Entity, Group,
        Identifier, Relation, SourceInfo, Span, TextValue,
    };
    use crate::types::DiagramType;
    use crate::layout::constants;
    use crate::layout::node::architecture_v2::ArchitectureV2Layout;
    use crate::layout::LayoutStrategy;

    fn entity_in_group(id: &str, label: &str, group: &str) -> Entity {
        Entity {
            id: Identifier::new_unchecked(id),
            label: label.to_string(),
            attributes: AttributeMap::default(),
            group_id: Some(Identifier::new_unchecked(group)),
            span: Span::dummy(),
        }
    }

    fn entity(id: &str, label: &str) -> Entity {
        Entity {
            id: Identifier::new_unchecked(id),
            label: label.to_string(),
            attributes: AttributeMap::default(),
            group_id: None,
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

    #[allow(dead_code)]
    fn make_group(id: &str, label: &str, entity_ids: Vec<&str>) -> Group {
        Group {
            id: Identifier::new_unchecked(id),
            label: label.to_string(),
            attributes: AttributeMap::default(),
            parent_id: None,
            depth: 0,
            entity_ids: entity_ids
                .into_iter()
                .map(|e| Identifier::new_unchecked(e))
                .collect(),
            child_group_ids: vec![],
            span: Span::dummy(),
        }
    }

    fn make_group_with_layout(id: &str, label: &str, layout: &str, entity_ids: Vec<&str>) -> Group {
        let mut attrs = AttributeMap::default();
        attrs
            .standard
            .insert("layout".to_string(), AttributeValue::String(TextValue::unquoted(layout.to_string())));
        Group {
            id: Identifier::new_unchecked(id),
            label: label.to_string(),
            attributes: attrs,
            parent_id: None,
            depth: 0,
            entity_ids: entity_ids
                .into_iter()
                .map(|e| Identifier::new_unchecked(e))
                .collect(),
            child_group_ids: vec![],
            span: Span::dummy(),
        }
    }

    fn etl_diagram_with_sizing(group_sizing: Option<&str>) -> Diagram {
        let attributes = group_sizing
            .map(|value| {
                vec![DiagramAttribute {
                    key: "group_sizing".to_string(),
                    value: AttributeValue::String(TextValue::unquoted(value.to_string())),
                    span: Span::dummy(),
                }]
            })
            .unwrap_or_default();

        Diagram {
            diagram_type: DiagramType::Architecture,
            attributes,
            entities: vec![
                entity_in_group("app_db", "业务数据库", "source"),
                entity_in_group("log_server", "日志服务器", "source"),
                entity_in_group("kafka", "消息队列(Kafka)", "process"),
                entity_in_group("flink", "流计算(Flink)", "process"),
                entity_in_group("spark", "批处理(Spark)", "process"),
                entity_in_group("hive", "数仓(Hive)", "storage"),
                entity_in_group("clickhouse", "OLAP引擎", "storage"),
                entity("bi", "BI可视化看板"),
            ],
            relations: vec![
                relation("app_db", "kafka"),
                relation("log_server", "kafka"),
                relation("kafka", "flink"),
                relation("kafka", "spark"),
                relation("spark", "hive"),
                relation("flink", "clickhouse"),
                relation("hive", "clickhouse"),
                relation("clickhouse", "bi"),
            ],
            groups: vec![
                make_group_with_layout("source", "数据源层", "horizontal", vec!["app_db", "log_server"]),
                make_group_with_layout(
                    "process",
                    "数据计算层",
                    "fan-out",
                    vec!["kafka", "flink", "spark"],
                ),
                make_group_with_layout("storage", "数据存储层", "vertical", vec!["hive", "clickhouse"]),
            ],
            style_decls: vec![],
            source_info: SourceInfo {
                file: None,
                line_count: 34,
            },
            ..Default::default()
        }
    }

    #[test]
    fn two_phase_etl_pipeline_layout() {
        let d = etl_diagram_with_sizing(None);
        let result = ArchitectureV2Layout::default().compute(&d);

        let source = result.groups.get("source").unwrap();
        let process = result.groups.get("process").unwrap();
        let storage = result.groups.get("storage").unwrap();

        // 三层自上而下
        assert!(source.y < process.y, "source above process");
        assert!(process.y < storage.y, "process above storage");

        // 顶层分组左对齐
        assert!((source.x - process.x).abs() < 1.0);
        assert!((process.x - storage.x).abs() < 1.0);

        // 计算层最宽（Spark/Flink 分叉），存储层较窄（竖排）
        assert!(
            process.width > storage.width,
            "process ({:.0}) should be wider than storage ({:.0})",
            process.width,
            storage.width
        );

        // Kafka 在 Spark/Flink 上方
        let kafka = result.nodes.get("kafka").unwrap();
        let spark = result.nodes.get("spark").unwrap();
        let flink = result.nodes.get("flink").unwrap();
        assert!(kafka.y + kafka.height < spark.y);
        assert!(kafka.y + kafka.height < flink.y);

        // Hive 在 ClickHouse 上方
        let hive = result.nodes.get("hive").unwrap();
        let ch = result.nodes.get("clickhouse").unwrap();
        assert!(hive.y + hive.height < ch.y);

        // 所有组内节点在组框内
        for (gid, members) in [
            ("source", vec!["app_db", "log_server"]),
            ("process", vec!["kafka", "flink", "spark"]),
            ("storage", vec!["hive", "clickhouse"]),
        ] {
            let g = result.groups.get(gid).unwrap();
            for eid in members {
                let n = result.nodes.get(eid).unwrap();
                assert!(
                    n.x >= g.x + constants::ARCH_V2_GROUP_PADDING - 0.5
                        && n.x + n.width <= g.x + g.width - constants::ARCH_V2_GROUP_PADDING + 0.5
                        && n.y >= g.y + GROUP_LABEL_HEIGHT + constants::ARCH_V2_GROUP_PADDING - 0.5
                        && n.y + n.height <= g.y + g.height - constants::ARCH_V2_GROUP_PADDING + 0.5,
                    "{eid} should stay inside {gid}"
                );
            }
        }

        // BI 在存储层下方
        let bi = result.nodes.get("bi").unwrap();
        assert!(storage.y + storage.height < bi.y);
    }

    #[test]
    fn two_phase_uniform_group_sizing() {
        let d = etl_diagram_with_sizing(Some("uniform"));
        let result = ArchitectureV2Layout::default().compute(&d);

        let source = result.groups.get("source").unwrap();
        let process = result.groups.get("process").unwrap();
        let storage = result.groups.get("storage").unwrap();

        assert!(
            (source.width - process.width).abs() < 1.0,
            "uniform: source/process width"
        );
        assert!(
            (process.width - storage.width).abs() < 1.0,
            "uniform: process/storage width"
        );

        // 较窄的存储层内容应大致居中
        let hive = result.nodes.get("hive").unwrap();
        let hive_cx = hive.x + hive.width / 2.0;
        let storage_cx = storage.x + storage.width / 2.0;
        assert!(
            (hive_cx - storage_cx).abs() < 24.0,
            "hive should center in uniform storage group"
        );
    }

    #[test]
    fn etl_uniform_survives_full_layout_pipeline() {
        use crate::layout::compute_layout;

        let d = etl_diagram_with_sizing(Some("uniform"));
        let result = compute_layout(&d).expect("full pipeline layout");

        let source = result.groups.get("source").unwrap();
        let process = result.groups.get("process").unwrap();
        let storage = result.groups.get("storage").unwrap();

        assert!(
            (source.width - process.width).abs() < 1.0,
            "after grid snap: source/process width {:.1} vs {:.1}",
            source.width,
            process.width
        );
        assert!(
            (process.width - storage.width).abs() < 1.0,
            "after grid snap: process/storage width"
        );
        assert!(
            (source.x - process.x).abs() < 1.0 && (process.x - storage.x).abs() < 1.0,
            "top groups should stay left-aligned"
        );
    }
}
