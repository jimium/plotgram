//! Edge routing: shared primitives + independent routers (in-tree).
//!
//! Extract `route::core` → `plotgram-route-core` and routers → `plotgram-route-*`
//! when volume justifies it. Implementations depend only on `plotgram-engine-api`
//! + `plotgram-model` (+ core), never on the facade `run` module.

pub mod core;
pub mod orthogonal;

pub use orthogonal::OrthogonalEdgeRouter;
