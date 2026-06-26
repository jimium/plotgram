//! 颜色查询工具（从 theme 上下文提取颜色值）。

use crate::ast::*;
use crate::types::DiagramType;
use crate::render::CompiledRenderContext;

/// SVG 署名区域的配色（pill 底 + 文字）。
pub struct AttributionStyle {
    pub pill_fill: String,
    pub pill_fill_opacity: f64,
    pub pill_stroke: String,
    pub pill_stroke_opacity: f64,
    pub text_fill: String,
    pub text_fill_opacity: f64,
}

/// 判断 canvas 背景是否为透明（不绘制全画布 rect）。
pub fn is_transparent_canvas(background: &str) -> bool {
    let trimmed = background.trim();
    let lower = trimmed.to_ascii_lowercase();
    if lower == "transparent" || lower == "none" {
        return true;
    }
    if let Some(hex) = trimmed.strip_prefix('#') {
        return match hex.len() {
            8 => hex[6..8].eq_ignore_ascii_case("00"),
            4 => hex.chars().nth(3).is_some_and(|c| c == '0'),
            _ => false,
        };
    }
    if let Some(inner) = lower
        .strip_prefix("rgba(")
        .and_then(|s| s.strip_suffix(')'))
    {
        let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
        if parts.len() == 4 {
            if let Ok(alpha) = parts[3].parse::<f64>() {
                return alpha <= 0.0;
            }
        }
    }
    false
}

/// 计算 sRGB 十六进制颜色的相对亮度（WCAG）。
pub fn relative_luminance(hex: &str) -> Option<f64> {
    let hex = hex.strip_prefix('#')?;
    let (r, g, b) = match hex.len() {
        6 | 8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            (r, g, b)
        }
        3 | 4 => {
            let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
            (r, g, b)
        }
        _ => return None,
    };

    fn linearize(channel: u8) -> f64 {
        let c = channel as f64 / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }

    let r = linearize(r);
    let g = linearize(g);
    let b = linearize(b);
    Some(0.2126 * r + 0.7152 * g + 0.0722 * b)
}

/// 根据 canvas 背景选择署名样式：不透明走主题自适应，透明走通用高对比 pill。
pub fn attribution_style(canvas_background: &str, muted_fallback: &str) -> AttributionStyle {
    if is_transparent_canvas(canvas_background) {
        return AttributionStyle {
            pill_fill: "#000000".to_string(),
            pill_fill_opacity: 0.45,
            pill_stroke: "#000000".to_string(),
            pill_stroke_opacity: 0.0,
            text_fill: "#ffffff".to_string(),
            text_fill_opacity: 0.88,
        };
    }

    let light_canvas = relative_luminance(canvas_background).unwrap_or(0.9) > 0.5;
    if light_canvas {
        AttributionStyle {
            pill_fill: canvas_background.to_string(),
            pill_fill_opacity: 0.82,
            pill_stroke: muted_fallback.to_string(),
            pill_stroke_opacity: 0.25,
            text_fill: "#6b7280".to_string(),
            text_fill_opacity: 0.55,
        }
    } else {
        AttributionStyle {
            pill_fill: canvas_background.to_string(),
            pill_fill_opacity: 0.82,
            pill_stroke: muted_fallback.to_string(),
            pill_stroke_opacity: 0.25,
            text_fill: "#9ca3af".to_string(),
            text_fill_opacity: 0.60,
        }
    }
}

pub fn canvas_background(_diagram: &Diagram, context: &CompiledRenderContext) -> String {
    context
        .compiled
        .canvas_block()
        .get("background")
        .and_then(|v| v.as_str())
        .unwrap_or("#fafafa")
        .to_string()
}

pub fn title_color(diagram: &Diagram, context: &CompiledRenderContext) -> String {
    let title = context
        .compiled
        .title_block(diagram.diagram_type.style_key());
    title
        .get("fill")
        .and_then(|v| v.as_str())
        .or_else(|| title.get("color").and_then(|v| v.as_str()))
        .unwrap_or("#333")
        .to_string()
}

/// 节点标签字号：优先读 entity 物化后的 `font_size`，再查 theme cascade。
pub fn entity_label_font_size(
    entity: &Entity,
    diagram_type: &DiagramType,
    context: &CompiledRenderContext,
    fallback: f64,
) -> f64 {
    if let Some(n) = super::style_mapping::style_number(entity, "font_size") {
        return n;
    }
    context
        .compiled
        .node_block(diagram_type.style_key(), None)
        .get("font_size")
        .and_then(|v| v.as_number())
        .unwrap_or(fallback)
}

