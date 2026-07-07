//! 图类型门面布局：State 图专属算法，底层共享 circular 引擎。

use crate::ast::Diagram;
use crate::layout::algorithm_config::CircularLayoutConfig;
use crate::layout::plan::ResolvedAlgoOptions;
use crate::layout::{AlgorithmOptionSpec, LayoutResult, LayoutStrategy};
use crate::types::DiagramType;

use super::CircularLayout;

/// State 图布局（`layout_algo: state`）。
///
/// 门面算法：底层共享 [`CircularLayout`] 引擎，State 图专属微调在此覆写。
pub struct StateLayout {
    config: CircularLayoutConfig,
}

impl StateLayout {
    pub fn new(config: CircularLayoutConfig) -> Self {
        Self { config }
    }

    pub fn from_options(options: &ResolvedAlgoOptions) -> Self {
        Self::new(CircularLayoutConfig::from_options(options))
    }
}

impl Default for StateLayout {
    fn default() -> Self {
        Self::new(CircularLayoutConfig::default())
    }
}

impl LayoutStrategy for StateLayout {
    fn name(&self) -> &'static str {
        "state"
    }

    fn applicable_diagram_types(&self) -> &'static [DiagramType] {
        &[DiagramType::State]
    }

    fn option_specs(&self) -> &'static [AlgorithmOptionSpec] {
        crate::layout::algorithm_config::CIRCULAR_LAYOUT_OPTIONS
    }

    fn compute(&self, diagram: &Diagram) -> LayoutResult {
        // 复用 circular 引擎；State 专属微调可在此覆写。
        CircularLayout::new(self.config).compute(diagram)
    }
}
