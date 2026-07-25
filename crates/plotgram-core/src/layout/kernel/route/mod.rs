//! 路由内核 IR（Phase 1）：与 [`super::coordinate`] 同构的路由问题模型。
//!
//! **本阶段不接线生产**。生产路径仍走 `layout::routing`；本模块提供：
//! - `RouteProblem` IR（变量 / 硬约束 / 分层目标）
//! - `ResourceGraph` 骨架（`BTreeMap`，确定性）
//! - H0–H6 可行性判定器（薄封装既有几何检查）
//! - 离线校验：对当前生产解反向检查硬约束违反
//!
//! ## 设计原则（对齐 coordinate kernel）
//!
//! 1. **无图类型语义**：不读 `DiagramType`
//! 2. **确定性**：`BTreeMap` / 显式排序键
//! 3. **硬约束与软目标分离**：硬约束违反 = 无解，不是「很贵」

pub mod auditor;
pub mod capacity;
pub mod feasibility;
pub mod graph;
pub mod model;
pub mod offline;
pub mod search;

pub use auditor::{RouteHardAuditReport, RouteHardViolation};
pub use capacity::{
    PORT_SIDE_CAPACITY, all_port_candidates, compile_min_separation_constraints,
    compile_port_capacity_constraints, id_to_port, paths_violate_min_separation,
    port_capacity_overloads, port_side_resource_id, port_to_id,
};
pub use feasibility::{HardConstraintKind, check_edge_hard_constraints};
pub use graph::{ResourceGraph, ResourceId, ResourceVertex, ResourceVertexId, ResourceVertexKind};
pub use model::{
    ConstraintSource, ConstraintSourceKind, EdgeId, EdgeVariable, LexCost, NodeId, ObstacleId,
    OrderedF64, PortId, RouteHardConstraint, RouteObjective, RouteObjectiveKind, RouteProblem,
    RouteSolverConfig, SolverStatus,
};
pub use offline::{OfflineViolationSummary, audit_layout_result, problem_signature};
pub use search::{SearchResult, lex_astar};
