//! Markdown 大纲导出编码器。

use std::fmt::Write;

use crate::ast::PreparedDiagram;
use crate::error::Result;
use crate::layout::LayoutIntentOverlay;
use crate::render::encode::{DiagramEncodeOutput, EncodingPath, FormatEncoder};
use crate::render::{RenderFormat, RenderOutput};

use crate::interchange::mindmap::{build_mindmap_tree, BuildTreeOptions, MindmapTree, MindmapTreeNode, RootTitleMode};

/// Markdown 大纲语法模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MarkdownOutlineSyntax {
    /// ATX 标题模式（默认）
    #[default]
    AtxHeadings,
    /// 嵌套列表模式
    NestedList,
}

/// Markdown 大纲导出选项。
#[derive(Debug, Clone)]
pub struct MarkdownOutlineOptions {
    pub syntax: MarkdownOutlineSyntax,
    pub root_title_mode: RootTitleMode,
    pub min_level: u8,
    pub max_level: u8,
    pub include_entity_ids: bool,
    pub strict_tree: bool,
}

impl Default for MarkdownOutlineOptions {
    fn default() -> Self {
        Self {
            syntax: MarkdownOutlineSyntax::AtxHeadings,
            root_title_mode: RootTitleMode::Separate,
            min_level: 1,
            max_level: 6,
            include_entity_ids: false,
            strict_tree: true,
        }
    }
}

pub struct MdOutlineEncoder;

impl FormatEncoder for MdOutlineEncoder {
    fn format(&self) -> RenderFormat {
        RenderFormat::MdOutline
    }

    fn name(&self) -> &str {
        "md-outline"
    }

    fn description(&self) -> &str {
        "Markdown outline export for document workflows"
    }

    fn encoding_path(&self) -> EncodingPath {
        EncodingPath::Diagram
    }

    fn encode_scene(&self, _scene: &crate::render::scene::ExportScene<'_>) -> Result<RenderOutput> {
        Err(crate::error::DrawifyError::render_internal_msg(
            "md-outline does not support scene encoding",
        ))
    }

    fn encode_from_diagram(
        &self,
        diagram: &PreparedDiagram,
        _layout_overlay: Option<&LayoutIntentOverlay>,
    ) -> Result<DiagramEncodeOutput> {
        let inner = diagram.inner();

        // 仅支持 mindmap
        if inner.diagram_type != crate::types::DiagramType::Mindmap {
            return Err(crate::error::DrawifyError::render_internal_msg(
                "md-outline export is only supported for mindmap diagrams",
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

        let text = encode_markdown_atx(&tree, &MarkdownOutlineOptions::default());

        Ok(DiagramEncodeOutput {
            output: RenderOutput::Text(text),
            report: None,
        })
    }

    fn file_extension(&self) -> &str {
        "md"
    }
}

/// 将 MindmapTree 编码为 ATX 标题格式的 Markdown 大纲。
pub fn encode_markdown_atx(tree: &MindmapTree, options: &MarkdownOutlineOptions) -> String {
    let mut output = String::new();
    let base_level = options.min_level;

    // 处理 title 与 root label 的关系
    let title_separate = options.root_title_mode == RootTitleMode::Separate
        && tree.title.as_deref() != Some(&tree.root.label);

    if title_separate {
        // 输出 # title
        if let Some(ref title) = tree.title {
            let _ = writeln!(output, "{} {}", "#".repeat(base_level as usize), title);
            let _ = writeln!(output);
        }
        // root 从 base_level + 1 开始
        emit_atx_node(&tree.root, base_level + 1, options, &mut output);
    } else {
        // root 就是 #
        emit_atx_node(&tree.root, base_level, options, &mut output);
    }

    // 孤立节点（宽松模式）
    if !tree.orphans.is_empty() {
        let _ = writeln!(output);
        let _ = writeln!(output, "## 未连接节点");
        for orphan in &tree.orphans {
            let _ = writeln!(output, "### {}", orphan.label);
        }
    }

    output
}

fn emit_atx_node(
    node: &MindmapTreeNode,
    level: u8,
    options: &MarkdownOutlineOptions,
    output: &mut String,
) {
    let clamped_level = level.min(options.max_level);

    if clamped_level <= options.max_level {
        let _ = writeln!(
            output,
            "{} {}",
            "#".repeat(clamped_level as usize),
            node.label
        );

        if options.include_entity_ids {
            let _ = writeln!(
                output,
                "<!-- drawify:entity-id={} -->",
                node.entity_id
            );
        }
    }

    let _ = writeln!(output);

    for child in &node.children {
        emit_atx_node(child, level + 1, options, output);
    }
}
