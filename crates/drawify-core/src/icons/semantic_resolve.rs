//! semantic → icon 推断（可开关）。

use crate::ast::{AttributeValue, Entity};

use super::catalog::{self, IconDef};
use super::registry;

/// semantic 推断选项。
#[derive(Debug, Clone, Copy)]
pub struct SemanticResolveOptions {
    /// 是否启用 semantic → icon 推断（默认开启）。
    pub inference_enabled: bool,
}

impl Default for SemanticResolveOptions {
    fn default() -> Self {
        Self {
            inference_enabled: true,
        }
    }
}

/// 从 `semantic` 属性推断图标；开关关闭或未设置 semantic 时返回 `None`。
pub fn resolve_icon_from_semantic(
    entity: &Entity,
    options: &SemanticResolveOptions,
) -> Option<&'static IconDef> {
    if !options.inference_enabled {
        return None;
    }
    let semantic = standard_atom(entity, "semantic")?;
    let icon_id = registry::semantic_to_icon_id(semantic)?;
    catalog::icon_by_id(icon_id)
}

fn standard_atom<'a>(entity: &'a Entity, key: &str) -> Option<&'a str> {
    let value = entity.attributes.standard.get(key)?;
    match value {
        AttributeValue::String(s) => Some(s.as_str()),
        _ => None,
    }
}
