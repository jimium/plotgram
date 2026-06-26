use crate::ast::Diagram;
use crate::layout::intent::topology::ValidTopologyIntent;
use crate::layout::node::common::acyclic;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use std::collections::{HashMap, HashSet};

use super::preset::SugiyamaPreset;

/// 边元数据：携带在 `DiGraph<String, EdgeMeta>` 的边权重中，
/// 供 `greedy_cycle_reversal` 判断是否可反转。
#[derive(Clone, Debug)]
pub(super) struct EdgeMeta {
    /// 是否允许 FAS 反转。真实边为 `true`，意图边为 `false`。
    pub reversible: bool,
}

#[derive(Clone, Debug)]
pub(super) enum LayerNodeKind {
    Real(NodeIndex),
    Dummy {
        source: NodeIndex,
        target: NodeIndex,
        segment: usize,
    },
}

#[derive(Clone, Debug)]
pub(super) struct LayerNode {
    pub kind: LayerNodeKind,
    pub rank: usize,
}

pub(super) struct ProperLayerGraph {
    pub graph: DiGraph<LayerNode, ()>,
    pub layers: Vec<Vec<NodeIndex>>,
    pub sizes: HashMap<NodeIndex, (f64, f64)>,
}

/// 构建 diagram 的有向图，可选注入拓扑意图约束边。
///
/// - 真实边：`EdgeMeta { reversible: true }`
/// - 意图边：`EdgeMeta { reversible: false }`
///   - `Below { from: A, to: B }` 注入 `B → A`（使 `rank(A) > rank(B)`，A 在 B 下方）
///   - `Above { from: A, to: B }` 注入 `A → B`（使 `rank(A) < rank(B)`，A 在 B 上方）
///
/// 意图边引用的节点不存在时静默跳过（由 `validate_topology_intents` 预先标记 `NotFound`）。
pub(super) fn build_graph_with_overlay(
    diagram: &Diagram,
    valid_topology: Option<&[ValidTopologyIntent]>,
) -> DiGraph<String, EdgeMeta> {
    let mut graph = DiGraph::<String, EdgeMeta>::new();
    let mut index = HashMap::new();

    for entity in &diagram.entities {
        let node = graph.add_node(entity.id.as_str().to_string());
        index.insert(entity.id.as_str().to_string(), node);
    }

    for relation in &diagram.relations {
        if let (Some(from), Some(to)) = (index.get(relation.from.as_str()), index.get(relation.to.as_str())) {
            graph.add_edge(*from, *to, EdgeMeta { reversible: true });
        }
    }

    if let Some(intents) = valid_topology {
        for v in intents {
            // ValidTopologyIntent.edge() 返回注入边方向：
            // Below(A,B) → B→A, Above(A,B) → A→B
            let (from, to) = v.edge();
            if let (Some(from_idx), Some(to_idx)) = (index.get(from), index.get(to)) {
                graph.add_edge(*from_idx, *to_idx, EdgeMeta { reversible: false });
            }
        }
    }

    graph
}

/// 贪心 FAS 去环，返回需要反转的边集合。
///
/// **意图边保护（路径 A）**：构建 FAS 邻接表时排除意图边（`reversible: false`），
/// 仅将真实边作为反转候选。意图边不参与 FAS 计算，因此永远不会被反转。
///
/// 前置条件：`validate_topology_intents` 已确保意图边不与真实边构成环。
/// 若真实边自身含环，FAS 仅反转真实边破环，意图边方向保持不变。
pub(super) fn greedy_cycle_reversal(graph: &DiGraph<String, EdgeMeta>) -> HashSet<(NodeIndex, NodeIndex)> {
    let nodes = graph.node_indices().collect::<Vec<_>>();
    let mut out_neighbors: HashMap<NodeIndex, Vec<NodeIndex>> = HashMap::new();
    let mut in_neighbors: HashMap<NodeIndex, Vec<NodeIndex>> = HashMap::new();

    for node in &nodes {
        let outs = graph
            .edges_directed(*node, Direction::Outgoing)
            .filter(|e| e.weight().reversible)
            .map(|e| e.target())
            .collect::<Vec<_>>();
        let ins = graph
            .edges_directed(*node, Direction::Incoming)
            .filter(|e| e.weight().reversible)
            .map(|e| e.source())
            .collect::<Vec<_>>();
        out_neighbors.insert(*node, outs);
        in_neighbors.insert(*node, ins);
    }

    acyclic::greedy_fas(&nodes, &out_neighbors, &in_neighbors)
}

