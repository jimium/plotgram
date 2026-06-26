//! 流程图布局（`layout_algo: flowchart`）。
//!
//! 图类型门面算法：底层共享 [`super::sugiyama_v2`] 引擎与
//! [`preset::FLOWCHART_PRESET`](super::sugiyama_v2::preset::FLOWCHART_PRESET)，
//! 流程图专属微调在此覆写。
//!
//! # 分治布局
//!
//! 含 group 时走分治路径（[`group_divide::divide_flowchart_with_groups`]）：
//! 每个 group 独立调用 Sugiyama 布局，再按拓扑序垂直堆叠合并。
//! 无 group 时走原路径（`engine::compute_with_preset`），不受影响。

pub mod group_divide;

use crate::ast::Diagram;
use crate::layout::algorithm_config::SugiyamaLayoutConfig;
use crate::layout::intent::topology::ValidTopologyIntent;
use crate::layout::node::sugiyama_v2::{engine, preset};
use crate::layout::plan::ResolvedAlgoOptions;
use crate::layout::{AlgorithmOptionSpec, LayoutResult, LayoutStrategy};
use crate::types::DiagramType;

/// 流程图布局（`layout_algo: flowchart`）。
pub struct FlowchartLayout {
    config: SugiyamaLayoutConfig,
}

impl FlowchartLayout {
    pub fn new(config: SugiyamaLayoutConfig) -> Self {
        Self { config }
    }

    pub fn from_options(options: &ResolvedAlgoOptions) -> Self {
        Self::new(SugiyamaLayoutConfig::from_options(options))
    }
}

impl Default for FlowchartLayout {
    fn default() -> Self {
        Self::new(SugiyamaLayoutConfig::default())
    }
}

impl LayoutStrategy for FlowchartLayout {
    fn name(&self) -> &'static str {
        "flowchart"
    }

    fn applicable_diagram_types(&self) -> &'static [DiagramType] {
        &[DiagramType::Flowchart]
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
        if group_divide::should_divide(diagram) {
            return group_divide::divide_flowchart_with_groups(diagram, self.config);
        }
        engine::compute_with_preset(diagram, &preset::FLOWCHART_PRESET, self.config)
    }

    fn compute_with_overlay(
        &self,
        diagram: &Diagram,
        valid_topology: Option<&[ValidTopologyIntent]>,
    ) -> LayoutResult {
        if group_divide::should_divide(diagram) {
            // 分治布局暂不支持拓扑意图叠加，有 group 时走分治路径
            return group_divide::divide_flowchart_with_groups(diagram, self.config);
        }
        engine::compute_with_preset_and_overlay(
            diagram,
            &preset::FLOWCHART_PRESET,
            self.config,
            valid_topology,
        )
    }
}
