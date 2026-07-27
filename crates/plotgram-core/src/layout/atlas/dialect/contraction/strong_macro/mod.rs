//! 两阶段架构图布局：组内 Sugiyama → 组间宏观定位 → 全局坐标回填
//!
//! 人类画架构图的顺序是「先定语义舞台（group），再在框内摆节点」。
//! 本模块将 group 提升为一等公民，而非节点包围盒的后验产物。
//!
//! # 与通用分治框架的关系
//!
//! 本模块复用 [`crate::layout::kernel::common::divide_and_conquer`] 的
//! `IntraLayout`、`GroupTree` 数据结构。组内布局的具体实现（含 hub 居中、
//! client 对齐等特化优化）保留在本模块。未来 flowchart 分治布局将实现
//! `IntraGroupLayouter` trait，共用同一套类型基础。

pub(super) use crate::layout::recipes::architecture::group_layout_hint::{
    align_nodes_in_column, assign_ranks_for_mode, resolve_group_layout_hint,
    resolve_group_layout_mode, GroupLayoutHint, GroupLayoutMode,
};
pub(super) use crate::layout::recipes::architecture::group_sizing::{
    parse_group_sizing, GroupSizingPolicy, GroupWidthBlock,
};
pub(super) use crate::layout::recipes::architecture::layout::acyclic::is_effective_edge;
pub(super) use crate::layout::recipes::architecture::layout::constants::PADDING;
pub(super) use crate::layout::recipes::architecture::layout::constants::{
    GROUP_GAP_X, GROUP_LABEL_HEIGHT, INTRA_LAYER_GAP, LAYER_GAP, NODE_GAP,
};
pub(super) use crate::layout::recipes::architecture::layout::order::{
    build_layers, order_layers_group_aware,
};
pub(super) use crate::layout::recipes::architecture::layout::postprocess::clamp_to_canvas;
pub(super) use crate::layout::recipes::architecture::layout::rank::{
    assign_intra_ranks, assign_super_macro_ranks,
};
pub(super) use crate::layout::recipes::architecture::layout::types::{
    ArchDiagramFacts, GraphIndex, GroupMap,
};
pub(super) use crate::ast::{Diagram, Group};
pub(super) use crate::layout::algorithm_config::ArchitectureV2LayoutConfig;
pub(super) use crate::layout::constants;
pub(super) use crate::layout::kernel::common::divide_and_conquer::{GroupTree, IntraLayout};
pub(super) use crate::layout::kernel::common::edge_gutter::estimate_side_gutters_with_hierarchy;
pub(super) use crate::layout::kernel::common::group_bounds::GroupPadding;
pub(super) use crate::layout::{GroupLayout, LayoutResult, NodeLayout};
pub(super) use std::collections::{HashMap, HashSet};

mod intra;
mod intra_builder;
mod macro_block;
mod phase_d;
mod super_graph;

use intra::*;
use macro_block::*;
use phase_d::*;
use super_graph::*;

/// 宏观布局块：顶层 group 或无组节点簇
pub(super) struct MacroBlock {
    id: String,
    is_group: bool,
    width: f64,
    height: f64,
    x: f64,
    y: f64,
    intra: IntraLayout,
}

impl GroupWidthBlock for MacroBlock {
    fn block_id(&self) -> &str {
        &self.id
    }

    fn is_group_block(&self) -> bool {
        self.is_group
    }
}

/// 宏观行内块的水平对齐策略（架构图默认左对齐，避免窄行居中偏移）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RowAlign {
    Start,
    Center,
}

pub(crate) fn compute_two_phase_layout(
    diagram: &Diagram,
    graph: &GraphIndex,
    group_map: &GroupMap,
    sizes: &HashMap<String, (f64, f64)>,
    reversed_edges: &HashSet<(String, String)>,
    layout_config: ArchitectureV2LayoutConfig,
) -> LayoutResult {
    let core = strong_layout_core(
        diagram,
        graph,
        group_map,
        sizes,
        reversed_edges,
        layout_config,
    );
    let mut nodes = core.nodes;
    let mut groups = core.provisional_groups;
    phase_d_postprocess(
        diagram,
        &mut nodes,
        &mut groups,
        &core.blocks,
        &core.block_row,
        graph,
        group_map,
        sizes,
        core.padding,
        core.sizing,
        reversed_edges,
    )
}

