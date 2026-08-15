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

    /// The route scene contains features the router cannot handle (e.g. groups
    /// without permission support). Routers must fail honestly, never silently
    /// ignore unsupported constraints (R5).
    #[error("unsupported route scene: {reason}")]
    UnsupportedRouteScene { reason: String },

    /// Feature named but not implemented (honest hard-fail, never silent).
    #[error("unsupported: {feature}")]
    Unsupported { feature: String },

    /// Author / constraint input cannot be realized.
    #[error("{message}")]
    InvalidInput { message: String },

    /// Layout invariant broken (bug, not author error).
    #[error("{message}")]
    InternalInvariant { message: String },

    #[error("{0}")]
    Message(String),
}

impl LayoutError {
    pub fn message(msg: impl Into<String>) -> Self {
        Self::Message(msg.into())
    }

    pub fn unsupported(feature: impl Into<String>) -> Self {
        Self::Unsupported {
            feature: feature.into(),
        }
    }

    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::InvalidInput {
            message: message.into(),
        }
    }

    pub fn invariant(message: impl Into<String>) -> Self {
        Self::InternalInvariant {
            message: message.into(),
        }
    }
}
