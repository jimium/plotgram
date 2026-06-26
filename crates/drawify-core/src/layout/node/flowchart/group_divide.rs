//! flowchart 分治布局：group 子图独立布局 + 组间堆叠合并
//!
//! 当流程图含 group 时，走分治路径：
//! 1. 每个 group（含无组节点虚拟块）独立调用 Sugiyama 布局
//! 2. 按跨 group 边拓扑排序 group
//! 3. 垂直堆叠各 group，合并全局坐标
//!
//! 无 group 时走原路径（`engine::compute_with_preset`），不受影响。
//!
//! # 与通用框架的关系
//!
//! 复用 [`crate::layout::node::common::divide_and_conquer`] 的
//! `IntraLayout`、`GroupTree`、`CrossGroupEdge`、`IntraGroupLayouter`、
//! `GroupArrangement` 类型与 trait。本模块实现 flowchart 场景的特化策略。

use crate::ast::{Diagram, DiagramAttribute, Entity, Relation};
use crate::layout::algorithm_config::SugiyamaLayoutConfig;
use crate::layout::node::common::divide_and_conquer::{
    CrossGroupEdge, GroupArrangement, GroupTree, IntraGroupLayouter, IntraLayout,
};
use crate::layout::node::sugiyama_v2::{engine, preset};
use crate::layout::{EdgeRoutingStyle, LayoutHints, LayoutResult, NodeLayout};
use crate::types::standard_attr_keys::diagram;
use std::collections::{HashMap, HashSet};

/// 无 group 节点的虚拟 group ID
pub const UNGROUPED_ID: &str = "__ungrouped__";

/// 组间排列方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrangementMode {
    /// 垂直堆叠（阶段划分，自上而下）
    Vertical,
    /// 水平堆叠（泳道图，从左到右）
    Horizontal,
}

impl ArrangementMode {
    /// 默认排列方向
    pub const DEFAULT: ArrangementMode = ArrangementMode::Vertical;

    /// 从 atom 字符串解析
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "vertical" => Some(ArrangementMode::Vertical),
            "horizontal" => Some(ArrangementMode::Horizontal),
            _ => None,
        }
    }
}

/// 组间对齐模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignMode {
    /// 水平居中（垂直排列时）/ 垂直居中（水平排列时）
    Center,
    /// 左对齐（垂直排列时）/ 顶部对齐（水平排列时）
    Left,
}

impl AlignMode {
    /// 默认对齐模式
    pub const DEFAULT: AlignMode = AlignMode::Center;

    /// 从 atom 字符串解析
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "center" => Some(AlignMode::Center),
            "left" => Some(AlignMode::Left),
            _ => None,
        }
    }
}

/// 从 diagram 属性读取组间排列配置
///
/// - `group_arrangement`：Atom，排列方向 vertical/horizontal（默认 vertical）
/// - `group_gap`：Number，group 间距（默认 60.0）
/// - `group_align`：Atom，对齐方式（默认 center）
fn read_arrangement_config(diagram: &Diagram) -> (f64, AlignMode, ArrangementMode) {
    let mut gap = 60.0;
    let mut align = AlignMode::DEFAULT;
    let mut mode = ArrangementMode::DEFAULT;

    for attr in &diagram.attributes {
        match attr.key.as_str() {
            diagram::GROUP_ARRANGEMENT => {
                if let Some(s) = attr.value.as_str() {
                    if let Some(m) = ArrangementMode::from_str(s) {
                        mode = m;
                    }
                }
            }
            diagram::GROUP_GAP => {
                if let crate::ast::AttributeValue::Number(n) = &attr.value {
                    if *n > 0.0 {
                        gap = *n;
                    }
                }
            }
            diagram::GROUP_ALIGN => {
                if let Some(s) = attr.value.as_str() {
                    if let Some(a) = AlignMode::from_str(s) {
                        align = a;
                    }
                }
            }
            _ => {}
        }
    }

    (gap, align, mode)
}

// ─── 组内布局策略 ─────────────────────────────────────────

