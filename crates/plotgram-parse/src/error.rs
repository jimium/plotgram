//! Parse errors.

use thiserror::Error;

#[derive(Debug, Error, Clone)]
pub enum ParseError {
    /// Stage not implemented yet (skeleton).
    #[error("parse stage `{stage}` not implemented: {detail}")]
    NotImplemented { stage: &'static str, detail: String },

    #[error("lex error at {line}:{column}: {message}")]
    Lex {
        line: u32,
        column: u32,
        message: String,
    },

    #[error("syntax error at {line}:{column}: {message}")]
    Syntax {
        line: u32,
        column: u32,
        message: String,
    },

    #[error("semantic error: {0}")]
    Semantic(String),

    #[error(transparent)]
    Port(#[from] plotgram_model::port::PortConstraintError),

    #[error(transparent)]
    NodeStructural(#[from] plotgram_model::graph::NodeStructuralError),
}
