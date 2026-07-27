//! `route_edges_orthogonal_inner` 总控调用的各 phase 实现。
//!
//! 从 `run.rs` 拆出（A4）；每个子模块承担一个流水线阶段，行为与拆前一致。
//!
//! R1 加深：孤儿 `trunk`（S4 feedback 重路由，全库零 call site）已删。

mod build;
mod finalize;
mod port_slot;
mod refine;

pub(crate) use build::phase_route_edges;
pub(crate) use finalize::phase_sanitize;
pub(crate) use port_slot::{aligned_fanin_target_port, phase_port_slot};
pub(crate) use refine::{phase_lane, phase_layer_order};
