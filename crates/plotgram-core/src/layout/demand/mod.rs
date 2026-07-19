//! 路由压力只读模型：band / corridor / port / EdgeDifficulty。
//!
//! 布局加缝、诊断 dump、B2 报告共用同一套数字；本模块**不改几何**。
//! 写权仍走 `SpaceBudget` / pipeline。

mod corridor;
mod dump;
mod features;
mod grid;
mod pierce;
mod port;
mod types;

pub use corridor::{
    compute_corridor_model, corridor_capacity_v1, find_corridor_chain, CorridorModel,
    CORRIDOR_LANE_PITCH,
};
pub use dump::{
    dump_edge_difficulty_if_enabled, dump_pre_route_pressure_if_enabled,
    log_pressure_snapshot_for_pre_route, PressureSnapshot,
};
pub use features::{
    cluster_ranks_by_y, collect_edge_features, resolve_ranks, score_edge, score_edges,
    DifficultyProfile,
};
pub use grid::{
    compute_grid_demand, compute_grid_demand_from_result, dump_grid_demand_if_enabled,
    edge_grid_overflows, GridDemand, GRID_DEMAND_PITCH, GRID_SOFT_CAP,
};
pub use pierce::{obstacle_hits_for_edge, preferred_l_skeleton};
pub use port::{aggregate_port_pressure, preferred_exit_side};
pub use types::{BandDemand, CorridorDemand, CorridorRisk, EdgeFeatures, PortPressure};

pub use crate::layout::edge_band_demand::{
    edge_band_demand, EdgeBandDemandBreakdown, EdgeBandDemandProfile,
};
