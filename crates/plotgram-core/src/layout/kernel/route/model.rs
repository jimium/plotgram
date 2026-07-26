//! `RouteProblem` IR —— 路由求解的唯一输入（对标 [`crate::layout::kernel::coordinate::model::CoordinateProblem`]）。
//!
//! Phase 1：类型与签名就位；求解器本体在 Phase 2 接入。

use super::graph::ResourceGraph;

// 共享代价词汇现居中立模块 [`crate::layout::kernel::cost`]（不随本旧管线删除）；
// 此处重导出以保持 `route::model::LexCost` 等既有路径可用。
pub use crate::layout::kernel::cost::{LexCost, OrderedF64, SolverStatus};

/// 边 ID：声明序下标（与 `StableEdgeId` / relations 下标对齐）。
pub type EdgeId = usize;

/// 节点稳定 ID（entity id）。
pub type NodeId = String;

/// 障碍 ID（节点或分组的稳定 id）。
pub type ObstacleId = String;

/// 端口侧标识（与 [`crate::layout::types::Port`] 数值对齐，IR 层用 u8 避免耦合）。
pub type PortId = u8;

/// 硬约束来源（可归因诊断）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstraintSourceKind {
    /// 端点边界 / stub 方向（H0）。
    EndpointGeometry,
    /// 节点障碍（H1）。
    NodeObstacle,
    /// 分组内部穿越（H2）。
    GroupInterior,
    /// 正交性（H3）。
    Orthogonality,
    /// 资源容量（H4）。
    ResourceCapacity,
    /// 并行/反向边间距（H5）。
    EdgeSeparation,
    /// 审计回流的禁行资源。
    AuditFeedback,
}

/// 硬约束来源（带诊断信息）。
#[derive(Debug, Clone)]
pub struct ConstraintSource {
    pub kind: ConstraintSourceKind,
    /// 涉及的边/节点/资源 id（诊断用）。
    pub entities: Vec<String>,
    pub note: &'static str,
}

/// P0 硬约束：定义可行域；违反 = 无解，不是「很贵」。
#[derive(Debug, Clone)]
pub enum RouteHardConstraint {
    /// 边不得穿过障碍（节点实体、非成员组内部）。
    ObstacleClearance {
        edge: EdgeId,
        obstacle: ObstacleId,
        min: f64,
        source: ConstraintSource,
    },
    /// 端点必须落在节点边界上、且 stub 方向与端口法向一致。
    EndpointOnBoundary {
        edge: EdgeId,
        node: NodeId,
        port: PortId,
        source: ConstraintSource,
    },
    /// 资源容量：同一通道/端口侧的并发占用上限。
    ResourceCapacity {
        resource: super::graph::ResourceId,
        capacity: u32,
        source: ConstraintSource,
    },
    /// 并行/反向边最小间距。
    MinSeparation {
        a: EdgeId,
        b: EdgeId,
        distance: f64,
        source: ConstraintSource,
    },
    /// 由硬审计违规回流生成的禁行约束。
    ForbiddenResource {
        edge: EdgeId,
        resource: super::graph::ResourceId,
        source: ConstraintSource,
    },
}

/// 软目标种类（词典序分层）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RouteObjectiveKind {
    /// Q2：边交叉。
    Crossings,
    /// Q3：弯折数。
    Bends,
    /// Q4：路径长度。
    Length,
    /// Q5：通道对齐 / 干线共享。
    Alignment,
    /// Q6：对称性。
    Symmetry,
}

/// 软目标项。
#[derive(Debug, Clone)]
pub struct RouteObjective {
    pub kind: RouteObjectiveKind,
    pub weight: f64,
    pub note: &'static str,
}

/// 单条边的路由变量。
#[derive(Debug, Clone)]
pub struct EdgeVariable {
    pub edge: EdgeId,
    pub from_node: NodeId,
    pub to_node: NodeId,
    /// 候选源端口（Port 数值）；空 = 未约束。
    pub from_port_candidates: Vec<PortId>,
    /// 候选宿端口。
    pub to_port_candidates: Vec<PortId>,
}

/// 求解器配置（Phase 1 占位）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RouteSolverConfig {
    pub max_repair_rounds: usize,
}

impl Default for RouteSolverConfig {
    fn default() -> Self {
        Self {
            max_repair_rounds: 2,
        }
    }
}

/// 路由问题：求解器的唯一输入。
#[derive(Debug, Clone)]
pub struct RouteProblem {
    /// 资源图（Phase 1 最小骨架；Phase 2 取代 corridor/OVG）。
    pub graph: ResourceGraph,
    /// 每条边一个路由变量（声明序）。
    pub edges: Vec<EdgeVariable>,
    /// P0 硬约束。
    pub hard: Vec<RouteHardConstraint>,
    /// 分层软目标。
    pub objectives: Vec<RouteObjective>,
    pub config: RouteSolverConfig,
}

impl RouteProblem {
    /// 边数量。
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// 确定性签名：同输入同哈希（用于 Phase 1 确定性单测）。
    pub fn signature(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        self.edge_count().hash(&mut h);
        self.graph.vertex_count().hash(&mut h);
        self.graph.edge_count().hash(&mut h);
        self.hard.len().hash(&mut h);
        for e in &self.edges {
            e.edge.hash(&mut h);
            e.from_node.hash(&mut h);
            e.to_node.hash(&mut h);
            e.from_port_candidates.hash(&mut h);
            e.to_port_candidates.hash(&mut h);
        }
        for hc in &self.hard {
            match hc {
                RouteHardConstraint::ObstacleClearance { edge, obstacle, .. } => {
                    1u8.hash(&mut h);
                    edge.hash(&mut h);
                    obstacle.hash(&mut h);
                }
                RouteHardConstraint::EndpointOnBoundary { edge, node, port, .. } => {
                    2u8.hash(&mut h);
                    edge.hash(&mut h);
                    node.hash(&mut h);
                    port.hash(&mut h);
                }
                RouteHardConstraint::ResourceCapacity {
                    resource,
                    capacity,
                    ..
                } => {
                    3u8.hash(&mut h);
                    resource.0.hash(&mut h);
                    capacity.hash(&mut h);
                }
                RouteHardConstraint::MinSeparation { a, b, .. } => {
                    4u8.hash(&mut h);
                    a.hash(&mut h);
                    b.hash(&mut h);
                }
                RouteHardConstraint::ForbiddenResource { edge, resource, .. } => {
                    5u8.hash(&mut h);
                    edge.hash(&mut h);
                    resource.0.hash(&mut h);
                }
            }
        }
        h.finish()
    }
}
