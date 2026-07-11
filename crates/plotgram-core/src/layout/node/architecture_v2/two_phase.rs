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

use super::group_sizing::{
    parse_group_sizing, GroupSizeBlock, GroupSizingPolicy,
};
use super::group_layout_hint::{
    align_nodes_in_column, assign_ranks_for_mode, parse_group_layout_hint,
    resolve_group_layout_hint, resolve_group_layout_mode, GroupLayoutHint, GroupLayoutMode,
};
use super::layout::acyclic::is_effective_edge;
use super::layout::constants::{
    GROUP_GAP_X, GROUP_LABEL_HEIGHT, INTRA_LAYER_GAP, LAYER_GAP, NEIGHBOR_PULL_FACTOR, NODE_GAP,
};
use super::layout::coordinate::{
    align_client_nodes_to_hubs, center_group_hub_nodes, layer_centers_from_placed,
    pull_toward_neighbors, rebalance_infrastructure_layers, resolve_x_overlaps,
    resolve_x_overlaps_with_gaps, uniform_initial_positions,
};
use super::layout::order::{build_layers, order_layers_group_aware};
use super::layout::postprocess::{clamp_to_canvas, compute_total_size};
use super::layout::rank::{assign_intra_ranks, assign_super_macro_ranks};
use super::layout::types::{GraphIndex, GroupMap};
use crate::layout::algorithm_config::ArchitectureV2LayoutConfig;
use crate::ast::{Diagram, Group};
use crate::layout::constants;
use crate::layout::node::common::divide_and_conquer::{
    GroupTree, IntraGroupLayouter, IntraLayout,
};
use crate::layout::node::common::edge_gutter::estimate_side_gutters_with_hierarchy;
use crate::layout::node::common::group_bounds::{
    compute_group_bounds, compute_group_bounds_with_side_gutters, container_padding_for_leaf,
    GroupPadding, SideGutter,
};
use crate::layout::group::constants::EPS;
use crate::layout::{GroupLayout, LayoutResult, NodeLayout};
use std::collections::{BTreeMap, HashMap, HashSet};

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

impl GroupSizeBlock for MacroBlock {
    fn block_height(&self) -> f64 {
        self.height
    }

    fn set_block_height(&mut self, height: f64) {
        self.height = height;
    }

    fn shift_intra_nodes_y(&mut self, delta: f64) {
        for nl in self.intra.nodes.values_mut() {
            nl.y += delta;
        }
        self.intra.content_height += delta;
    }
}

/// 宏观行内块的水平对齐策略（架构图默认左对齐，避免窄行居中偏移）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowAlign {
    Start,
    Center,
}

