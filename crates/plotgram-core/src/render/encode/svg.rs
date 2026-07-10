//! Plotgram SVG 渲染器
//!
//! 将 ExportScene 编码为 SVG 输出。
//! 布局与样式物化由 scene 中间层负责;SVG 几何由 render/paint 消费 scene 绘制。

use crate::error::Result;
use crate::render::paint::scene_svg;
use crate::render::{FormatEncoder, RenderFormat, RenderOutput, RenderRequest};
use crate::render::scene::{compute_layout, build_scene, ExportScene};

/// SVG 渲染器
pub struct SvgRenderer;

impl FormatEncoder for SvgRenderer {
    fn format(&self) -> RenderFormat {
        RenderFormat::Svg
    }

    fn name(&self) -> &str {
        "svg"
    }

    fn description(&self) -> &str {
        "Scalable Vector Graphics (SVG) - 矢量图形格式，适合网页嵌入和打印"
    }

    fn encode_scene(&self, scene: &ExportScene<'_>) -> Result<RenderOutput> {
        Ok(RenderOutput::Text(encode_scene_inner(scene)))
    }

    fn file_extension(&self) -> &str {
        "svg"
    }
}

/// 便捷入口:从 RenderRequest 走完整流水线(布局 → 物化 → 编码)产出 SVG。
pub fn encode(request: &RenderRequest<'_>) -> Result<String> {
    let layout = compute_layout(request.diagram)?;
    let scene = build_scene(request, layout)?;
    Ok(encode_scene_inner(&scene))
}

