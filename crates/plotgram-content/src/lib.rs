//! Node content blocks (ADR-005, `docs/specs/content-md-spec.md`).
//!
//! Pipeline (three stages, geometry written exactly once):
//!
//! ```text
//! parse(text)                      -> ContentDoc      (structure only)
//! measure(&doc, &MeasureParams)    -> ContentLayout   (sole geometry writer)
//! emit_svg(&layout, &ContentPaint) -> <g> fragment    (expansion only, zero new decisions)
//! ```
//!
//! Boundary rules:
//! - No font files are ever opened; widths come from codepoint-class heuristics
//!   (constants calibrated offline via `scripts/calibrate_content_measure.py`).
//! - `MeasureParams` is an input; this crate never loads themes or defaults sources.
//! - Paint (colors) only enters at `emit_svg`; it never influences geometry.
//! - Deterministic: pure functions, `Vec`-only, no `HashMap`, no wall clock.

pub mod ast;
pub mod emit;
pub mod measure;
pub mod parse;

pub use ast::{Block, ContentDoc, Line, ListItem, Run, RunStyle};
pub use emit::{emit_svg, ContentPaint};
pub use measure::{measure, Align, ContentLayout, LineBox, MeasureParams, RunBox};
pub use parse::parse;

/// Convenience: parse + measure in one call (pre-layout orchestration entry).
pub fn measure_content(text: &str, params: &MeasureParams) -> ContentLayout {
    measure(&parse(text), params)
}
