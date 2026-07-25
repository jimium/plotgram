//! 分层布局内核：从 Diagram 产出 LayeredDraft（rank + order + 间距）。
//!
//! LayeredKernel 封装 Sugiyama 管线的 Step 1-7：
//! 图构建 → FAS → DAG → rank 分配 → 密度感知 → proper graph → 排序。
//!
//! 产出的 [`LayeredDraft`] 是纯数据 IR，供 CoordinateKernel 消费。

use crate::ast::Diagram;
use crate::layout::kernel::layered::graph;
use crate::layout::kernel::layered::order;
use crate::layout::kernel::layered::rank;
use crate::layout::kernel::layered::preset::SugiyamaPreset;
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::{HashMap, HashSet};

use super::engine::{
    apply_density_aware_spacing, apply_group_rank_constraints, apply_same_layer_rank_overrides,
    apply_sink_rank_constraints, apply_state_semantic_rank_constraints, compute_per_layer_gaps,
    extract_end_ids, identify_same_layer_edges, SameLayerEdge,
};

/// 分层布局中间产物：rank + order + 间距的完整描述。
///
/// 由 [`LayeredKernel::compute`] 产出，供坐标求解阶段消费。
/// 包含从 Diagram 到排序后分层图的全部中间状态。
#[derive(Clone)]
pub(in crate::layout) struct LayeredDraft {
    // ─── 图结构 ───
    /// DAG（节点权重 = entity_id 字符串）。
    pub dag: DiGraph<String, ()>,
    /// 含 dummy 节点的分层图。
    pub proper_graph: DiGraph<graph::LayerNode, ()>,
    /// 排序后的层（proper graph 索引）。
    pub layers: Vec<Vec<NodeIndex>>,
    /// proper 节点尺寸 (width, height)。
    pub sizes: HashMap<NodeIndex, (f64, f64)>,

    // ─── 尺寸与间距 ───
    /// 逐层 boundary gap（gap[i] 用于 rank i 与 i+1 之间）。
    pub per_layer_gaps: Vec<f64>,
    /// 画布 padding。
    pub padding: f64,
    /// 完整 preset（坐标赋值需要）。
    pub preset: SugiyamaPreset,

    // ─── 语义信息（供 hints / product 使用）───
    /// entity_id → rank 映射。
    pub sugiyama_ranks: HashMap<String, usize>,
    /// 同层边 (from_entity_id, to_entity_id)。
    pub same_layer_edges: Vec<(String, String)>,
    /// feedback hubs (hub_entity_id, primary_pred_entity_id)。
    pub feedback_hubs: Vec<(String, String)>,
    /// 布局方向：true = left-to-right。
    pub horizontal: bool,
    /// 是否有同层边偏置。
    pub has_order_bias: bool,
    /// type=end 的实体 ID 列表（供坐标求解器结构 objectives）。
    pub end_ids: Vec<String>,
}

/// 分层布局内核。
///
/// 封装 Sugiyama 管线的图构建、rank 分配、排序阶段。
/// 不包含图类型语义分支——差异通过 preset 参数注入。
pub(in crate::layout) struct LayeredKernel;