/// 根据反转边集合构建 DAG。
///
/// 输入图的边权重为 `EdgeMeta`，输出 DAG 的边权重为 `()`（下游 rank/order/coordinate
/// 只读拓扑结构，不读边权重）。反转的边方向调换，未反转的边方向保持。
pub(super) fn build_dag(
    graph: &DiGraph<String, EdgeMeta>,
    reversed_edges: &HashSet<(NodeIndex, NodeIndex)>,
) -> DiGraph<String, ()> {
    let mut dag = DiGraph::<String, ()>::new();
    let mut remap = HashMap::new();

    for node in graph.node_indices() {
        let new_node = dag.add_node(graph[node].clone());
        remap.insert(node, new_node);
    }

    for edge in graph.edge_indices() {
        let (from, to) = graph.edge_endpoints(edge).unwrap();
        let new_from = remap[&from];
        let new_to = remap[&to];
        if reversed_edges.contains(&(from, to)) {
            dag.add_edge(new_to, new_from, ());
        } else {
            dag.add_edge(new_from, new_to, ());
        }
    }

    dag
}

pub(super) fn build_node_sizes(
    diagram: &Diagram,
    dag: &DiGraph<String, ()>,
    preset: &SugiyamaPreset,
) -> HashMap<NodeIndex, (f64, f64)> {
    let (default_w, default_h) = preset.default_node_size();
    dag.node_indices()
        .map(|node| {
            let (width, height) = diagram
                .find_entity(&dag[node])
                .map(|entity| super::postprocess::sized_node_for(diagram, entity, preset))
                .unwrap_or((default_w, default_h));
            (node, (width, height))
        })
        .collect()
}

pub(super) fn build_proper_layer_graph(
    diagram: &Diagram,
    dag: &DiGraph<String, ()>,
    ranks: &HashMap<NodeIndex, usize>,
    preset: &SugiyamaPreset,
) -> ProperLayerGraph {
    let original_sizes = build_node_sizes(diagram, dag, preset);
    let (default_w, default_h) = preset.default_node_size();
    let (dummy_w, dummy_h) = preset.dummy_node_size();
    let mut graph = DiGraph::<LayerNode, ()>::new();
    let mut layers = vec![Vec::new(); ranks.values().copied().max().unwrap_or(0) + 1];
    let mut real_nodes = HashMap::new();
    let mut sizes = HashMap::new();

    for node in dag.node_indices() {
        let rank = ranks[&node];
        let expanded = graph.add_node(LayerNode {
            kind: LayerNodeKind::Real(node),
            rank,
        });
        layers[rank].push(expanded);
        real_nodes.insert(node, expanded);
        sizes.insert(
            expanded,
            original_sizes
                .get(&node)
                .copied()
                .unwrap_or((default_w, default_h)),
        );
    }

    for edge in dag.edge_indices() {
        let (from, to) = dag.edge_endpoints(edge).unwrap();
        let from_rank = ranks[&from];
        let to_rank = ranks[&to];
        let mut prev = real_nodes[&from];

        if to_rank <= from_rank + 1 {
            graph.add_edge(prev, real_nodes[&to], ());
            continue;
        }

        for rank in (from_rank + 1)..to_rank {
            let dummy = graph.add_node(LayerNode {
                kind: LayerNodeKind::Dummy {
                    source: from,
                    target: to,
                    segment: rank - from_rank,
                },
                rank,
            });
            layers[rank].push(dummy);
            sizes.insert(dummy, (dummy_w, dummy_h));
            graph.add_edge(prev, dummy, ());
            prev = dummy;
        }

        graph.add_edge(prev, real_nodes[&to], ());
    }

    ProperLayerGraph { graph, layers, sizes }
}
