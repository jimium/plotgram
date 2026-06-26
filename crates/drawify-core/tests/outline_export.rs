//! Mindmap interchange 导出集成测试。

use drawify_core::pipeline::parse_prepare_validate;
use drawify_core::prepare::StyleRequest;
use drawify_core::render::{RenderFormat, RenderRequest};
use drawify_core::pipeline::render_text;

fn simple_mindmap_source() -> &'static str {
    r#"diagram mindmap {
    title: "头脑风暴"
    entity root "产品规划" { type: root }
    entity feature "功能需求" { type: main }
    entity tech "技术方案" { type: main }
    entity market "市场调研" { type: main }
    root -> feature
    root -> tech
    root -> market
}"#
}

#[test]
fn mindmap_to_markdown_atx() {
    let source = simple_mindmap_source();
    let output = parse_prepare_validate(source, &StyleRequest::default());
    assert!(output.is_valid(), "parse errors: {:?}", output.errors);
    let prepared = output.diagram.unwrap();

    let request = RenderRequest::new(&prepared, RenderFormat::MdOutline);
    let text = render_text(&request).unwrap();

    assert!(text.contains("# 头脑风暴"), "should contain title heading");
    assert!(text.contains("## 产品规划"), "should contain root heading");
    assert!(text.contains("### 功能需求"), "should contain child heading");
    assert!(text.contains("### 技术方案"));
    assert!(text.contains("### 市场调研"));
}

#[test]
fn mindmap_to_opml() {
    let source = simple_mindmap_source();
    let output = parse_prepare_validate(source, &StyleRequest::default());
    assert!(output.is_valid());
    let prepared = output.diagram.unwrap();

    let request = RenderRequest::new(&prepared, RenderFormat::Opml);
    let text = render_text(&request).unwrap();

    assert!(text.contains("<?xml"), "should be XML");
    assert!(text.contains("<opml version=\"2.0\">"));
    assert!(text.contains("<title>头脑风暴</title>"));
    assert!(text.contains("产品规划"));
    assert!(text.contains("功能需求"));
}

#[test]
fn mindmap_to_freemind() {
    let source = simple_mindmap_source();
    let output = parse_prepare_validate(source, &StyleRequest::default());
    assert!(output.is_valid());
    let prepared = output.diagram.unwrap();

    let request = RenderRequest::new(&prepared, RenderFormat::Freemind);
    let text = render_text(&request).unwrap();

    assert!(text.contains("<?xml"), "should be XML");
    assert!(text.contains("<map version=\"1.0.1\">"));
    assert!(text.contains("产品规划"));
    assert!(text.contains("功能需求"));
}

#[test]
fn non_mindmap_rejected_for_md_outline() {
    let source = r#"diagram flowchart {
        entity a "A"
        entity b "B"
        a -> b
    }"#;
    let output = parse_prepare_validate(source, &StyleRequest::default());
    assert!(output.is_valid());
    let prepared = output.diagram.unwrap();

    let request = RenderRequest::new(&prepared, RenderFormat::MdOutline);
    let result = render_text(&request);
    assert!(result.is_err(), "non-mindmap should be rejected");
}
