//! draw.io 样式映射：形状、颜色、箭头、id 清洗。

use tautcore_model::graph::Arrow;
use tautcore_model::NodeShape;

/// `NodeShape` → draw.io style 片段（全部为 L0 等价形状）。
pub(crate) fn shape_to_drawio_style(shape: NodeShape) -> String {
    match shape {
        NodeShape::Rect => "shape=rectangle".to_string(),
        NodeShape::RoundedRect => "rounded=1".to_string(),
        NodeShape::Circle => "ellipse;aspect=fixed".to_string(),
        NodeShape::Diamond => "shape=rhombus".to_string(),
        NodeShape::Stadium => "rounded=1;arcSize=50".to_string(),
        NodeShape::Cylinder => "shape=cylinder3;boundedLbl=1".to_string(),
        NodeShape::Hexagon => "shape=hexagon".to_string(),
        NodeShape::Parallelogram => "shape=parallelogram".to_string(),
        NodeShape::Document => "shape=document".to_string(),
        NodeShape::Cloud => "shape=cloud".to_string(),
        NodeShape::Subprocess => "shape=process".to_string(),
        NodeShape::Person => "shape=umlActor".to_string(),
    }
}

/// Arrow 语义 → drawio 箭头 style 片段。
///
/// Response（`-->`）的虚线由 resolve 层的 `stroke_dasharray` 单独驱动，
/// 此处不重复写入 `dashed=1`。
pub(crate) fn arrow_style_parts(arrow: Arrow) -> &'static [&'static str] {
    match arrow {
        Arrow::Forward => &["endArrow=block"],
        Arrow::Response => &["endArrow=block"],
        Arrow::Bidirectional => &["endArrow=block", "startArrow=block"],
    }
}

/// 将主题 / 内联颜色字符串转为 draw.io style 可用格式。
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
    if color.starts_with('#') || color.starts_with("rgb") {
        return color.to_string();
    }
    let hex = color.trim_start_matches('#');
    if !hex.is_empty() && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return format!("#{hex}");
    }
    color.to_string()
}

/// id 中的非法字符替换为 `_`，保证 mxCell id 合法。
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

/// strokeWidth 文本（≈1 时不写入，返回 `None`）。
pub(crate) fn fmt_stroke_width(width: f64) -> Option<i32> {
    if (width - 1.0).abs() <= 0.01 {
        return None;
    }
    Some(width.round() as i32)
}

/// 透明度（0..1）→ drawio 百分比整数。
pub(crate) fn fmt_opacity(op: f64) -> i32 {
    (op.clamp(0.0, 1.0) * 100.0).round() as i32
}

/// 字重 ≥ 600 视为加粗（drawio fontStyle=1）。
pub(crate) fn is_bold(font_weight: Option<&String>) -> bool {
    font_weight
        .and_then(|w| w.parse::<u32>().ok())
        .map(|w| w >= 600)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_shape_ids_and_widths() {
        // (input, expected)
        let color_cases = [
            ("#C8E6C9", "#C8E6C9"),
            ("C8E6C9", "#C8E6C9"),
            ("none", "none"),
            ("DEFAULT", "default"),
            ("", "default"),
            ("rgb(1,2,3)", "rgb(1,2,3)"),
        ];
        for (input, expected) in color_cases {
            assert_eq!(to_drawio_color(input), expected, "input={input:?}");
        }

        assert_eq!(sanitize_id("a.b c"), "a_b_c");
        assert_eq!(sanitize_id("ok-id_1"), "ok-id_1");

        assert_eq!(fmt_stroke_width(1.0), None);
        assert_eq!(fmt_stroke_width(0.995), None);
        assert_eq!(fmt_stroke_width(2.4), Some(2));

        assert_eq!(fmt_opacity(0.4), 40);
        assert!(is_bold(Some(&"700".to_string())));
        assert!(!is_bold(Some(&"400".to_string())));
        assert!(!is_bold(None));
    }

    #[test]
    fn every_shape_maps_to_a_style() {
        for &shape in NodeShape::ALL {
            let s = shape_to_drawio_style(shape);
            assert!(!s.is_empty(), "{shape} must map to a style fragment");
        }
    }
}
