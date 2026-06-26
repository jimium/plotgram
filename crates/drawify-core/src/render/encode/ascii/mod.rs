//! Drawify ASCII export pipeline.
//!
//! This module has two responsibilities:
//! - Render diagrams into plain-text ASCII art.
//! - Normalize arbitrary text/byte inputs into ASCII-safe output streams.
//!
//! ASCII 只消费 `diagram + layout`,不需要 `ExportScene` 的视觉物化(fill/stroke/width)。
//! [`encode_direct`] 为独立路径,由 `pipeline::render::render_output` 在 ASCII 格式时直接调用,
//! 跳过 `build_scene` 的视觉物化步骤。

use crate::ast::Diagram;
use crate::error::Result;
use crate::layout::LayoutResult;
use crate::render::{FormatEncoder, RenderFormat, RenderOutput, RenderRequest};
use crate::render::scene::ExportScene;
use crate::render::encode::{DiagramEncodeOutput, EncodingPath};

mod config;
mod error;
mod canvas;
mod draw;
mod stream;

pub use config::{
    AsciiDetectedEncoding, AsciiExportMetadata, AsciiExportOptions, AsciiExportResult,
    AsciiInputEncodingHint, AsciiInvalidInputPolicy, AsciiNewline, AsciiNonAsciiPolicy,
    AsciiOutputEncoding,
};
pub use error::{AsciiExportError, AsciiExportErrorCode};
pub use stream::{export_bytes, export_reader_to_writer, export_string, sanitize_inline_fragment};

use draw::generate_ascii;

const SCALE_X: f64 = 6.0;
const SCALE_Y: f64 = 12.0;
const PADDING: usize = 2;
const NODE_PAD_H: usize = 2;

pub struct AsciiRenderer;

impl FormatEncoder for AsciiRenderer {
    fn format(&self) -> RenderFormat {
        RenderFormat::Ascii
    }

    fn name(&self) -> &str {
        "ascii"
    }

    fn description(&self) -> &str {
        "ASCII text export for terminals, logs, and plain-text workflows"
    }

    fn encode_scene(&self, scene: &ExportScene<'_>) -> Result<RenderOutput> {
        let result = generate_ascii(scene.diagram(), &scene.layout, &AsciiExportOptions::default())
            .map_err(|err| crate::error::DrawifyError::render_internal_msg(err.to_string()))?;
        Ok(RenderOutput::Text(result.text))
    }

    fn file_extension(&self) -> &str {
        "txt"
    }

    fn encoding_path(&self) -> EncodingPath {
        EncodingPath::Diagram
    }

    fn encode_from_diagram(
        &self,
        diagram: &crate::ast::PreparedDiagram,
        layout_overlay: Option<&crate::layout::LayoutIntentOverlay>,
    ) -> crate::error::Result<DiagramEncodeOutput> {
        let (layout, report) = crate::layout::compute_layout_with_plan_and_overlay(
            diagram.inner(),
            diagram.layout_plan(),
            layout_overlay,
        ).map_err(|e| crate::error::DrawifyError::layout_failed_msg(e.to_string()))?;

        let text = encode_direct(diagram.inner(), &layout, &AsciiExportOptions::default())?;
        Ok(DiagramEncodeOutput {
            output: RenderOutput::Text(text),
            report,
        })
    }
}

/// 便捷入口:从 RenderRequest 走完整流水线产出 ASCII(含 metadata)。
///
/// 注意:ASCII 不需要视觉物化,此函数内部只做布局 + ASCII 编码;
/// `pipeline::render::render_output` 在 ASCII 格式时直接调用 [`encode_direct`] 跳过物化。
pub fn encode_with_report(request: &RenderRequest<'_>) -> Result<AsciiExportResult> {
    let layout = crate::render::scene::compute_layout(request.diagram)?;
    encode_direct_with_report(request.diagram.inner(), &layout, &request.ascii_options)
}

pub fn encode(request: &RenderRequest<'_>) -> Result<String> {
    Ok(encode_with_report(request)?.text)
}

/// 独立路径:直接从 diagram + layout 生成 ASCII(跳过视觉物化)。
///
/// 由 `pipeline::render::render_output` 在 ASCII 格式时调用。
pub fn encode_direct(
    diagram: &Diagram,
    layout: &LayoutResult,
    options: &AsciiExportOptions,
) -> Result<String> {
    encode_direct_with_report(diagram, layout, options).map(|r| r.text)
}

/// 独立路径(含 metadata):直接从 diagram + layout 生成 ASCII,返回完整结果。
///
/// 供需要 metadata 的调用方(如 WASM)使用,避免重复布局计算。
pub fn encode_direct_with_report(
    diagram: &Diagram,
    layout: &LayoutResult,
    options: &AsciiExportOptions,
) -> Result<AsciiExportResult> {
    generate_ascii(diagram, layout, options)
        .map_err(|err| crate::error::DrawifyError::render_internal_msg(err.to_string()))
}

