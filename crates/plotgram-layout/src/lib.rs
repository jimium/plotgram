//! Layout algorithms for plotgram.
//!
//! Implements [`LayoutAlgorithm`](plotgram_engine_api::LayoutAlgorithm) for each
//! registered layout; depends on `plotgram-engine-api` + `plotgram-model` +
//! `plotgram-algo` — never on the engine facade (ADR-006).

#![forbid(unsafe_code)]

pub mod layout;
mod params;

pub use layout::HierarchicalLayout;
