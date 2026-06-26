//! draw.io 样式映射：形状、颜色、箭头、XML 转义。

use crate::ast::ArrowType;
use crate::render::visual::NodeShape;
use crate::types::GraphicStyleId;

use super::report::DegradeTier;

/// 将 `NodeShape` 映射为 draw.io style 字符串片段，返回 (style_part, degrade_tier)。
pub(crate) fn shape_to_drawio_style(shape: &NodeShape) -> (String, DegradeTier) {
    match shape {
        NodeShape::Rect => ("shape=rectangle".to_string(), DegradeTier::L0),
        NodeShape::RoundedRect => ("rounded=1".to_string(), DegradeTier::L0),
        NodeShape::Circle => ("ellipse;aspect=fixed".to_string(), DegradeTier::L0),
        NodeShape::Diamond => ("shape=rhombus".to_string(), DegradeTier::L0),
        NodeShape::Stadium => ("rounded=1;arcSize=50".to_string(), DegradeTier::L0),
        NodeShape::Cylinder => ("shape=cylinder3;boundedLbl=1".to_string(), DegradeTier::L0),
        NodeShape::Hexagon => ("shape=hexagon".to_string(), DegradeTier::L0),
        NodeShape::Parallelogram => ("shape=parallelogram".to_string(), DegradeTier::L0),
        NodeShape::Document => ("shape=document".to_string(), DegradeTier::L0),
        NodeShape::Cloud => ("shape=cloud".to_string(), DegradeTier::L0),
        NodeShape::Subprocess => ("shape=process".to_string(), DegradeTier::L1),
        NodeShape::Person => ("shape=umlActor".to_string(), DegradeTier::L1),
    }
}

/// 根据 `relation.arrow` 返回 drawio 边的箭头样式片段（spec §7.3）。
pub(crate) fn arrow_style_parts(arrow: &ArrowType) -> &'static [&'static str] {
    match arrow {
        ArrowType::Active => &["endArrow=block"],
        ArrowType::Passive => &["endArrow=block", "dashed=1"],
        ArrowType::Bidirectional => &["endArrow=block", "startArrow=block"],
    }
}

/// 将 Drawify/SVG 颜色字符串转为 draw.io style 可用格式。
pub(crate) fn to_drawio_color(color: &str) -> String {
    let color = color.trim();
    if color.is_empty() {
        return "default".to_string();
    }
    if color.eq_ignore_ascii_case("none")
        || color.eq_ignore_ascii_case("default")
        || color.eq_ignore_ascii_case("transparent")
    {
        return color.to_ascii_lowercase();
    }
    if color.starts_with('#') {
        return color.to_string();
    }
    if color.starts_with("rgb") {
        return color.to_string();
    }
    let hex = color.trim_start_matches('#');
    if !hex.is_empty() && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return format!("#{}", hex);
    }
    color.to_string()
}

/// XML 特殊字符转义。
pub(crate) fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// 根据背景色的相对亮度，返回对比色（深色背景→白色字，浅色背景→深色字）。
pub(crate) fn contrast_font_color(hex: &str) -> String {
    let hex = hex.trim();
    if hex.len() < 6 {
        return "333333".to_string();
    }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255) as f64;
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(255) as f64;
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255) as f64;
    let luminance = (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255.0;
    if luminance > 0.5 {
        "333333".to_string()
    } else {
        "FFFFFF".to_string()
    }
}

pub(crate) fn is_sketch_graphic_style(style: &GraphicStyleId) -> bool {
    matches!(style, GraphicStyleId::Excalidraw | GraphicStyleId::CrossHatch)
}

/// 将 entity id 中的非法字符替换为 `_`，保证 mxCell id 合法。
pub(crate) fn sanitize_id(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// 导出 strokeWidth（默认 1 时不写入）。
pub(crate) fn fmt_stroke_width(width: f64) -> Option<i32> {
    if (width - 1.0).abs() <= 0.01 {
        return None;
    }
    Some(width.round() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::visual::NodeShape;

    #[test]
    fn test_to_drawio_color() {
        assert_eq!(to_drawio_color("#C8E6C9"), "#C8E6C9");
        assert_eq!(to_drawio_color("C8E6C9"), "#C8E6C9");
        assert_eq!(to_drawio_color("555555"), "#555555");
        assert_eq!(to_drawio_color("none"), "none");
        assert_eq!(to_drawio_color("DEFAULT"), "default");
        assert_eq!(to_drawio_color(""), "default");
    }

    #[test]
    fn test_xml_escape() {
        assert_eq!(xml_escape("<test>"), "&lt;test&gt;");
        assert_eq!(xml_escape("a & b"), "a &amp; b");
        assert_eq!(xml_escape("\"quoted\""), "&quot;quoted&quot;");
        assert_eq!(xml_escape("it's"), "it&apos;s");
    }

    #[test]
    fn test_sanitize_id() {
        assert_eq!(sanitize_id("hello_world"), "hello_world");
        assert_eq!(sanitize_id("my-node"), "my-node");
        assert_eq!(sanitize_id("特殊字符"), "____");
        assert_eq!(sanitize_id("a.b"), "a_b");
        assert_eq!(sanitize_id("a b"), "a_b");
    }

    #[test]
    fn test_shape_to_drawio_style() {
        assert_eq!(
            shape_to_drawio_style(&NodeShape::Rect).0,
            "shape=rectangle"
        );
        assert_eq!(
            shape_to_drawio_style(&NodeShape::RoundedRect).0,
            "rounded=1"
        );
        assert_eq!(
            shape_to_drawio_style(&NodeShape::Circle).0,
            "ellipse;aspect=fixed"
        );
        assert_eq!(
            shape_to_drawio_style(&NodeShape::Diamond).0,
            "shape=rhombus"
        );
        assert_eq!(
            shape_to_drawio_style(&NodeShape::Cylinder).0,
            "shape=cylinder3;boundedLbl=1"
        );
        assert_eq!(shape_to_drawio_style(&NodeShape::Cloud).0, "shape=cloud");
        assert_eq!(
            shape_to_drawio_style(&NodeShape::Stadium).0,
            "rounded=1;arcSize=50"
        );
        let (style, tier) = shape_to_drawio_style(&NodeShape::Person);
        assert_eq!(style, "shape=umlActor");
        assert_eq!(tier, DegradeTier::L1);
        let (style, tier) = shape_to_drawio_style(&NodeShape::Subprocess);
        assert_eq!(style, "shape=process");
        assert_eq!(tier, DegradeTier::L1);
    }

    #[test]
    fn test_arrow_style_parts_mapping() {
        use crate::ast::ArrowType;

        assert_eq!(
            arrow_style_parts(&ArrowType::Active),
            &["endArrow=block"][..]
        );
        assert_eq!(
            arrow_style_parts(&ArrowType::Passive),
            &["endArrow=block", "dashed=1"][..]
        );
        assert_eq!(
            arrow_style_parts(&ArrowType::Bidirectional),
            &["endArrow=block", "startArrow=block"][..]
        );
    }
}