/// R3-5：Atlas / 测试共用的收缩→放置→expand 核（不含 Phase D 物化）。
pub struct StrongExpandResult {
    pub nodes: HashMap<String, NodeLayout>,
    pub slots: std::collections::BTreeMap<String, crate::layout::atlas::plan::Slot>,
    pub sugiyama_ranks: HashMap<String, usize>,
    pub canvas_padding: f64,
}

/// R3-5：contract → macro place → expand → 显式 slots（无 `slots_from_ranks`）。
pub fn solve_strong_contract_expand(
    diagram: &Diagram,
    layout_config: ArchitectureV2LayoutConfig,
) -> StrongExpandResult {
    use crate::layout::kernel::common::node_sizing;
    use crate::layout::recipes::architecture::layout::{acyclic, types};

    let sizes = node_sizing::standard_node_sizes(diagram);
    let mut graph = types::GraphIndex::build(diagram);
    let group_map = types::build_group_map(diagram);
    let constraint_edges: Vec<(&str, &str)> = diagram
        .constraints
        .iter()
        .map(|c| (c.from.as_str(), c.to.as_str()))
        .collect();
    acyclic::inject_irreversible_edges(&mut graph, &constraint_edges);
    let constraint_set: HashSet<(String, String)> = constraint_edges
        .iter()
        .map(|(f, t)| (f.to_string(), t.to_string()))
        .collect();
    let reversed_edges = acyclic::find_edges_to_reverse(&graph, &constraint_set);
    let core = strong_layout_core(
        diagram,
        &graph,
        &group_map,
        &sizes,
        &reversed_edges,
        layout_config,
    );
    StrongExpandResult {
        nodes: core.nodes,
        slots: core.slots,
        sugiyama_ranks: core.sugiyama_ranks,
        canvas_padding: core.canvas_padding,
    }
}

struct StrongLayoutCore {
    nodes: HashMap<String, NodeLayout>,
    provisional_groups: HashMap<String, GroupLayout>,
    blocks: Vec<MacroBlock>,
    block_row: HashMap<String, usize>,
    padding: GroupPadding,
    sizing: GroupSizingPolicy,
    slots: std::collections::BTreeMap<String, crate::layout::atlas::plan::Slot>,
    sugiyama_ranks: HashMap<String, usize>,
    canvas_padding: f64,
}

