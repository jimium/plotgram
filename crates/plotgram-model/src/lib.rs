//! Plotgram data model.
//!
//! Core types shared across all crates:
//! - `geometry`: Point, Rect primitives
//! - `attr`: AttrValue, AttrMap (free-form attribute maps)
//! - `graph`: Node (role / host_group / anchor / partition_cell), Edge, Group, Graph, Arrow
//! - `partition`: PartitionGrid / PartitionCell (ADR-008 orthogonal swimlanes / matrix)
//! - `port`: Side, PortConstraint, PortRef (dsl-spec §7.4)
//! - `contract`: AlgorithmRef, LayoutContract (engine entry — no profile name)
//! - `sizes`: NodeSizes (preferred sizes measured before layout)
//! - `result`: LayoutResult (geometry; EdgePlacement carries resolved PortRef)
//! - `render`: RenderMeta, RenderInput (renderer entry — graph + layout + chrome)
//! - `profile`: DiagramType, Profile (DSL-layer only; must not leak into engine)
//! - `archetype`: ArchetypeDef, ARCHETYPES (named shape×variant×icon packs; fill-only expansion)
//!
//! Sequence time axis = [`graph::Graph::edges_in_declaration_order`] (no `Edge::seq`).

pub mod archetype;
pub mod attr;
pub mod contract;
pub mod geometry;
pub mod graph;
pub mod partition;
pub mod port;
pub mod profile;
pub mod render;
pub mod result;
pub mod sizes;

pub use sizes::{MissingNodeSize, NodeSizes};
