//! Plotgram layout and edge routing engine.
//!
//! Architecture follows yFiles principles:
//! - Layout: places nodes (hierarchical, tree, circular, sequence, …)
//! - Routing: computes edge geometry with frozen nodes (orthogonal, organic, straight, …)

pub mod layout;
pub mod route;