pub(super) fn compute_two_phase_layout(
    diagram: &Diagram,
    graph: &GraphIndex,
    group_map: &GroupMap,
    sizes: &HashMap<String, (f64, f64)>,
    reversed_edges: &HashSet<(String, String)>,
    layout_config: ArchitectureV2LayoutConfig,
) -> LayoutResult {
    // Phase D：默认用 asymmetric architecture_v2 壳；仅当 config 显式覆盖 group_padding 时退回 uniform
    let padding = if (layout_config.group_padding - constants::ARCH_V2_GROUP_PADDING).abs()
        < f64::EPSILON
    {
        GroupPadding::architecture_v2()
    } else {
        GroupPadding::uniform(layout_config.group_padding, GROUP_LABEL_HEIGHT)
    };
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
    let (super_members, super_edges, pair_edge_counts, edge_weights) =
        build_super_graph(graph, group_map, reversed_edges);
    let group_decl = crate::layout::decl_order::group_sibling_decl_index(diagram);
    let macro_ranks = assign_super_macro_ranks(
        &super_members,
        &super_edges,
        &edge_weights,
        &graph.node_ids,
        &group_decl,
    );

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
    // Phase 1：two_phase 只输出 content-fit 初值；Equal/Uniform 仅由 L1 GroupFramePass 执行。

    position_macro_blocks(
        &mut blocks,
        &macro_ranks,
        &super_edges,
        &pair_edge_counts,
        canvas_padding,
        // 初值左对齐；L1 Center 在 pipeline 中对单行做居中
        RowAlign::Start,
        &group_decl,
    );

    // ── Phase C: 回填全局坐标 ──
    let (mut nodes, mut groups) = compose_global_layout(&blocks, &padding);

    // Phase C+: 两阶段 spacing 微调
    // 组框已定，对涉及跨组边的组内节点朝跨组边方向做小幅 x 微调，
    // 减少跨组边折弯。这是"先定组框再微调组内节点"的反转步骤。
    // L1 Equal 在 pipeline 中拉齐；此处始终基于 content-fit 初值微调。
    nudge_intra_nodes_toward_cross_group_edges(
        &mut nodes,
        &groups,
        &super_edges,
        &super_members,
        graph,
        reversed_edges,
    );

    // ── 后处理：基础设施行居中 ──
    // 从元数据重建全局层（替代旧版从 y 坐标反推）
    let layers = rebuild_layers_from_metadata(&blocks, &macro_ranks);
    rebalance_infrastructure_layers(graph, group_map, &layers, sizes, &mut nodes);
    clamp_to_canvas(&mut nodes, sizes);
    // Phase F：同 leaf-group 内近邻 y 带节点微对齐（修小幅错位，不改层拓扑）
    align_intra_group_same_rank_y(diagram, &mut nodes);

    // EGB：节点落定后估计逐组侧 gutter，重算 group bounds 并持久化至 hints。
    let bounds_padding = padding;
    let base_groups = compute_group_bounds(diagram, &nodes, bounds_padding);
    let t_egb = std::time::Instant::now();
    let side_gutters = estimate_side_gutters_with_hierarchy(diagram, &nodes, &base_groups);
    let egb_ms = t_egb.elapsed().as_secs_f64() * 1000.0;
    let computed_groups = compute_group_bounds_with_side_gutters(
        diagram,
        &nodes,
        bounds_padding,
        container_padding_for_leaf(bounds_padding),
        Some(&side_gutters),
    );
    merge_egb_groups(
        diagram,
        &mut groups,
        computed_groups,
        &side_gutters,
        bounds_padding,
    );
    // Uniform/Equal 条带：各顶层叶子 EGB 增量可能不同，拉齐到同带最大增量，避免等宽被拆。
    // Fit 逃生舱跳过。
    if sizing != GroupSizingPolicy::Fit {
        equalize_top_leaf_egb_deltas(diagram, &mut groups, &side_gutters, bounds_padding);
    }
    let gf_spec = crate::layout::group_frame::resolve_group_frame_spec(diagram, "architecture");
    let mut layout_scratch = LayoutResult {
        nodes: std::mem::take(&mut nodes),
        groups: std::mem::take(&mut groups),
        edges: vec![],
        total_width: 0.0,
        total_height: 0.0,
        hints: Default::default(),
    };
    crate::layout::group_frame::resolve_all_sibling_overlaps(
        &gf_spec,
        diagram,
        &mut layout_scratch,
    );
    crate::layout::group_frame::expand_groups_to_contain_contents(
        diagram,
        &mut layout_scratch.groups,
        &layout_scratch.nodes,
        bounds_padding,
        container_padding_for_leaf(bounds_padding),
    );
    // Phase F：仅 Fit 时收回高于 base∪egb 的残余空壳（uniform/Equal 条带不收缩）
    if sizing == GroupSizingPolicy::Fit {
        crate::layout::group_frame::shrink_groups_to_required_padding(
            diagram,
            &mut layout_scratch.groups,
            &layout_scratch.nodes,
            bounds_padding,
            container_padding_for_leaf(bounds_padding),
            Some(&side_gutters),
        );
    }
    nodes = layout_scratch.nodes;
    groups = layout_scratch.groups;
    let max_side_gutter = side_gutters
        .values()
        .flat_map(|g| [g.left, g.right, g.top, g.bottom])
        .fold(0.0_f64, f64::max);
    let pre_egb_area: f64 = base_groups.values().map(|g| g.width * g.height).sum();
    let post_egb_area: f64 = groups.values().map(|g| g.width * g.height).sum();
    let canvas_area_delta_pct = if pre_egb_area > EPS {
        (post_egb_area - pre_egb_area) / pre_egb_area * 100.0
    } else {
        0.0
    };
    let gutter_budget_debug = crate::layout::GutterBudgetDebug {
        egb_ms,
        prs_ms: 0.0,
        prs_grew: false,
        max_side_gutter,
        canvas_area_delta_pct,
    };

    // 空间契约：边感知间距写入 hints，并做一次水平缝 enforce
    let space_budget = crate::layout::space_budget::SpaceBudget::from_diagram(diagram);
    crate::layout::space_budget::enforce_horizontal_gaps(&mut nodes, &space_budget);
    crate::layout::group_frame::expand_groups_to_contain_contents(
        diagram,
        &mut groups,
        &nodes,
        bounds_padding,
        container_padding_for_leaf(bounds_padding),
    );

    let (total_width, total_height) = compute_total_size(&nodes, &groups);

    let sibling_corridors =
        crate::layout::group::build_sibling_corridors(diagram, &groups);
    let corridors = crate::layout::group::merge_corridors(&sibling_corridors, &groups);
    let group_routing = crate::layout::group::GroupRoutingHints {
        corridors,
        border_shell_pad: crate::layout::group::GROUP_BORDER_SHELL_PAD,
        side_gutters,
    };

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
            group_routing: Some(group_routing),
            gutter_budget_debug: Some(gutter_budget_debug),
            space_budget: Some(space_budget),
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
        .map(|g| resolve_group_layout_hint(g, diagram.diagram_type.clone()))
        .unwrap_or(GroupLayoutHint::Auto);
    let mode = resolve_group_layout_mode(hint, members, graph, reversed);

    // Phase 3：复杂拓扑 / Sugiyama 模式委托 sugiyama_v2（hint 几何模式仍走本地路径）
    if mode == GroupLayoutMode::Sugiyama {
        return super::intra_sugiyama::layout_intra_with_sugiyama_v2(diagram, members);
    }

    let ranks = assign_ranks_for_mode(&mode, members, graph, reversed);
    let decl_index: HashMap<String, usize> = members
        .iter()
        .enumerate()
        .map(|(i, id)| (id.clone(), i))
        .collect();
    let layers = build_layers(&ranks, &decl_index);
    let mut ordered_layers = order_layers_group_aware(
        graph,
        &intra_map,
        &layers,
        reversed,
        &decl_index,
    );

    let member_set: HashSet<String> = members.iter().cloned().collect();
    let space_budget = crate::layout::space_budget::SpaceBudget::from_diagram(diagram);
    let mut nodes = assign_coordinates_intra(
        graph,
        &ordered_layers,
        sizes,
        &member_set,
        Some(&space_budget),
    );

    center_group_hub_nodes(graph, &intra_map, &ordered_layers, sizes, &mut nodes);
    align_client_nodes_to_hubs(graph, &intra_map, &ordered_layers, sizes, &mut nodes);

    if mode == GroupLayoutMode::Vertical {
        align_nodes_in_column(&mut nodes);
    }

    normalize_to_origin(&mut nodes);
    let (mut content_width, mut content_height) = content_bbox(&nodes);

    // 过扁组（宽 >> 高）回退 Grid，改善 private_subnet 类单行布局
    const MIN_GROUP_ASPECT: f64 = 0.25;
    if mode == GroupLayoutMode::Horizontal
        && members.len() >= 3
        && content_width > f64::EPSILON
        && content_height < content_width * MIN_GROUP_ASPECT
    {
        let grid_mode = GroupLayoutMode::Grid;
        let ranks = assign_ranks_for_mode(&grid_mode, members, graph, reversed);
        let layers = build_layers(&ranks, &decl_index);
        ordered_layers = order_layers_group_aware(graph, &intra_map, &layers, reversed, &decl_index);
        nodes = assign_coordinates_intra(
            graph,
            &ordered_layers,
            sizes,
            &member_set,
            Some(&space_budget),
        );
        center_group_hub_nodes(graph, &intra_map, &ordered_layers, sizes, &mut nodes);
        align_client_nodes_to_hubs(graph, &intra_map, &ordered_layers, sizes, &mut nodes);
        normalize_to_origin(&mut nodes);
        (content_width, content_height) = content_bbox(&nodes);
    }

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
            width: intra.content_width + padding.horizontal_extent(),
            height: intra.content_height + padding.vertical_extent(),
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
    let (super_members, super_edges, pair_edge_counts, edge_weights) =
        build_super_graph_for_group(group_id, group_tree, graph, reversed);
    let group_decl = crate::layout::decl_order::group_sibling_decl_index(diagram);
    let macro_ranks = assign_super_macro_ranks(
        &super_members,
        &super_edges,
        &edge_weights,
        &graph.node_ids,
        &group_decl,
    );

    // 4.5 嵌套 sibling：Phase 1 起 Equal 仅由 L1 执行；此处只保留 content-fit 初值。
    let _child_group_ids: Vec<String> = children.to_vec();

    // 5. 宏观定位（初值左对齐；L1 完成 Center）
    position_intra_macro_blocks(
        &mut blocks,
        &macro_ranks,
        &super_edges,
        &pair_edge_counts,
        RowAlign::Start,
    );

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

impl GroupSizeBlock for IntraMacroBlock {
    fn block_height(&self) -> f64 {
        self.height
    }

    fn set_block_height(&mut self, height: f64) {
        self.height = height;
    }

    fn shift_intra_nodes_y(&mut self, delta: f64) {
        for nl in self.intra.nodes.values_mut() {
            nl.y += delta;
        }
        self.intra.content_height += delta;
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
    // 有向边权（跨组实际边数），供加权 FAS 裁决双向对
    let mut edge_weights: HashMap<(String, String), usize> = HashMap::new();
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
                        *edge_weights
                            .entry((from_super.clone(), to_super.clone()))
                            .or_insert(0) += 1;
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

    (super_members, super_edges, pair_edge_counts, edge_weights)
}

/// 容器组内宏观块定位（复用顶层 position_macro_blocks 逻辑，但 padding=0）
fn position_intra_macro_blocks(
    blocks: &mut [IntraMacroBlock],
    macro_ranks: &HashMap<String, usize>,
    super_edges: &HashSet<(String, String)>,
    pair_edge_counts: &HashMap<(String, String), usize>,
    row_align: RowAlign,
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
            // Iteration 2：band 内统一 lane_budget gap
            let ordered_ids: Vec<String> = rank_indices
                .iter()
                .map(|&i| blocks[i].id.clone())
                .collect();
            let gap = band_uniform_gap(&ordered_ids, pair_edge_counts);
            let mut x_cursor = 0.0;
            for (pos, &i) in rank_indices.iter().enumerate() {
                blocks[i].x = x_cursor;
                blocks[i].y = y_cursor;
                x_cursor += blocks[i].width;
                if pos + 1 < rank_indices.len() {
                    x_cursor += gap;
                }
            }
        }

        let extra_layer_gap = adaptive_vertical_rank_gap(
            rank,
            blocks,
            macro_ranks,
            &cross_edge_counts,
            pair_edge_counts,
        );
        let effective_layer_gap = LAYER_GAP + extra_layer_gap;

        y_cursor += max_height + effective_layer_gap;
    }

    if row_align == RowAlign::Center {
        center_rank_rows(macro_ranks, blocks.len(), |i| {
            (blocks[i].id.clone(), blocks[i].x, blocks[i].width)
        })
        .into_iter()
        .for_each(|(i, shift)| blocks[i].x += shift);
    }
}