fn strong_layout_core(
    diagram: &Diagram,
    graph: &GraphIndex,
    group_map: &GroupMap,
    sizes: &HashMap<String, (f64, f64)>,
    reversed_edges: &HashSet<(String, String)>,
    layout_config: ArchitectureV2LayoutConfig,
) -> StrongLayoutCore {
    let padding =
        if (layout_config.group_padding - constants::ARCH_V2_GROUP_PADDING).abs() < f64::EPSILON {
            GroupPadding::architecture()
        } else {
            GroupPadding::uniform(layout_config.group_padding, GROUP_LABEL_HEIGHT)
        };
    let canvas_padding = layout_config.padding;

    let group_tree = GroupTree::build(diagram);
    let intra_by_group = phase_a_intra_layout(
        diagram,
        &group_tree,
        &group_map.top_groups,
        graph,
        sizes,
        reversed_edges,
        &padding,
    );

    let (super_members, super_edges, pair_edge_counts, edge_weights) =
        build_super_graph(graph, group_map, reversed_edges);
    let group_decl = crate::layout::decl_order::group_sibling_decl_index(diagram);
    let constraint_super_edges: HashSet<(String, String)> = diagram
        .constraints
        .iter()
        .filter_map(|c| {
            let from_super = super_node_id(c.from.as_str(), group_map);
            let to_super = super_node_id(c.to.as_str(), group_map);
            if from_super != to_super {
                Some((from_super, to_super))
            } else {
                None
            }
        })
        .collect();
    let macro_ranks = assign_super_macro_ranks(
        &super_members,
        &super_edges,
        &edge_weights,
        &graph.node_ids,
        &group_decl,
        &constraint_super_edges,
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

    let block_row = position_macro_blocks(
        &mut blocks,
        &macro_ranks,
        &super_edges,
        &pair_edge_counts,
        canvas_padding,
        RowAlign::Start,
        &group_decl,
    );

    // R3-5：expand_contraction（Super=内容原点）替代 compose_global_layout 手写回填
    let (mut nodes, mut groups) = expand_global_layout(&blocks, &padding);

    nudge_intra_nodes_toward_cross_group_edges(
        &mut nodes,
        &groups,
        &super_edges,
        &super_members,
        graph,
        reversed_edges,
    );

    // 与 Phase D 种子一致：canvas clamp 后再建 slots（层归属来自对齐层，order 按 x）
    clamp_to_canvas(&mut nodes, sizes);
    let (slots, sugiyama_ranks) = member_slots_from_blocks(&blocks, &block_row, &nodes);

    StrongLayoutCore {
        nodes,
        provisional_groups: groups,
        blocks,
        block_row,
        padding,
        sizing,
        slots,
        sugiyama_ranks,
        canvas_padding,
    }
}

/// Super 点 = 内容原点（组块含 padding.left/top）；与 `compose_global_layout` 数值对齐。
fn expand_global_layout(
    blocks: &[MacroBlock],
    padding: &GroupPadding,
) -> (HashMap<String, NodeLayout>, HashMap<String, GroupLayout>) {
    use crate::layout::atlas::dialect::contraction::meta::{expand_contraction, ContractionMeta};
    use crate::layout::atlas::dialect::contraction::weak::ArrangementMode;
    use std::collections::BTreeMap;

    let mut intras = BTreeMap::new();
    let mut entity_to_super = BTreeMap::new();
    let mut super_ids = Vec::new();
    let mut super_nodes = HashMap::new();
    let mut groups = HashMap::new();

    for block in blocks {
        super_ids.push(block.id.clone());
        for nid in block.intra.nodes.keys() {
            entity_to_super.insert(nid.clone(), block.id.clone());
        }
        intras.insert(block.id.clone(), block.intra.clone());

        let (sx, sy) = if block.is_group {
            (block.x + padding.left, block.y + padding.top)
        } else {
            (block.x, block.y)
        };
        super_nodes.insert(
            block.id.clone(),
            NodeLayout {
                x: sx,
                y: sy,
                width: block.width,
                height: block.height,
            },
        );

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
        }
    }

    let meta = ContractionMeta {
        intras,
        super_ids,
        entity_to_super,
        mode: ArrangementMode::Vertical,
    };
    let nodes = expand_contraction(&meta, &super_nodes);
    (nodes, groups)
}

/// 层归属：同行对齐 `intra.layers`；同层 order：按 expand 后 x（确定性，对齐旧 slots_from_ranks）。
fn member_slots_from_blocks(
    blocks: &[MacroBlock],
    block_row: &HashMap<String, usize>,
    nodes: &HashMap<String, NodeLayout>,
) -> (
    std::collections::BTreeMap<String, crate::layout::atlas::plan::Slot>,
    HashMap<String, usize>,
) {
    use crate::layout::atlas::plan::Slot;
    use std::collections::BTreeMap;

    let layers = rebuild_layers_from_metadata(blocks, block_row);
    let mut slots = BTreeMap::new();
    let mut sugiyama_ranks = HashMap::new();
    for (rank, layer) in layers.iter().enumerate() {
        let mut ids: Vec<String> = layer
            .iter()
            .filter(|id| nodes.contains_key(id.as_str()))
            .cloned()
            .collect();
        ids.sort_by(|a, b| {
            let xa = nodes.get(a).map(|n| n.x).unwrap_or(0.0);
            let xb = nodes.get(b).map(|n| n.x).unwrap_or(0.0);
            xa.partial_cmp(&xb)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.cmp(b))
        });
        for (order, id) in ids.into_iter().enumerate() {
            sugiyama_ranks.insert(id.clone(), rank);
            slots.insert(id, Slot { rank, order });
        }
    }
    (slots, sugiyama_ranks)
}

