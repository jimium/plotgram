//! 路由问题模型（doc16 §4 / R1 Slice 1）。
//!
//! 提供 router 的**只读、已编译**输入：`PreparedRoutingInput`。取代「router 直接解析
//! 完整 `Diagram`/`LayoutResult`」的现状，为后续 solver/materializer/auditor 拆分打基础。
//!
//! ## 模块结构
//!
//! - [`stable_edge`]：`StableEdgeId` + `StableEdgeStore`——稳定边身份（下标即声明序）。
//! - [`frozen_store`]：`FrozenNodeStore` / `FrozenGroupStore`——只读节点/分组快照。
//! - [`contract`]：富 `RoutingContract`——typed roles/intents（区别于既有极简同名类型）。
//! - [`prepared`]：`PreparedRoutingInput` + `prepare()` + problem signature。
//! - [`solution`]（Slice 2）：`RouteSolution` + family-neutral `RoutePath` 骨架。
//! - [`materialize`]（Slice 2）：`GeometryMaterializer`（geometry 唯一写者）+ geometry typestate。
//! - [`audit`]（Slice 2）：`RouteAuditor`——冻结前只读 hard 审计。
//!
//! ## 设计红线（AGENTS.md）
//!
//! - §2 确定性：所有迭代按 id / 声明序显式排序，不依赖 HashMap 迭代顺序。
//! - Kernel 不读取 `DiagramType`：`prepare`/`compile` 只消费 relations/groups/nodes 几何与声明语义。

pub mod audit;
pub mod contract;
pub mod frozen_store;
pub mod materialize;
pub mod prepared;
pub mod repair;
pub mod solution;
pub mod stable_edge;

pub use contract::{
    CircleId, CorridorResource, EdgeRole, EdgeRoleSet, LabelPolicy, MergeGroupId, MergeIntent,
    ParallelGroupId, PortIntent, RoutingContract, RoutingTopologyMetadata, SideGutterResource,
    TransitIntent,
};
pub use frozen_store::{FrozenGroupStore, FrozenNodeStore};
pub use prepared::{PreparedRoutingInput, RoutingCanvas};
pub use stable_edge::{StableEdge, StableEdgeId, StableEdgeStore};

pub use audit::{AuditReport, AuditViolation, RouteAuditContext, RouteAuditor, ViolationKind};
pub use repair::{RepairPriority, RouteAuditReport, RouteConstraintId, RouteRepairIntent};
pub use materialize::{
    AuditedRouteGeometry, FrozenRouteGeometry, GeometryMaterializer, MaterializedRouteGeometry,
};
pub use solution::{
    choose_docking_strategy, BundleSolution, CubicPath, DegradedReason, DockingStrategy,
    EmptyRouteReason, EndpointAssignment, GeometryFamily, LaneAssignment, OrthogonalPath,
    RadialPath, RoutePath, RouteScore, RouteSolution, RoutingDiagnostics, SplinePath, StraightPath,
};
