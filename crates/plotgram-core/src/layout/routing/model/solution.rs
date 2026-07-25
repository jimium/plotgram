//! `RouteSolution` + family-neutral `RoutePath` skeleton（doc16 §4.5 / R2 Slice 2）。
//!
//! `RouteSolution` 是 solver 阶段的**结构化解**：端口分配、family-neutral 路径骨架、lane /
//! bundle 分配、annotation、诊断与打分。它**不含几何写权**——把 `RoutePath` 物化为最终
//! `PathGeometry`（points/controls）是 [`super::materialize::GeometryMaterializer`] 的唯一职责。
//!
//! ## 设计红线（AGENTS.md / doc16）
//!
//! - §4.5：geometry family 改变（如 Bezier 穿障退化为折线）**必须**记录
//!   [`DegradedReason`]，禁止「偷偷变 Polyline」。
//! - §2 确定性：所有集合按 [`StableEdgeId`]（声明序）显式排序，不依赖 HashMap 迭代顺序。

use super::stable_edge::StableEdgeId;
use crate::layout::geometry::Point;
use crate::layout::routing::route_annotation::RouteAnnotationSet;
use crate::layout::types::Port;

/// 同侧多边的汇流策略（Slice C：从 slot.rs 迁入公共模型）。
///
/// 根据同一节点同一侧的边数自适应选择分布模式，实现"入口箭头合并"效果。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DockingStrategy {
    /// 1 条边：单点居中
    #[default]
    Single,
    /// 2-3 条边：紧凑分布（间距压缩到 16px），接近汇流但仍可区分
    Compact,
    /// 4+ 条边：汇流模式，所有边共享中心入口点，路径自然分支
    Concentrate,
}

/// 根据同侧边数选择汇流策略
pub fn choose_docking_strategy(count: usize) -> DockingStrategy {
    match count {
        0..=1 => DockingStrategy::Single,
        2..=3 => DockingStrategy::Compact,
        _ => DockingStrategy::Concentrate,
    }
}

/// solver 阶段的结构化解（doc16 §4.5）。
///
/// 该结构是 topology-solve 的产物；`paths` 为 family-neutral 骨架，尚未物化为渲染几何。
#[derive(Debug, Clone, Default)]
pub struct RouteSolution {
    /// 逐边端口分配（按声明序）。
    pub ports: Vec<EndpointAssignment>,
    /// 逐边 family-neutral 路径骨架（按声明序，与 `ports` 对齐）。
    pub paths: Vec<RoutePath>,
    /// 通道（lane）分配。
    pub lanes: Vec<LaneAssignment>,
    /// bundle（并行边归并）解。
    pub bundles: Vec<BundleSolution>,
    /// 旁路注解表（复用既有 [`RouteAnnotationSet`]）。
    pub annotations: RouteAnnotationSet,
    /// 诊断（含 family 退化原因）。
    pub diagnostics: RoutingDiagnostics,
    /// 解的打分（用于择优 / 回归观测）。
    pub score: RouteScore,
}

impl RouteSolution {
    /// 按边下标取路径骨架。
    pub fn path(&self, edge: StableEdgeId) -> Option<&RoutePath> {
        self.paths.get(edge.index())
    }

    /// 按边下标取端口分配。
    pub fn endpoint(&self, edge: StableEdgeId) -> Option<&EndpointAssignment> {
        self.ports.iter().find(|e| e.edge == edge)
    }
}

/// 逐边端口分配（Slice C1 完整化：side + slot + anchor + capacity + protected stub）。
#[derive(Debug, Clone, PartialEq)]
pub struct EndpointAssignment {
    pub edge: StableEdgeId,
    pub from_port: Port,
    pub to_port: Port,
    /// from 端锚点坐标（slot 分配结果）
    pub from_anchor: Point,
    /// to 端锚点坐标
    pub to_anchor: Point,
    /// from 端 slot 下标（同侧排序位）
    pub from_slot_index: u16,
    /// to 端 slot 下标
    pub to_slot_index: u16,
    /// from 端同侧容量（该侧总边数）
    pub from_side_capacity: u8,
    /// to 端同侧容量
    pub to_side_capacity: u8,
    /// 受保护 stub（feedback/monitor 边，sanitize 不得缩短）
    pub protected_stub: bool,
    /// 汇流策略
    pub docking: DockingStrategy,
}

impl EndpointAssignment {
    /// 构造最小实例（兼容旧代码过渡）。
    pub fn minimal(edge: StableEdgeId, from_port: Port, to_port: Port) -> Self {
        Self {
            edge,
            from_port,
            to_port,
            from_anchor: Point::zero(),
            to_anchor: Point::zero(),
            from_slot_index: 0,
            to_slot_index: 0,
            from_side_capacity: 1,
            to_side_capacity: 1,
            protected_stub: false,
            docking: DockingStrategy::Single,
        }
    }

