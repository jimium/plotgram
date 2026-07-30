//! Parser: tokens / source → [`crate::ast::FileAst`].
//!
//! Skeleton only.

use crate::ast::FileAst;
use crate::error::ParseError;

/// Parse a full file into an AST.
pub fn parse_file(source: &str) -> Result<FileAst, ParseError> {
    let _ = source;
    Err(ParseError::NotImplemented {
        stage: "parser",
        detail: "diagram / node / group / edge grammar pending (dsl-spec)".into(),
    })
}
