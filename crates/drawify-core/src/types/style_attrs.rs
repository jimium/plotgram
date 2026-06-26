//! 样式属性名与值类型校验（entity/relation 物化 style 与 StyleDecl 共用）。

use std::collections::HashMap;

use crate::ast::{AttributeValue, Span, StyleDeclKind};
use crate::error::DiagnosticError;

use super::style_attr_keys;

pub const VALID_ENTITY_STYLE_ATTRS: &[&str] = &[
    style_attr_keys::FILL,
    style_attr_keys::STROKE,
    style_attr_keys::STROKE_WIDTH,
    style_attr_keys::STROKE_DASHARRAY,
    style_attr_keys::SHAPE,
    style_attr_keys::LABEL_WEIGHT,
    style_attr_keys::WIDTH,
    style_attr_keys::HEIGHT,
    style_attr_keys::TEXT_FILL,
    style_attr_keys::FONT_SIZE,
    style_attr_keys::FONT_WEIGHT,
    style_attr_keys::RADIUS,
    style_attr_keys::TRANSFORM,
];

pub const VALID_RELATION_STYLE_ATTRS: &[&str] = &[
    style_attr_keys::STROKE,
    style_attr_keys::STROKE_WIDTH,
    style_attr_keys::STROKE_DASHARRAY,
    style_attr_keys::DASHED,
    style_attr_keys::LABEL_COLOR,
    style_attr_keys::TEXT_FILL,
    style_attr_keys::FONT_SIZE,
    // 边标签样式
    style_attr_keys::LABEL_BG,
    style_attr_keys::LABEL_BG_OPACITY,
    style_attr_keys::LABEL_BORDER,
    style_attr_keys::LABEL_BORDER_WIDTH,
    style_attr_keys::LABEL_BORDER_RADIUS,
    style_attr_keys::LABEL_PADDING,
    style_attr_keys::LABEL_FONT_SIZE,
    style_attr_keys::LABEL_FONT_WEIGHT,
    style_attr_keys::LABEL_POSITION,
    style_attr_keys::LABEL_ROTATION,
];

pub fn is_string_like(value: &AttributeValue) -> bool {
    matches!(value, AttributeValue::String(_))
}

pub fn is_atom_like(value: &AttributeValue) -> bool {
    value.as_str().is_some()
}

pub fn is_text_like(value: &AttributeValue) -> bool {
    value.as_str().is_some()
}

pub fn is_number_like(value: &AttributeValue) -> bool {
    matches!(value, AttributeValue::Number(_))
}

pub fn is_boolean_like(value: &AttributeValue) -> bool {
    matches!(value, AttributeValue::Boolean(_))
}

/// 样式属性校验错误类型
#[derive(Debug)]
pub enum StylePropError {
    /// 未知样式属性键
    UnknownKey { key: String, allowed: Vec<String> },
    /// 值类型不匹配
    TypeMismatch { key: String, expected: &'static str },
}

/// 校验单条样式键值；返回结构化错误（若有）。
pub fn validate_style_property(
    key: &str,
    value: &AttributeValue,
    allowed_keys: &[&str],
    _context: &str,
) -> Option<StylePropError> {
    if !allowed_keys.contains(&key) {
        return Some(StylePropError::UnknownKey {
            key: key.to_string(),
            allowed: allowed_keys.iter().map(|s| s.to_string()).collect(),
        });
    }

    let expected: Option<&'static str> = match key {
        "fill" | "stroke" | "stroke_dasharray" | "text_fill" | "transform"
        | "label_color" | "label_bg" | "label_border" => {
            if !is_string_like(value) {
                Some("String")
            } else {
                None
            }
        }
        "stroke_width" | "width" | "height" | "font_size" | "radius"
        | "label_bg_opacity" | "label_border_width" | "label_border_radius"
        | "label_padding" | "label_font_size" => {
            if !is_number_like(value) {
                Some("Number")
            } else {
                None
            }
        }
        "shape" | "label_weight" | "font_weight" | "label_font_weight" | "label_position"
        | "label_rotation" => {
            if !is_atom_like(value) && !is_string_like(value) && !is_number_like(value) {
                Some("atom、字符串或数值")
            } else {
                None
            }
        }
        "dashed" => {
            if !is_boolean_like(value) {
                Some("Boolean")
            } else {
                None
            }
        }
        _ => None,
    };

    expected.map(|exp| StylePropError::TypeMismatch {
        key: key.to_string(),
        expected: exp,
    })
}

/// 校验样式属性 map；将错误追加到 `errors`。
///
/// - 未知键 → E004 InvalidAttribute
/// - 类型不匹配 → E016 StyleTypeMismatch
pub fn collect_style_map_errors(
    style: &HashMap<String, AttributeValue>,
    allowed_keys: &[&str],
    context: &str,
    span: Span,
    errors: &mut Vec<DiagnosticError>,
) {
    for (key, value) in style {
        if let Some(err) = validate_style_property(key, value, allowed_keys, context) {
            match err {
                StylePropError::UnknownKey { key, allowed } => {
                    errors.push(DiagnosticError::invalid_attribute(
                        span,
                        &key,
                        context,
                        &allowed.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                    ));
                }
                StylePropError::TypeMismatch { key, expected } => {
                    let actual = match value {
                        AttributeValue::String(_) => "String",
                        AttributeValue::Number(_) => "Number",
                        AttributeValue::Boolean(_) => "Boolean",
                        _ => "other",
                    };
                    errors.push(DiagnosticError::style_type_mismatch(
                        span, &key, expected, actual,
                    ));
                }
            }
        }
    }
}

/// 根据 StyleDecl 种类返回允许的样式键列表。
pub fn allowed_keys_for_decl(kind: StyleDeclKind) -> &'static [&'static str] {
    match kind {
        StyleDeclKind::Node => VALID_ENTITY_STYLE_ATTRS,
        StyleDeclKind::Edge => VALID_RELATION_STYLE_ATTRS,
    }
}