/// flowchart 组内布局策略
///
/// 内部构建子 Diagram 并调用 `engine::compute_with_preset`（走 LayoutStrategy 入口），
/// 实现"组内布局由外部算法决定"（约束 3）。
pub struct FlowchartIntraGroupLayouter<'a> {
    diagram: &'a Diagram,
    config: SugiyamaLayoutConfig,
}

impl<'a> FlowchartIntraGroupLayouter<'a> {
    pub fn new(diagram: &'a Diagram, config: SugiyamaLayoutConfig) -> Self {
        Self { diagram, config }
    }

    /// 构建子 Diagram（只含 members 中的节点和内部边）
    fn build_sub_diagram(&self, members: &[String]) -> Diagram {
        let member_set: HashSet<&str> = members.iter().map(|s| s.as_str()).collect();

        // 过滤实体，group_id 清空（子图内部不再有 group 嵌套）
        let entities: Vec<Entity> = self
            .diagram
            .entities
            .iter()
            .filter(|e| member_set.contains(e.id.as_str()))
            .map(|e| {
                let mut cloned = e.clone();
                cloned.group_id = None;
                cloned
            })
            .collect();

        // 过滤内部边（from 和 to 都在 members 中）
        let relations: Vec<Relation> = self
            .diagram
            .relations
            .iter()
            .filter(|r| {
                member_set.contains(r.from.as_str()) && member_set.contains(r.to.as_str())
            })
            .cloned()
            .collect();

        // 继承外层方向等关键属性
        let attributes: Vec<DiagramAttribute> = self.diagram.attributes.clone();

        Diagram {
            diagram_type: self.diagram.diagram_type.clone(),
            attributes,
            entities,
            relations,
            groups: vec![],
            style_decls: vec![],
            doc_comment: None,
            source_info: Default::default(),
        }
    }
}

impl<'a> IntraGroupLayouter for FlowchartIntraGroupLayouter<'a> {
    fn layout_intra(&self, _group_id: &str, members: &[String]) -> IntraLayout {
        if members.is_empty() {
            return IntraLayout::empty();
        }

        // 单节点直接返回（避免子图布局的 padding 开销）
        if members.len() == 1 {
            let entity = self
                .diagram
                .entities
                .iter()
                .find(|e| e.id.as_str() == members[0].as_str());
            if let Some(e) = entity {
                let (w, h) = crate::layout::node::common::node_sizing::standard_node_size(e);
                return IntraLayout::single(&members[0], w, h);
            }
        }

        let sub_diagram = self.build_sub_diagram(members);
        let result = engine::compute_with_preset(
            &sub_diagram,
            &preset::FLOWCHART_PRESET,
            self.config,
        );

        // 从 LayoutResult 提取 IntraLayout
        //
        // 节点坐标保留 preset padding（不平移到原点），避免改变绝对坐标导致
        // grid_snap 的层聚类与槽位分配产生不同结果（曾引发节点重叠）。
        //
        // content_width/height 必须与 `compute_group_bounds` 的 GroupPadding 一致，
        // 否则 `refresh_layout_bounds` 重算 group bounds 后，group 间的 gap 会有偏差
        // （偏差 = 2*preset_padding - y_top - group_padding）。
        let layers = rebuild_layers_from_ranks(&result.hints.sugiyama_ranks, members);

        // 节点包围框（含 preset padding）
        let min_x = result.nodes.values().map(|n| n.x).fold(f64::INFINITY, f64::min);
        let min_y = result.nodes.values().map(|n| n.y).fold(f64::INFINITY, f64::min);
        let max_x = result.nodes.values().map(|n| n.x + n.width).fold(0.0_f64, f64::max);
        let max_y = result.nodes.values().map(|n| n.y + n.height).fold(0.0_f64, f64::max);

        let nodes = result.nodes;

        // content 尺寸 = 节点包围框 + GroupPadding（与 compute_group_bounds 一致）
        // GroupPadding::uniform(group_padding, header_height):
        //   x_delta = group_padding * 2
        //   y_delta = group_padding * 2 + header_height
        let group_padding = self.config.group_padding;
        let header_height = 16.0;
        let content_width = (max_x - min_x).max(0.0) + group_padding * 2.0;
        let content_height = (max_y - min_y).max(0.0) + group_padding * 2.0 + header_height;

        IntraLayout {
            nodes,
            content_width,
            content_height,
            layers,
        }
    }
}

