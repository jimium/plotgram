//! Plotgram layout engine facade: registry + `run`.
//!
//! Layout / route **implementations** live under [`layout`] and in the
//! `plotgram-router` crate. They depend on `plotgram-engine-api` +
//! `plotgram-model` + shared parts in `plotgram-algo` — never reverse-depend
//! on this facade's `run` wiring (ADR-006).

#![forbid(unsafe_code)]

mod finalize;
pub mod layout;
pub mod params;
mod registry;
mod run;

/// Shared graph-drawing algorithm parts (see `plotgram-algo` / `PARTS.md`).
pub use plotgram_algo as algo;

/// Independent edge router crate (core primitives + orthogonal router + verify + score).
pub use plotgram_router as router;

pub use plotgram_engine_api::{
    EdgeGeometryMode, EdgeRouter, LayoutAlgorithm, LayoutError, LayoutInput, LayoutOutput,
    OrthogonalRouteParams, PortAnchor, RouteScene, TerminalPair,
};
pub use layout::{
    GroupAlign, GroupPolicy, GroupSizing, HierarchicalLayout, HierarchicalParams,
    HierarchicalPreset, Orientation, RoutingStyle,
};
pub use params::{BindError, BindWarning, OptionsBinder};
pub use plotgram_router::OrthogonalEdgeRouter;
pub use run::run;
