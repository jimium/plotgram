//! ER 图布局（`layout_algo: er`）。
//!
//! 图类型门面算法：底层共享 [`super::sugiyama_v2`] 引擎与
//! [`preset::ER_PRESET`](super::sugiyama_v2::preset::ER_PRESET)，
//! ER 图专属微调（实体尺寸估算、边距对齐等）在此覆写。

use crate::ast::Diagram;
use crate::layout::algorithm_config::SugiyamaLayoutConfig;
use crate::layout::intent::topology::ValidTopologyIntent;
use crate::layout::node::sugiyama_v2::{engine, preset};
use crate::layout::plan::ResolvedAlgoOptions;
use crate::layout::{AlgorithmOptionSpec, LayoutResult, LayoutStrategy};
use crate::types::DiagramType;

/// ER 图布局（`layout_algo: er`）。
pub struct ErLayout {
    config: SugiyamaLayoutConfig,
}

impl ErLayout {
    pub fn new(config: SugiyamaLayoutConfig) -> Self {
        Self { config }
    }

    pub fn from_options(options: &ResolvedAlgoOptions) -> Self {
        Self::new(SugiyamaLayoutConfig::from_options(options))
    }
}

impl Default for ErLayout {
    fn default() -> Self {
        Self::new(SugiyamaLayoutConfig::default())
    }
}

impl LayoutStrategy for ErLayout {
    fn name(&self) -> &'static str {
        "er"
    }

    fn applicable_diagram_types(&self) -> &'static [DiagramType] {
        &[DiagramType::Er]
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
        engine::compute_with_preset(diagram, &preset::ER_PRESET, self.config)
    }

    fn compute_with_overlay(
        &self,
        diagram: &Diagram,
        valid_topology: Option<&[ValidTopologyIntent]>,
    ) -> LayoutResult {
        engine::compute_with_preset_and_overlay(
            diagram,
            &preset::ER_PRESET,
            self.config,
            valid_topology,
        )
    }
}