/// 从 sugiyama_ranks 重建层结构（按 y 自上而下）
fn rebuild_layers_from_ranks(
    ranks: &Option<HashMap<String, usize>>,
    members: &[String],
) -> Vec<Vec<String>> {
    let Some(ranks) = ranks else {
        return vec![];
    };

    let max_rank = members
        .iter()
        .filter_map(|m| ranks.get(m))
        .copied()
        .max()
        .unwrap_or(0);

    let mut layers: Vec<Vec<String>> = vec![Vec::new(); max_rank + 1];
    for m in members {
        if let Some(&r) = ranks.get(m) {
            if r <= max_rank {
                layers[r].push(m.clone());
            }
        }
    }
    // 确定性排序
    for layer in &mut layers {
        layer.sort();
    }
    layers
}

// ─── 组间排列策略 ─────────────────────────────────────────

/// 堆叠排列：拓扑排序 + 垂直/水平堆叠
///
/// 实现 [`GroupArrangement`] trait，用于 flowchart 场景。
/// 支持垂直堆叠（阶段划分）和水平堆叠（泳道图）。
pub struct StackingArrangement {
    /// group 间距
    pub gap: f64,
    /// 对齐模式
    pub align: AlignMode,
    /// 排列方向
    pub mode: ArrangementMode,
}

impl StackingArrangement {
    pub fn new(gap: f64, align: AlignMode, mode: ArrangementMode) -> Self {
        Self { gap, align, mode }
    }
}

impl GroupArrangement for StackingArrangement {
    fn arrange(
        &self,
        group_ids: &[String],
        intra_layouts: &HashMap<String, IntraLayout>,
        cross_edges: &[CrossGroupEdge],
    ) -> HashMap<String, (f64, f64)> {
        // 1. 拓扑排序
        let order = topological_sort_groups(group_ids, cross_edges);

        let mut offsets = HashMap::new();

        match self.mode {
            ArrangementMode::Vertical => {
                // 垂直堆叠：group 自上而下排列，水平方向对齐
                let max_width = intra_layouts
                    .values()
                    .map(|l| l.content_width)
                    .fold(0.0_f64, f64::max);

                let mut y_offset = 0.0;
                for gid in &order {
                    let intra = intra_layouts.get(gid);
                    if intra.is_none() {
                        offsets.insert(gid.clone(), (0.0, y_offset));
                        continue;
                    }
                    let intra = intra.unwrap();
                    let x_offset = match self.align {
                        AlignMode::Center => (max_width - intra.content_width) / 2.0,
                        AlignMode::Left => 0.0,
                    };
                    offsets.insert(gid.clone(), (x_offset, y_offset));
                    y_offset += intra.content_height + self.gap;
                }
            }
            ArrangementMode::Horizontal => {
                // 水平堆叠：group 从左到右排列，垂直方向对齐
                let max_height = intra_layouts
                    .values()
                    .map(|l| l.content_height)
                    .fold(0.0_f64, f64::max);

                let mut x_offset = 0.0;
                for gid in &order {
                    let intra = intra_layouts.get(gid);
                    if intra.is_none() {
                        offsets.insert(gid.clone(), (x_offset, 0.0));
                        continue;
                    }
                    let intra = intra.unwrap();
                    let y_offset = match self.align {
                        AlignMode::Center => (max_height - intra.content_height) / 2.0,
                        AlignMode::Left => 0.0,
                    };
                    offsets.insert(gid.clone(), (x_offset, y_offset));
                    x_offset += intra.content_width + self.gap;
                }
            }
        }

        offsets
    }
}

