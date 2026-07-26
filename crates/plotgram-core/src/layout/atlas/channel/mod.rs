//! 抽象通道图（22 号文 §5.1 I.6）：**在没有坐标的抽象通道图上选路**。
//!
//! Hierarchical 生产路径经 `channel_metric` 消费本模块；非 Hier 图种仍可走
//! `layout::routing`（Wave3 记债）。
//!
//! 设计约束（详见 [README](./README.md)）：
//! 1. **零几何**：无 `Point` / `Rect` / 像素坐标，只有 id 与拓扑
//! 2. **合法性在构建期拒绝**：穿组（H2）与斜线（H3）不可表达，而非事后检测
//! 3. **确定性**：`BTreeMap` + 显式排序键（AGENTS.md §2）
//! 4. **lane 数是输出**：轨道占用无上限，导出 Demand 交度量相撑开（B1）

pub mod bundle;
pub mod derive;
pub mod graph;
pub mod search;
pub mod substrate;
pub mod verify;

pub use bundle::{Bundle, detect_bundles};
pub use derive::{
    BlueprintIndex, ChannelBlueprint, DeriveError, DerivePortsOptions, GroupSpec, NodeSpec,
    SegmentRef, derive_node_ports, derive_substrate,
};
pub use graph::{ChannelGraph, EndpointError, Occupancy, Transition, Via};
pub use search::{RouteOutcome, ScopeMask, route, route_candidates, route_node_sides};
pub use substrate::{
    EdgeId, Gate, GateCapacity, GateId, GateSide, GroupId, GroupScope, NodeKey,
    PenetrationViolation, PortSide, PortSlot, PortSlotId, Substrate, SubstrateError, Track,
    TrackId, TrackOrient,
};
pub use verify::{RouteScopeViolation, verify_route_scope};

#[cfg(test)]
mod tests;
