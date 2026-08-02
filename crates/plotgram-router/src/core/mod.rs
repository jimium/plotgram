//! Shared orthogonal routing primitives — no layout/router strategy.
//!
//! Used by hierarchical built-in ink and by independent edge routers.
//! Must not branch on profile / diagram type (AGENTS.md).

mod ortho;
mod rect;

pub use ortho::{normalize_polyline, orthogonal_elbow, overlap_len, port_anchor};
pub use rect::{expand_union, padding_rect, segment_intersects_rect, union_rects};
