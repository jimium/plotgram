//! Plotgram layout engine facade: registry + `run`.
//!
//! Layout / route **implementations** live under [`layout`] and [`route`] as
//! in-tree modules (extract to separate crates when large). They depend on
//! `plotgram-engine-api` + `plotgram-model` + shared parts in `plotgram-algo`
//! — never reverse-depend on this facade's `run` wiring (ADR-006).

#![forbid(unsafe_code)]

mod finalize;
pub mod layout;
pub mod params;
mod registry;
pub mod route;
mod run;

/// Shared graph-drawing algorithm parts (see `plotgram-algo` / `PARTS.md`).
pub use plotgram_algo as algo;

pub use plotgram_engine_api::{
    EdgeGeometryMode, EdgeRouter, LayoutAlgorithm, LayoutError, LayoutInput, LayoutOutput,
    RouteInput,
};
pub use layout::{
    GroupAlign, GroupPolicy, GroupSizing, HierarchicalLayout, HierarchicalParams,
    HierarchicalPreset, Orientation, RoutingStyle,
};
pub use params::{BindError, BindWarning, OptionsBinder};
pub use route::OrthogonalEdgeRouter;
pub use run::run;
