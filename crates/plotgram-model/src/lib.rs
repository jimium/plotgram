//! Plotgram data model.
//!
//! Core types shared across all crates:
//! - `geometry`: Point, Rect primitives
//! - `attr`: AttrValue, AttrMap (free-form attribute maps)
//! - `graph`: Node, Edge (ports / edge_group first-class), Group, Graph
//! - `port`: Side, PortConstraint, PortRef (dsl-spec §7.4)
//! - `contract`: AlgorithmRef, LayoutContract (engine entry — no diagram_type)
//! - `result`: LayoutResult (geometry; EdgePlacement carries resolved PortRef)
//! - `render`: RenderMeta, RenderInput (renderer entry — graph + layout + chrome)
//! - `profile`: DiagramType, Profile (DSL-layer only; must not leak into engine)
//!
//! Sequence time axis = [`graph::Graph::edges_in_declaration_order`] (no `Edge::seq`).

pub mod attr;
pub mod contract;
pub mod geometry;
pub mod graph;
pub mod port;
pub mod profile;
pub mod render;
pub mod result;