/// 节点标签文字颜色：优先读 entity 物化后的 `text_fill`，再查 theme cascade。
pub fn entity_text_fill(
    entity: &Entity,
    diagram_type: &DiagramType,
    context: &CompiledRenderContext,
    fallback: &str,
) -> String {
    if let Some(AttributeValue::String(v)) = entity.attributes.style.get("text_fill") {
        return v.to_string();
    }
    node_text_fill_from_context(diagram_type, context, fallback)
}

pub fn primary_text_color(
    diagram_type: &DiagramType,
    context: &CompiledRenderContext,
    fallback: &str,
) -> String {
    node_text_fill_from_context(diagram_type, context, fallback)
}

fn node_text_fill_from_context(
    diagram_type: &DiagramType,
    context: &CompiledRenderContext,
    fallback: &str,
) -> String {
    let node = context
        .compiled
        .node_block(diagram_type.style_key(), None);
    node.get("text_fill")
        .and_then(|v| v.as_str())
        .or_else(|| node.get("label_color").and_then(|v| v.as_str()))
        .unwrap_or(fallback)
        .to_string()
}

pub fn muted_text_color(
    diagram_type: &DiagramType,
    context: &CompiledRenderContext,
    fallback: &str,
) -> String {
    let edge = context
        .compiled
        .edge_block(diagram_type.style_key(), None);
    edge.get("text_fill")
        .and_then(|v| v.as_str())
        .or_else(|| edge.get("label_color").and_then(|v| v.as_str()))
        .unwrap_or(fallback)
        .to_string()
}

pub fn edge_stroke(diagram: &Diagram, context: &CompiledRenderContext, fallback: &str) -> String {
    context
        .compiled
        .edge_block(diagram.diagram_type.style_key(), None)
        .get("stroke")
        .and_then(|v| v.as_str())
        .unwrap_or(fallback)
        .to_string()
}

/// 边描边颜色（从 theme cascade 读取，带回退值）。
pub fn edge_stroke_color(
    diagram_type: &DiagramType,
    context: &CompiledRenderContext,
    fallback: &str,
) -> String {
    context
        .compiled
        .edge_block(diagram_type.style_key(), None)
        .get("stroke")
        .and_then(|v| v.as_str())
        .unwrap_or(fallback)
        .to_string()
}

/// 边标签颜色（从 theme cascade 读取，带回退值）。
pub fn edge_label_color(
    diagram_type: &DiagramType,
    context: &CompiledRenderContext,
    fallback: &str,
) -> String {
    context
        .compiled
        .edge_block(diagram_type.style_key(), None)
        .get("label_color")
        .and_then(|v| v.as_str())
        .unwrap_or(fallback)
        .to_string()
}

/// 分组填充色（从 theme cascade 读取，带回退值）。
pub fn group_fill_color(
    diagram_type: &DiagramType,
    context: &CompiledRenderContext,
    fallback: &str,
) -> String {
    context
        .compiled
        .group_block(diagram_type.style_key())
        .get("fill")
        .and_then(|v| v.as_str())
        .unwrap_or(fallback)
        .to_string()
}

/// 分组描边色（从 theme cascade 读取，带回退值）。
pub fn group_stroke_color(
    diagram_type: &DiagramType,
    context: &CompiledRenderContext,
    fallback: &str,
) -> String {
    context
        .compiled
        .group_block(diagram_type.style_key())
        .get("stroke")
        .and_then(|v| v.as_str())
        .unwrap_or(fallback)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transparent_canvas_detection() {
        assert!(is_transparent_canvas("transparent"));
        assert!(is_transparent_canvas("none"));
        assert!(is_transparent_canvas("#RRGGBB00"));
        assert!(is_transparent_canvas("rgba(0, 0, 0, 0)"));
        assert!(!is_transparent_canvas("#FAFAFA"));
        assert!(!is_transparent_canvas("#101820"));
    }

    #[test]
    fn relative_luminance_orders_light_and_dark() {
        let light = relative_luminance("#FAFAFA").unwrap();
        let dark = relative_luminance("#101820").unwrap();
        assert!(light > 0.5);
        assert!(dark < 0.5);
        assert!(light > dark);
    }

    #[test]
    fn attribution_style_uses_universal_colors_for_transparent_canvas() {
        let style = attribution_style("transparent", "#999");
        assert_eq!(style.text_fill, "#ffffff");
        assert_eq!(style.pill_fill, "#000000");
    }
}
