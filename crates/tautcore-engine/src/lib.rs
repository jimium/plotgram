//! Tautcore layout engine facade: registry + `run`.
//!
//! Layout implementations live in `tautcore-layout`; edge routers in
//! `tautcore-router`. Both depend on `tautcore-engine-api` + `tautcore-model` +
//! shared parts in `tautcore-algo` — never reverse-depend on this facade's
//! `run` wiring (ADR-006).

#![forbid(unsafe_code)]

mod finalize;
mod registry;
mod run;

/// Shared graph-drawing algorithm parts (see `tautcore-algo` / `PARTS.md`).
pub use tautcore_algo as algo;

/// Independent edge router crate (core primitives + routers + verify + score).
pub use tautcore_router as router;

pub use tautcore_engine_api::{
    EdgeGeometryMode, EdgeRouter, LayoutAlgorithm, LayoutDiagnostics, LayoutError, LayoutInput,
    LayoutOutput, LayoutWarning, OrthogonalRouteParams, PortAnchor, Relaxation, RouteScene,
    TerminalPair,
};
pub use tautcore_router::{
    CurvedEdgeRouter, OctilinearEdgeRouter, OrthogonalEdgeRouter, PolylineEdgeRouter,
    StraightEdgeRouter,
};
pub use run::run;
