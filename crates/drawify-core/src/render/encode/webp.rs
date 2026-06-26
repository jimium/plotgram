//! WebP 渲染器
//!
//! 将 `ExportScene` 经由共享的 SVG 编码器栅格化为 WebP。

use crate::ast::Diagram;
use crate::error::Result;
use crate::render::encode::rasterize;
use crate::render::encode::svg;
use crate::render::{FormatEncoder, RenderFormat, RenderOutput, RenderRequest};
use crate::render::scene::{build_scene, compute_layout, ExportScene};

pub struct WebpRenderer;

impl FormatEncoder for WebpRenderer {
    fn format(&self) -> RenderFormat {
        RenderFormat::Webp
    }

    fn name(&self) -> &str {
        "webp"
    }

    fn description(&self) -> &str {
        "WebP 压缩图像格式"
    }

    fn encode_scene(&self, scene: &ExportScene<'_>) -> Result<RenderOutput> {
        encode_scene_inner(scene).map(RenderOutput::Binary)
    }

    fn file_extension(&self) -> &str {
        "webp"
    }
}

/// 便捷入口:从 RenderRequest 走完整流水线(布局 → 物化 → 编码)产出 WebP。
pub fn encode(request: &RenderRequest<'_>) -> Result<Vec<u8>> {
    let layout = compute_layout(request.diagram)?;
    let scene = build_scene(request, layout)?;
    encode_scene_inner(&scene)
}

pub fn encode_scene_inner(scene: &ExportScene<'_>) -> Result<Vec<u8>> {
    let svg_output = svg::encode_scene_inner(scene);
    render_svg_to_webp_bytes(&svg_output)
}

fn render_svg_to_webp_bytes(svg_data: &str) -> Result<Vec<u8>> {
    rasterize::render_svg_to_image_bytes(svg_data, image::ImageFormat::WebP, "webp")
}

/// 直接保存为 WebP 文件
pub fn render_svg_to_webp_file(svg_data: &str, output_path: &str, _diagram: &Diagram) {
    let _ = rasterize::render_svg_to_image_file(svg_data, output_path);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        ArrowType, AttributeMap, AttributeValue, Diagram, DiagramAttribute, Entity, Identifier,
        PreparedDiagram, Relation, SourceInfo, Span, TextValue,
    };
    use crate::types::DiagramType;

    fn create_simple_diagram() -> PreparedDiagram {
        let span = Span::dummy();
        PreparedDiagram::new(Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: vec![],
            entities: vec![
                Entity {
                    id: Identifier::new_unchecked("a"),
                    label: "Start".to_string(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span,
                },
                Entity {
                    id: Identifier::new_unchecked("b"),
                    label: "End".to_string(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span,
                },
            ],
            relations: vec![Relation {
                from: Identifier::new_unchecked("a"),
                to: Identifier::new_unchecked("b"),
                arrow: ArrowType::Active,
                label: None,
                head_label: None,
                tail_label: None,
                attributes: AttributeMap::default(),
                span,
            }],
            groups: vec![],
            style_decls: vec![],
            source_info: SourceInfo {
                file: None,
                line_count: 1,
            },
            ..Default::default()
        })
    }

    #[test]
    fn webp_render_request_exports_webp_bytes() {
        let diagram = create_simple_diagram();
        let request = RenderRequest::new(&diagram, RenderFormat::Webp);
        let bytes = encode(&request).expect("webp bytes");

        assert!(bytes.starts_with(b"RIFF"));
        assert!(bytes.windows(4).any(|chunk| chunk == b"WEBP"));
        assert!(bytes.len() > 32);
    }
}
