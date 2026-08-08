//! Compose phase: FAS, ranking, properify, ordering, port finalize,
//! D1.0 TrackOrder.
//! Produces the [`super::model::PlanGraph`] every downstream phase reads.

pub mod boundary;
pub mod bundle;
pub mod cycle;
pub mod graph_index;
pub mod order;
pub mod ports;
pub mod properify;
pub mod rank;
pub mod track_order;
pub mod verify;
