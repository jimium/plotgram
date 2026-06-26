//! FreeMind (.mm) 导出编码器（L0 only：结构 + TEXT，不导出样式）。

use crate::ast::PreparedDiagram;
use crate::error::Result;
use crate::layout::LayoutIntentOverlay;
use crate::render::encode::{DiagramEncodeOutput, EncodingPath, FormatEncoder};
use crate::render::{RenderFormat, RenderOutput};

use crate::interchange::mindmap::{
    build_mindmap_tree, MindmapTree, MindmapTreeNode, BuildTreeOptions, RootTitleMode,
};

/// FreeMind 导出选项。
#[derive(Debug, Clone)]
pub struct FreemindExportOptions {
    pub root_title_mode: RootTitleMode,
    pub strict_tree: bool,
    pub include_entity_ids: bool,
    pub id_prefix: String,
}

impl Default for FreemindExportOptions {
    fn default() -> Self {
        Self {
            root_title_mode: RootTitleMode::Separate,
            strict_tree: true,
            include_entity_ids: false,
            id_prefix: "Freemind_".to_string(),
        }
    }
}

pub struct FreemindEncoder;

impl FormatEncoder for FreemindEncoder {
    fn format(&self) -> RenderFormat {
        RenderFormat::Freemind
    }

    fn name(&self) -> &str {
        "freemind"
    }

    fn description(&self) -> &str {
        "FreeMind .mm export for mind mapping tools"
    }

    fn encoding_path(&self) -> EncodingPath {
        EncodingPath::Diagram
    }

    fn encode_scene(&self, _scene: &crate::render::scene::ExportScene<'_>) -> Result<RenderOutput> {
        Err(crate::error::DrawifyError::render_internal_msg(
            "freemind does not support scene encoding",
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
                "freemind export is only supported for mindmap diagrams",
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

        let text = encode_freemind(&tree, &FreemindExportOptions::default());

        Ok(DiagramEncodeOutput {
            output: RenderOutput::Text(text),
            report: None,
        })
    }

    fn file_extension(&self) -> &str {
        "mm"
    }
}

/// 将 MindmapTree 编码为 FreeMind 1.0.1 XML（L0 only）。
pub fn encode_freemind(tree: &MindmapTree, options: &FreemindExportOptions) -> String {
    let mut output = String::new();
    let mut id_counter = 0usize;

    output.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    output.push_str("<map version=\"1.0.1\">\n");

    // root node
    emit_freemind_node(&tree.root, options, &mut output, &mut id_counter, true);

    output.push_str("</map>\n");

    output
}

fn emit_freemind_node(
    node: &MindmapTreeNode,
    options: &FreemindExportOptions,
    output: &mut String,
    id_counter: &mut usize,
    is_root: bool,
) {
    let id = if is_root {
        format!("{}Root", options.id_prefix)
    } else {
        *id_counter += 1;
        format!("{}{}", options.id_prefix, id_counter)
    };

    let entity_id_attr = if options.include_entity_ids {
        format!(" drawifyEntityId=\"{}\"", node.entity_id)
    } else {
        String::new()
    };

    let folded_attr = if is_root { " FOLDED=\"false\"" } else { "" };

    let has_children = !node.children.is_empty();

    if has_children {
        output.push_str(&format!(
            "  <node TEXT=\"{}\" ID=\"{}\"{}{}>\n",
            xml_escape(&node.label),
            id,
            folded_attr,
            entity_id_attr
        ));
        for child in &node.children {
            emit_freemind_node(child, options, output, id_counter, false);
        }
        output.push_str("  </node>\n");
    } else {
        output.push_str(&format!(
            "  <node TEXT=\"{}\" ID=\"{}\"{}{}/>\n",
            xml_escape(&node.label),
            id,
            folded_attr,
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
