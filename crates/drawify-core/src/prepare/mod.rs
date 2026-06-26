//! Prepare：将 RawDiagram 规范化为下游可消费的 PreparedDiagram。
//!
//! 当前阶段实现：
//! - `apply_profile_defaults`（补全缺失的 `entity.type`）
//! - `materialize_diagram_styles`（将 CompiledTheme 物化到 `attributes.style`，定义在 `theme` 模块）
//! - `resolve_compiled_theme`（从 diagram `theme` / request 解析出 `CompiledTheme`）
//!
//! ## `layout_plan` 生命周期
//!
//! `PreparedDiagram::new(diagram)` 在构造末尾调用 `LayoutPlan::resolve`，将
//! `layout_algo` / `edge_routing` 及其 option 解析结果挂在 `layout_plan()` 上。
//!
//! | 阶段 | 行为 |
//! |------|------|
//! | **创建** | `prepare()` → `PreparedDiagram::new` 解析 plan |
//! | **读取** | `validate` / `render` 通过 `layout_plan()` 复用，避免重复解析 AST |
//! | **失效** | 修改 diagram 的 `layout_algo`、`edge_routing` 或其配置块后 plan 过时 |
//! | **刷新** | `refresh_layout_plan()`，或 `PreparedDiagram::new(inner().clone())`（patch 路径） |
//! | **序列化** | JSON 仅含 diagram；反序列化时自动重新 `resolve` |

pub mod bridge;
pub mod profile;
pub mod structure;
pub mod styles;
pub mod style_resolve;

pub use bridge::{attribute_to_style_value, merge_style_block_into_attrs, style_value_to_attribute};
pub use profile::apply_profile_defaults;
pub use structure::expand_structure;
pub use styles::{
    apply_style_decls, validate_style_decls,
};
pub use style_resolve::{resolve_compiled_theme, theme_from_diagram_attrs, StyleRequest};

use crate::ast::PreparedDiagram;
use crate::profile::profile_for;
use crate::types::style_attr_keys;
use crate::types::standard_attr_keys::entity;

/// 检查 PreparedDiagram 不变量（I1–I3，见 pipeline-spec §4.4）。
pub fn assert_prepared_invariants(diagram: &PreparedDiagram) {
    let diagram = diagram.inner();
    assert!(
        diagram.style_decls.is_empty(),
        "I1: style_decls must be empty after prepare"
    );

    let profile = profile_for(&diagram.diagram_type);
    if profile.default_entity_type.is_some() {
        for entity in &diagram.entities {
            assert!(
                entity.attributes.standard.contains_key(entity::TYPE),
                "I2: entity '{}' should have standard[\"{}\"] when profile defines default_entity_type",
                entity.id,
                entity::TYPE
            );
        }
    }

    for entity in &diagram.entities {
        assert!(
            entity.attributes.style.contains_key(style_attr_keys::FILL)
                || entity.attributes.style.contains_key(style_attr_keys::STROKE),
            "I3: entity '{}' style must contain fill or stroke",
            entity.id
        );
    }
    for relation in &diagram.relations {
        assert!(
            relation.attributes.style.contains_key(style_attr_keys::FILL)
                || relation.attributes.style.contains_key(style_attr_keys::STROKE),
            "I3: relation {}->{} style must contain fill or stroke",
            relation.from,
            relation.to
        );
    }
}

#[cfg(debug_assertions)]
pub fn debug_assert_prepared_invariants(diagram: &PreparedDiagram) {
    assert_prepared_invariants(diagram);
}

#[cfg(test)]
mod tests;
