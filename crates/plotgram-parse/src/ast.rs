//! DSL AST (parse tree before lowering to [`plotgram_model::graph::Graph`]).
//!
//! Kept separate from model so the grammar can evolve without bloating IR.

use plotgram_model::attr::AttrMap;

/// Root of a `.pgm` file.
#[derive(Debug, Clone)]
pub struct FileAst {
    /// Leading `//` doc comments (optional).
    pub doc_comment: Option<String>,
    pub diagram: DiagramAst,
}

/// `diagram { … }` body.
#[derive(Debug, Clone)]
pub struct DiagramAst {
    /// Diagram-level attributes (`profile`, `title`, `layout`, …) in source order.
    pub attrs: AttrMap,
    pub items: Vec<DiagramItem>,
}

#[derive(Debug, Clone)]
pub enum DiagramItem {
    Node(NodeAst),
    Group(GroupAst),
    Edge(EdgeAst),
}

#[derive(Debug, Clone)]
pub struct NodeAst {
    pub id: String,
    pub attrs: AttrMap,
}

#[derive(Debug, Clone)]
pub struct GroupAst {
    pub id: String,
    pub attrs: AttrMap,
    pub items: Vec<DiagramItem>,
}

/// Edge endpoints may be bare node ids or `@group` sugar (dsl-spec §7.6).
#[derive(Debug, Clone)]
pub enum EndpointAst {
    Node(String),
    /// `@group_id` — expanded later into `group_anchor`.
    GroupFrame(String),
}

#[derive(Debug, Clone)]
pub struct EdgeAst {
    pub source: EndpointAst,
    pub target: EndpointAst,
    /// `->` / `-->` / `<->` as model [`plotgram_model::graph::Arrow`] after lower.
    pub arrow: plotgram_model::graph::Arrow,
    pub attrs: AttrMap,
}
