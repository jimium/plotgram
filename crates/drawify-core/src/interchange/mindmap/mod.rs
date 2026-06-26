//! Mindmap interchange：Markdown 大纲 / OPML / FreeMind 的导出与导入。

pub mod tree;
pub mod diagram;
pub mod export;
pub mod import;

pub use tree::{MindmapTree, MindmapTreeNode, TreeValidationError, RootTitleMode};
pub use diagram::{build_mindmap_tree, mindmap_tree_to_diagram, BuildTreeOptions, DiagramBuildOptions};
pub use export::markdown::{MdOutlineEncoder, MarkdownOutlineOptions, MarkdownOutlineSyntax};
pub use export::opml::{OpmlEncoder, OpmlExportOptions};
pub use export::freemind::{FreemindEncoder, FreemindExportOptions};
pub use import::markdown::{
    MarkdownImportOptions, MarkdownImportSyntax, EntityIdStrategy,
    ImportError, ImportOutput, ImportWarning,
};

/// Input format for interchange import.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputFormat {
    /// Drawify DSL (default, existing path)
    Drawify,
    /// Markdown outline → mindmap
    MdOutline,
}

/// Import interchange source to Diagram AST.
pub fn import_interchange(
    source: &str,
    format: InputFormat,
    options: &MarkdownImportOptions,
) -> Result<crate::ast::Diagram, ImportError> {
    match format {
        InputFormat::MdOutline => import::markdown::import_markdown_outline(source, options),
        InputFormat::Drawify => Err(ImportError::UnsupportedFormat),
    }
}
