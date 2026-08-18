//! TreeDialect：mindmap = Tree + radial/organic Profile（23 §8.6.1）。

use crate::ast::Diagram;
use crate::layout::algorithm_config::MindmapLayoutConfig;
use crate::layout::pipeline::plan::ResolvedAlgoOptions;
use crate::layout::recipes::mindmap::MindmapLayout;
use crate::layout::types::LayoutResult;
use crate::layout::LayoutStrategy;

use super::atlas_contract::AtlasContract;
use super::scheme::Scheme;

pub struct TreeDialect;

impl TreeDialect {
    pub fn compile(scheme: &Scheme) -> AtlasContract {
        AtlasContract::Tree {
            scheme_id: scheme.id,
        }
    }

    /// 落笔：委托现有 MindmapLayout（零算法重写）。
    pub fn materialize(diagram: &Diagram, options: &ResolvedAlgoOptions) -> LayoutResult {
        MindmapLayout::from_options(options).compute(diagram)
    }

    pub fn materialize_default(diagram: &Diagram) -> LayoutResult {
        MindmapLayout::new(MindmapLayoutConfig::default()).compute(diagram)
    }
}