// ─── Phase A: 组内布局 ───────────────────────────────────

/// Phase A：递归构建每个顶层 group 的组内 IntraLayout。
pub(super) fn phase_a_intra_layout(
    diagram: &Diagram,
    group_tree: &GroupTree,
    top_groups: &[String],
    graph: &GraphIndex,
    sizes: &HashMap<String, (f64, f64)>,
    reversed_edges: &HashSet<(String, String)>,
    padding: &GroupPadding,
) -> HashMap<String, IntraLayout> {
    let mut intra_by_group: HashMap<String, IntraLayout> = HashMap::new();
    for gid in top_groups {
        intra_by_group.insert(
            gid.clone(),
            layout_intra_group_recursive(
                diagram,
                gid,
                group_tree,
                graph,
                sizes,
                reversed_edges,
                padding,
            ),
        );
    }
    intra_by_group
}

// ─── Phase D: 后处理 ─────────────────────────────────────

/// 容器组内部的宏观块（与顶层 MacroBlock 类似，但仅用于组内）
pub(super) struct IntraMacroBlock {
    id: String,
    is_group: bool,
    width: f64,
    height: f64,
    x: f64,
    y: f64,
    intra: IntraLayout,
}

impl GroupWidthBlock for IntraMacroBlock {
    fn block_id(&self) -> &str {
        &self.id
    }

    fn is_group_block(&self) -> bool {
        self.is_group
    }
}

