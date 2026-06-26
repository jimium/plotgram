//! Scene JSON 导出器
//!
//! 将 `ExportScene` 编码为稳定的 JSON 导出格式,作为 Exporter 层的对外契约。

use crate::ast::SourceInfo;
use crate::error::Result;
use crate::render::{FormatEncoder, RenderFormat, RenderOutput, RenderRequest};
use crate::render::scene::{
    build_scene, compute_layout, ExportCanvas, ExportEdge, ExportGroup, ExportNode, ExportScene,
};
use serde::Serialize;

pub struct JsonRenderer;

impl FormatEncoder for JsonRenderer {
    fn format(&self) -> RenderFormat {
        RenderFormat::Json
    }

    fn name(&self) -> &str {
        "json"
    }

    fn description(&self) -> &str {
        "Scene JSON - Exporter 中间层的结构化导出格式"
    }

    fn encode_scene(&self, scene: &ExportScene<'_>) -> Result<RenderOutput> {
        encode_scene_inner(scene).map(RenderOutput::Text)
    }

    fn file_extension(&self) -> &str {
        "json"
    }
}

#[derive(Serialize)]
struct ExportSceneJson<'a> {
    schema_version: &'static str,
    format: &'static str,
    diagram_type: &'a crate::types::DiagramType,
    theme_id: &'a str,
    theme_name: &'a str,
    graphic_style: &'static str,
    canvas: &'a ExportCanvas,
    nodes: &'a [ExportNode<'a>],
    edges: &'a [ExportEdge<'a>],
    groups: &'a [ExportGroup<'a>],
    source_info: &'a SourceInfo,
}

impl<'a> ExportSceneJson<'a> {
    fn from_scene(scene: &'a ExportScene<'a>) -> Self {
        let diagram = scene.diagram();
        let compiled = &scene.context.compiled;
        Self {
            schema_version: "0.1",
            format: "drawify.export_scene",
            diagram_type: &diagram.diagram_type,
            theme_id: &compiled.id,
            theme_name: &compiled.name,
            graphic_style: scene.context.graphic_style.as_str(),
            canvas: &scene.canvas,
            nodes: &scene.nodes,
            edges: &scene.edges,
            groups: &scene.groups,
            source_info: &diagram.source_info,
        }
    }
}

/// 便捷入口:从 RenderRequest 走完整流水线(布局 → 物化 → 编码)产出 JSON。
pub fn encode(request: &RenderRequest<'_>) -> Result<String> {
    let layout = compute_layout(request.diagram)?;
    let scene = build_scene(request, layout)?;
    encode_scene_inner(&scene)
}

pub fn encode_scene_inner(scene: &ExportScene<'_>) -> Result<String> {
    let export = ExportSceneJson::from_scene(scene);
    Ok(serde_json::to_string_pretty(&export).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        ArrowType, AttributeMap, Diagram, Entity, Identifier,
        PreparedDiagram, Relation, SourceInfo, Span,
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
                label: Some("next".to_string()),
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
    fn json_render_request_exports_scene_json() {
        let diagram = create_simple_diagram();
        let request = RenderRequest::new(&diagram, RenderFormat::Json);
        let json = encode(&request).expect("scene json");

        assert!(json.contains("\"format\": \"drawify.export_scene\""));
        assert!(json.contains("\"diagram_type\": \"flowchart\""));
        assert!(json.contains("\"graphic_style\": \"standard\""));
        assert!(json.contains("\"canvas\""));
        assert!(json.contains("\"nodes\""));
        assert!(json.contains("\"edges\""));
    }
}
