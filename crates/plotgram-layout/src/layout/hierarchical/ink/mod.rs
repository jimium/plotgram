//! Ink phase: pure Plan+Metric expansion into orthogonal polylines, plus the
//! bypass writers that never rank/order-participate: the self-loop stub
//! (`selfloop.rs`) and zero-span undirected side-links (`intralayer.rs`).

pub mod intralayer;
pub mod route;
pub mod selfloop;
pub mod verify;
