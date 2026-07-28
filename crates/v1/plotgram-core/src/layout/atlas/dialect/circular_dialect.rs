//! CircularDialect：圆排布 + curved 路由（23 §8.6.3）。

use crate::ast::Diagram;
use crate::layout::pipeline::plan::ResolvedAlgoOptions;
use crate::layout::recipes::circular::CircularLayout;
use crate::layout::types::LayoutResult;
use crate::layout::LayoutStrategy;

use super::atlas_contract::AtlasContract;
use super::scheme::Scheme;

pub struct CircularDialect;

impl CircularDialect {
    pub fn compile(scheme: &Scheme) -> AtlasContract {
        AtlasContract::Circular {
            scheme_id: scheme.id,
        }
    }

    pub fn materialize(diagram: &Diagram, options: &ResolvedAlgoOptions) -> LayoutResult {
        CircularLayout::from_options(options).compute(diagram)
    }
}
