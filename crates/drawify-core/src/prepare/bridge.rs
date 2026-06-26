//! Bridge utilities: convert between `StyleValue` and `AttributeValue`.

use crate::ast::{AttributeValue, TextValue};
use crate::ast::StyleSource;
use crate::theme::StyleValue;

/// 将 theme 的 `StyleValue` 转换为 AST 的 `AttributeValue`。
///
/// 用于 `materialize_styles()` 将 StyleSheet cascade 结果写入 `attributes.style`。
pub fn style_value_to_attribute(value: &StyleValue) -> AttributeValue {
    match value {
        StyleValue::String(s) => AttributeValue::String(TextValue::quoted(s.clone())),
        StyleValue::Number(n) => AttributeValue::Number(*n),
        StyleValue::Boolean(b) => AttributeValue::Boolean(*b),
        StyleValue::Array(arr) => {
            // 数组转为逗号分隔字符串（如 stroke_dasharray: "5,3"）
            let s = arr
                .iter()
                .map(|n| {
                    if *n == (*n as i64) as f64 {
                        (*n as i64).to_string()
                    } else {
                        format!("{n:.2}")
                    }
                })
                .collect::<Vec<_>>()
                .join(",");
            AttributeValue::String(TextValue::quoted(s))
        }
    }
}

/// 将 AST 的 `AttributeValue` 转换为 theme 的 `StyleValue`。
///
/// 用于从 `attributes.style` 读取已物化的值回 theme 体系。
pub fn attribute_to_style_value(value: &AttributeValue) -> Option<StyleValue> {
    match value {
        AttributeValue::String(s) => {
            Some(StyleValue::String(s.to_string()))
        }
        AttributeValue::Number(n) => Some(StyleValue::Number(*n)),
        AttributeValue::Boolean(b) => Some(StyleValue::Boolean(*b)),
        AttributeValue::Config { .. } => None,
    }
}

/// 将 `StyleBlock` 合并到 `attributes.style`，使用 `or_insert` 语义。
///
/// 已存在的键（含 Parser 写入的内联 `style.*`）不会被覆盖。
pub fn merge_style_block_into_attrs(
    target: &mut crate::ast::StyleMap,
    source: &crate::theme::StyleBlock,
    source_kind: StyleSource,
) {
    for (key, value) in source.iter() {
        target.or_insert_with_source(
            key.clone(),
            style_value_to_attribute(value),
            source_kind.clone(),
        );
    }
}