/// 拓扑排序 group（Kahn's algorithm，确定性）
///
/// 入度为 0 的 group 按 `group_ids` 原始顺序（声明顺序）选择，保证确定性
/// （AGENTS.md 第 2 条）且符合主流程方向。
/// 有环时，剩余 group 按 `group_ids` 原始顺序追加。
///
/// P1.3: 使用 BinaryHeap 维护就绪队列，按声明顺序为优先级。
/// 原实现每轮 `sorted_queue.sort_by` 重排整个队列，O(G² log G)；
/// BinaryHeap 的 push/pop 均为 O(log G)，整体降至 O(G log G)。
fn topological_sort_groups(
    group_ids: &[String],
    cross_edges: &[CrossGroupEdge],
) -> Vec<String> {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;

    let id_set: HashSet<&str> = group_ids.iter().map(|s| s.as_str()).collect();

    // 声明顺序索引（用于确定性排序）
    let order_index: HashMap<&str, usize> = group_ids
        .iter()
        .enumerate()
        .map(|(i, g)| (g.as_str(), i))
        .collect();

    // 构建依赖图
    let mut in_degree: HashMap<String, usize> = HashMap::new();
    let mut out_edges: HashMap<String, Vec<String>> = HashMap::new();
    for gid in group_ids {
        in_degree.insert(gid.clone(), 0);
        out_edges.insert(gid.clone(), Vec::new());
    }

    for edge in cross_edges {
        let from = edge.from_group.as_deref().unwrap_or(UNGROUPED_ID);
        let to = edge.to_group.as_deref().unwrap_or(UNGROUPED_ID);
        if from != to && id_set.contains(from) && id_set.contains(to) {
            out_edges.get_mut(from).unwrap().push(to.to_string());
            *in_degree.get_mut(to).unwrap() += 1;
        }
    }

    // 去重 out_edges（同组间多条边只算一次依赖）
    for edges in out_edges.values_mut() {
        edges.sort();
        edges.dedup();
    }

    // BinaryHeap 维护就绪队列：按声明顺序（order_index）排序
    // BinaryHeap 是 max-heap，用 Reverse 使声明索引小的（更早声明的）优先弹出
    let mut ready: BinaryHeap<Reverse<(usize, String)>> = group_ids
        .iter()
        .filter(|g| in_degree.get(*g).copied().unwrap_or(0) == 0)
        .map(|g| {
            let idx = order_index.get(g.as_str()).copied().unwrap_or(usize::MAX);
            Reverse((idx, g.clone()))
        })
        .collect();

    let mut result = Vec::new();

    while let Some(Reverse((_, g))) = ready.pop() {
        result.push(g.clone());
        // 释放后继节点：入度 -1，归零则入队
        for next in out_edges.get(&g).unwrap_or(&vec![]) {
            let d = in_degree.get_mut(next).unwrap();
            *d -= 1;
            if *d == 0 {
                let idx = order_index.get(next.as_str()).copied().unwrap_or(usize::MAX);
                ready.push(Reverse((idx, next.clone())));
            }
        }
    }

    // 处理环：剩余 group 按 group_ids 原始顺序（声明顺序）追加
    if result.len() < group_ids.len() {
        let remaining: Vec<String> = group_ids
            .iter()
            .filter(|g| !result.contains(g))
            .cloned()
            .collect();
        result.extend(remaining);
    }

    result
}

// ─── 分治布局入口 ─────────────────────────────────────────

