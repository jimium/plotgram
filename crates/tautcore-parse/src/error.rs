//! Parse errors and non-fatal warnings.

use thiserror::Error;

/// Non-fatal parse diagnostic (dsl-spec §4.2: unknown diagram keys warn and ignore).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseWarning {
    pub message: String,
}

impl ParseWarning {
    pub fn unknown_diagram_key(key: impl Into<String>) -> Self {
        let key = key.into();
        Self {
            message: format!("unknown diagram attribute `{key}`; ignored (dsl-spec §4.2)"),
        }
    }
}

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

    #[error("duplicate id `{id}` (first declared at line {first_line})")]
    DuplicateId { id: String, first_line: u32 },

    #[error("unknown profile `{value}`; expected one of: flowchart, sequence, architecture, state, er, mindmap")]
    UnknownProfile { value: String },

    #[error("duplicate attribute `{key}` in {context}")]
    DuplicateAttr { key: String, context: String },

    #[error(transparent)]
    Port(#[from] tautcore_model::port::PortConstraintError),

    #[error(transparent)]
    NodeStructural(#[from] tautcore_model::graph::NodeStructuralError),
}

impl ParseError {
    pub fn syntax(line: u32, column: u32, message: impl Into<String>) -> Self {
        Self::Syntax {
            line,
            column,
            message: message.into(),
        }
    }

    pub fn lex(line: u32, column: u32, message: impl Into<String>) -> Self {
        Self::Lex {
            line,
            column,
            message: message.into(),
        }
    }
}