/// 计算每个宏观块的行居中偏移量。
///
/// 按 rank 分行，行宽 = 行内块的最大右边界 - origin；
/// 最宽行保持不动，窄行整体右移 `(max_row_width - row_width) / 2`。
/// 返回 `(block_index, shift_x)` 列表（shift 为 0 的块不返回）。
fn center_rank_rows(
    macro_ranks: &HashMap<String, usize>,
    block_count: usize,
    block_info: impl Fn(usize) -> (String, f64, f64),
) -> Vec<(usize, f64)> {
    // rank → (行右边界, 行内块索引)
    let mut rows: HashMap<usize, (f64, Vec<usize>)> = HashMap::new();
    for i in 0..block_count {
        let (id, x, width) = block_info(i);
        let rank = macro_ranks.get(&id).copied().unwrap_or(0);
        let entry = rows.entry(rank).or_insert((f64::NEG_INFINITY, Vec::new()));
        entry.0 = entry.0.max(x + width);
        entry.1.push(i);
    }

    let max_extent = rows
        .values()
        .map(|(extent, _)| *extent)
        .fold(f64::NEG_INFINITY, f64::max);
    if !max_extent.is_finite() {
        return Vec::new();
    }

    let mut shifts = Vec::new();
    let mut ranks: Vec<usize> = rows.keys().copied().collect();
    ranks.sort_unstable();
    for rank in ranks {
        let (extent, indices) = &rows[&rank];
        let shift = (max_extent - extent) / 2.0;
        if shift > f64::EPSILON {
            for &i in indices {
                shifts.push((i, shift));
            }
        }
    }
    shifts
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
            (block.x + padding.left, block.y + padding.top)
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
        a.y
            .partial_cmp(&b.y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
            .then(a.id.cmp(&b.id))
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
    let decl_index: HashMap<String, usize> = members
        .iter()
        .enumerate()
        .map(|(i, id)| (id.clone(), i))
        .collect();
    let layers = build_layers(&ranks, &decl_index);
    let member_set: HashSet<String> = members.iter().cloned().collect();

    let mut nodes = assign_coordinates_intra(
        graph,
        &layers,
        sizes,
        &member_set,
        Some(&crate::layout::space_budget::SpaceBudget::from_diagram(diagram)),
    );
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
    budget: Option<&crate::layout::space_budget::SpaceBudget>,
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

        let adjusted = if let Some(b) = budget {
            resolve_x_overlaps_with_gaps(layer, &positions, sizes, |a, c| b.min_gap(a, c))
        } else {
            resolve_x_overlaps(layer, &positions, sizes)
        };

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
    // 有向边权（跨组实际边数），供加权 FAS 裁决双向对
    let mut edge_weights: HashMap<(String, String), usize> = HashMap::new();
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
                    *edge_weights
                        .entry((from_super.clone(), to_super.clone()))
                        .or_insert(0) += 1;
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

    (super_members, super_edges, pair_edge_counts, edge_weights)
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
            width: intra.content_width + padding.horizontal_extent(),
            height: intra.content_height + padding.vertical_extent(),
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
const CROSS_EDGE_LAYER_GAP_SCALE: f64 = 12.0;
/// 层间距额外增加的上限
const MAX_EXTRA_LAYER_GAP: f64 = 80.0;
/// 相邻 rank 组对跨组边为垂直间隙额外增加的像素
const CROSS_EDGE_PAIR_VERTICAL_GAP_SCALE: f64 = 10.0;
/// 组对垂直间隙额外增加的上限
const MAX_EXTRA_PAIR_VERTICAL_GAP: f64 = 56.0;
/// 每条同 rank 跨组边为组间距额外增加的像素（lane_budget）
const CROSS_EDGE_GROUP_GAP_SCALE: f64 = 8.0;
/// 组间距额外增加的上限（Phase 2：与 corridor_load 预算对齐，略抬高）
const MAX_EXTRA_GROUP_GAP: f64 = 56.0;

/// 计算相邻组块间的间距（Phase 2：lane_budget ↔ 跨组边负载）。
///
/// `gap = GROUP_GAP_X + min(edge_count × scale, max_extra)`。
/// 高负载 pair 预留更宽通道，降低事后 PRS 扩壳。
fn adaptive_group_gap(pair_edge_count: usize) -> f64 {
    let extra = (pair_edge_count as f64 * CROSS_EDGE_GROUP_GAP_SCALE).min(MAX_EXTRA_GROUP_GAP);
    GROUP_GAP_X + extra
}

/// 同一 RankBand 内统一水平间距：取该行所有相邻 pair 的 lane_budget 最大值。
fn band_uniform_gap(
    ordered_ids: &[String],
    pair_edge_counts: &HashMap<(String, String), usize>,
) -> f64 {
    if ordered_ids.len() < 2 {
        return GROUP_GAP_X;
    }
    let mut max_gap = GROUP_GAP_X;
    for w in ordered_ids.windows(2) {
        let pair = if w[0] <= w[1] {
            (w[0].clone(), w[1].clone())
        } else {
            (w[1].clone(), w[0].clone())
        };
        let count = pair_edge_counts.get(&pair).copied().unwrap_or(0);
        max_gap = max_gap.max(adaptive_group_gap(count));
    }
    max_gap
}

/// 相邻 macro rank 之间：取 rank 总跨组边密度与「上下行组对」最大边数的较大值，放大垂直通道。
fn adaptive_vertical_rank_gap<B: super::group_sizing::GroupWidthBlock>(
    rank: usize,
    blocks: &[B],
    macro_ranks: &HashMap<String, usize>,
    cross_edge_counts: &HashMap<usize, usize>,
    pair_edge_counts: &HashMap<(String, String), usize>,
) -> f64 {
    let from_rank = cross_edge_counts
        .get(&rank)
        .map(|&c| (c as f64 * CROSS_EDGE_LAYER_GAP_SCALE).min(MAX_EXTRA_LAYER_GAP))
        .unwrap_or(0.0);

    let mut ids_a: Vec<String> = blocks
        .iter()
        .filter(|b| {
            b.is_group_block() && macro_ranks.get(b.block_id()).copied() == Some(rank)
        })
        .map(|b| b.block_id().to_string())
        .collect();
    let mut ids_b: Vec<String> = blocks
        .iter()
        .filter(|b| {
            b.is_group_block() && macro_ranks.get(b.block_id()).copied() == Some(rank + 1)
        })
        .map(|b| b.block_id().to_string())
        .collect();
    ids_a.sort();
    ids_b.sort();

    let mut pair_max = 0usize;
    for a in &ids_a {
        for b in &ids_b {
            let pair = if a <= b {
                (a.clone(), b.clone())
            } else {
                (b.clone(), a.clone())
            };
            pair_max = pair_max.max(pair_edge_counts.get(&pair).copied().unwrap_or(0));
        }
    }
    let from_pair = (pair_max as f64 * CROSS_EDGE_PAIR_VERTICAL_GAP_SCALE)
        .min(MAX_EXTRA_PAIR_VERTICAL_GAP);

    from_rank.max(from_pair)
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
    row_align: RowAlign,
    group_decl: &HashMap<String, usize>,
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
        rank_indices.sort_by(|&a, &b| {
            crate::layout::decl_order::cmp_by_decl_then_id(group_decl, &blocks[a].id, &blocks[b].id)
        });

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
            // Iteration 2：band 内统一 lane_budget gap（取相邻 pair 最大值）
            let ordered_ids: Vec<String> = rank_indices
                .iter()
                .map(|&i| blocks[i].id.clone())
                .collect();
            let gap = band_uniform_gap(&ordered_ids, pair_edge_counts);
            let mut x_cursor = canvas_padding;
            for (pos, &i) in rank_indices.iter().enumerate() {
                blocks[i].x = x_cursor;
                blocks[i].y = y_cursor;
                x_cursor += blocks[i].width;
                if pos + 1 < rank_indices.len() {
                    x_cursor += gap;
                }
            }
        }

        let extra_layer_gap = adaptive_vertical_rank_gap(
            rank,
            blocks,
            macro_ranks,
            &cross_edge_counts,
            pair_edge_counts,
        );
        let effective_layer_gap = LAYER_GAP + extra_layer_gap;

        y_cursor += max_height + effective_layer_gap;
    }

    if row_align == RowAlign::Center {
        center_rank_rows(macro_ranks, blocks.len(), |i| {
            (blocks[i].id.clone(), blocks[i].x, blocks[i].width)
        })
        .into_iter()
        .for_each(|(i, shift)| blocks[i].x += shift);
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
                        x: block.x + padding.left + local.x,
                        y: block.y + padding.top + local.y,
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

/// 将 EGB 结果合并进 compose 产出的组框：顶层叶子组保留 macro 定位，容器/嵌套组采用重算结果。
fn merge_egb_groups(
    diagram: &Diagram,
    compose_groups: &mut HashMap<String, GroupLayout>,
    computed_groups: HashMap<String, GroupLayout>,
    side_gutters: &BTreeMap<String, SideGutter>,
    base_padding: GroupPadding,
) {
    let top_level: HashSet<String> = diagram
        .groups
        .iter()
        .filter(|g| g.parent_id.is_none())
        .map(|g| g.id.as_str().to_string())
        .collect();

    for (gid, cgl) in computed_groups {
        let Some(ast_group) = diagram.groups.iter().find(|g| g.id.as_str() == gid) else {
            compose_groups.insert(gid, cgl);
            continue;
        };
        let is_top_leaf = top_level.contains(&gid) && !is_container_group_ast(ast_group);
        if is_top_leaf {
            if let Some(budget) = side_gutters.get(&gid) {
                if let Some(gl) = compose_groups.get_mut(&gid) {
                    // max(base, egb) 增量扩壳；顶层不扩左侧，避免拆 SharedLines 左缘
                    expand_gutter_delta_in_place(gl, *budget, base_padding, true);
                }
            }
        } else {
            compose_groups.insert(gid, cgl);
        }
    }
}

/// Uniform：顶层叶子按「全带最大 EGB 增量」同步扩壳，保持等宽。
fn equalize_top_leaf_egb_deltas(
    diagram: &Diagram,
    groups: &mut HashMap<String, GroupLayout>,
    side_gutters: &BTreeMap<String, SideGutter>,
    base: GroupPadding,
) {
    let top_leaves: Vec<String> = diagram
        .groups
        .iter()
        .filter(|g| g.parent_id.is_none() && g.child_group_ids.is_empty())
        .map(|g| g.id.as_str().to_string())
        .collect();
    if top_leaves.len() < 2 {
        return;
    }

    // 顶层左缘锁定：水平增量只补到右侧，保持等宽且不拆 SharedLines。
    let mut max_right = 0.0_f64;
    let mut max_top = 0.0_f64;
    let mut max_bottom = 0.0_f64;
    for gid in &top_leaves {
        let Some(budget) = side_gutters.get(gid) else {
            continue;
        };
        let h = (budget.left - base.left).max(0.0) + (budget.right - base.right).max(0.0);
        max_right = max_right.max(h);
        max_top = max_top.max((budget.top - base.top).max(0.0));
        max_bottom = max_bottom.max((budget.bottom - base.bottom).max(0.0));
    }

    for gid in &top_leaves {
        let Some(gl) = groups.get_mut(gid) else {
            continue;
        };
        let budget = side_gutters.get(gid).copied().unwrap_or_default();
        let have_h =
            (budget.left - base.left).max(0.0) + (budget.right - base.right).max(0.0);
        // merge 时 lock_left，实际已扩的是 right 侧的 have_r
        let have_r = (budget.right - base.right).max(0.0);
        let have_t = (budget.top - base.top).max(0.0);
        let have_b = (budget.bottom - base.bottom).max(0.0);
        let add_r = max_right - have_r;
        let add_t = max_top - have_t;
        let add_b = max_bottom - have_b;
        let _ = have_h;
        if add_r + add_t + add_b <= f64::EPSILON {
            continue;
        }
        gl.width += add_r;
        gl.y -= add_t;
        gl.height += add_t + add_b;
    }
}

fn is_container_group_ast(group: &Group) -> bool {
    !group.child_group_ids.is_empty()
}

/// 仅扩出超出 base padding 的 EGB 差额（总内边 = max(base, egb)）。
/// `lock_left`：顶层叶子锁定左缘，把通道预留放在有边的一侧。
fn expand_gutter_delta_in_place(
    gl: &mut GroupLayout,
    budget: SideGutter,
    base: GroupPadding,
    lock_left: bool,
) {
    let left = if lock_left {
        0.0
    } else {
        (budget.left - base.left).max(0.0)
    };
    let right = (budget.right - base.right).max(0.0);
    let top = (budget.top - base.top).max(0.0);
    let bottom = (budget.bottom - base.bottom).max(0.0);
    if left + right + top + bottom <= f64::EPSILON {
        return;
    }
    gl.x -= left;
    gl.width += left + right;
    gl.y -= top;
    gl.height += top + bottom;
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

    // 对每组：计算动态微调（按 gid 排序保证确定性）
    let mut group_ids: Vec<String> = group_node_targets.keys().cloned().collect();
    group_ids.sort();
    for gid in group_ids {
        let Some(entries) = group_node_targets.get(&gid) else {
            continue;
        };
        let Some(gl) = groups.get(&gid) else {
            continue;
        };
        let pad = GroupPadding::architecture_v2().left;
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

        // 应用最终位置，clamp 到组框。
        // Fit 下节点宽 ≈ 组内可用宽时，浮点误差可能使 max < min；
        // 先排序边界再 clamp，避免 release 下 f64::clamp panic。
        for (node_id, width, desired_x) in planned {
            let Some(nl) = nodes.get_mut(&node_id) else {
                continue;
            };
            let node_min_x = group_min_x;
            let node_max_x = group_max_x - width;
            let lo = node_min_x.min(node_max_x);
            let hi = node_min_x.max(node_max_x);
            nl.x = desired_x.clamp(lo, hi);
        }
    }
}

/// Phase F：同 leaf-group 内 y 中心接近的节点微对齐到中位数。
///
/// 仅处理中心距 < `NODE_GAP` 的近邻带，避免把不同 Sugiyama 层强行并层。
fn align_intra_group_same_rank_y(diagram: &Diagram, nodes: &mut HashMap<String, NodeLayout>) {
    let mut leaf_members: Vec<(String, Vec<String>)> = diagram
        .groups
        .iter()
        .filter(|g| g.child_group_ids.is_empty())
        .map(|g| {
            let mut ids: Vec<String> = g.entity_ids.iter().map(|id| id.as_str().to_string()).collect();
            ids.sort();
            (g.id.as_str().to_string(), ids)
        })
        .collect();
    leaf_members.sort_by(|a, b| a.0.cmp(&b.0));

    for (_gid, members) in leaf_members {
        if members.len() < 2 {
            continue;
        }
        let mut items: Vec<(String, f64)> = members
            .iter()
            .filter_map(|id| {
                nodes
                    .get(id)
                    .map(|nl| (id.clone(), nl.y + nl.height / 2.0))
            })
            .collect();
        if items.len() < 2 {
            continue;
        }
        items.sort_by(|a, b| {
            a.1.partial_cmp(&b.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });

        let mut i = 0;
        while i < items.len() {
            let mut j = i + 1;
            while j < items.len() && (items[j].1 - items[i].1) < NODE_GAP {
                j += 1;
            }
            if j - i >= 2 {
                let mut centers: Vec<f64> = items[i..j].iter().map(|(_, cy)| *cy).collect();
                centers.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let median = centers[centers.len() / 2];
                for (id, _) in &items[i..j] {
                    if let Some(nl) = nodes.get_mut(id) {
                        let target_y = median - nl.height / 2.0;
                        let delta = (target_y - nl.y)
                            .clamp(-CROSS_GROUP_Y_ALIGN_MAX, CROSS_GROUP_Y_ALIGN_MAX);
                        nl.y += delta;
                    }
                }
            }
            i = j;
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

        // Phase 1：strategy.compute 只出 content-fit；process（3 节点）应宽于 source/storage。
        // 全管线 L1 Equal 另由 etl_default_equal_survives_full_layout_pipeline 覆盖。
        assert!(
            process.width > source.width + 8.0,
            "content-fit: process should be wider than source ({:.1} vs {:.1})",
            process.width,
            source.width
        );
        assert!(
            process.width > storage.width + 8.0,
            "content-fit: process should be wider than storage ({:.1} vs {:.1})",
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

        // 所有组内节点在组框内（按 architecture_v2 非对称 padding）
        let pad = GroupPadding::architecture_v2();
        for (gid, members) in [
            ("source", vec!["app_db", "log_server"]),
            ("process", vec!["kafka", "flink", "spark"]),
            ("storage", vec!["hive", "clickhouse"]),
        ] {
            let g = result.groups.get(gid).unwrap();
            for eid in members {
                let n = result.nodes.get(eid).unwrap();
                assert!(
                    n.x >= g.x + pad.left - 0.5
                        && n.x + n.width <= g.x + g.width - pad.right + 0.5
                        && n.y >= g.y + pad.top - 0.5
                        && n.y + n.height <= g.y + g.height - pad.bottom + 0.5,
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
        // strategy.compute 不再做 Equal；全管线 L1 负责条带拉齐。
        let d = etl_diagram_with_sizing(Some("uniform"));
        let result = crate::layout::compute_layout(&d).expect("layout");

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
    fn etl_default_equal_survives_full_layout_pipeline() {
        use crate::layout::compute_layout;
        let d = etl_diagram_with_sizing(None);
        let result = compute_layout(&d).expect("layout");

        let source = result.groups.get("source").unwrap();
        let process = result.groups.get("process").unwrap();
        let storage = result.groups.get("storage").unwrap();

        assert!(
            (source.width - process.width).abs() < 1.0,
            "default Equal: source/process width {:.1} vs {:.1}",
            source.width,
            process.width
        );
        assert!(
            (process.width - storage.width).abs() < 1.0,
            "default Equal: process/storage width"
        );
    }

    #[test]
    fn etl_fit_escape_keeps_content_widths() {
        use crate::layout::compute_layout;
        let d = etl_diagram_with_sizing(Some("fit"));
        let result = compute_layout(&d).expect("layout");

        let source = result.groups.get("source").unwrap();
        let process = result.groups.get("process").unwrap();
        assert!(
            process.width > source.width + 8.0,
            "fit escape: process should stay wider than source"
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
        // RankBand Center：单 group 行相对画布居中，不再强制左对齐
        let canvas_left = [source, process, storage]
            .iter()
            .map(|g| g.x)
            .fold(f64::INFINITY, f64::min);
        let canvas_right = [source, process, storage]
            .iter()
            .map(|g| g.x + g.width)
            .fold(f64::NEG_INFINITY, f64::max);
        let canvas_cx = (canvas_left + canvas_right) / 2.0;
        for (name, g) in [("source", source), ("process", process), ("storage", storage)] {
            let g_cx = g.x + g.width / 2.0;
            assert!(
                (g_cx - canvas_cx).abs() < 8.0,
                "{name} should be centered in RankBand, cx={g_cx:.1} canvas_cx={canvas_cx:.1}"
            );
        }
    }
}