/// flowchart 分治布局入口
///
/// 检测到 diagram 含 group 时调用此函数。无 group 时应走原路径
/// （`engine::compute_with_preset`）。
pub fn divide_flowchart_with_groups(
    diagram: &Diagram,
    config: SugiyamaLayoutConfig,
) -> LayoutResult {
    // 1. 构建分组树
    let tree = GroupTree::build(diagram);

    // 2. 识别顶层 group（按 diagram 声明顺序，保证主流程方向）和无 group 节点
    let top_groups: Vec<String> = diagram
        .groups
        .iter()
        .filter(|g| g.parent_id.is_none())
        .map(|g| g.id.as_str().to_string())
        .collect();
    let ungrouped: Vec<String> = diagram
        .entities
        .iter()
        .filter(|e| e.group_id.is_none())
        .map(|e| e.id.as_str().to_string())
        .collect();

    // 3. 构建 entity → 顶层 group 映射（用于跨 group 边收集）
    let entity_to_group = build_entity_to_top_group(diagram, &top_groups, &tree);

    // 4. 组内布局
    let layouter = FlowchartIntraGroupLayouter::new(diagram, config);
    let mut intra_layouts: HashMap<String, IntraLayout> = HashMap::new();

    for gid in &top_groups {
        let members = tree.descendant_entities(gid);
        let intra = layouter.layout_intra(gid, &members);
        intra_layouts.insert(gid.clone(), intra);
    }

    // 无 group 节点作为虚拟 group
    if !ungrouped.is_empty() {
        let intra = layouter.layout_intra(UNGROUPED_ID, &ungrouped);
        intra_layouts.insert(UNGROUPED_ID.to_string(), intra);
    }

    // 5. 收集跨 group 边
    let cross_edges = collect_cross_edges(diagram, &entity_to_group);

    // 6. 组间排列
    let mut all_group_ids = top_groups.clone();
    if !ungrouped.is_empty() {
        all_group_ids.push(UNGROUPED_ID.to_string());
    }
    let (gap, align, mode) = read_arrangement_config(diagram);
    let arrangement = StackingArrangement::new(gap, align, mode);
    let order = topological_sort_groups(&all_group_ids, &cross_edges);
    let offsets = arrangement.arrange(&all_group_ids, &intra_layouts, &cross_edges);

    // 7. 合并全局坐标
    let mut nodes: HashMap<String, NodeLayout> = HashMap::new();

    for gid in &all_group_ids {
        let intra = &intra_layouts[gid];
        let (x_off, y_off) = offsets[gid];

        for (id, node) in &intra.nodes {
            let mut global_node = node.clone();
            global_node.x += x_off;
            global_node.y += y_off;
            nodes.insert(id.clone(), global_node);
        }
    }

    let groups = crate::layout::group::finalize_routing_groups(
        diagram,
        &nodes,
        "flowchart",
        config.group_padding,
    );

    let stacking_corridors = crate::layout::group::build_stacking_corridors(
        &order,
        &groups,
        mode == ArrangementMode::Vertical,
    );
    let group_routing = crate::layout::group::GroupRoutingHints {
        corridors: crate::layout::group::merge_corridors(&stacking_corridors, &groups),
        border_shell_pad: crate::layout::group::GROUP_BORDER_SHELL_PAD,
    };

    // 8. 计算总尺寸
    let padding = preset::FLOWCHART_PRESET.padding;
    let total_width = nodes
        .values()
        .map(|n| n.x + n.width)
        .fold(0.0_f64, f64::max)
        .max(groups.values().map(|g| g.x + g.width).fold(0.0_f64, f64::max))
        + padding;
    let total_height = nodes
        .values()
        .map(|n| n.y + n.height)
        .fold(0.0_f64, f64::max)
        .max(groups.values().map(|g| g.y + g.height).fold(0.0_f64, f64::max))
        + padding;

    LayoutResult {
        nodes,
        groups,
        edges: vec![],
        total_width,
        total_height,
        hints: LayoutHints {
            edge_routing_style: EdgeRoutingStyle::Orthogonal,
            group_routing: Some(group_routing),
            ..Default::default()
        },
    }
}

/// 构建 entity → 顶层 group 映射
fn build_entity_to_top_group(
    diagram: &Diagram,
    top_groups: &[String],
    tree: &GroupTree,
) -> HashMap<String, String> {
    let top_set: HashSet<&str> = top_groups.iter().map(|s| s.as_str()).collect();
    let mut mapping = HashMap::new();

    for gid in top_groups {
        for entity_id in tree.descendant_entities(gid) {
            mapping.insert(entity_id, gid.clone());
        }
    }

    // 无 group 节点不插入 mapping（get 返回 None 表示无 group）
    let _ = top_set; // 仅用于文档清晰
    let _ = diagram;
    mapping
}

