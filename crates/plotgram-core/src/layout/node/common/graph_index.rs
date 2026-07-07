//! 有向图索引
//!
//! 从 [`Diagram`] 构建邻接表形式的有向图索引，供布局算法快速查询前驱/后继。
//!
//! 当前主要消费者：`architecture_v2`（过滤 `Passive` 边）。
//! `force_directed` 需要无向图语义，暂不复用本结构。
//! `sugiyama_v2` 直接使用 `petgraph::DiGraph`，未引入本层。

use std::collections::HashMap;

use crate::ast::{ArrowType, Diagram};

/// 有向图邻接表索引。
///
/// 字段均为 `pub`，便于布局算法直接读取，避免方法调用的额外开销。
pub struct DirectedGraphIndex {
    /// 节点 ID 列表（按 `Diagram` 声明顺序）
    pub node_ids: Vec<String>,
    /// 前向邻接表：`from -> [to, ...]`
    pub out_edges: HashMap<String, Vec<String>>,
    /// 后向邻接表：`to -> [from, ...]`
    pub in_edges: HashMap<String, Vec<String>>,
}

impl DirectedGraphIndex {
    /// 构建图索引，默认过滤 `Passive` 边（不参与分层/去环）。
    pub fn build(diagram: &Diagram) -> Self {
        Self::build_with(diagram, true)
    }

    /// 构建图索引，可选择是否过滤 `Passive` 边。
    ///
    /// - `filter_passive = true`：跳过 `ArrowType::Passive` 边（架构图分层场景）
    /// - `filter_passive = false`：保留所有边
    pub fn build_with(diagram: &Diagram, filter_passive: bool) -> Self {
        let mut node_ids = Vec::with_capacity(diagram.entities.len());
        let mut node_order = HashMap::new();
        let mut out_edges: HashMap<String, Vec<String>> = HashMap::new();
        let mut in_edges: HashMap<String, Vec<String>> = HashMap::new();

        for entity in &diagram.entities {
            let id = entity.id.as_str().to_string();
            node_ids.push(id.clone());
            node_order.insert(id.clone(), node_ids.len() - 1);
            out_edges.insert(id.clone(), Vec::new());
            in_edges.insert(id.clone(), Vec::new());
        }

        for relation in &diagram.relations {
            if filter_passive && relation.arrow == ArrowType::Passive {
                continue;
            }

            let from = relation.from.as_str().to_string();
            let to = relation.to.as_str().to_string();
            if !node_order.contains_key(&from) || !node_order.contains_key(&to) {
                continue;
            }

            out_edges.entry(from.clone()).or_default().push(to.clone());
            in_edges.entry(to).or_default().push(from);
        }

        Self {
            node_ids,
            out_edges,
            in_edges,
        }
    }
}
