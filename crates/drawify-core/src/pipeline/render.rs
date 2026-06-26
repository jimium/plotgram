//! 渲染编排:compute_layout → build_scene → encode。
//!
//! 本模块串联 [`crate::layout`]、[`crate::render::scene`]、[`crate::render::encode`] 三个服务,
//! 将 [`PreparedDiagram`] 转为 [`RenderOutput`]。
//!
//! ```text
//! PreparedDiagram
//!     ├─ EncodingPath::Diagram → encode_from_diagram() → RenderOutput
//!     └─ EncodingPath::Scene
//!         ↓ scene::compute_layout()    → LayoutResult
//!         ↓ scene::build_scene()       → ExportScene
//!         ↓ encode::encode_scene()     → RenderOutput
//! ```
//!
//! `EncodingPath::Diagram` 的编码器(ASCII / Markdown / OPML / FreeMind)跳过视觉物化,
//! 直接从 PreparedDiagram 编码。

use crate::ast::PreparedDiagram;
use crate::error::{DrawifyError, Result};
use crate::layout::RefinementReport;
use crate::render::encode::encoder_for;
use crate::render::encode::drawio::ExportReport;
use crate::render::scene::export_scene;
use crate::render::{RenderFormat, RenderOutput, RenderRequest};

/// 渲染产物 + 意图修正报告 + 导出报告。
///
/// `report` 为 `None` 表示 `RenderRequest.layout_overlay` 为 `None`（无意图叠加）；
/// 为 `Some(RefinementReport::default())` 表示有 overlay 但无意图被消费（空报告）。
///
/// `export_report` 为 `Some` 时携带编码器级别的导出降级报告（目前仅 drawio）。
pub struct RenderOutputWithReport {
    pub output: RenderOutput,
    pub report: Option<RefinementReport>,
    pub export_report: Option<ExportReport>,
}

impl RenderOutputWithReport {
    /// 拆分为 `(output, report)` 元组，便于解构。
    pub fn into_parts(self) -> (RenderOutput, Option<RefinementReport>) {
        (self.output, self.report)
    }
}

impl From<RenderOutput> for RenderOutputWithReport {
    fn from(output: RenderOutput) -> Self {
        Self {
            output,
            report: None,
            export_report: None,
        }
    }
}

/// 渲染流水线:布局 → 物化 → 编码。
///
/// `EncodingPath::Diagram` 的编码器跳过视觉物化,直接从 PreparedDiagram 编码;
/// `EncodingPath::Scene` 的编码器走完整流水线。
///
/// 返回 `RenderOutput`，丢弃意图修正报告。需要报告时使用 [`render_output_with_report`]。
pub fn render_output(request: &RenderRequest<'_>) -> Result<RenderOutput> {
    render_output_with_report(request).map(|r| r.output)
}

/// 与 [`render_output`] 相同，但额外返回意图修正报告。
///
/// `report` 为 `None` 表示 `request.layout_overlay` 为 `None`；
/// 为 `Some(report)` 表示已消费 overlay（可能为空报告）。
pub fn render_output_with_report(request: &RenderRequest<'_>) -> Result<RenderOutputWithReport> {
    let encoder = encoder_for(request.format)?;
    match encoder.encoding_path() {
        crate::render::encode::EncodingPath::Diagram => {
            let result = encoder.encode_from_diagram(request.diagram, request.layout_overlay)?;
            Ok(RenderOutputWithReport {
                output: result.output,
                report: result.report,
                export_report: None,
            })
        }
        crate::render::encode::EncodingPath::Scene => {
            let scene = export_scene(request)?;
            let (output, export_report) = encoder.encode_scene_with_report(&scene)?;
            Ok(RenderOutputWithReport {
                output,
                report: scene.refinement_report,
                export_report,
            })
        }
    }
}

/// 渲染为文本(SVG/ASCII/JSON)。
pub fn render_text(request: &RenderRequest<'_>) -> Result<String> {
    match render_output(request)? {
        RenderOutput::Text(text) => Ok(text),
        RenderOutput::Binary(_) => Err(DrawifyError::render_internal_msg(format!(
            "format '{}' produces binary output, use render_bytes instead",
            request.format
        ))),
    }
}

/// 渲染为字节(PNG/WebP)。
pub fn render_bytes(request: &RenderRequest<'_>) -> Result<Vec<u8>> {
    match render_output(request)? {
        RenderOutput::Binary(bytes) => Ok(bytes),
        RenderOutput::Text(_) => Err(DrawifyError::render_internal_msg(format!(
            "format '{}' produces text output, use render_text instead",
            request.format
        ))),
    }
}

