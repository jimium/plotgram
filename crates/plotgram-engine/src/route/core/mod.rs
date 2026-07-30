//! Shared orthogonal routing primitives — no layout/router strategy.
//!
//! Used by hierarchical built-in ink and by independent edge routers.
//! Must not branch on profile / diagram type (AGENTS.md).

mod ortho;
mod rect;

pub use ortho::{orthogonal_elbow, port_anchor};
pub use rect::{expand_union, padding_rect, union_rects};
