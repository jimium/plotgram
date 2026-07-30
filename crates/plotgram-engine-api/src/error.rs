//! Errors from layout / routing plugins and the engine facade.

use plotgram_model::sizes::MissingNodeSize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LayoutError {
    #[error(transparent)]
    MissingNodeSize(#[from] MissingNodeSize),

    #[error("unknown layout algorithm `{name}`")]
    UnknownLayout { name: String },

    #[error("unknown edge router `{name}`")]
    UnknownRouter { name: String },

    #[error("layout `{layout}` does not support deferred edge routing")]
    LayoutCannotDeferEdges { layout: String },

    #[error("{0}")]
    Message(String),
}

impl LayoutError {
    pub fn message(msg: impl Into<String>) -> Self {
        Self::Message(msg.into())
    }
}