/// 从 PreparedDiagram 直接渲染 JSON(便捷入口,供 diff/export 等场景使用)。
pub fn render_json(diagram: &PreparedDiagram) -> String {
    let request = RenderRequest::new(diagram, RenderFormat::Json);
    render_text(&request).unwrap_or_default()
}

/// 使用内联 style JSON 渲染(供 CLI / Server 传入自定义主题)。
pub fn render_with_style_json(
    diagram: &PreparedDiagram,
    format: RenderFormat,
    style_json: &str,
) -> Result<RenderOutput> {
    let mut request = RenderRequest::new(diagram, format);
    request.explicit_style_json = Some(style_json);
    render_output(&request)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::DiagramType;
    use crate::ast::{Diagram, SourceInfo};

    fn sample_prepared() -> PreparedDiagram {
        PreparedDiagram::new(Diagram::new(
            DiagramType::Flowchart,
            SourceInfo {
                file: None,
                line_count: 1,
            },
        ))
    }

    #[test]
    fn render_request_supports_style_json_bridge() {
        let prepared = sample_prepared();

        let output = render_with_style_json(
            &prepared,
            RenderFormat::Json,
            r##"{
                "version":"0.2",
                "id":"custom.inline",
                "name":"Inline",
                "tokens":{"colors":{"canvas":"#ffffff","text":"#333333"}},
                "defaults":{
                    "canvas":{"background":"#ffffff"},
                    "title":{"fill":"#333333"},
                    "node":{"fill":"#eeeeee","stroke":"#999999"},
                    "edge":{"stroke":"#999999"},
                    "group":{"fill":"#f5f5f5","stroke":"#cccccc"}
                },
                "diagrams":{}
            }"##,
        )
        .unwrap();

        let text = output.into_text().unwrap();
        assert!(text.contains("\"diagram_type\""));
    }

    #[test]
    fn render_request_returns_style_error_for_invalid_style_json() {
        let prepared = sample_prepared();

        let mut request = RenderRequest::new(&prepared, RenderFormat::Svg);
        request.explicit_style_json = Some("{\"version\":}");

        assert!(render_output(&request).is_err());
    }

    #[cfg(feature = "raster")]
    #[test]
    fn semantic_icon_renders_glyph_in_svg() {
        use crate::pipeline::prepare::parse_prepare_validate;
        use crate::prepare::StyleRequest;

        let source = r#"diagram flowchart {
    entity db "Orders DB" {
        type: service
        semantic: mysql
    }
}"#;
        let output = parse_prepare_validate(source, &StyleRequest::default());
        assert!(output.is_valid(), "{:?}", output.errors);
        let prepared = output.diagram.unwrap();
        let svg = render_text(&RenderRequest::new(&prepared, RenderFormat::Svg)).unwrap();
        assert!(svg.contains("Orders DB"));
        assert!(svg.contains(r#"text-anchor="start""#), "icon layout uses start anchor");
        assert!(svg.contains("scale("), "expected icon glyph transform");
    }

    #[test]
    fn render_text_rejects_binary_format() {
        let prepared = sample_prepared();

        let request = RenderRequest::new(&prepared, RenderFormat::Png);
        assert!(render_text(&request).is_err());
    }

    #[cfg(not(feature = "raster"))]
    #[test]
    fn render_output_rejects_raster_format_without_feature() {
        let prepared = sample_prepared();

        let request = RenderRequest::new(&prepared, RenderFormat::Png);
        let err = render_output(&request).unwrap_err().to_string();
        assert!(err.contains("raster"));
    }

    #[test]
    fn render_bytes_rejects_text_format() {
        let prepared = sample_prepared();

        let request = RenderRequest::new(&prepared, RenderFormat::Svg);
        assert!(render_bytes(&request).is_err());
    }

    // ── Phase 2: render_output_with_report 集成测试 ──────────

    use crate::layout::intent::{LayoutIntentOverlay, TopologyIntent};
    use crate::pipeline::prepare::parse_prepare_validate;
    use crate::prepare::StyleRequest;

    fn sample_prepared_with_entities() -> PreparedDiagram {
        let source = r#"diagram flowchart {
            entity a "A"
            entity b "B"
            entity c "C"
        }"#;
        let output = parse_prepare_validate(source, &StyleRequest::default());
        assert!(output.is_valid(), "{:?}", output.errors);
        output.diagram.unwrap()
    }

    #[test]
    fn render_output_with_report_returns_none_when_no_overlay() {
        let prepared = sample_prepared_with_entities();
        let request = RenderRequest::new(&prepared, RenderFormat::Svg);

        let result = render_output_with_report(&request).unwrap();
        assert!(result.report.is_none(), "no overlay → no report");
        assert!(matches!(result.output, RenderOutput::Text(_)));
    }

    #[test]
    fn render_output_with_report_returns_report_when_overlay_present() {
        let prepared = sample_prepared_with_entities();
        let overlay = LayoutIntentOverlay {
            topology: vec![TopologyIntent::Below {
                from: "a".into(),
                to: "b".into(),
            }],
            geometric: vec![],
        };
        let mut request = RenderRequest::new(&prepared, RenderFormat::Svg);
        request.layout_overlay = Some(&overlay);

        let result = render_output_with_report(&request).unwrap();
        let report = result.report.expect("overlay present → report should be Some");
        assert_eq!(report.results.len(), 1);
        assert_eq!(report.satisfied, 1, "Below(a,b) with no real edges → Satisfied");
    }

    #[test]
    fn render_output_with_report_ascii_path_returns_report() {
        let prepared = sample_prepared_with_entities();
        let overlay = LayoutIntentOverlay {
            topology: vec![TopologyIntent::Below {
                from: "a".into(),
                to: "b".into(),
            }],
            geometric: vec![],
        };
        let mut request = RenderRequest::new(&prepared, RenderFormat::Ascii);
        request.layout_overlay = Some(&overlay);

        let result = render_output_with_report(&request).unwrap();
        // ASCII 路径也应返回报告
        let report = result.report.expect("ASCII path should still return report");
        assert_eq!(report.satisfied, 1);
        assert!(matches!(result.output, RenderOutput::Text(_)));
    }

    #[test]
    fn render_output_with_report_reports_conflicted_intent() {
        // Real edge a→b; Below(a,b) → edge b→a → cycle → Conflicted
        let source = r#"diagram flowchart {
            entity a "A"
            entity b "B"
            a -> b
        }"#;
        let output = parse_prepare_validate(source, &StyleRequest::default());
        let prepared = output.diagram.unwrap();

        let overlay = LayoutIntentOverlay {
            topology: vec![TopologyIntent::Below {
                from: "a".into(),
                to: "b".into(),
            }],
            geometric: vec![],
        };
        let mut request = RenderRequest::new(&prepared, RenderFormat::Svg);
        request.layout_overlay = Some(&overlay);

        let result = render_output_with_report(&request).unwrap();
        let report = result.report.expect("overlay present → report");
        assert_eq!(report.conflicted, 1);
        assert_eq!(report.satisfied, 0);
    }

    #[test]
    fn render_output_with_report_reports_not_found_intent() {
        let prepared = sample_prepared_with_entities();
        let overlay = LayoutIntentOverlay {
            topology: vec![TopologyIntent::Below {
                from: "a".into(),
                to: "ghost".into(),
            }],
            geometric: vec![],
        };
        let mut request = RenderRequest::new(&prepared, RenderFormat::Svg);
        request.layout_overlay = Some(&overlay);

        let result = render_output_with_report(&request).unwrap();
        let report = result.report.expect("overlay present → report");
        assert_eq!(report.not_found, 1);
    }

    #[test]
    fn render_output_with_report_geometric_intent_satisfied() {
        let prepared = sample_prepared_with_entities();
        let overlay = LayoutIntentOverlay {
            topology: vec![],
            geometric: vec![crate::layout::intent::GeometricIntent::Pin {
                node: "a".into(),
                axis: crate::layout::intent::PinAxis::Both,
            }],
        };
        let mut request = RenderRequest::new(&prepared, RenderFormat::Svg);
        request.layout_overlay = Some(&overlay);

        let result = render_output_with_report(&request).unwrap();
        let report = result.report.expect("overlay present → report");
        assert_eq!(report.satisfied, 1);
        assert_eq!(report.results[0].kind, "pin");
    }

    #[test]
    fn render_output_with_report_empty_overlay_returns_empty_report() {
        let prepared = sample_prepared_with_entities();
        let overlay = LayoutIntentOverlay::default();
        let mut request = RenderRequest::new(&prepared, RenderFormat::Svg);
        request.layout_overlay = Some(&overlay);

        let result = render_output_with_report(&request).unwrap();
        let report = result.report.expect("overlay present → report");
        assert!(report.is_empty());
        assert_eq!(report.satisfied + report.partial + report.conflicted + report.not_found, 0);
    }
}