pub(super) fn normalize_to_origin(nodes: &mut HashMap<String, NodeLayout>) {
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

pub(super) fn content_bbox(nodes: &HashMap<String, NodeLayout>) -> (f64, f64) {
    if nodes.is_empty() {
        return (0.0, 0.0);
    }
    let max_x = nodes
        .values()
        .map(|n| n.x + n.width)
        .fold(0.0_f64, f64::max);
    let max_y = nodes
        .values()
        .map(|n| n.y + n.height)
        .fold(0.0_f64, f64::max);
    (max_x, max_y)
}

/// Dialect / legacy 共用入口：从 Diagram 编译并跑 StrongMacro 收缩。
pub(crate) fn compute_two_phase_layout_entry(
    diagram: &Diagram,
    layout_config: ArchitectureV2LayoutConfig,
) -> LayoutResult {
    use crate::layout::kernel::common::node_sizing;
    use crate::layout::recipes::architecture::layout::{acyclic, types};

    let sizes = node_sizing::standard_node_sizes(diagram);
    let mut graph = types::GraphIndex::build(diagram);
    let group_map = types::build_group_map(diagram);
    let constraint_edges: Vec<(&str, &str)> = diagram
        .constraints
        .iter()
        .map(|c| (c.from.as_str(), c.to.as_str()))
        .collect();
    acyclic::inject_irreversible_edges(&mut graph, &constraint_edges);
    let constraint_set: HashSet<(String, String)> = constraint_edges
        .iter()
        .map(|(f, t)| (f.to_string(), t.to_string()))
        .collect();
    let reversed_edges = acyclic::find_edges_to_reverse(&graph, &constraint_set);
    compute_two_phase_layout(
        diagram,
        &graph,
        &group_map,
        &sizes,
        &reversed_edges,
        layout_config,
    )
}

// ─── Phase B: 宏观组间定位 ───────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        ArrowType, AttributeMap, AttributeValue, Diagram, DiagramAttribute, Entity, Group,
        Identifier, Relation, SourceInfo, Span, TextValue,
    };
    use crate::layout::constants;
    use crate::layout::recipes::architecture::ArchitectureV2Layout;
    use crate::layout::LayoutStrategy;
    use crate::types::DiagramType;

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

    fn make_group_with_layout(id: &str, label: &str, layout: &str, entity_ids: Vec<&str>) -> Group {
        let mut attrs = AttributeMap::default();
        attrs.standard.insert(
            "layout".to_string(),
            AttributeValue::String(TextValue::unquoted(layout.to_string())),
        );
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

    fn etl_diagram_with_track(track: Option<&str>) -> Diagram {
        let mut attributes = track
            .map(|value| {
                let mut options = std::collections::HashMap::new();
                options.insert(
                    "track".to_string(),
                    AttributeValue::String(TextValue::unquoted(value.to_string())),
                );
                vec![DiagramAttribute {
                    key: "group_frame".to_string(),
                    value: AttributeValue::Config {
                        algo: "stack".to_string(),
                        options,
                    },
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
                make_group_with_layout(
                    "source",
                    "数据源层",
                    "horizontal",
                    vec!["app_db", "log_server"],
                ),
                make_group_with_layout(
                    "process",
                    "数据计算层",
                    "fan-out",
                    vec!["kafka", "flink", "spark"],
                ),
                make_group_with_layout(
                    "storage",
                    "数据存储层",
                    "vertical",
                    vec!["hive", "clickhouse"],
                ),
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
        let d = etl_diagram_with_track(None);
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
        let pad = GroupPadding::architecture();
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
    fn group_frame_track_dsl_ignored_stays_fit() {
        use crate::layout::recipes::architecture::group_sizing::{parse_group_sizing, GroupSizingPolicy};
        use crate::layout::recipes::frame_spec::{resolve_group_frame_spec, TrackSizing};

        // G-pre：uniform/equal DSL 不再映射到 Equal；恒 Fit
        for track in [Some("uniform"), Some("equal"), Some("fit"), None] {
            let d = etl_diagram_with_track(track);
            assert_eq!(
                resolve_group_frame_spec(&d, "architecture").track_sizing,
                TrackSizing::Fit
            );
            assert_eq!(parse_group_sizing(&d), GroupSizingPolicy::Fit);
        }
    }

    #[test]
    fn architecture_default_track_is_fit() {
        use crate::layout::recipes::architecture::group_sizing::{parse_group_sizing, GroupSizingPolicy};
        use crate::layout::recipes::frame_spec::{resolve_group_frame_spec, TrackSizing};

        let d = etl_diagram_with_track(None);
        assert_eq!(
            resolve_group_frame_spec(&d, "architecture").track_sizing,
            TrackSizing::Fit
        );
        assert_eq!(parse_group_sizing(&d), GroupSizingPolicy::Fit);
    }

    #[test]
    fn etl_fit_escape_keeps_content_widths() {
        // Stage 7：Atlas Hierarchical 默认 Equal 条带；不再断言 Fit 内容宽差。
        use crate::layout::compute_layout;
        use crate::layout::atlas::provenance_check::assert_channel_provenance_coverage;
        let d = etl_diagram_with_track(Some("fit"));
        let result = compute_layout(&d).expect("layout");
        assert!(result.groups.contains_key("source"));
        assert!(result.groups.contains_key("process"));
        assert_eq!(result.edges.len(), d.relations.len());
        if let Some(plan) = result.hints.atlas_plan.as_ref() {
            assert_channel_provenance_coverage(plan).expect("provenance");
        }
    }

    #[test]
    fn etl_layout_pipeline_produces_groups() {
        use crate::layout::compute_layout;
        let d = etl_diagram_with_track(Some("equal"));
        let result = compute_layout(&d).expect("full pipeline layout");
        assert!(result.groups.contains_key("source"));
        assert!(result.groups.contains_key("process"));
        assert!(result.groups.contains_key("storage"));
    }
}
