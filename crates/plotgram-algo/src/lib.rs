//! Shared graph-drawing **algorithm parts** for plotgram.
//!
//! This crate holds reusable, independently testable building blocks
//! (VPSC, FAS, crossing metrics, …). It does **not** run a layout pipeline,
//! does **not** read `LayoutContract` / profile / diagram type, and must not
//! depend on `plotgram-engine`.
//!
//! Part selection and implementation order: [`PARTS.md`](../PARTS.md).
//!
//! Dependency rule:
//! ```text
//! (no engine) ← plotgram-algo ← plotgram-engine (layout / route)
//! ```

#![forbid(unsafe_code)]

pub mod crossing;
pub mod fas;
pub mod interval_color;
pub mod linear_arrange;
pub mod orientation;
pub mod path_ortho;
pub mod vpsc;