    /// 投影为内部路径构建用的轻量 `Endpoint` 视图（Slice C1.4）。
    ///
    /// `node_id` 为该端所在的节点 ID；`target` 为对端节点中心（仅供排序参考，
    /// 路径构建不使用）。
    pub fn project_endpoint(
        &self,
        is_from: bool,
        node_id: String,
        target: Point,
    ) -> crate::layout::routing::edge_routing_orthogonal::slot::Endpoint {
        use crate::layout::routing::edge_routing_orthogonal::slot::Endpoint;
        Endpoint {
            edge_index: self.edge.index(),
            is_from,
            target_x: target.x,
            target_y: target.y,
            lane: if is_from {
                self.from_slot_index as usize
            } else {
                self.to_slot_index as usize
            },
            node_id,
            side: if is_from {
                self.from_port
            } else {
                self.to_port
            },
            anchor: if is_from {
                self.from_anchor
            } else {
                self.to_anchor
            },
        }
    }
}

/// 通道分配（Slice C3.1 扩展：逐段 cross-axis 偏移 + 全局 lane index）。
#[derive(Debug, Clone, PartialEq)]
pub struct LaneAssignment {
    pub edge: StableEdgeId,
    /// 逐段 cross-axis 偏移（与 OrthogonalPath.points 的段对齐）。
    /// 空表示该边无偏移。
    pub segment_offsets: Vec<f64>,
    /// 全局 lane index（兼容旧逻辑，0 为中心，正负表示两侧偏移）。
    pub lane: i32,
}

/// bundle（并行/共线边归并）解（Slice C3.3 扩展：trunk 拓扑 + junction + 退化标记）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BundleSolution {
    /// 归并到同一 bundle 的边（按声明序）。
    pub members: Vec<StableEdgeId>,
    /// trunk 共享段（起止点坐标）。
    pub trunk_segments: Vec<(Point, Point)>,
    /// junction 点（分支点）。
    pub junctions: Vec<Point>,
    /// 退化标记（合流失败时保留原路径）。
    pub degraded: bool,
}

/// family-neutral 路径骨架（doc16 §4.5）。
///
/// 骨架描述「路径拓扑与关键点」，但不承诺最终渲染采样；物化由
/// [`super::materialize::GeometryMaterializer`] 完成。
#[derive(Debug, Clone, PartialEq)]
pub enum RoutePath {
    Straight(StraightPath),
    Orthogonal(OrthogonalPath),
    Cubic(CubicPath),
    Spline(SplinePath),
    Radial(RadialPath),
    Empty(EmptyRouteReason),
}

impl RoutePath {
    /// 该路径的 geometry family（用于退化记录与审计）。
    pub fn family(&self) -> GeometryFamily {
        match self {
            RoutePath::Straight(_) => GeometryFamily::Straight,
            RoutePath::Orthogonal(_) => GeometryFamily::Orthogonal,
            RoutePath::Cubic(_) => GeometryFamily::Cubic,
            RoutePath::Spline(_) => GeometryFamily::Spline,
            RoutePath::Radial(_) => GeometryFamily::Radial,
            RoutePath::Empty(_) => GeometryFamily::Empty,
        }
    }

    /// 提取路径点（Orthogonal 变体返回内部 points，其他变体返回端点）。
    pub fn points(&self) -> &[Point] {
        match self {
            RoutePath::Orthogonal(p) => &p.points,
            RoutePath::Straight(p) => std::slice::from_ref(&p.start),
            _ => &[],
        }
    }

    /// 路径是否为空（无点或 Empty 变体）。
    pub fn is_empty(&self) -> bool {
        match self {
            RoutePath::Orthogonal(p) => p.points.is_empty(),
            RoutePath::Empty(_) => true,
            _ => false,
        }
    }

    /// 路径点数。
    pub fn point_count(&self) -> usize {
        match self {
            RoutePath::Orthogonal(p) => p.points.len(),
            RoutePath::Straight(_) => 2,
            _ => 0,
        }
    }

    /// 从点集构造 Orthogonal 路径（空点集返回 Empty）。
    pub fn orthogonal(points: Vec<Point>) -> Self {
        if points.is_empty() {
            RoutePath::Empty(EmptyRouteReason::Unresolved)
        } else {
            RoutePath::Orthogonal(OrthogonalPath { points })
        }
    }
}

/// 直线段：两端点。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StraightPath {
    pub start: Point,
    pub end: Point,
}

