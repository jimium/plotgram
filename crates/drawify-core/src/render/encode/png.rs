//! PNG 渲染器
//!
//! 将 `ExportScene` 经由共享的 SVG 编码器栅格化为 PNG。

use crate::ast::Diagram;
use crate::error::Result;
use crate::render::encode::rasterize;
use crate::render::encode::svg;
use crate::render::{FormatEncoder, RenderFormat, RenderOutput, RenderRequest};
use crate::render::scene::{build_scene, compute_layout, ExportScene};

pub struct PngRenderer;

impl FormatEncoder for PngRenderer {
    fn format(&self) -> RenderFormat {
        RenderFormat::Png
    }

    fn name(&self) -> &str {
        "png"
    }

    fn description(&self) -> &str {
        "PNG 位图格式"
    }

    fn encode_scene(&self, scene: &ExportScene<'_>) -> Result<RenderOutput> {
        encode_scene_inner(scene).map(RenderOutput::Binary)
    }

    fn file_extension(&self) -> &str {
        "png"
    }
}

/// 便捷入口:从 RenderRequest 走完整流水线(布局 → 物化 → 编码)产出 PNG。
pub fn encode(request: &RenderRequest<'_>) -> Result<Vec<u8>> {
    let layout = compute_layout(request.diagram)?;
    let scene = build_scene(request, layout)?;
    encode_scene_inner(&scene)
}

pub fn encode_scene_inner(scene: &ExportScene<'_>) -> Result<Vec<u8>> {
    let svg_output = svg::encode_scene_inner(scene);
    render_svg_to_png_bytes(&svg_output)
}

fn render_svg_to_png_bytes(svg_data: &str) -> Result<Vec<u8>> {
    rasterize::render_svg_to_image_bytes(svg_data, image::ImageFormat::Png, "png")
}

/// 直接保存为 PNG 文件
pub fn render_svg_to_png_file(svg_data: &str, output_path: &str, _diagram: &Diagram) {
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
    fn png_render_request_exports_png_bytes() {
        let diagram = create_simple_diagram();
        let request = RenderRequest::new(&diagram, RenderFormat::Png);
        let bytes = encode(&request).expect("png bytes");

        assert!(bytes.starts_with(&[0x89, b'P', b'N', b'G']));
        assert!(bytes.len() > 32);
    }
}
