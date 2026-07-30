//! Plugin surface for layout and edge routing (no algorithms).
//!
//! Concrete layouts/routers depend on this crate + `plotgram-model`.
//! The `plotgram-engine` facade depends on implementations — never the reverse.
//!
//! See ADR-006.

#![forbid(unsafe_code)]

mod error;
mod traits;

pub use error::LayoutError;
pub use traits::{
    EdgeGeometryMode, EdgeRouter, LayoutAlgorithm, LayoutInput, LayoutOutput, RouteInput,
};