impl LayeredKernel {
    /// 从 Diagram + preset 计算分层布局 IR。
    ///
    /// 执行 Step 1-7：图构建 → FAS → DAG → rank → 密度感知 → proper → 排序。
    pub fn compute(
        diagram: &Diagram,
        preset: &SugiyamaPreset,
    ) -> LayeredDraft {
        let horizontal =
            crate::layout::resolve_effective_direction(diagram) == Some("left-to-right");

        // Step 1-2: 图构建 + FAS
        let g = graph::build_graph(diagram);
        let reversed_edges = graph::greedy_cycle_reversal(&g);
        let dag = graph::build_dag(&g, &reversed_edges);

        // Step 3: 同层边识别
        let reversed_edge_ids: HashSet<(String, String)> = reversed_edges
            .iter()
            .map(|&(from, to)| (g[from].clone(), g[to].clone()))
            .collect();

        let is_standard = preset.node_sizing
            == crate::layout::kernel::common::node_sizing::NodeSizing::Standard;
        let same_layer_edges: Vec<SameLayerEdge> =
            if is_standard && diagram.groups.is_empty() {
                identify_same_layer_edges(&dag, &reversed_edge_ids)
            } else {
                Vec::new()
            };
        let exempt_nodes: HashSet<NodeIndex> =
            same_layer_edges.iter().map(|e| e.to).collect();

        // Step 4: Rank 分配（5 轮覆盖）
        let mut ranks = rank::assign_ranks_network_simplex_style(&dag);
        if preset.node_sizing
            == crate::layout::kernel::common::node_sizing::NodeSizing::State
        {
            apply_state_semantic_rank_constraints(&dag, &mut ranks, diagram);
        } else if is_standard {
            apply_sink_rank_constraints(&dag, &mut ranks, diagram, &exempt_nodes);
        }
        apply_group_rank_constraints(&dag, &mut ranks, diagram);
        if is_standard {
            apply_sink_rank_constraints(&dag, &mut ranks, diagram, &exempt_nodes);
        }
        apply_same_layer_rank_overrides(&mut ranks, &same_layer_edges);

        // Step 5: 密度感知间距
        let adjusted_preset = apply_density_aware_spacing(&dag, &ranks, *preset);
        let per_layer_gaps =
            compute_per_layer_gaps(&dag, &ranks, adjusted_preset.layer_gap);

        // Step 6: Proper Layer Graph
        let proper =
            graph::build_proper_layer_graph(diagram, &dag, &ranks, &adjusted_preset);

        // 构建 order bias（proper graph 索引）
        let order_bias: HashMap<NodeIndex, NodeIndex> = same_layer_edges
            .iter()
            .map(|e| (e.to, e.from))
            .collect();
        let dag_to_proper: HashMap<NodeIndex, NodeIndex> = proper
            .graph
            .node_indices()
            .filter_map(|n| match proper.graph[n].kind {
                graph::LayerNodeKind::Real(dag_node) => Some((dag_node, n)),
                _ => None,
            })
            .collect();
        let order_bias_proper: HashMap<NodeIndex, NodeIndex> = order_bias
            .iter()
            .filter_map(|(&to, &from)| {
                match (dag_to_proper.get(&to), dag_to_proper.get(&from)) {
                    (Some(&to_p), Some(&from_p)) => Some((to_p, from_p)),
                    _ => None,
                }
            })
            .collect();

        // Step 7: 排序
        let node_group = build_node_group_map(diagram, &dag, &proper.graph);
        let group_decl = crate::layout::decl_order::group_sibling_decl_index(diagram);
        let layers = order::order_layers_weighted_median(
            &proper.graph,
            proper.layers,
            adjusted_preset.ordering_sweeps,
            adjusted_preset.long_edge_barycenter_weight,
            &node_group,
            &group_decl,
            &order_bias_proper,
        );

        // 导出语义信息（在 dag move 之前计算）
        let sugiyama_ranks: HashMap<String, usize> = dag
            .node_indices()
            .map(|n| (dag[n].clone(), ranks[&n]))
            .collect();
        let same_layer_edge_ids: Vec<(String, String)> = same_layer_edges
            .iter()
            .map(|e| (dag[e.from].clone(), dag[e.to].clone()))
            .collect();
        let feedback_hub_ids: Vec<(String, String)> = same_layer_edges
            .iter()
            .map(|e| (dag[e.to].clone(), dag[e.from].clone()))
            .collect();

        LayeredDraft {
            dag,
            proper_graph: proper.graph,
            layers,
            sizes: proper.sizes,
            per_layer_gaps,
            padding: adjusted_preset.padding,
            preset: adjusted_preset,
            sugiyama_ranks,
            same_layer_edges: same_layer_edge_ids,
            feedback_hubs: feedback_hub_ids,
            horizontal,
            has_order_bias: !order_bias.is_empty(),
            end_ids: extract_end_ids(diagram),
        }
    }
}

/// 构建 layered graph 节点 → group_id 映射。
fn build_node_group_map(
    diagram: &Diagram,
    dag: &DiGraph<String, ()>,
    layered_graph: &DiGraph<graph::LayerNode, ()>,
) -> HashMap<NodeIndex, Option<String>> {
    let node_to_top =
        crate::layout::kernel::common::group_map::build_node_to_top_group(diagram);

    layered_graph
        .node_indices()
        .map(|n| {
            let group = match &layered_graph[n].kind {
                graph::LayerNodeKind::Real(original) => {
                    let entity_id = &dag[*original];
                    node_to_top.get(entity_id).cloned()
                }
                graph::LayerNodeKind::Dummy { .. } => None,
            };
            (n, group)
        })
        .collect()
}
