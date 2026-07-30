//! Lexer (tokenizer) — skeleton.

use crate::error::ParseError;

/// Token kinds (to be filled with dsl-spec terminals).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    // Placeholder until the real lexer lands.
    Eof,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub line: u32,
    pub column: u32,
}

/// Tokenize `source`. Skeleton returns a single EOF (no scanning yet).
pub fn tokenize(source: &str) -> Result<Vec<Token>, ParseError> {
    let _ = source;
    Ok(vec![Token {
        kind: TokenKind::Eof,
        line: 1,
        column: 1,
    }])
}
