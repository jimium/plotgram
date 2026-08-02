//! Independent edge routing: shared primitives + orthogonal router + verification.
//!
//! Depends only on `plotgram-engine-api` + `plotgram-model` + `plotgram-algo`.
//! Never depends on `plotgram-engine` (facade) or layout implementations.

#![forbid(unsafe_code)]

pub mod core;
pub mod fixture;
pub mod orthogonal;
pub mod score;
pub mod verify;

pub use orthogonal::OrthogonalEdgeRouter;
