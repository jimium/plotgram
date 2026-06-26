//! draw.io 节点图标导出（Architecture 等，spec §6.4）。

use crate::icons::{render_entity_content, resolve};
use crate::render::paint::color_queries::{entity_label_font_size, entity_text_fill};
use crate::render::paint::svg_utils::{label_weight, FONT_SIZE};
use crate::render::scene::{ExportNode, ExportScene};

use super::report::{DegradeTier, DrawioExportOptions, ExportReport, ExportWarning};

/// 节点图标导出计划。
pub(crate) struct NodeIconPlan {
    pub(crate) tier: DegradeTier,
    /// L1：替换形状样式为 `shape=image;image=data:...`
    pub(crate) image_style: Option<String>,
    /// 图标已嵌入 SVG 时，标签已在 image 内，mxCell value 留空
    pub(crate) label_in_image: bool,
}

pub(crate) fn plan_node_icon(
    node: &ExportNode<'_>,
    scene: &ExportScene<'_>,
    options: &DrawioExportOptions,
    report: &mut ExportReport,
    entity_id: &str,
) -> NodeIconPlan {
    let has_icon = resolve(
        node.entity,
        node.style.shape.clone(),
        &scene.context.icon_resolve,
    )
    .is_some();

    if !has_icon {
        return NodeIconPlan {
            tier: DegradeTier::L0,
            image_style: None,
            label_in_image: false,
        };
    }

    if !options.embed_icons {
        report.warnings.push(ExportWarning {
            code: "ICON_EMBED_DISABLED".to_string(),
            entity_id: Some(entity_id.to_string()),
            edge_index: None,
            tier: DegradeTier::L2,
            message: format!("节点 '{}' 图标未嵌入（embed_icons=false），仅保留标签", entity_id),
        });
        return NodeIconPlan {
            tier: DegradeTier::L2,
            image_style: None,
            label_in_image: false,
        };
    }

    match build_icon_image_style(node, scene) {
        Some(image_style) => NodeIconPlan {
            tier: DegradeTier::L1,
            image_style: Some(image_style),
            label_in_image: true,
        },
        None => {
            report.warnings.push(ExportWarning {
                code: "ICON_EMBED_FAILED".to_string(),
                entity_id: Some(entity_id.to_string()),
                edge_index: None,
                tier: DegradeTier::L2,
                message: format!(
                    "节点 '{}' 图标 SVG 嵌入失败，仅保留标签",
                    entity_id
                ),
            });
            NodeIconPlan {
                tier: DegradeTier::L2,
                image_style: None,
                label_in_image: false,
            }
        }
    }
}

fn build_icon_image_style(node: &ExportNode<'_>, scene: &ExportScene<'_>) -> Option<String> {
    let w = node.layout.width;
    let h = node.layout.height;
    if w <= 0.0 || h <= 0.0 {
        return None;
    }

    let diagram_type = &scene.diagram().diagram_type;
    let text_color = entity_text_fill(node.entity, diagram_type, &scene.context, "#333333");
    let font_size = entity_label_font_size(node.entity, diagram_type, &scene.context, FONT_SIZE);
    let font_weight = label_weight(&node.style, "500");

    let inner = render_entity_content(
        node.entity,
        0.0,
        0.0,
        w,
        h,
        node.style.shape.clone(),
        &text_color,
        font_size,
        font_weight,
        &scene.context.icon_resolve,
    );
    if inner.trim().is_empty() {
        return None;
    }

    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">{inner}</svg>"#
    );
    let data_uri = encode_svg_data_uri(&svg);
    Some(format!(
        "shape=image;image={data_uri};aspect=fixed;verticalAlign=middle;verticalLabelPosition=middle"
    ))
}

/// draw.io data URI：对 SVG 做 URL 百分号编码（encodeURIComponent 语义子集）。
fn encode_svg_data_uri(svg: &str) -> String {
    let mut out = String::from("data:image/svg+xml,");
    for byte in svg.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            b' ' => out.push_str("%20"),
            b'"' => out.push_str("%22"),
            b'#' => out.push_str("%23"),
            b'%' => out.push_str("%25"),
            b'&' => out.push_str("%26"),
            b'+' => out.push_str("%2B"),
            b',' => out.push_str("%2C"),
            b'/' => out.push_str("%2F"),
            b':' => out.push_str("%3A"),
            b';' => out.push_str("%3B"),
            b'<' => out.push_str("%3C"),
            b'=' => out.push_str("%3D"),
            b'>' => out.push_str("%3E"),
            b'?' => out.push_str("%3F"),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_svg_data_uri_escapes_hash_and_spaces() {
        let svg = "<svg fill=\"#fff\"> </svg>";
        let uri = encode_svg_data_uri(svg);
        assert!(uri.starts_with("data:image/svg+xml,"));
        assert!(uri.contains("%23"));
        assert!(uri.contains("%20"));
        assert!(!uri.contains('#'));
    }
}
