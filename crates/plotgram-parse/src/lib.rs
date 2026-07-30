//! Plotgram DSL parser (`.pgm` → model).
//!
//! Pipeline (target):
//!
//! ```text
//! source
//!   → lex / parse          → DslAst
//!   → lower                → Graph (+ diagram attrs)
//!   → expand @group sugar  → group_anchor nodes
//!   → lift structural attrs
//!   → profile expand       → default layout / edge_routing
//!   → ParseOutput          → ready for measure → LayoutContract
//! ```
//!
//! This crate must **not** depend on `plotgram-engine` / layout crates.
//! Theme / MeasureParams stay in the orchestrator (cli); parse may later
//! call `plotgram-content` only for node `content:` MD strings.
//!
//! Status: **skeleton** — public API and stages exist; full grammar is TODO.

#![forbid(unsafe_code)]

pub mod ast;
pub mod error;
pub mod expand;
pub mod lexer;
pub mod lower;
pub mod parser;

pub use error::ParseError;
pub use expand::DiagramMeta;

use plotgram_model::contract::{AlgorithmRef, LayoutContract};
use plotgram_model::graph::Graph;
use plotgram_model::profile::DiagramType;
use plotgram_model::render::RenderMeta;
use plotgram_model::sizes::NodeSizes;

/// Successful parse + profile expansion (sizes still empty — measure fills them).
#[derive(Debug, Clone)]
pub struct ParseOutput {
    /// Structural graph (anchors expanded, structural attrs lifted).
    pub graph: Graph,
    /// Resolved layout algorithm (from `layout:` or `profile:` defaults).
    pub layout: AlgorithmRef,
    /// Independent router, if any (`None` = layout built-in edges).
    pub edge_routing: Option<AlgorithmRef>,
    /// Profile id after resolve (`None` if author only set `layout:`).
    pub profile: Option<DiagramType>,
    /// Chrome for render (title / theme / render_style).
    pub meta: RenderMeta,
}

impl ParseOutput {
    /// Build a [`LayoutContract`] once preferred sizes are known.
    pub fn into_contract(self, node_sizes: NodeSizes) -> LayoutContract {
        LayoutContract {
            layout: self.layout,
            edge_routing: self.edge_routing,
            graph: self.graph,
            node_sizes,
        }
    }
}

/// Parse a `.pgm` source string into [`ParseOutput`].
///
/// Skeleton: returns [`ParseError::NotImplemented`] until the grammar lands.
pub fn parse(source: &str) -> Result<ParseOutput, ParseError> {
    let _tokens = lexer::tokenize(source)?;
    let _ast = parser::parse_file(source)?;
    Err(ParseError::NotImplemented {
        stage: "parser",
        detail: "DSL grammar not implemented yet; see dsl-spec 2.6".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skeleton_entry_is_wired() {
        let err = parse("diagram { profile: flowchart }").unwrap_err();
        assert!(matches!(
            err,
            ParseError::NotImplemented { stage: "parser", .. }
        ));
    }
}
