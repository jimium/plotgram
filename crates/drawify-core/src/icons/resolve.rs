//! 从 AST 实体解析应使用的图标。

use crate::ast::{AttributeValue, Entity};
use crate::render::visual::NodeShape;

use super::catalog::{self, IconDef, IconPlacement};
use super::semantic_resolve::{resolve_icon_from_semantic, SemanticResolveOptions};

/// 图标解析选项。
#[derive(Debug, Clone, Copy)]
pub struct ResolveOptions {
    /// 是否启用 semantic → icon 推断（默认开启）。
    pub semantic_inference: bool,
}

impl Default for ResolveOptions {
    fn default() -> Self {
        Self {
            semantic_inference: true,
        }
    }
}

impl ResolveOptions {
    fn semantic_options(&self) -> SemanticResolveOptions {
        SemanticResolveOptions {
            inference_enabled: self.semantic_inference,
        }
    }
}

/// 按优先级从实体解析图标。
///
/// 1. `icon: none` → 不渲染
/// 2. `icon: <id>` → 显式图标
/// 3. semantic 推断（开关开启时）
///
/// 形状不兼容或 `placement: None` 时返回 `None`（静默）。
pub fn resolve(
    entity: &Entity,
    node_shape: NodeShape,
    options: &ResolveOptions,
) -> Option<&'static IconDef> {
    let candidate = resolve_candidate(entity, options)?;
    if !is_compatible(candidate, node_shape) {
        return None;
    }
    Some(candidate)
}

fn resolve_candidate(entity: &Entity, options: &ResolveOptions) -> Option<&'static IconDef> {
    match explicit_icon(entity) {
        ExplicitIcon::None => None,
        ExplicitIcon::Some(icon) => Some(icon),
        ExplicitIcon::Unset => resolve_icon_from_semantic(entity, &options.semantic_options()),
    }
}

enum ExplicitIcon {
    Unset,
    None,
    Some(&'static IconDef),
}

fn explicit_icon(entity: &Entity) -> ExplicitIcon {
    let Some(value) = entity.attributes.standard.get("icon") else {
        return ExplicitIcon::Unset;
    };
    let key = match value {
        AttributeValue::String(s) => {
            catalog::normalize_key(s)
        }
        _ => return ExplicitIcon::Unset,
    };
    if key == "none" {
        return ExplicitIcon::None;
    }
    match catalog::icon_by_key(&key) {
        Some(icon) => ExplicitIcon::Some(icon),
        None => ExplicitIcon::Unset,
    }
}

fn is_compatible(icon: &IconDef, node_shape: NodeShape) -> bool {
    if icon.placement == IconPlacement::None {
        return false;
    }
    !icon.incompatible_shapes.contains(&node_shape)
}

/// 从 entity 的 `attributes.style.shape` 读取节点形状；缺省为矩形。
pub fn node_shape_from_entity(entity: &Entity) -> NodeShape {
    let Some(shape_value) = entity.attributes.style.get("shape") else {
        return NodeShape::Rect;
    };
    let shape_name = match shape_value {
        AttributeValue::String(v) => v.as_str(),
        _ => return NodeShape::Rect,
    };
    parse_node_shape(shape_name)
}

fn parse_node_shape(s: &str) -> NodeShape {
    match s {
        "rect" | "rectangle" => NodeShape::Rect,
        "rounded_rect" | "rounded-rect" => NodeShape::RoundedRect,
        "circle" => NodeShape::Circle,
        "diamond" => NodeShape::Diamond,
        "cylinder" => NodeShape::Cylinder,
        "hexagon" => NodeShape::Hexagon,
        "person" | "actor" => NodeShape::Person,
        "stadium" => NodeShape::Stadium,
        "parallelogram" => NodeShape::Parallelogram,
        "document" => NodeShape::Document,
        "cloud" => NodeShape::Cloud,
        "subprocess" => NodeShape::Subprocess,
        _ => NodeShape::Rect,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{AttributeMap, Entity, Identifier, Span, Position, TextValue};

    fn span() -> Span {
        Span::new(Position::new(1, 1), Position::new(1, 1))
    }

    fn entity_with(attrs: &[(&str, &str)]) -> Entity {
        let mut standard = AttributeMap::default().standard;
        for (k, v) in attrs {
            standard.insert(k.to_string(), AttributeValue::String(TextValue::unquoted(v.to_string())));
        }
        Entity {
            id: Identifier::new("kafka").unwrap(),
            label: "Kafka".to_string(),
            attributes: AttributeMap {
                standard,
                ..Default::default()
            },
            group_id: None,
            span: span(),
        }
    }

    #[test]
    fn icon_none_suppresses_semantic_inference() {
        let entity = entity_with(&[("semantic", "mysql"), ("icon", "none")]);
        assert!(resolve(&entity, NodeShape::RoundedRect, &ResolveOptions::default()).is_none());
    }

    #[test]
    fn explicit_icon_overrides_semantic() {
        let entity = entity_with(&[("semantic", "mysql"), ("icon", "redis")]);
        let icon = resolve(&entity, NodeShape::RoundedRect, &ResolveOptions::default()).unwrap();
        assert_eq!(icon.id, "redis");
    }

    #[test]
    fn semantic_inference_resolves_icon() {
        let entity = entity_with(&[("semantic", "mysql")]);
        let icon = resolve(&entity, NodeShape::RoundedRect, &ResolveOptions::default()).unwrap();
        assert_eq!(icon.id, "mysql");
    }

    #[test]
    fn semantic_inference_disabled() {
        let entity = entity_with(&[("semantic", "mysql")]);
        let options = ResolveOptions {
            semantic_inference: false,
        };
        assert!(resolve(&entity, NodeShape::RoundedRect, &options).is_none());
    }

    #[test]
    fn incompatible_shape_returns_none() {
        let entity = entity_with(&[("semantic", "mysql")]);
        assert!(resolve(&entity, NodeShape::Cylinder, &ResolveOptions::default()).is_none());
    }

    #[test]
    fn entity_id_does_not_auto_match_icon() {
        let entity = entity_with(&[]);
        assert!(resolve(&entity, NodeShape::RoundedRect, &ResolveOptions::default()).is_none());
    }
}