pub fn encode_scene_inner(scene: &ExportScene<'_>) -> String {
    scene_svg::encode(scene)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::*;
    use crate::prepare::StyleRequest;
    use crate::pipeline::{parse, prepare};
    use crate::render::RenderRequest;
    use crate::render::paint::svg_utils;
    use crate::types::DiagramType;

    fn create_simple_diagram() -> PreparedDiagram {
        let span = Span::dummy();
        PreparedDiagram::new(Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: vec![DiagramAttribute {
                key: "title".to_string(),
                value: AttributeValue::String(TextValue::quoted("Test Diagram".to_string())),
                span,
            }],
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
                    label: "Process".to_string(),
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
            constraints: vec![],
            style_decls: vec![],
            source_info: SourceInfo {
                file: None,
                line_count: 1,
            },
            ..Default::default()
        })
    }

    #[test]
    fn test_svg_renderer_name() {
        assert_eq!(SvgRenderer.name(), "svg");
    }

    #[test]
    fn test_svg_renderer_extension() {
        assert_eq!(SvgRenderer.file_extension(), "svg");
    }

    #[test]
    fn test_svg_renderer_render_without_title_by_default() {
        let diagram = create_simple_diagram();
        let svg = encode(&RenderRequest::new(&diagram, RenderFormat::Svg)).unwrap();
        assert!(
            !svg.contains("Test Diagram"),
            "title should not be drawn unless show_title is enabled"
        );
    }

    #[test]
    fn test_svg_renderer_render() {
        let diagram = create_simple_diagram();
        let mut request = RenderRequest::new(&diagram, RenderFormat::Svg);
        request.show_title = true;
        let svg = encode(&request).unwrap();

        assert!(svg.starts_with("<svg"));
        assert!(svg.contains("xmlns"));
        assert!(svg.contains("</svg>"));
        assert!(svg.contains("Test Diagram"));
        assert!(svg.contains("<text"), "SVG should contain text elements");
        assert!(svg.contains("rect"), "SVG should contain rect elements for nodes");
        #[cfg(feature = "svg-debug")]
        {
            assert!(svg.contains(r#"data-dfy-debug="1""#));
            assert!(svg.contains(r#"data-dfy-kind="node" data-dfy-id="a""#));
            assert!(svg.contains(r#"data-dfy-kind="node" data-dfy-id="b""#));
            assert!(
                svg.contains(
                    r#"data-dfy-kind="edge" data-dfy-index="0" data-dfy-from="a" data-dfy-to="b" data-dfy-arrow="active""#
                )
            );
        }
    }

    #[test]
    fn test_svg_escape_xml() {
        assert_eq!(svg_utils::escape_xml("<test>"), "&lt;test&gt;");
        assert_eq!(svg_utils::escape_xml("a & b"), "a &amp; b");
        assert_eq!(svg_utils::escape_xml("\"quoted\""), "&quot;quoted&quot;");
    }

    #[test]
    fn test_svg_renderer_applies_resolved_style_context() {
        let diagram = create_simple_diagram();
        let request = RenderRequest {
            diagram: &diagram,
            format: RenderFormat::Svg,
            explicit_theme_id: None,
            explicit_style_json: Some(
                r##"{
                    "version":"0.2",
                    "id":"custom.svg-test",
                    "name":"SVG Test",
                    "tokens":{
                        "colors":{
                            "canvas":"#101820",
                            "text":"#F2AA4C"
                        }
                    },
                    "defaults":{
                        "canvas":{"background":"#101820"},
                        "title":{"fill":"#F2AA4C"},
                        "node":{"fill":"#243447","stroke":"#67B7D1"},
                        "edge":{"stroke":"#67B7D1"},
                        "group":{"stroke":"#89A6FB"}
                    },
                    "diagrams":{}
                }"##,
            ),
            scene_theme_id: None,
            explicit_graphic_style: None,
            scene_graphic_style: None,
            dark_mode: false,
            attribution: true,
            show_title: false,
            transparent_background: false,
            semantic_inference: true,
            ascii_options: crate::render::encode::ascii::AsciiExportOptions::default(),
        };

        let svg = encode(&request).unwrap();
        // canvas background 由 theme defaults.canvas 控制
        assert!(svg.contains("fill=\"#101820\""));
        // per-node / per-edge 样式现在来自 attributes.style（theme 物化）
    }

    #[test]
    fn test_svg_renderer_applies_excalidraw_graphic_style() {
        let diagram = create_simple_diagram();
        let request = RenderRequest {
            diagram: &diagram,
            format: RenderFormat::Svg,
            explicit_theme_id: None,
            explicit_style_json: None,
            scene_theme_id: None,
            explicit_graphic_style: Some(crate::types::GraphicStyleId::Excalidraw),
            scene_graphic_style: None,
            dark_mode: false,
            attribution: true,
            show_title: false,
            transparent_background: false,
            semantic_inference: true,
            ascii_options: crate::render::encode::ascii::AsciiExportOptions::default(),
        };

        let svg = encode(&request).unwrap();
        assert!(svg.contains("data-graphic-style=\"excalidraw\""));
        assert!(svg.contains(" C "));
        assert!(svg.contains("marker-end=\"url(#arrow-active)\""));
    }

    #[test]
    fn test_svg_renderer_applies_spatial_clarity_graphic_style() {
        let diagram = create_simple_diagram();
        let request = RenderRequest {
            diagram: &diagram,
            format: RenderFormat::Svg,
            explicit_theme_id: Some("common.clean-light"),
            explicit_style_json: None,
            scene_theme_id: None,
            explicit_graphic_style: Some(crate::types::GraphicStyleId::SpatialClarity),
            scene_graphic_style: None,
            dark_mode: false,
            attribution: true,
            show_title: false,
            transparent_background: false,
            semantic_inference: true,
            ascii_options: crate::render::encode::ascii::AsciiExportOptions::default(),
        };

        let svg = encode(&request).unwrap();
        assert!(svg.contains("id=\"sc-shadow\""));
        assert!(svg.contains("data-graphic-style=\"spatial-clarity\""));
        assert!(svg.contains("fill-opacity=\"0.96\""));
        assert!(svg.contains("marker-end=\"url(#arrow-active)\""));
    }

    #[test]
    fn svg_includes_attribution_by_default() {
        let diagram = create_simple_diagram();
        let svg = encode(&RenderRequest::new(&diagram, RenderFormat::Svg)).unwrap();

        assert!(svg.contains("class=\"plotgram-attribution\""));
        assert!(svg.contains("powered by plotgram"));
        assert!(svg.contains("href=\"https://plotgram.studio\""));
    }

    #[test]
    fn svg_attribution_can_be_disabled() {
        let diagram = create_simple_diagram();
        let mut request = RenderRequest::new(&diagram, RenderFormat::Svg);
        request.attribution = false;

        let svg = encode(&request).unwrap();
        assert!(!svg.contains("plotgram-attribution"));
        assert!(!svg.contains("powered by plotgram"));
    }

    #[test]
    fn svg_transparent_canvas_omits_background_rect_but_keeps_attribution() {
        let diagram = create_simple_diagram();
        let request = RenderRequest {
            diagram: &diagram,
            format: RenderFormat::Svg,
            explicit_theme_id: None,
            explicit_style_json: Some(
                r##"{
                    "version":"0.2",
                    "id":"custom.transparent",
                    "name":"Transparent",
                    "tokens":{"colors":{"canvas":"#00000000","text":"#333333"}},
                    "defaults":{
                        "canvas":{"background":"#00000000"},
                        "title":{"fill":"#333333"},
                        "node":{"fill":"#eeeeee","stroke":"#999999"},
                        "edge":{"stroke":"#999999"},
                        "group":{"fill":"#f5f5f5","stroke":"#cccccc"}
                    },
                    "diagrams":{}
                }"##,
            ),
            scene_theme_id: None,
            explicit_graphic_style: None,
            scene_graphic_style: None,
            dark_mode: false,
            attribution: true,
            show_title: false,
            transparent_background: false,
            semantic_inference: true,
            ascii_options: crate::render::encode::ascii::AsciiExportOptions::default(),
        };

        let svg = encode(&request).unwrap();
        assert!(!svg.contains(r##"fill="transparent""##));
        assert!(svg.contains("class=\"plotgram-attribution\""));
        assert!(svg.contains("fill=\"#000000\""));
        assert!(svg.contains("fill=\"#ffffff\""));
    }

    #[test]
    fn svg_transparent_background_forces_opaque_theme_to_omit_canvas_rect() {
        let diagram = create_simple_diagram();
        let request = RenderRequest {
            diagram: &diagram,
            format: RenderFormat::Svg,
            explicit_theme_id: None,
            explicit_style_json: Some(
                r##"{
                    "version":"0.2",
                    "id":"custom.opaque",
                    "name":"Opaque",
                    "tokens":{"colors":{"canvas":"#101820","text":"#F2AA4C"}},
                    "defaults":{
                        "canvas":{"background":"#101820"},
                        "title":{"fill":"#F2AA4C"},
                        "node":{"fill":"#243447","stroke":"#67B7D1"},
                        "edge":{"stroke":"#67B7D1"},
                        "group":{"stroke":"#89A6FB"}
                    },
                    "diagrams":{}
                }"##,
            ),
            scene_theme_id: None,
            explicit_graphic_style: None,
            scene_graphic_style: None,
            dark_mode: false,
            attribution: true,
            show_title: false,
            transparent_background: true,
            semantic_inference: true,
            ascii_options: crate::render::encode::ascii::AsciiExportOptions::default(),
        };

        let scene = crate::render::scene::export_scene(&request).expect("export scene");
        assert_eq!(scene.canvas.background, "transparent");

        let svg = encode(&request).unwrap();
        assert!(
            !svg.contains("fill=\"#101820\""),
            "forced transparent background should omit canvas rect fill"
        );
        assert!(svg.contains("class=\"plotgram-attribution\""));
    }

    #[test]
    fn er_user_post_svg_keeps_table_nodes_in_viewbox() {
        let source = include_str!("../../../../../showcase/er/s.user-post.pgm");
        let raw = parse(source).expect("parse user-post");
        let output = prepare(raw, &StyleRequest::default()).expect("prepare user-post");
        let prepared = &output.diagram;
        let mut request = RenderRequest::new(&prepared, RenderFormat::Svg);
        request.attribution = false;
        let svg = encode(&request).unwrap();

        assert!(svg.contains("PK id"));
        assert!(svg.contains("FK user_id"));
        assert!(
            !svg.contains("rect x=\"-"),
            "table rect should not clip outside viewbox"
        );
    }

    // ─── 平行边渲染 ───────────────────────────────────────────────

    /// 创建 4 条平行边 a→b。
    fn create_parallel_edges_diagram() -> PreparedDiagram {
        let span = Span::dummy();
        PreparedDiagram::new(Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: vec![
                DiagramAttribute {
                    key: "direction".to_string(),
                    value: AttributeValue::String(TextValue::quoted("left-to-right".to_string())),
                    span,
                },
                DiagramAttribute {
                    key: "edge_routing".to_string(),
                    value: AttributeValue::String(TextValue::quoted("orthogonal".to_string())),
                    span,
                },
            ],
            entities: vec![
                Entity {
                    id: Identifier::new_unchecked("a"),
                    label: "a".to_string(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span,
                },
                Entity {
                    id: Identifier::new_unchecked("b"),
                    label: "b".to_string(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span,
                },
            ],
            relations: (0..4).map(|_| Relation {
                from: Identifier::new_unchecked("a"),
                to: Identifier::new_unchecked("b"),
                arrow: ArrowType::Active,
                label: None,
                head_label: None,
                tail_label: None,
                attributes: AttributeMap::default(),
                span,
            }).collect(),
            groups: vec![],
            constraints: vec![],
            style_decls: vec![],
            source_info: SourceInfo {
                file: None,
                line_count: 1,
            },
            ..Default::default()
        })
    }

    #[test]
    fn svg_parallel_edges_render_paths() {
        let diagram = create_parallel_edges_diagram();
        let request = RenderRequest::new(&diagram, RenderFormat::Svg);
        let svg = encode(&request).expect("encode parallel-edge diagram");

        assert!(svg.contains("<path"), "parallel edges should render path elements");
        assert!(
            !svg.contains("stroke-opacity=\"0.35\""),
            "edges should not use legacy bundle stroke-opacity"
        );
    }

    #[test]
    fn svg_simple_edges_have_no_bundle_stroke_opacity() {
        let diagram = create_simple_diagram();
        let request = RenderRequest::new(&diagram, RenderFormat::Svg);
        let svg = encode(&request).expect("encode simple diagram");

        assert!(
            !svg.contains("stroke-opacity=\"0.35\""),
            "non-bundled edges should not have bundle stroke-opacity"
        );
    }
}
