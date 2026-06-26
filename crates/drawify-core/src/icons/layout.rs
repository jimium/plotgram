//! 布局阶段图标尺寸调整。

use crate::ast::Entity;
use crate::layout::constants::DEFAULT_LABEL_FONT_SIZE;

use super::render::extra_node_width;
use super::resolve::{node_shape_from_entity, resolve, ResolveOptions};

/// 在基础节点尺寸上叠加图标占位（`icon_size + gap`，并满足 catalog `min_node_*`）。
pub fn apply_icon_to_node_size(
    entity: &Entity,
    width: f64,
    height: f64,
    options: &ResolveOptions,
) -> (f64, f64) {
    let shape = node_shape_from_entity(entity);
    let Some(def) = resolve(entity, shape, options) else {
        return (width, height);
    };

    let extra = extra_node_width(def, DEFAULT_LABEL_FONT_SIZE);
    if extra <= 0.0 {
        return (width, height);
    }

    let width = (width + extra).max(def.min_node_width);
    let height = height.max(def.min_node_height);
    (width, height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{AttributeMap, AttributeValue, Entity, Identifier, Span, Position, TextValue};

    fn span() -> Span {
        Span::new(Position::new(1, 1), Position::new(1, 1))
    }

    fn entity_with(attrs: &[(&str, &str)], label: &str) -> Entity {
        let mut standard = AttributeMap::default().standard;
        for (k, v) in attrs {
            standard.insert(k.to_string(), AttributeValue::String(TextValue::unquoted(v.to_string())));
        }
        Entity {
            id: Identifier::new("db").unwrap(),
            label: label.to_string(),
            attributes: AttributeMap {
                standard,
                ..Default::default()
            },
            group_id: None,
            span: span(),
        }
    }

    #[test]
    fn semantic_expands_node_width() {
        let entity = entity_with(&[("semantic", "mysql")], "Orders DB");
        let (w, _) = apply_icon_to_node_size(&entity, 80.0, 40.0, &ResolveOptions::default());
        assert!(w > 80.0);
    }

    #[test]
    fn icon_none_does_not_expand() {
        let entity = entity_with(&[("semantic", "mysql"), ("icon", "none")], "Orders DB");
        let (w, h) = apply_icon_to_node_size(&entity, 80.0, 40.0, &ResolveOptions::default());
        assert_eq!((w, h), (80.0, 40.0));
    }
}
