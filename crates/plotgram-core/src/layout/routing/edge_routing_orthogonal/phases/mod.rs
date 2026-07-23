//! `route_edges_orthogonal_inner` 总控调用的各 phase 实现。
//!
//! 从 `run.rs` 拆出（A4）；每个子模块承担一个流水线阶段，行为与拆前一致。

mod build;
mod finalize;
mod port_correction;
mod port_slot;
mod refine;
mod trunk;

pub(crate) use build::phase_route_edges;
pub(crate) use finalize::phase_sanitize;
pub(crate) use port_correction::phase_port_correction;
pub(crate) use port_slot::{aligned_fanin_target_port, phase_port_slot};
pub(crate) use refine::{
    phase_lane, phase_layer_order, phase_reroute, phase_straighten_align, phase_stub_fix,
};
pub(crate) use trunk::{extract_protected_vertical_trunks, phase_reroute_feedback_after_trunk};
