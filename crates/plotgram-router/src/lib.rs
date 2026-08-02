//! Independent edge routing: shared primitives + routers + verification.
//!
//! Depends only on `plotgram-engine-api` + `plotgram-model` + `plotgram-algo`.
//! Never depends on `plotgram-engine` (facade) or layout implementations.

#![forbid(unsafe_code)]

pub mod core;
pub mod curved;
pub mod fixture;
pub mod octilinear;
pub mod orthogonal;
pub mod polyline;
pub mod score;
pub mod straight;
pub mod verify;

pub use curved::CurvedEdgeRouter;
pub use octilinear::OctilinearEdgeRouter;
pub use orthogonal::OrthogonalEdgeRouter;
pub use polyline::PolylineEdgeRouter;
pub use straight::StraightEdgeRouter;