/// 正交折线：折点序列（含首尾）。
#[derive(Debug, Clone, PartialEq)]
pub struct OrthogonalPath {
    pub points: Vec<Point>,
}

/// 三次贝塞尔：两端点 + 两控制点。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CubicPath {
    pub start: Point,
    pub end: Point,
    pub controls: [Point; 2],
}

/// 样条：采样折点序列（含首尾）。
#[derive(Debug, Clone, PartialEq)]
pub struct SplinePath {
    pub points: Vec<Point>,
}

/// 放射（mindmap/circular）：两端点 + 两控制点。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RadialPath {
    pub start: Point,
    pub end: Point,
    pub controls: [Point; 2],
}

/// 空路径原因（doc16 §4.5：不得静默产生空几何）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmptyRouteReason {
    /// 两端点重合，退化为无长度路径。
    Degenerate,
    /// 边被显式抑制（如自环折叠）。
    Suppressed,
    /// 端点节点缺失（compile 阶段声明性事实，非 solver 失败）。
    MissingEndpoint,
    /// solver 未能求解出路径（错误路径下的占位）。
    Unresolved,
}

impl EmptyRouteReason {
    /// 是否为**声明性**合法空边（Slice D2）。
    ///
    /// 声明性原因（端点缺失 / 显式抑制 / 退化端点）表示空几何是上游编译事实，
    /// auditor 放行；`Unresolved`（solver 失败占位）不属声明性——静默空几何仍是违规。
    pub fn is_declared_empty(self) -> bool {
        !matches!(self, EmptyRouteReason::Unresolved)
    }
}

/// geometry family 标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeometryFamily {
    Straight,
    Orthogonal,
    Cubic,
    Spline,
    Radial,
    Empty,
}

/// family 退化原因（doc16 §4.5）。geometry family 改变时**必须**产出。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DegradedReason {
    /// 请求的 family 因障碍无法满足，退化为 `actual`。
    ObstacleFallback {
        requested: GeometryFamily,
        actual: GeometryFamily,
    },
    /// 因端点重合退化为空路径。
    DegenerateEndpoints,
    /// 路径穿越非自身节点障碍物（Slice C2b 从 path_solver 迁入）。
    ObstacleCrossing,
    /// 路径存在与其他边的间距违规。
    SpacingViolation,
    /// 路径经过拥堵通道桶。
    Congestion,
    /// rip-up 后仍无法找到洁净路径（优雅降级保留原路径）。
    NoCleanPath,
}

/// 路由诊断。
#[derive(Debug, Clone, Default)]
pub struct RoutingDiagnostics {
    /// 逐边退化记录（按声明序）。
    pub degraded: Vec<(StableEdgeId, DegradedReason)>,
    /// 自由文本诊断（如 solver 迭代轮次说明）。
    pub notes: Vec<String>,
}

impl RoutingDiagnostics {
    pub fn is_clean(&self) -> bool {
        self.degraded.is_empty()
    }
}

/// 解的打分（越小越优；用于择优与回归观测）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RouteScore {
    /// 总墨量（路径长度和）。
    pub ink: f64,
    /// 交叉数。
    pub crossings: u32,
    /// 拐点数。
    pub bends: u32,
}

impl Default for RouteScore {
    fn default() -> Self {
        Self {
            ink: 0.0,
            crossings: 0,
            bends: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn family_matches_variant() {
        assert_eq!(
            RoutePath::Straight(StraightPath {
                start: p(0.0, 0.0),
                end: p(1.0, 1.0)
            })
            .family(),
            GeometryFamily::Straight
        );
        assert_eq!(
            RoutePath::Orthogonal(OrthogonalPath { points: vec![] }).family(),
            GeometryFamily::Orthogonal
        );
        assert_eq!(
            RoutePath::Empty(EmptyRouteReason::Degenerate).family(),
            GeometryFamily::Empty
        );
    }

    #[test]
    fn solution_lookup_by_edge() {
        let mut sol = RouteSolution::default();
        sol.paths.push(RoutePath::Straight(StraightPath {
            start: p(0.0, 0.0),
            end: p(10.0, 0.0),
        }));
        sol.ports.push(EndpointAssignment::minimal(
            StableEdgeId(0), Port::Right, Port::Left,
        ));
        assert!(matches!(
            sol.path(StableEdgeId(0)),
            Some(RoutePath::Straight(_))
        ));
        assert_eq!(
            sol.endpoint(StableEdgeId(0)).unwrap().from_port,
            Port::Right
        );
        assert!(sol.path(StableEdgeId(1)).is_none());
    }

    #[test]
    fn diagnostics_clean_by_default() {
        assert!(RoutingDiagnostics::default().is_clean());
    }
}
