//! Lower [`crate::ast::FileAst`] → [`plotgram_model::graph::Graph`] + diagram attrs.
//!
//! Does not expand `@group` or profiles — see [`crate::expand`].

use crate::ast::FileAst;
use crate::error::ParseError;
use crate::expand::DiagramMeta;
use plotgram_model::graph::Graph;

/// Intermediate after AST lowering, before sugar / profile expand.
#[derive(Debug, Clone)]
pub struct Lowered {
    pub graph: Graph,
    pub meta: DiagramMeta,
}

/// Lower AST to a graph. Skeleton: not implemented.
pub fn lower(_ast: &FileAst) -> Result<Lowered, ParseError> {
    Err(ParseError::NotImplemented {
        stage: "lower",
        detail: "AST → Graph lowering pending".into(),
    })
}
