//! 路由压力与空间需求模型：band / corridor / port / EdgeDifficulty / SpaceBudget。
//!
//! 布局加缝、诊断 dump、B2 报告共用同一套数字；本模块**不改几何**。
//! 写权仍走 `SpaceBudget` / pipeline。

pub mod band;
mod corridor;
mod dump;
mod features;
mod grid;
mod pierce;
mod port;
pub mod space_budget;
pub mod space_budget_guard;
mod types;

pub use corridor::{
    compute_corridor_model, corridor_capacity_v1, find_corridor_chain, CorridorModel,
    CORRIDOR_LANE_PITCH,
};
pub use dump::PressureSnapshot;
pub use features::{
    cluster_ranks_by_y, collect_edge_features, resolve_ranks, score_edge, score_edges,
    DifficultyProfile,
};
pub use grid::{
    compute_grid_demand, compute_grid_demand_from_result,
    edge_grid_overflows, GridDemand, GRID_DEMAND_PITCH, GRID_SOFT_CAP,
};
pub use pierce::{obstacle_hits_for_edge, preferred_l_skeleton};
pub use port::{aggregate_port_pressure, preferred_exit_side};
pub use types::{BandDemand, CorridorDemand, CorridorRisk, EdgeFeatures, PortPressure};

pub use band::{
    edge_band_demand, EdgeBandDemandBreakdown, EdgeBandDemandProfile,
};
