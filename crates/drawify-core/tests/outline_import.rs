//! Mindmap interchange 导入集成测试。

use drawify_core::interchange::mindmap::{
    import_interchange, InputFormat, MarkdownImportOptions,
};
use drawify_core::pipeline::import_prepare_validate;
use drawify_core::prepare::StyleRequest;
use drawify_core::render::{RenderFormat, RenderRequest};
use drawify_core::pipeline::render_text;

#[test]
fn markdown_outline_to_mindmap_svg() {
    let source = r#"# 头脑风暴

## 产品规划

### 功能需求
### 技术方案
### 市场调研
"#;
    let diagram = import_interchange(source, InputFormat::MdOutline, &MarkdownImportOptions::default()).unwrap();
    let output = import_prepare_validate(diagram, &StyleRequest::default());
    assert!(output.is_valid(), "import errors: {:?}", output.errors);

    let prepared = output.diagram.unwrap();
    // Should have 4 entities: root + 3 children
    assert_eq!(prepared.inner().entities.len(), 4);

    // Should be renderable as SVG
    let request = RenderRequest::new(&prepared, RenderFormat::Svg);
    let svg = render_text(&request).unwrap();
    assert!(svg.contains("<svg"));
    assert!(svg.contains("产品规划"));
}

#[test]
fn markdown_import_entity_count() {
    let source = r#"# Root

## Child A

### Grandchild 1
### Grandchild 2

## Child B
"#;
    let diagram = import_interchange(source, InputFormat::MdOutline, &MarkdownImportOptions::default()).unwrap();
    let output = import_prepare_validate(diagram, &StyleRequest::default());
    assert!(output.is_valid());
    let prepared = output.diagram.unwrap();
    // root + child_a + grandchild_1 + grandchild_2 + child_b = 5
    assert_eq!(prepared.inner().entities.len(), 5);
}