const BOX_H: char = '─';
const BOX_V: char = '│';
const BOX_TL: char = '┌';
const BOX_TR: char = '┐';
const BOX_BL: char = '└';
const BOX_BR: char = '┘';
const ARROW_RIGHT: char = '▶';
const ARROW_DOWN: char = '▼';
const ARROW_LEFT: char = '◀';
const ARROW_UP: char = '▲';
const DASH_H: char = '·';
const DASH_V: char = '¦';
const GROUP_TL: char = '┌';
const GROUP_TR: char = '┐';
const GROUP_BL: char = '└';
const GROUP_BR: char = '┘';

fn calculate_canvas_size(layout: &LayoutResult, has_title: bool) -> (usize, usize) {
    let title_rows = if has_title { 2 } else { 0 };
    let w = (layout.total_width / SCALE_X).ceil() as usize + PADDING * 2 + 4;
    let h = (layout.total_height / SCALE_Y).ceil() as usize + PADDING * 2 + title_rows + 4;
    (w.max(40), h.max(20))
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::canvas::{truncate_string, DisplayCanvas, GridMapper};
    use super::draw::{direction_arrow, draw_box, draw_segment, route_edge_points, RouteBusInfo};
    use crate::ast::{
        ArrowType, AttributeMap, AttributeValue, Diagram, DiagramAttribute, Entity, Identifier,
        Relation, SourceInfo, Span, TextValue,
    };
    use crate::types::DiagramType;
    use crate::layout::{EdgeLayout, PathGeometry, Port};
    use crate::layout::geometry::Point;
    use std::collections::HashMap;

    fn create_simple_diagram() -> Diagram {
        let span = Span::dummy();
        Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: vec![],
            entities: vec![
                Entity {
                    id: Identifier::new_unchecked("start"),
                    label: "开始".to_string(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span,
                },
                Entity {
                    id: Identifier::new_unchecked("process"),
                    label: "处理中".to_string(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span,
                },
                Entity {
                    id: Identifier::new_unchecked("end"),
                    label: "结束".to_string(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span,
                },
            ],
            relations: vec![
                Relation {
                    from: Identifier::new_unchecked("start"),
                    to: Identifier::new_unchecked("process"),
                    arrow: ArrowType::Active,
                    label: None,
                    head_label: None,
                    tail_label: None,
                    attributes: AttributeMap::default(),
                    span,
                },
                Relation {
                    from: Identifier::new_unchecked("process"),
                    to: Identifier::new_unchecked("end"),
                    arrow: ArrowType::Active,
                    label: None,
                    head_label: None,
                    tail_label: None,
                    attributes: AttributeMap::default(),
                    span,
                },
            ],
            groups: vec![],
            style_decls: vec![],
            source_info: SourceInfo {
                file: None,
                line_count: 1,
            },
            ..Default::default()
        }
    }

    #[test]
    fn test_ascii_renderer_name() {
        assert_eq!(AsciiRenderer.name(), "ascii");
    }

    #[test]
    fn test_ascii_renderer_extension() {
        assert_eq!(AsciiRenderer.file_extension(), "txt");
    }

    #[test]
    fn test_ascii_renderer_render() {
        let diagram = create_simple_diagram();
        let prepared = crate::ast::PreparedDiagram::new(diagram);
        let request = RenderRequest::new(&prepared, RenderFormat::Ascii);
        let output = encode(&request).unwrap();

        assert!(!output.is_empty());
        assert!(output.contains('┌') || output.contains('─') || output.contains('│'));
        assert!(output.contains("开始") || output.contains("处理") || output.contains("结束"));
    }

    #[test]
    fn test_truncate_string() {
        assert_eq!(truncate_string("hello", 10), "hello");
        assert_eq!(truncate_string("hello world", 8), "hello w…");
        assert_eq!(truncate_string("你好世界", 5), "你好…");
    }

    #[test]
    fn test_calculate_canvas_size() {
        let layout = LayoutResult {
            nodes: HashMap::new(),
            groups: HashMap::new(),
            edges: vec![],
            total_width: 500.0,
            total_height: 300.0,
            hints: Default::default(),
        };

        let (w, h) = calculate_canvas_size(&layout, true);
        assert!(w >= 40);
        assert!(h >= 20);
    }

    #[test]
    fn test_draw_box() {
        let mut canvas = DisplayCanvas::new(30, 10);
        draw_box(
            &mut canvas, 0, 0, 10, 3, BOX_TL, BOX_TR, BOX_BL, BOX_BR, BOX_V, BOX_H,
        );
        let out = canvas.to_string();
        assert!(out.contains('┌'));
        assert!(out.contains('─'));
        assert!(out.contains('│'));
    }

    #[test]
    fn test_draw_segment_merges_junctions() {
        let mut canvas = DisplayCanvas::new(20, 10);
        draw_segment(&mut canvas, 5, 1, 5, 6, false, &[]);
        draw_segment(&mut canvas, 2, 3, 8, 3, false, &[]);
        let out = canvas.to_string();
        assert!(out.contains('┼'));
    }

    #[test]
    fn test_route_edge_points_uses_shared_source_bus() {
        let mapper = GridMapper::new(false);
        let edge = EdgeLayout {
            geometry: PathGeometry::Straight {
                start: Point::new(60.0, 24.0),
                end: Point::new(12.0, 72.0),
            },
            labels: vec![],
            from_port: Port::Bottom,
            to_port: Port::Top,
        };
        let points = route_edge_points(
            &mapper,
            &edge,
            (12, 4),
            (4, 8),
            Some(RouteBusInfo { trunk_x: 10, count: 2 }),
            None,
        );
        assert_eq!(points, vec![(12, 4), (12, 5), (10, 5), (10, 6), (4, 6), (4, 8)]);
    }

    #[test]
    fn test_route_edge_points_balances_single_vertical_flow() {
        let mapper = GridMapper::new(false);
        let edge = EdgeLayout {
            geometry: PathGeometry::Straight {
                start: Point::new(60.0, 24.0),
                end: Point::new(108.0, 96.0),
            },
            labels: vec![],
            from_port: Port::Bottom,
            to_port: Port::Top,
        };
        let points = route_edge_points(&mapper, &edge, (12, 4), (20, 10), None, None);
        assert_eq!(points, vec![(12, 4), (12, 7), (20, 7), (20, 10)]);
    }

    #[test]
    fn test_direction_arrow() {
        assert_eq!(direction_arrow(0, 0, 0, 5), Some(ARROW_DOWN));
        assert_eq!(direction_arrow(0, 5, 0, 0), Some(ARROW_UP));
        assert_eq!(direction_arrow(0, 0, 5, 0), Some(ARROW_RIGHT));
    }

    #[test]
    fn test_display_width_cjk() {
        let mut canvas = DisplayCanvas::new(20, 3);
        canvas.write_text(0, 0, "用户登录");
        let out = canvas.to_string();
        assert!(out.contains("用户登录"));
        assert!(!out.contains("用 户"));
    }

    #[test]
    fn test_export_string_escapes_controls_and_non_ascii() {
        let options = AsciiExportOptions {
            non_ascii_policy: AsciiNonAsciiPolicy::Escape,
            ..AsciiExportOptions::default()
        };
        let result = export_string("A\t中\x7f", &options).unwrap();
        assert_eq!(result.text, "A\\t\\u{4E2D}\\x7F");
        assert_eq!(result.metadata.escaped_control_chars, 2);
        assert_eq!(result.metadata.escaped_non_ascii, 1);
    }

    #[test]
    fn test_export_bytes_detects_utf16le() {
        let options = AsciiExportOptions::default();
        let bytes = [0xFF, 0xFE, b'A', 0x00, 0x2D, 0x4E];
        let result = export_bytes(&bytes, &options).unwrap();
        assert_eq!(result.metadata.detected_input_encoding, AsciiDetectedEncoding::Utf16Le);
        assert_eq!(result.text, "A\\u{4E2D}");
    }

    #[test]
    fn test_export_bytes_rejects_invalid_utf8_when_requested() {
        let options = AsciiExportOptions {
            input_encoding: AsciiInputEncodingHint::Utf8,
            invalid_input_policy: AsciiInvalidInputPolicy::Error,
            ..AsciiExportOptions::default()
        };
        let err = export_bytes(&[0xF0, 0x28, 0x8C, 0x28], &options).unwrap_err();
        assert_eq!(err.code(), AsciiExportErrorCode::InvalidSequence);
    }

    #[test]
    fn test_export_string_appends_metadata_footer() {
        let options = AsciiExportOptions {
            include_metadata: true,
            field_separator: " ; ".to_string(),
            ..AsciiExportOptions::default()
        };
        let result = export_string("hello", &options).unwrap();
        assert!(result.text.contains("# ascii_export ; detected_input="));
        assert!(result.metadata.metadata_appended);
    }

    #[test]
    fn test_export_reader_streams_large_input() {
        let input = "Graph中".repeat(16_384);
        let options = AsciiExportOptions {
            chunk_size: 512,
            ..AsciiExportOptions::default()
        };
        let mut reader = std::io::Cursor::new(input.as_bytes());
        let mut writer = Vec::new();
        let metadata = export_reader_to_writer(&mut reader, &mut writer, &options).unwrap();
        let text = String::from_utf8(writer).unwrap();
        assert!(text.starts_with("Graph\\u{4E2D}"));
        assert!(metadata.chunks_processed > 1);
        assert_eq!(metadata.chunk_size, 512);
        assert_eq!(metadata.output_bytes, text.len());
    }

    #[test]
    fn test_sanitize_inline_fragment_keeps_single_line_layout() {
        let sanitized = sanitize_inline_fragment("Line 1\nLine 2\t中", &AsciiExportOptions::default()).unwrap();
        assert_eq!(sanitized, "Line 1\\nLine 2\\t\\u{4E2D}");
    }
}
