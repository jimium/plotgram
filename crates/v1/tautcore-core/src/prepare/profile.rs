//! Profile defaults: fill in missing `entity.type` from DiagramProfile.

use crate::ast::{AttributeValue, Diagram, TextValue};
use crate::profile::profile_for;
use crate::types::standard_attr_keys::entity;

/// 按 DiagramProfile 补全缺失的 `entity.type` 等标准属性。
///
/// - 仅当 `type` **缺失**时补全；用户显式写的 `type` 永不覆盖
/// - 补全写入 `attributes.standard`（语义属性），不是 `attributes.style`（视觉属性）
/// - 必须在 `materialize_styles` 之前执行（palette 查表依赖 `entity.type`）
///
/// 幂等：已补全的图重复调用结果不变。
pub fn apply_profile_defaults(mut diagram: Diagram) -> crate::error::Result<Diagram> {
    let profile = profile_for(&diagram.diagram_type);
    let Some(default_type) = profile.default_entity_type else {
        return Ok(diagram);
    };

    debug_assert!(
        profile.default_entity_type_is_valid(),
        "profile {:?} default_entity_type {:?} must pass supports_entity_type",
        profile.kind,
        default_type
    );

    for entity_node in &mut diagram.entities {
        if !entity_node.attributes.standard.contains_key(entity::TYPE) {
            entity_node.attributes.standard.insert(
                entity::TYPE.to_string(),
                AttributeValue::String(TextValue::unquoted(default_type.to_string())),
            );
        }
    }

    Ok(diagram)
}
