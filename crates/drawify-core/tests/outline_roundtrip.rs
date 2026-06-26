//! Mindmap interchange round-trip 集成测试。

use drawify_core::interchange::mindmap::{
    build_mindmap_tree, BuildTreeOptions, RootTitleMode,
    import_interchange, InputFormat, MarkdownImportOptions,
};
use drawify_core::interchange::mindmap::export::markdown::{encode_markdown_atx, MarkdownOutlineOptions, MarkdownOutlineSyntax};
use drawify_core::pipeline::parse_prepare_validate;
use drawify_core::prepare::StyleRequest;
use drawify_core::pipeline::import_prepare_validate;
use drawify_core::interchange::mindmap::MindmapTreeNode;

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

/// Collect all (label, depth) pairs from a tree via DFS.
fn collect_labels_and_depths(
    node: &MindmapTreeNode,
    depth: usize,
) -> Vec<(String, usize)> {
    let mut result = vec![(node.label.clone(), depth)];
    for child in &node.children {
        result.extend(collect_labels_and_depths(child, depth + 1));
    }
    result
}

#[test]
fn dsl_export_md_import_structure_isomorphic() {
    // Step 1: Parse DSL → Diagram
    let output = parse_prepare_validate(simple_mindmap_source(), &StyleRequest::default());
    assert!(output.is_valid());
    let prepared = output.diagram.unwrap();
    let diagram = prepared.inner();

    // Step 2: Build MindmapTree
    let tree = build_mindmap_tree(diagram, &BuildTreeOptions::default()).unwrap();
    let original_labels = collect_labels_and_depths(&tree.root, 0);

    // Step 3: Export to Markdown
    let md_options = MarkdownOutlineOptions {
        syntax: MarkdownOutlineSyntax::AtxHeadings,
        root_title_mode: RootTitleMode::Separate,
        ..Default::default()
    };
    let md_text = encode_markdown_atx(&tree, &md_options);

    // Step 4: Import Markdown back
    let imported_diagram = import_interchange(&md_text, InputFormat::MdOutline, &MarkdownImportOptions::default()).unwrap();
    let import_output = import_prepare_validate(imported_diagram, &StyleRequest::default());
    assert!(import_output.is_valid(), "import errors: {:?}", import_output.errors);
    let imported_prepared = import_output.diagram.unwrap();

    // Step 5: Build MindmapTree from imported diagram
    let imported_tree = build_mindmap_tree(imported_prepared.inner(), &BuildTreeOptions::default()).unwrap();
    let imported_labels = collect_labels_and_depths(&imported_tree.root, 0);

    // Step 6: Assert structure isomorphic (same labels + depths, same order)
    assert_eq!(original_labels, imported_labels,
        "round-trip should preserve label structure and depth order");
}

#[test]
fn md_import_export_md_normalized() {
    let source = r#"# 头脑风暴

## 产品规划

### 功能需求
### 技术方案
### 市场调研
"#;

    // Import
    let diagram = import_interchange(source, InputFormat::MdOutline, &MarkdownImportOptions::default()).unwrap();
    let import_output = import_prepare_validate(diagram, &StyleRequest::default());
    assert!(import_output.is_valid());
    let prepared = import_output.diagram.unwrap();

    // Build tree and export
    let tree = build_mindmap_tree(prepared.inner(), &BuildTreeOptions::default()).unwrap();
    let md_options = MarkdownOutlineOptions {
        syntax: MarkdownOutlineSyntax::AtxHeadings,
        root_title_mode: RootTitleMode::Separate,
        ..Default::default()
    };
    let exported = encode_markdown_atx(&tree, &md_options);

    // Should contain the same structure
    assert!(exported.contains("# 头脑风暴"));
    assert!(exported.contains("## 产品规划"));
    assert!(exported.contains("### 功能需求"));
}
