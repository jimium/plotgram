//! OPML 导出编码器。

use crate::ast::PreparedDiagram;
use crate::error::Result;
use crate::layout::LayoutIntentOverlay;
use crate::render::encode::{DiagramEncodeOutput, EncodingPath, FormatEncoder};
use crate::render::{RenderFormat, RenderOutput};

use crate::interchange::mindmap::{
    build_mindmap_tree, MindmapTree, MindmapTreeNode, BuildTreeOptions, RootTitleMode,
};

/// OPML 导出选项。
#[derive(Debug, Clone)]
pub struct OpmlExportOptions {
    pub root_title_mode: RootTitleMode,
    pub include_metadata: bool,
    pub strict_tree: bool,
    pub include_date: bool,
}

impl Default for OpmlExportOptions {
    fn default() -> Self {
        Self {
            root_title_mode: RootTitleMode::Separate,
            include_metadata: true,
            strict_tree: true,
            include_date: true,
        }
    }
}

pub struct OpmlEncoder;

impl FormatEncoder for OpmlEncoder {
    fn format(&self) -> RenderFormat {
        RenderFormat::Opml
    }

    fn name(&self) -> &str {
        "opml"
    }

    fn description(&self) -> &str {
        "OPML outline export for outliners and RSS tools"
    }

    fn encoding_path(&self) -> EncodingPath {
        EncodingPath::Diagram
    }

    fn encode_scene(&self, _scene: &crate::render::scene::ExportScene<'_>) -> Result<RenderOutput> {
        Err(crate::error::DrawifyError::render_internal_msg(
            "opml does not support scene encoding",
        ))
    }

    fn encode_from_diagram(
        &self,
        diagram: &PreparedDiagram,
        _layout_overlay: Option<&LayoutIntentOverlay>,
    ) -> Result<DiagramEncodeOutput> {
        let inner = diagram.inner();

        if inner.diagram_type != crate::types::DiagramType::Mindmap {
            return Err(crate::error::DrawifyError::render_internal_msg(
                "opml export is only supported for mindmap diagrams",
            ));
        }

        let options = BuildTreeOptions {
            root_title_mode: RootTitleMode::Separate,
            strict_tree: true,
        };

        let tree = build_mindmap_tree(inner, &options).map_err(|errs| {
            crate::error::DrawifyError::render_internal_msg(format!(
                "tree validation failed: {:?}",
                errs
            ))
        })?;

        let text = encode_opml(&tree, &OpmlExportOptions::default());

        Ok(DiagramEncodeOutput {
            output: RenderOutput::Text(text),
            report: None,
        })
    }

    fn file_extension(&self) -> &str {
        "opml"
    }
}

/// 将 MindmapTree 编码为 OPML 2.0 XML。
pub fn encode_opml(tree: &MindmapTree, options: &OpmlExportOptions) -> String {
    let mut output = String::new();

    output.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    output.push_str("<opml version=\"2.0\">\n");
    output.push_str("  <head>\n");

    // title
    let head_title = tree.title.as_deref().unwrap_or(&tree.root.label);
    output.push_str(&format!(
        "    <title>{}</title>\n",
        xml_escape(head_title)
    ));

    // dateCreated
    if options.include_date {
        output.push_str("    <dateCreated></dateCreated>\n");
    }

    output.push_str("  </head>\n");
    output.push_str("  <body>\n");

    // root outline
    emit_opml_node(&tree.root, options, &mut output);

    // orphans
    for orphan in &tree.orphans {
        output.push_str(&format!(
            "    <outline text=\"{}\"{} drawifyOrphan=\"true\"/>\n",
            xml_escape(&orphan.label),
            if options.include_metadata {
                format!(" drawifyEntityId=\"{}\"", orphan.entity_id)
            } else {
                String::new()
            }
        ));
    }

    output.push_str("  </body>\n");
    output.push_str("</opml>\n");

    output
}

fn emit_opml_node(
    node: &MindmapTreeNode,
    options: &OpmlExportOptions,
    output: &mut String,
) {
    let indent = "    ";
    let has_children = !node.children.is_empty();

    let entity_id_attr = if options.include_metadata {
        format!(" drawifyEntityId=\"{}\"", node.entity_id)
    } else {
        String::new()
    };

    if has_children {
        output.push_str(&format!(
            "{}<outline text=\"{}\"{}>\n",
            indent,
            xml_escape(&node.label),
            entity_id_attr
        ));
        for child in &node.children {
            emit_opml_node(child, options, output);
        }
        output.push_str(&format!("{}</outline>\n", indent));
    } else {
        output.push_str(&format!(
            "{}<outline text=\"{}\"{}/>\n",
            indent,
            xml_escape(&node.label),
            entity_id_attr
        ));
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
