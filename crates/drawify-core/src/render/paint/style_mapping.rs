//! AST style → Renderer style 映射工具。

use crate::ast::*;
use crate::render::visual::{EdgeStyle, LabelAnchor, LabelRotation, NodeStyle, NodeShape};
use crate::types::style_attr_keys;

/// 将 entity 的 `attributes.style` 映射为 `NodeStyle`。
pub fn node_style_from_attributes(entity: &Entity) -> NodeStyle {
    let mut style = NodeStyle::default();
    let s = &entity.attributes.style;

    if let Some(AttributeValue::String(v)) = s.get(style_attr_keys::FILL) {
        style.fill = v.to_string();
    }
    if let Some(AttributeValue::String(v)) = s.get(style_attr_keys::STROKE) {
        style.stroke = v.to_string();
    }
    if let Some(AttributeValue::Number(v)) = s.get(style_attr_keys::STROKE_WIDTH) {
        style.stroke_width = *v;
    }
    if let Some(AttributeValue::String(v)) = s.get(style_attr_keys::STROKE_DASHARRAY) {
        style.stroke_dasharray = Some(v.to_string());
    }
    if let Some(shape_value) = s.get(style_attr_keys::SHAPE) {
        let shape_name = match shape_value {
            AttributeValue::String(v) => v.as_str(),
            _ => "",
        };
        if !shape_name.is_empty() {
            style.shape = parse_node_shape(shape_name);
        }
    }
    if let Some(v) = s.get(style_attr_keys::LABEL_WEIGHT) {
        if let Some(w) = attribute_as_string(v) {
            style.label_weight = Some(w);
        }
    }
    if let Some(v) = s.get(style_attr_keys::TRANSFORM) {
        if let Some(t) = attribute_as_string(v) {
            style.transform = Some(t);
        }
    }
    style.radius = style_number(entity, style_attr_keys::RADIUS);

    style
}

/// 将 relation 的 `attributes.style` 映射为 `EdgeStyle`（含 EdgeLabelStyle）。
pub fn edge_style_from_attributes(relation: &Relation) -> EdgeStyle {
    let mut style = EdgeStyle::default();
    let s = &relation.attributes.style;

    if let Some(AttributeValue::String(v)) = s.get(style_attr_keys::STROKE) {
        style.stroke = v.to_string();
    }
    if let Some(AttributeValue::Number(v)) = s.get(style_attr_keys::STROKE_WIDTH) {
        style.stroke_width = *v;
    }
    if let Some(AttributeValue::String(v)) = s.get(style_attr_keys::STROKE_DASHARRAY) {
        style.stroke_dasharray = Some(v.to_string());
    }
    if let Some(AttributeValue::Boolean(v)) = s.get(style_attr_keys::DASHED) {
        style.dashed = *v;
    }

    // ── 边标签样式 ──
    let ls = &mut style.label_style;

    // 文字颜色：label_color 优先，回退 text_fill
    if let Some(AttributeValue::String(v)) = s.get(style_attr_keys::LABEL_COLOR) {
        ls.text_color = v.to_string();
    } else if let Some(AttributeValue::String(v)) = s.get(style_attr_keys::TEXT_FILL) {
        ls.text_color = v.to_string();
    }

    // 字号：label_font_size 优先，回退 font_size
    if let Some(n) = style_number_from(s, style_attr_keys::LABEL_FONT_SIZE) {
        ls.font_size = n;
    } else if let Some(n) = style_number_from(s, style_attr_keys::FONT_SIZE) {
        ls.font_size = n;
    }

    // 字重
    if let Some(v) = s.get(style_attr_keys::LABEL_FONT_WEIGHT).and_then(attribute_as_string) {
        ls.font_weight = Some(v);
    }

    // 背景色（"none" = 透明）
    if let Some(AttributeValue::String(v)) = s.get(style_attr_keys::LABEL_BG) {
        ls.bg_color = if v.eq_ignore_ascii_case("none") {
            None
        } else {
            Some(v.to_string())
        };
    }

    // 背景不透明度
    if let Some(n) = style_number_from(s, style_attr_keys::LABEL_BG_OPACITY) {
        ls.bg_opacity = n.clamp(0.0, 1.0);
    }

    // 边框色（"none" = 无边框）
    if let Some(AttributeValue::String(v)) = s.get(style_attr_keys::LABEL_BORDER) {
        ls.border_color = if v.eq_ignore_ascii_case("none") {
            None
        } else {
            Some(v.to_string())
        };
    }

    // 边框宽度
    if let Some(n) = style_number_from(s, style_attr_keys::LABEL_BORDER_WIDTH) {
        ls.border_width = n;
    }

    // 圆角
    if let Some(n) = style_number_from(s, style_attr_keys::LABEL_BORDER_RADIUS) {
        ls.border_radius = n;
    }

    // 内边距（统一应用到水平与垂直）
    if let Some(n) = style_number_from(s, style_attr_keys::LABEL_PADDING) {
        ls.padding = (n, n);
    }

    // 位置锚点
    if let Some(v) = s.get(style_attr_keys::LABEL_POSITION).and_then(attribute_as_string) {
        ls.anchor = parse_label_anchor(&v);
    }

    // 旋转
    if let Some(v) = s.get(style_attr_keys::LABEL_ROTATION) {
        ls.rotation = parse_label_rotation(v);
    }

    style
}

