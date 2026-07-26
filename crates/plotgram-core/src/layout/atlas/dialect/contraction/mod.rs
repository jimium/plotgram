//! Contraction 递归门面（23 §7.5.2–5.3）。
//!
//! Stage 5：Weak → 原 flowchart 分治；StrongMacro → 原 architecture two_phase。
//! 实现迁入本目录后，recipes 侧不再保留独立分层。

use crate::ast::Diagram;
use crate::layout::algorithm_config::{ArchitectureV2LayoutConfig, SugiyamaLayoutConfig};
use crate::layout::atlas::dialect::profile::{GroupPolicy, HierarchicalProfile};
use crate::layout::types::LayoutResult;
use crate::layout::{GroupTable, NodeLayout};
use std::collections::HashMap;

pub mod strong_macro;
pub mod weak;

/// 有组时是否走收缩（分治 / macro）而非 flat LayeredKernel。
pub fn should_contract(diagram: &Diagram) -> bool {
    weak::should_divide(diagram)
}

/// Weak 组间堆叠：只出节点（Atlas 再 materialize 组框）。
pub fn contract_weak_nodes(
    diagram: &Diagram,
    config: SugiyamaLayoutConfig,
) -> weak::DivideNodesOutput {
    weak::divide_flowchart_nodes(diagram, config)
}

/// 组装 Weak 分治的 LayoutResult 壳（无 edges）。
pub fn assemble_weak_result(
    diagram: &Diagram,
    nodes: HashMap<String, NodeLayout>,
    groups: HashMap<String, crate::layout::GroupLayout>,
    order: &[String],
    mode: weak::ArrangementMode,
    sugiyama_ranks: HashMap<String, usize>,
    canvas_padding: f64,
) -> LayoutResult {
    weak::assemble_divide_result(
        diagram,
        nodes,
        groups,
        order,
        mode,
        sugiyama_ranks,
        canvas_padding,
    )
}

/// StrongMacro：完整节点+组框布局（legacy / Atlas 种子共用）。
pub fn contract_strong_macro(
    diagram: &Diagram,
    config: ArchitectureV2LayoutConfig,
) -> LayoutResult {
    strong_macro::compute_two_phase_layout_entry(diagram, config)
}

/// 按 Profile 选择收缩策略。
pub fn contract_nodes_for_profile(
    diagram: &Diagram,
    profile: &HierarchicalProfile,
    flow_config: SugiyamaLayoutConfig,
    arch_config: ArchitectureV2LayoutConfig,
) -> ContractNodesOutput {
    match profile.group_policy {
        GroupPolicy::Weak => {
            let out = contract_weak_nodes(diagram, flow_config);
            ContractNodesOutput::Weak(out)
        }
        GroupPolicy::StrongMacro => {
            let result = contract_strong_macro(diagram, arch_config);
            ContractNodesOutput::Strong(result)
        }
    }
}

pub enum ContractNodesOutput {
    Weak(weak::DivideNodesOutput),
    Strong(LayoutResult),
}

impl ContractNodesOutput {
    pub fn into_nodes_and_ranks(self) -> (HashMap<String, NodeLayout>, HashMap<String, usize>, f64) {
        match self {
            Self::Weak(o) => (o.nodes, o.sugiyama_ranks, o.canvas_padding),
            Self::Strong(r) => {
                let ranks = r.hints.sugiyama_ranks.clone().unwrap_or_default();
                let pad = 40.0; // architecture 默认 canvas padding 近似
                (r.nodes, ranks, pad)
            }
        }
    }
}

/// 组表类型别名（materialize 后）。
pub type GroupMap = GroupTable;
