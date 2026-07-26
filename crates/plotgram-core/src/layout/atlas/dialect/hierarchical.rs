//! HierarchicalDialect：flowchart / architecture / hierarchical-state 共用分层方言。
//!
//! Scheme 解析与 [`super::compile_atlas`] 对齐（Post-S7 Wave0）；本入口仅产出
//! [`HierarchicalContract`]——若 `compile_atlas` 选择非 Hierarchical Dialect
//!（如 circular-state），回落为 `hierarchical_state` 供度量相测试入口。

use crate::ast::Diagram;

use super::atlas_contract::AtlasContract;
use super::contract::HierarchicalContract;
use super::scheme::Scheme;
use super::{compile_atlas, Dialect};

/// flowchart + architecture → HierarchicalContract。
pub struct HierarchicalDialect;

impl HierarchicalDialect {
    /// 与生产 [`compile_atlas`] 同源的 Scheme 解析。
    pub fn resolve_scheme(diagram: &Diagram) -> Scheme {
        let (scheme, contract) = compile_atlas(diagram);
        match contract {
            AtlasContract::Hierarchical(_) => scheme,
            _ => Scheme::hierarchical_state(),
        }
    }
}

impl Dialect for HierarchicalDialect {
    fn compile(&self, diagram: &Diagram, scheme: &Scheme) -> HierarchicalContract {
        let _ = diagram;
        HierarchicalContract::from_scheme(scheme)
    }
}

/// 编译入口：委托 `compile_atlas`，保证与生产 Scheme 一致。
pub fn compile_hierarchical(diagram: &Diagram) -> HierarchicalContract {
    let (scheme, contract) = compile_atlas(diagram);
    match contract {
        AtlasContract::Hierarchical(hc) => hc,
        _ => {
            // 非 Hierarchical 图种被误调时：钉 hierarchical_state，避免静默用错 Profile
            crate::perf_log!(
                "[atlas] compile_hierarchical: non-hierarchical dialect → hierarchical_state"
            );
            HierarchicalDialect.compile(diagram, &Scheme::hierarchical_state())
        }
    }
}
