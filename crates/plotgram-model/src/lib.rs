//! Plotgram data model.
//!
//! Core types shared across all crates:
//! - `geometry`: Point, Rect primitives
//! - `attr`: AttrValue, AttrMap (free-form attribute maps)
//! - `graph`: Node, Edge, Group, Graph (Diagram IR)
//! - `contract`: AlgorithmRef, LayoutContract (engine entry — no diagram_type)
//! - `result`: LayoutResult (geometry output)
//! - `render`: RenderMeta, RenderInput (renderer entry — graph + layout + chrome)
//! - `profile`: DiagramType, Profile (DSL-layer only; must not leak into engine)

pub mod attr;
pub mod contract;
pub mod geometry;
pub mod graph;
pub mod profile;
pub mod render;
pub mod result;
