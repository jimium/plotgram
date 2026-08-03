//! Compose phase: FAS, ranking, properify, ordering, port finalize.
//! Produces the [`super::model::PlanGraph`] every downstream phase reads.

pub mod cycle;
pub mod graph_index;
pub mod order;
pub mod ports;
pub mod properify;
pub mod rank;