/// 收集跨 group 边
fn collect_cross_edges(
    diagram: &Diagram,
    entity_to_group: &HashMap<String, String>,
) -> Vec<CrossGroupEdge> {
    let mut edges = Vec::new();
    for r in &diagram.relations {
        let from = r.from.as_str().to_string();
        let to = r.to.as_str().to_string();
        let from_group = entity_to_group.get(&from).cloned();
        let to_group = entity_to_group.get(&to).cloned();

        // 跨 group 边：from_group != to_group（含一方无 group 的情况）
        if from_group != to_group {
            edges.push(CrossGroupEdge {
                from,
                to,
                from_group,
                to_group,
            });
        }
    }
    edges
}

/// 判断 diagram 是否应该走分治路径
///
/// 仅当存在至少一个 group 时返回 true
pub fn should_divide(diagram: &Diagram) -> bool {
    !diagram.groups.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{ArrowType, Entity, Group, Identifier, Span};

    fn entity(id: &str, group: Option<&str>) -> Entity {
        Entity {
            id: Identifier::new_unchecked(id),
            label: id.to_string(),
            attributes: crate::ast::AttributeMap::default(),
            group_id: group.map(|g| Identifier::new_unchecked(g)),
            span: Span::dummy(),
        }
    }

    fn group(id: &str) -> Group {
        Group {
            id: Identifier::new_unchecked(id),
            label: id.to_string(),
            attributes: crate::ast::AttributeMap::default(),
            parent_id: None,
            depth: 0,
            entity_ids: vec![],
            child_group_ids: vec![],
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
            attributes: crate::ast::AttributeMap::default(),
            span: Span::dummy(),
        }
    }

    #[test]
    fn topological_sort_simple_chain() {
        let group_ids = vec!["g1".to_string(), "g2".to_string(), "g3".to_string()];
        let cross_edges = vec![
            CrossGroupEdge {
                from: "a".to_string(),
                to: "b".to_string(),
                from_group: Some("g1".to_string()),
                to_group: Some("g2".to_string()),
            },
            CrossGroupEdge {
                from: "b".to_string(),
                to: "c".to_string(),
                from_group: Some("g2".to_string()),
                to_group: Some("g3".to_string()),
            },
        ];
        let order = topological_sort_groups(&group_ids, &cross_edges);
        assert_eq!(order, vec!["g1", "g2", "g3"]);
    }

    #[test]
    fn topological_sort_with_cycle() {
        // g1 → g2 → g1（环），g3 独立
        // 声明顺序：g1, g2, g3
        let group_ids = vec!["g1".to_string(), "g2".to_string(), "g3".to_string()];
        let cross_edges = vec![
            CrossGroupEdge {
                from: "a".to_string(),
                to: "b".to_string(),
                from_group: Some("g1".to_string()),
                to_group: Some("g2".to_string()),
            },
            CrossGroupEdge {
                from: "b".to_string(),
                to: "a".to_string(),
                from_group: Some("g2".to_string()),
                to_group: Some("g1".to_string()),
            },
        ];
        let order = topological_sort_groups(&group_ids, &cross_edges);
        // g3 入度为 0，先出；g1/g2 成环，按声明顺序追加
        assert_eq!(order[0], "g3");
        assert_eq!(order[1], "g1");
        assert_eq!(order[2], "g2");
        assert_eq!(order.len(), 3);
    }

    #[test]
    fn topological_sort_deterministic() {
        // 两个独立 group，无跨边，应按声明顺序
        let group_ids = vec!["g2".to_string(), "g1".to_string()];
        let cross_edges = vec![];
        let order = topological_sort_groups(&group_ids, &cross_edges);
        // 声明顺序：g2 先，g1 后
        assert_eq!(order, vec!["g2", "g1"]);
    }

    #[test]
    fn stacking_arrangement_vertical() {
        let mut intra_layouts = HashMap::new();
        intra_layouts.insert(
            "g1".to_string(),
            IntraLayout {
                nodes: HashMap::new(),
                content_width: 100.0,
                content_height: 50.0,
                layers: vec![],
            },
        );
        intra_layouts.insert(
            "g2".to_string(),
            IntraLayout {
                nodes: HashMap::new(),
                content_width: 80.0,
                content_height: 60.0,
                layers: vec![],
            },
        );

        let arrangement = StackingArrangement::new(60.0, AlignMode::Center, ArrangementMode::Vertical);
        let group_ids = vec!["g1".to_string(), "g2".to_string()];
        let offsets = arrangement.arrange(&group_ids, &intra_layouts, &[]);

        // g1 在顶部，居中
        let (x1, y1) = offsets["g1"];
        assert_eq!(y1, 0.0);
        assert_eq!(x1, 0.0); // max_width=100, g1 宽 100，居中偏移 0

        // g2 在下方，居中
        let (x2, y2) = offsets["g2"];
        assert_eq!(y2, 50.0 + 60.0); // g1 高度 + gap
        assert_eq!(x2, (100.0 - 80.0) / 2.0); // 居中
    }

    #[test]
    fn stacking_arrangement_horizontal() {
        let mut intra_layouts = HashMap::new();
        intra_layouts.insert(
            "g1".to_string(),
            IntraLayout {
                nodes: HashMap::new(),
                content_width: 100.0,
                content_height: 50.0,
                layers: vec![],
            },
        );
        intra_layouts.insert(
            "g2".to_string(),
            IntraLayout {
                nodes: HashMap::new(),
                content_width: 80.0,
                content_height: 60.0,
                layers: vec![],
            },
        );

        let arrangement = StackingArrangement::new(40.0, AlignMode::Center, ArrangementMode::Horizontal);
        let group_ids = vec!["g1".to_string(), "g2".to_string()];
        let offsets = arrangement.arrange(&group_ids, &intra_layouts, &[]);

        // g1 在左侧，垂直居中
        let (x1, y1) = offsets["g1"];
        assert_eq!(x1, 0.0);
        assert_eq!(y1, (60.0 - 50.0) / 2.0); // max_height=60, g1 高 50，居中偏移 5

        // g2 在右侧，垂直居中
        let (x2, y2) = offsets["g2"];
        assert_eq!(x2, 100.0 + 40.0); // g1 宽度 + gap
        assert_eq!(y2, 0.0); // max_height=60, g2 高 60，居中偏移 0
    }

    #[test]
    fn should_divide_detects_groups() {
        let diagram_with_group = Diagram {
            groups: vec![group("g1")],
            ..Default::default()
        };
        assert!(should_divide(&diagram_with_group));

        let diagram_no_group = Diagram {
            groups: vec![],
            ..Default::default()
        };
        assert!(!should_divide(&diagram_no_group));
    }

    #[test]
    fn collect_cross_edges_separates_groups() {
        let diagram = Diagram {
            entities: vec![entity("a", Some("g1")), entity("b", Some("g2"))],
            relations: vec![
                relation("a", "b"), // 跨 group
            ],
            groups: vec![group("g1"), group("g2")],
            ..Default::default()
        };

        let tree = GroupTree::build(&diagram);
        let top_groups = tree.top_groups();
        let entity_to_group = build_entity_to_top_group(&diagram, &top_groups, &tree);
        let cross_edges = collect_cross_edges(&diagram, &entity_to_group);

        assert_eq!(cross_edges.len(), 1);
        assert_eq!(cross_edges[0].from_group, Some("g1".to_string()));
        assert_eq!(cross_edges[0].to_group, Some("g2".to_string()));
    }

    #[test]
    fn divide_flowchart_two_groups_no_overlap() {
        // g1: a → b, g2: c → d, 跨 group 边: b → c
        let diagram = Diagram {
            diagram_type: crate::types::DiagramType::Flowchart,
            entities: vec![
                entity("a", Some("g1")),
                entity("b", Some("g1")),
                entity("c", Some("g2")),
                entity("d", Some("g2")),
            ],
            relations: vec![
                relation("a", "b"),
                relation("b", "c"), // 跨 group
                relation("c", "d"),
            ],
            groups: vec![group("g1"), group("g2")],
            ..Default::default()
        };

        let result = divide_flowchart_with_groups(&diagram, SugiyamaLayoutConfig::default());

        // 验证：4 个节点都有坐标
        assert_eq!(result.nodes.len(), 4);
        assert!(result.nodes.contains_key("a"));
        assert!(result.nodes.contains_key("b"));
        assert!(result.nodes.contains_key("c"));
        assert!(result.nodes.contains_key("d"));

        // 验证：2 个 group 包围框
        assert_eq!(result.groups.len(), 2);
        assert!(result.groups.contains_key("g1"));
        assert!(result.groups.contains_key("g2"));

        // 验证：group 包围框不重叠（g1 在 g2 上方）
        let g1 = &result.groups["g1"];
        let g2 = &result.groups["g2"];
        assert!(
            g1.y + g1.height <= g2.y,
            "g1 bottom ({}) should be <= g2 top ({})",
            g1.y + g1.height,
            g2.y
        );

        // 验证：总尺寸合理
        assert!(result.total_width > 0.0);
        assert!(result.total_height > 0.0);
    }

    #[test]
    fn divide_flowchart_ungrouped_nodes() {
        // g1: a → b, 无 group: c, 跨 group 边: b → c
        let diagram = Diagram {
            diagram_type: crate::types::DiagramType::Flowchart,
            entities: vec![
                entity("a", Some("g1")),
                entity("b", Some("g1")),
                entity("c", None),
            ],
            relations: vec![relation("a", "b"), relation("b", "c")],
            groups: vec![group("g1")],
            ..Default::default()
        };

        let result = divide_flowchart_with_groups(&diagram, SugiyamaLayoutConfig::default());

        // 验证：3 个节点都有坐标
        assert_eq!(result.nodes.len(), 3);
        assert!(result.nodes.contains_key("c"));

        // 验证：只有 1 个 group 包围框（虚拟 group 不产出）
        assert_eq!(result.groups.len(), 1);
        assert!(result.groups.contains_key("g1"));
    }

    #[test]
    fn read_arrangement_config_defaults() {
        let diagram = Diagram {
            ..Default::default()
        };
        let (gap, align, mode) = read_arrangement_config(&diagram);
        assert_eq!(gap, 60.0);
        assert_eq!(align, AlignMode::Center);
        assert_eq!(mode, ArrangementMode::Vertical);
    }

    #[test]
    fn read_arrangement_config_custom() {
        use crate::ast::{AttributeValue, DiagramAttribute, TextValue};

        let diagram = Diagram {
            attributes: vec![
                DiagramAttribute {
                    key: "group_gap".to_string(),
                    value: AttributeValue::Number(120.0),
                    span: Span::dummy(),
                },
                DiagramAttribute {
                    key: "group_align".to_string(),
                    value: AttributeValue::String(TextValue::unquoted("left")),
                    span: Span::dummy(),
                },
                DiagramAttribute {
                    key: "group_arrangement".to_string(),
                    value: AttributeValue::String(TextValue::unquoted("horizontal")),
                    span: Span::dummy(),
                },
            ],
            ..Default::default()
        };
        let (gap, align, mode) = read_arrangement_config(&diagram);
        assert_eq!(gap, 120.0);
        assert_eq!(align, AlignMode::Left);
        assert_eq!(mode, ArrangementMode::Horizontal);
    }

    #[test]
    fn read_arrangement_config_ignores_invalid() {
        use crate::ast::{AttributeValue, DiagramAttribute, TextValue};

        // gap <= 0 应被忽略，align/arrangement 非法值应被忽略
        let diagram = Diagram {
            attributes: vec![
                DiagramAttribute {
                    key: "group_gap".to_string(),
                    value: AttributeValue::Number(-10.0),
                    span: Span::dummy(),
                },
                DiagramAttribute {
                    key: "group_align".to_string(),
                    value: AttributeValue::String(TextValue::unquoted("invalid")),
                    span: Span::dummy(),
                },
                DiagramAttribute {
                    key: "group_arrangement".to_string(),
                    value: AttributeValue::String(TextValue::unquoted("diagonal")),
                    span: Span::dummy(),
                },
            ],
            ..Default::default()
        };
        let (gap, align, mode) = read_arrangement_config(&diagram);
        assert_eq!(gap, 60.0); // 默认值
        assert_eq!(align, AlignMode::Center); // 默认值
        assert_eq!(mode, ArrangementMode::Vertical); // 默认值
    }
}
