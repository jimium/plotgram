//! `RouteProblem` IR —— 路由求解的唯一输入（对标 [`crate::layout::kernel::coordinate::model::CoordinateProblem`]）。
//!
//! Phase 1：类型与签名就位；求解器本体在 Phase 2 接入。

use super::graph::ResourceGraph;
use std::fmt;

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

/// 可比较的有序 f64（NaN 视为最大，保证确定性全序）。
#[derive(Debug, Clone, Copy)]
pub struct OrderedF64(pub f64);

impl PartialEq for OrderedF64 {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}

impl Eq for OrderedF64 {}

impl PartialOrd for OrderedF64 {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OrderedF64 {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

/// 词典序代价：高位优先，低位不得破坏高位。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct LexCost {
    /// Q1：硬约束残差（理想为 0；>0 表示降级解）。
    pub q1_hard_residual: OrderedF64,
    pub q2_crossings: u32,
    pub q3_bends: u32,
    pub q4_length: OrderedF64,
    pub q5_alignment: OrderedF64,
    pub q6_symmetry: OrderedF64,
}

impl Default for LexCost {
    fn default() -> Self {
        Self {
            q1_hard_residual: OrderedF64(0.0),
            q2_crossings: 0,
            q3_bends: 0,
            q4_length: OrderedF64(0.0),
            q5_alignment: OrderedF64(0.0),
            q6_symmetry: OrderedF64(0.0),
        }
    }
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

/// 求解状态（对齐 coordinate `SolverStatus`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolverStatus {
    Converged,
    Degraded,
    Infeasible,
}

impl fmt::Display for SolverStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Converged => write!(f, "converged"),
            Self::Degraded => write!(f, "degraded"),
            Self::Infeasible => write!(f, "infeasible"),
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
