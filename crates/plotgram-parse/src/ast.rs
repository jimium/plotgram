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

/// Algorithm config block (dsl-spec §2.7): `atom ["{" option_pair* "}"]`.
#[derive(Debug, Clone)]
pub struct AlgorithmConfigAst {
    pub name: String,
    pub options: AttrMap,
}

/// `diagram { … }` body.
#[derive(Debug, Clone)]
pub struct DiagramAst {
    /// Diagram-level attributes (`profile`, `title`, `theme`, `render_style`, …).
    pub attrs: AttrMap,
    /// `layout:` algorithm config (dsl-spec §4.2).
    pub layout: Option<AlgorithmConfigAst>,
    /// `edge_routing:` algorithm config.
    pub edge_routing: Option<AlgorithmConfigAst>,
    /// `partition { … }` block (dsl-spec §11.10 / ADR-008).
    pub partition: Option<PartitionAst>,
    pub items: Vec<DiagramItem>,
}

/// `partition { (column|row <id> { … })* }` (dsl-spec §11.10).
#[derive(Debug, Clone)]
pub struct PartitionAst {
    /// Axis declarations in source order.
    pub axes: Vec<PartitionAxisAst>,
}

/// One axis entry inside a partition block.
#[derive(Debug, Clone)]
pub struct PartitionAxisAst {
    /// `true` = column, `false` = row.
    pub is_column: bool,
    pub id: String,
    /// Optional `label: "…"` attribute.
    pub label: Option<String>,
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
