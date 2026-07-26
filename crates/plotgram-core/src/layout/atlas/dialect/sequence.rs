//! SequenceDialect：BuiltinEdges（23 §8.6.2）。

use crate::ast::Diagram;
use crate::layout::pipeline::plan::ResolvedAlgoOptions;
use crate::layout::recipes::sequence::SequenceLayout;
use crate::layout::types::LayoutResult;
use crate::layout::LayoutStrategy;

use super::atlas_contract::AtlasContract;
use super::scheme::Scheme;

pub struct SequenceDialect;

impl SequenceDialect {
    pub fn compile(scheme: &Scheme) -> AtlasContract {
        AtlasContract::Sequence {
            scheme_id: scheme.id,
        }
    }

    /// 布局即边几何；调用方不得再跑 channel Ink。
    pub fn materialize(diagram: &Diagram, options: &ResolvedAlgoOptions) -> LayoutResult {
        SequenceLayout::from_options(options).compute(diagram)
    }
}
