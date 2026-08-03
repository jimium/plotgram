//! Plotgram layout engine facade: registry + `run`.
//!
//! Layout implementations live in `plotgram-layout`; edge routers in
//! `plotgram-router`. Both depend on `plotgram-engine-api` + `plotgram-model` +
//! shared parts in `plotgram-algo` — never reverse-depend on this facade's
//! `run` wiring (ADR-006).

#![forbid(unsafe_code)]

mod finalize;
mod registry;
mod run;

/// Shared graph-drawing algorithm parts (see `plotgram-algo` / `PARTS.md`).
pub use plotgram_algo as algo;

/// Independent edge router crate (core primitives + routers + verify + score).
pub use plotgram_router as router;

pub use plotgram_engine_api::{
    EdgeGeometryMode, EdgeRouter, LayoutAlgorithm, LayoutError, LayoutInput, LayoutOutput,
    OrthogonalRouteParams, PortAnchor, RouteScene, TerminalPair,
};
pub use plotgram_router::{
    CurvedEdgeRouter, OctilinearEdgeRouter, OrthogonalEdgeRouter, PolylineEdgeRouter,
    StraightEdgeRouter,
};
pub use run::run;
