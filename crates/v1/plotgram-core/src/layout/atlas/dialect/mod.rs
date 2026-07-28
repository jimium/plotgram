//! Dialect / Scheme / Contract（Stage 5–6）。
//!
//! `compile_atlas` 是唯一读 `DiagramType` 的入口；度量相 / Ink 只认
//! [`AtlasContract`] 与 Profile。

mod atlas_contract;
mod contract;
mod circular_dialect;
mod frame_adapt;
mod hierarchical;
mod kind;
mod profile;
mod scheme;
mod sequence;
mod tree;

pub mod contraction;

pub use atlas_contract::AtlasContract;
pub use circular_dialect::CircularDialect;
pub use contract::{ContractOccupant, HierarchicalContract};
pub use frame_adapt::apply_legacy_layout_attrs;
pub use hierarchical::{compile_hierarchical, HierarchicalDialect};
pub use kind::DialectKind;
pub use profile::{
    Density, GroupAlign, GroupPolicy, GroupSizing, HierarchicalPreset, HierarchicalProfile,
};
pub use scheme::{
    Scheme, SchemeId, CIRCULAR_DEFAULT, CIRCULAR_STATE, HIERARCHICAL_ARCH_EQUAL_TRACK_ORTHO,
    HIERARCHICAL_FLOW_ORTHO, HIERARCHICAL_STATE, SEQUENCE_BUILTIN_EDGES, TREE_RADIAL_ORGANIC,
};
pub use sequence::SequenceDialect;
pub use tree::TreeDialect;

use crate::ast::Diagram;
use crate::layout::pipeline::plan::diagram_algorithm_name;
use crate::layout::recipes::state::{fas_reversal_ratio, SUGIYAMA_REVERSAL_THRESHOLD};
use crate::types::standard_attr_keys::diagram as dsl;
use crate::types::DiagramType;

/// 图种语义编译器（Hierarchical 仍实现；其它 Dialect 用关联函数 compile）。
pub trait Dialect {
    fn compile(&self, diagram: &Diagram, scheme: &Scheme) -> HierarchicalContract;
}

/// 统一编译入口：解析 Scheme 并产出 [`AtlasContract`]。
pub fn compile_atlas(diagram: &Diagram) -> (Scheme, AtlasContract) {
    let scheme = resolve_scheme(diagram);
    let contract = match scheme.kind {
        DialectKind::Hierarchical => {
            let hc = HierarchicalDialect.compile(diagram, &scheme);
            AtlasContract::Hierarchical(hc)
        }
        DialectKind::Tree => TreeDialect::compile(&scheme),
        DialectKind::Sequence => SequenceDialect::compile(&scheme),
        DialectKind::Circular => CircularDialect::compile(&scheme),
    };
    (scheme, contract)
}

fn resolve_scheme(diagram: &Diagram) -> Scheme {
    if let Some(id) = diagram
        .attributes
        .iter()
        .find(|a| a.key == "scheme")
        .and_then(|a| a.value.as_str())
    {
        if let Some(mut scheme) = Scheme::lookup(id) {
            if scheme.kind == DialectKind::Hierarchical {
                frame_adapt::apply_legacy_layout_attrs(diagram, &mut scheme.profile);
            }
            return scheme;
        }
    }
    let mut scheme = default_scheme_for(diagram);
    if scheme.kind == DialectKind::Hierarchical {
        frame_adapt::apply_legacy_layout_attrs(diagram, &mut scheme.profile);
    }
    scheme
}

fn default_scheme_for(diagram: &Diagram) -> Scheme {
    match diagram.diagram_type {
        DiagramType::Architecture => Scheme::hierarchical_arch_equal_track_ortho(),
        DiagramType::Flowchart => Scheme::hierarchical_flow_ortho(),
        DiagramType::Mindmap => Scheme::tree_radial_organic(),
        DiagramType::Sequence => Scheme::sequence_builtin_edges(),
        DiagramType::Er => Scheme::circular_default(),
        DiagramType::State => {
            if state_prefers_circular(diagram) {
                Scheme::circular_state()
            } else {
                Scheme::hierarchical_state()
            }
        }
        DiagramType::Custom(_) => Scheme::hierarchical_flow_ortho(),
    }
}

/// 承接原 `StateRecipe` 的 circular 决策（迁入 Dialect 层）。
pub fn state_prefers_circular(diagram: &Diagram) -> bool {
    if diagram_algorithm_name(diagram, dsl::LAYOUT) == Some("circular") {
        return true;
    }
    fas_reversal_ratio(diagram) >= SUGIYAMA_REVERSAL_THRESHOLD
}