/// 解析 label_rotation 属性值为 LabelRotation
fn parse_label_rotation(v: &AttributeValue) -> LabelRotation {
    match v {
        AttributeValue::Number(n) => LabelRotation::Fixed(*n),
        AttributeValue::String(s) => {
            let s = s.trim();
            if s.eq_ignore_ascii_case("none") {
                LabelRotation::None
            } else if s.eq_ignore_ascii_case("along_edge") || s.eq_ignore_ascii_case("along") {
                LabelRotation::AlongEdge
            } else if let Ok(n) = s.parse::<f64>() {
                LabelRotation::Fixed(n)
            } else {
                LabelRotation::None
            }
        }
        _ => LabelRotation::None,
    }
}

/// 解析 label_position 字符串为 LabelAnchor
fn parse_label_anchor(s: &str) -> LabelAnchor {
    let s = s.trim();
    if s.eq_ignore_ascii_case("middle") {
        LabelAnchor::Middle
    } else if s.eq_ignore_ascii_case("start") {
        LabelAnchor::Start
    } else if s.eq_ignore_ascii_case("end") {
        LabelAnchor::End
    } else if let Some(rest) = s.strip_prefix("t:") {
        let t = rest.trim().parse::<f64>().unwrap_or(0.5).clamp(0.0, 1.0);
        LabelAnchor::AtPath(t)
    } else {
        LabelAnchor::Middle
    }
}

/// 从 style map 读取数值（支持 Number 或 String 解析）。
fn style_number_from(s: &crate::ast::StyleMap, key: &str) -> Option<f64> {
    match s.get(key)? {
        AttributeValue::Number(n) => Some(*n),
        AttributeValue::String(s) => s.parse().ok(),
        _ => None,
    }
}

/// 从 `attributes.style` 读取数值属性（支持 Number 或 String）。
pub fn style_number(entity: &Entity, key: &str) -> Option<f64> {
    match entity.attributes.style.get(key)? {
        AttributeValue::Number(n) => Some(*n),
        AttributeValue::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn attribute_as_string(value: &AttributeValue) -> Option<String> {
    match value {
        AttributeValue::String(s) => Some(s.to_string()),
        _ => None,
    }
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

pub fn edge_paint_attrs(style: &EdgeStyle, dash_pattern: Option<&str>) -> String {
    let mut attrs = Vec::new();
    let final_dash = style
        .stroke_dasharray
        .as_deref()
        .filter(|value| !value.is_empty())
        .or_else(|| dash_pattern.filter(|value| !value.is_empty()));
    if let Some(dash) = final_dash {
        attrs.push(format!(r#"stroke-dasharray="{dash}""#));
    }
    if let Some(linecap) = &style.stroke_linecap {
        attrs.push(format!(r#"stroke-linecap="{linecap}""#));
    }
    if let Some(linejoin) = &style.stroke_linejoin {
        attrs.push(format!(r#"stroke-linejoin="{linejoin}""#));
    }
    attrs.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_label_rotation_none_string() {
        assert_eq!(
            parse_label_rotation(&AttributeValue::String(TextValue::quoted("none"))),
            LabelRotation::None
        );
    }

    #[test]
    fn parse_label_rotation_along_edge() {
        assert_eq!(
            parse_label_rotation(&AttributeValue::String(TextValue::quoted("along_edge"))),
            LabelRotation::AlongEdge
        );
        assert_eq!(
            parse_label_rotation(&AttributeValue::String(TextValue::unquoted("along"))),
            LabelRotation::AlongEdge
        );
        assert_eq!(
            parse_label_rotation(&AttributeValue::String(TextValue::quoted("ALONG_EDGE"))),
            LabelRotation::AlongEdge
        );
    }

    #[test]
    fn parse_label_rotation_fixed_number() {
        assert_eq!(
            parse_label_rotation(&AttributeValue::Number(45.0)),
            LabelRotation::Fixed(45.0)
        );
    }

    #[test]
    fn parse_label_rotation_fixed_numeric_string() {
        assert_eq!(
            parse_label_rotation(&AttributeValue::String(TextValue::quoted("-30"))),
            LabelRotation::Fixed(-30.0)
        );
    }

    #[test]
    fn parse_label_rotation_invalid_falls_back_to_none() {
        assert_eq!(
            parse_label_rotation(&AttributeValue::String(TextValue::quoted("garbage"))),
            LabelRotation::None
        );
    }

    #[test]
    fn edge_style_from_attributes_reads_label_rotation() {
        let mut rel = crate::ast::Relation {
            from: crate::ast::Identifier::new_unchecked("a"),
            to: crate::ast::Identifier::new_unchecked("b"),
            arrow: crate::ast::ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: crate::ast::AttributeMap::default(),
            span: crate::ast::Span::dummy(),
        };
        rel.attributes.style.insert(
            "label_rotation".to_string(),
            AttributeValue::String(TextValue::quoted("along_edge")),
        );
        let style = edge_style_from_attributes(&rel);
        assert_eq!(style.label_style.rotation, LabelRotation::AlongEdge);
    }
}
