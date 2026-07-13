use crate::ast::{ArrowType, Diagram};
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
    /// 是否允许 FAS 反转。真实边为 `true`，DSL `constrain` 边为 `false`。
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

/// 构建 diagram 的有向图。
///
/// - 主动边（非 `Passive`）：`EdgeMeta { reversible: true }`
/// - `Passive`（`-->`）：不参与分层/去环/排序，仅由路由绘制（与 architecture 一致）
/// - DSL `constrain` 边（若有）：`EdgeMeta { reversible: false }`，由调用方经
///   [`inject_irreversible_edges`] 注入，FAS 不会反转它们。
pub(super) fn build_graph(diagram: &Diagram) -> DiGraph<String, EdgeMeta> {
    let mut graph = DiGraph::<String, EdgeMeta>::new();
    let mut index = HashMap::new();

    for entity in &diagram.entities {
        let node = graph.add_node(entity.id.as_str().to_string());
        index.insert(entity.id.as_str().to_string(), node);
    }

    for relation in &diagram.relations {
        if relation.arrow == ArrowType::Passive {
            continue;
        }
        if let (Some(from), Some(to)) = (index.get(relation.from.as_str()), index.get(relation.to.as_str())) {
            graph.add_edge(*from, *to, EdgeMeta { reversible: true });
        }
    }

    // Phase 2: inject diagram.constraints as irreversible edges.
    for c in &diagram.constraints {
        if let (Some(from), Some(to)) = (index.get(c.from.as_str()), index.get(c.to.as_str())) {
            graph.add_edge(*from, *to, EdgeMeta { reversible: false });
        }
    }

    graph
}

/// 贪心 FAS 去环，返回需要反转的边集合。
///
/// **不可逆边保护**：构建 FAS 邻接表时排除 `reversible: false` 的边（DSL constrain），
/// 仅将真实边作为反转候选。不可逆边不参与 FAS 计算，因此永远不会被反转。
///
/// 若真实边自身含环，FAS 仅反转真实边破环，constrain 方向保持不变。
pub(super) fn greedy_cycle_reversal(graph: &DiGraph<String, EdgeMeta>) -> HashSet<(NodeIndex, NodeIndex)> {
    // L10：按节点 id 排序，避免 petgraph 插入序影响 FAS 结果。
    let mut nodes = graph.node_indices().collect::<Vec<_>>();
    nodes.sort_by(|a, b| graph[*a].cmp(&graph[*b]));
    let mut out_neighbors: HashMap<NodeIndex, Vec<NodeIndex>> = HashMap::new();
    let mut in_neighbors: HashMap<NodeIndex, Vec<NodeIndex>> = HashMap::new();

    for node in &nodes {
        let mut outs = graph
            .edges_directed(*node, Direction::Outgoing)
            .filter(|e| e.weight().reversible)
            .map(|e| e.target())
            .collect::<Vec<_>>();
        outs.sort_by(|a, b| graph[*a].cmp(&graph[*b]));
        let mut ins = graph
            .edges_directed(*node, Direction::Incoming)
            .filter(|e| e.weight().reversible)
            .map(|e| e.source())
            .collect::<Vec<_>>();
        ins.sort_by(|a, b| graph[*a].cmp(&graph[*b]));
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
