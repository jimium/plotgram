//! Layout algorithms for plotgram.
//!
//! Implements [`LayoutAlgorithm`](plotgram_engine_api::LayoutAlgorithm) for each
//! registered layout; depends on `plotgram-engine-api` + `plotgram-model` +
//! `plotgram-algo` — never on the engine facade (ADR-006).

#![forbid(unsafe_code)]

pub mod layout;
mod params;

pub use layout::{
    build_debug_trace, group_penetration_violations, verify_no_group_penetration,
    GroupPenetrationViolation, HierarchicalLayout, LayoutDebugTrace,
};

/// Frame-pad contract constants (see `layout::hierarchical::group_frame`).
pub use layout::hierarchical::group_frame::GROUP_FRAME_GAP;
/// Empty partition band width floor (see `layout::hierarchical::metric::partition_bands`).
pub use layout::hierarchical::PARTITION_EMPTY_BAND_MIN;
