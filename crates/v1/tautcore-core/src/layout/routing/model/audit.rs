//! `RouteAuditor`：几何冻结前的**只读** hard 审计（doc16 §4.6 / §5.4 / R2 Slice 2）。
//!
//! auditor 消费 [`MaterializedRouteGeometry`]（只读），产出 [`AuditReport`]。审计通过后
//! 才能推进到 [`AuditedRouteGeometry`] typestate；失败时（doc16 §5.4）Coordinator 应编译
//! repair intents 回到 solver model 重解，**不得**直接改最终 points。
//!
//! ## Slice 2 范围
//!
//! 本 Slice 落地一组**通用几何完整性** hard check（不含图名特判，符合 AGENTS.md §5）：
//!
//! - 端点/折点无 `NaN`/`Inf`。
//! - 折线至少 2 点；直线/贝塞尔两端不重合（退化端点）。
//! - 折点数上限（防御性，捕获失控几何）。
//!
//! 穿组（`edge_crosses_group_interior`）/ 穿节点等 obstacle-model hard check 由
//! Slice E1 的 [`RouteAuditor::audit_extended`] 承担（只读，消费 frozen nodes/groups +
//! annotations）；违规编译为 repair intent 交 Coordinator 固定轮次再解（E2/E3）。

use super::materialize::{AuditedRouteGeometry, MaterializedRouteGeometry};
use super::repair::RouteConstraintId;
use super::solution::EmptyRouteReason;
use super::stable_edge::StableEdgeId;
use crate::layout::geometry::Point;
use crate::layout::routing::route_annotation::{EdgeRouteAnnotation, RouteAnnotationSet};
use crate::layout::types::{GroupLayout, NodeLayout, PathGeometry, Port};
use std::collections::{HashMap, HashSet};

/// 单条违规记录。
#[derive(Debug, Clone, PartialEq)]
pub struct AuditViolation {
    pub edge: StableEdgeId,
    pub kind: ViolationKind,
    /// 违规涉及的资源标识（穿越的节点/分组 id；E2 编译 `forbidden_resources` 消费）。
    pub resource: Option<String>,
}

/// 违规类型（通用几何完整性 + Slice E1 扩展 hard check）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViolationKind {
    /// 坐标含 NaN / Inf。
    NonFinite,
    /// 折线点数不足（< 2）。
    TooFewPoints,
    /// 端点重合（零长度路径，非空路径场景）。
    DegenerateEndpoints,
    /// 折点数超过防御性上限。
    TooManyPoints,
    /// 端点未落在端点节点边界带内（E1）。
    EndpointBoundary,
    /// 首末段方向与 port 不一致 / 受保护 stub 被缩短（E1）。
    StubDirection,
    /// 正交 family 存在非轴对齐段（E1）。
    NonOrthogonalSegment,
    /// 穿越非端点节点内部（E1，与 lint `edge_through_node` 同语义）。
    ThroughNode,
    /// 穿越无关分组内部（E1，与 lint `edge_crosses_group_interior` 同语义）。
    GroupInterior,
    /// trunk 共享段与 merge annotation 不一致（E1）。
    MergeInconsistent,
}

impl ViolationKind {
    /// 映射到被违反的路由约束（E2 编译 repair intent 消费）。
    ///
    /// 通用几何完整性违规（NonFinite 等）是内部不变量破坏，不走 repair，返回 `None`。
    pub fn constraint_id(self) -> Option<RouteConstraintId> {
        match self {
            ViolationKind::NonFinite
            | ViolationKind::TooFewPoints
            | ViolationKind::DegenerateEndpoints
            | ViolationKind::TooManyPoints => None,
            ViolationKind::EndpointBoundary => Some(RouteConstraintId::EndpointBoundary),
            ViolationKind::StubDirection => Some(RouteConstraintId::StubDirection),
            ViolationKind::NonOrthogonalSegment => {
                Some(RouteConstraintId::NonOrthogonalSegment)
            }
            ViolationKind::ThroughNode => Some(RouteConstraintId::EdgeThroughNode),
            ViolationKind::GroupInterior => Some(RouteConstraintId::EdgeCrossesGroupInterior),
            ViolationKind::MergeInconsistent => Some(RouteConstraintId::MergeInconsistent),
        }
    }
}

/// 审计报告。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AuditReport {
    pub violations: Vec<AuditViolation>,
}

impl AuditReport {
    /// 无 hard 违规即通过。
    pub fn passed(&self) -> bool {
        self.violations.is_empty()
    }

    /// 人类可读摘要（诊断日志用）。
    pub fn describe(&self) -> String {
        if self.violations.is_empty() {
            "clean".to_string()
        } else {
            format!("{} violation(s)", self.violations.len())
        }
    }
}

/// 只读几何审计器（doc16 §4.6：auditor 只读）。
pub struct RouteAuditor;

/// 折点数防御性上限（捕获失控几何；正常正交路径远小于此）。
const MAX_POLYLINE_POINTS: usize = 4096;

/// E1：端点须落在节点边界带内的容差（px；覆盖 snap 量化与 dock 偏移）。
const ENDPOINT_BOUNDARY_BAND: f64 = 8.0;
/// E1：轴对齐判定容差（px）。
const AXIS_EPS: f64 = 0.01;
/// E1：merge interval 与 trunk 段贴合的 coord 容差（px）。
const MERGE_COORD_TOL: f64 = 2.0;
/// E1：受保护 stub 允许的缩短容差（px）。
const STUB_SHORTEN_TOL: f64 = 2.0;

/// Slice E1：扩展审计上下文（只读，来自 `PreparedRoutingInput` 的 frozen
/// nodes/groups + C 末冻结的旁路注解）。
pub struct RouteAuditContext<'a> {
    /// 冻结节点快照。
    pub nodes: &'a HashMap<String, NodeLayout>,
    /// 冻结分组快照。
    pub groups: &'a HashMap<String, GroupLayout>,
    /// 每条边端点节点 id（[`StableEdgeId`] 声明序）。
    pub edge_endpoints: Vec<(String, String)>,
    /// entity id → 相关分组（自身组及祖先链）；穿组检查跳过端点相关组。
    pub related_groups: HashMap<String, HashSet<String>>,
    /// 正交 family 才执行 orthogonality / endpoint / stub / merge 检查。
    pub orthogonal_family: bool,
    /// C 末冻结的旁路注解（stub / merge 一致性输入）。
    pub annotations: Option<&'a RouteAnnotationSet>,
}

impl<'a> RouteAuditContext<'a> {
    /// 从只读路由输入编译审计上下文（不读 `DiagramType`）。
    pub fn from_prepared(
        input: &'a super::prepared::PreparedRoutingInput<'a>,
        annotations: Option<&'a RouteAnnotationSet>,
    ) -> Self {
        Self::compile(
            input.nodes(),
            input.groups(),
            input.diagram,
            &input.family,
            annotations,
        )
    }

    /// 从冻结快照 + 声明语义直接编译（E3：Coordinator repair loop 入口）。
    pub fn compile(
        nodes: &'a HashMap<String, NodeLayout>,
        groups: &'a HashMap<String, GroupLayout>,
        diagram: &crate::ast::Diagram,
        family: &str,
        annotations: Option<&'a RouteAnnotationSet>,
    ) -> Self {
        // 边端点按声明序（与 StableEdgeId 同序）。
        let edge_endpoints = diagram
            .relations
            .iter()
            .map(|r| (r.from.as_str().to_string(), r.to.as_str().to_string()))
            .collect();
        // entity → 自身组及祖先链（与 lint `endpoint_related_groups` 同语义）。
        let parent_of: HashMap<&str, &str> = diagram
            .groups
            .iter()
            .filter_map(|g| g.parent_id.as_ref().map(|p| (g.id.as_str(), p.as_str())))
            .collect();
        let mut related_groups: HashMap<String, HashSet<String>> = HashMap::new();
        for entity in &diagram.entities {
            let Some(gid) = entity.group_id.as_ref() else {
                continue;
            };
            let mut set = HashSet::new();
            let mut current = Some(gid.as_str());
            while let Some(g) = current {
                if !set.insert(g.to_string()) {
                    break;
                }
                current = parent_of.get(g).copied();
            }
            related_groups.insert(entity.id.as_str().to_string(), set);
        }
        Self {
            nodes,
            groups,
            edge_endpoints,
            related_groups,
            orthogonal_family: family == "orthogonal",
            annotations,
        }
    }
}

impl RouteAuditor {
    /// 对已物化几何执行只读 hard 审计。
    ///
    /// Slice D2：声明性空边（[`EmptyRouteReason::is_declared_empty`]）是上游编译
    /// 事实，不报 `TooFewPoints`；无原因 / `Unresolved` 的空折线仍是违规。
    pub fn audit(materialized: &MaterializedRouteGeometry) -> AuditReport {
        let mut violations = Vec::new();
        for (idx, (id, geometry)) in materialized.entries().iter().enumerate() {
            if let PathGeometry::Polyline { points } = geometry {
                if points.is_empty()
                    && materialized
                        .empty_reason(idx)
                        .is_some_and(EmptyRouteReason::is_declared_empty)
                {
                    continue;
                }
            }
            Self::audit_geometry(*id, geometry, &mut violations);
        }
        AuditReport { violations }
    }

    /// 审计并在通过时推进 typestate；失败时返回报告供 Coordinator 编译 repair。
    ///
    /// 这是唯一构造 [`AuditedRouteGeometry`] 的入口——保证进入冻结前必经 auditor。
    pub fn audit_and_advance(
        materialized: MaterializedRouteGeometry,
    ) -> Result<AuditedRouteGeometry, (AuditReport, MaterializedRouteGeometry)> {
        let report = Self::audit(&materialized);
        if report.passed() {
            Ok(AuditedRouteGeometry::from_audited(materialized))
        } else {
            Err((report, materialized))
        }
    }

    fn audit_geometry(
        id: StableEdgeId,
        geometry: &PathGeometry,
        out: &mut Vec<AuditViolation>,
    ) {
        let finite = |pt: &Point| pt.x.is_finite() && pt.y.is_finite();
        match geometry {
            PathGeometry::Straight { start, end } => {
                if !finite(start) || !finite(end) {
                    out.push(AuditViolation {
                        edge: id,
                        kind: ViolationKind::NonFinite,
                        resource: None,
                    });
                } else if start == end {
                    out.push(AuditViolation {
                        edge: id,
                        kind: ViolationKind::DegenerateEndpoints,
                        resource: None,
                    });
                }
            }
            PathGeometry::Bezier {
                start,
                end,
                controls,
            } => {
                if !finite(start) || !finite(end) || controls.iter().any(|c| !finite(c)) {
                    out.push(AuditViolation {
                        edge: id,
                        kind: ViolationKind::NonFinite,
                        resource: None,
                    });
                } else if start == end {
                    out.push(AuditViolation {
                        edge: id,
                        kind: ViolationKind::DegenerateEndpoints,
                        resource: None,
                    });
                }
            }
            PathGeometry::Polyline { points } => {
                if points.iter().any(|pt| !finite(pt)) {
                    out.push(AuditViolation {
                        edge: id,
                        kind: ViolationKind::NonFinite,
                        resource: None,
                    });
                } else if points.len() < 2 {
                    // 空/单点折线：几何不足以成路径。
                    out.push(AuditViolation {
                        edge: id,
                        kind: ViolationKind::TooFewPoints,
                        resource: None,
                    });
                } else if points.len() > MAX_POLYLINE_POINTS {
                    out.push(AuditViolation {
                        edge: id,
                        kind: ViolationKind::TooManyPoints,
                        resource: None,
                    });
                }
            }
        }
    }

    /// Slice E1：扩展只读 hard 审计（obstacle-model + 正交约定）。
    ///
    /// 与通用几何完整性（[`Self::audit`]，门控 typestate）分离：本审计产出的违规
    /// 编译为 repair intent（E2）交 Coordinator 固定轮次再解（E3），不 panic。
    /// 全部检查只读；遍历序为 StableEdgeId 升序 + 节点/分组 id 升序（§2 确定性）。
    pub fn audit_extended(
        materialized: &MaterializedRouteGeometry,
        ctx: &RouteAuditContext<'_>,
    ) -> AuditReport {
        let mut violations = Vec::new();
        let mut node_ids: Vec<&String> = ctx.nodes.keys().collect();
        node_ids.sort();
        let mut group_ids: Vec<&String> = ctx.groups.keys().collect();
        group_ids.sort();

        for (idx, (id, geometry)) in materialized.entries().iter().enumerate() {
            // 声明性空边直接跳过（无几何可审）。
            if materialized.empty_reason(idx).is_some() {
                continue;
            }
            let (pts, is_curve): (Vec<Point>, bool) = match geometry {
                PathGeometry::Polyline { points } => (points.clone(), false),
                PathGeometry::Straight { start, end } => (vec![*start, *end], false),
                // 曲线族：采样折线做穿节点/穿组硬审；跳过正交约定检查。
                PathGeometry::Bezier { .. } => (geometry.sample(16), true),
            };
            if pts.len() < 2 {
                continue;
            }
            let endpoints = ctx.edge_endpoints.get(idx);
            let ann = ctx.annotations.and_then(|set| set.get(idx));
            // 已显式 degraded 的边（如 spline fallback）不重复报正交约定违规。
            let degraded = ann.is_some_and(|a| a.degraded.is_some());

            if ctx.orthogonal_family && !degraded && !is_curve {
                Self::check_orthogonality(*id, &pts, &mut violations);
                if let Some((from_id, to_id)) = endpoints {
                    Self::check_endpoint_boundary(*id, &pts, from_id, to_id, ctx, &mut violations);
                }
                if let Some(ann) = ann {
                    Self::check_stub(*id, &pts, ann, &mut violations);
                    Self::check_merge_consistency(*id, &pts, ann, &mut violations);
                }
            }
            if let Some((from_id, to_id)) = endpoints {
                Self::check_node_interior(*id, &pts, from_id, to_id, ctx, &node_ids, &mut violations);
                Self::check_group_interior(*id, &pts, from_id, to_id, ctx, &group_ids, &mut violations);
            }
        }
        AuditReport { violations }
    }

    /// E1：正交 family 全段轴对齐。
    fn check_orthogonality(id: StableEdgeId, pts: &[Point], out: &mut Vec<AuditViolation>) {
        for w in pts.windows(2) {
            let dx = (w[1].x - w[0].x).abs();
            let dy = (w[1].y - w[0].y).abs();
            if dx > AXIS_EPS && dy > AXIS_EPS {
                out.push(AuditViolation {
                    edge: id,
                    kind: ViolationKind::NonOrthogonalSegment,
                    resource: None,
                });
                return;
            }
        }
    }

    /// E1：端点落在端点节点边界带内。
    fn check_endpoint_boundary(
        id: StableEdgeId,
        pts: &[Point],
        from_id: &str,
        to_id: &str,
        ctx: &RouteAuditContext<'_>,
        out: &mut Vec<AuditViolation>,
    ) {
        for (node_id, pt) in [(from_id, pts[0]), (to_id, pts[pts.len() - 1])] {
            let Some(nl) = ctx.nodes.get(node_id) else {
                continue; // 节点不在冻结快照（如 sequence 自产边）：不审。
            };
            if Self::rect_boundary_distance(pt, nl.x, nl.y, nl.width, nl.height)
                > ENDPOINT_BOUNDARY_BAND
            {
                out.push(AuditViolation {
                    edge: id,
                    kind: ViolationKind::EndpointBoundary,
                    resource: Some(node_id.to_string()),
                });
            }
        }
    }

    /// 点到矩形边界的距离（内外均为非负）。
    fn rect_boundary_distance(p: Point, x: f64, y: f64, w: f64, h: f64) -> f64 {
        let dx_out = (x - p.x).max(p.x - (x + w)).max(0.0);
        let dy_out = (y - p.y).max(p.y - (y + h)).max(0.0);
        if dx_out > 0.0 || dy_out > 0.0 {
            (dx_out * dx_out + dy_out * dy_out).sqrt()
        } else {
            (p.x - x).min(x + w - p.x).min(p.y - y).min(y + h - p.y)
        }
    }

    /// E1：首末段方向与 port 一致 + 受保护 stub 不被缩短。
    fn check_stub(
        id: StableEdgeId,
        pts: &[Point],
        ann: &EdgeRouteAnnotation,
        out: &mut Vec<AuditViolation>,
    ) {
        let first = (pts[0], pts[1]);
        let last = (pts[pts.len() - 1], pts[pts.len() - 2]);
        // 两端均检查：段方向（从节点指向外）须与 port 外向一致。
        for (port, (a, b), guard) in [
            (ann.from_port, first, ann.stub_start.map(|s| (ann.start, s))),
            (ann.to_port, last, ann.stub_end.map(|s| (ann.end, s))),
        ] {
            let dx = b.x - a.x;
            let dy = b.y - a.y;
            if dx.abs() <= AXIS_EPS && dy.abs() <= AXIS_EPS {
                continue; // 零长首段：交给 canonicalize/完整性审计。
            }
            let dir_ok = match port {
                Port::Top => dy < -AXIS_EPS && dx.abs() <= AXIS_EPS,
                Port::Bottom => dy > AXIS_EPS && dx.abs() <= AXIS_EPS,
                Port::Left => dx < -AXIS_EPS && dy.abs() <= AXIS_EPS,
                Port::Right => dx > AXIS_EPS && dy.abs() <= AXIS_EPS,
            };
            // 受保护 stub 最短长度：首段长不得短于冻结 stub 长度减容差。
            let len_ok = match guard {
                Some((s0, s1)) => {
                    let need = ((s1.x - s0.x).powi(2) + (s1.y - s0.y).powi(2)).sqrt();
                    (dx * dx + dy * dy).sqrt() >= need - STUB_SHORTEN_TOL
                }
                None => true,
            };
            if !dir_ok || !len_ok {
                out.push(AuditViolation {
                    edge: id,
                    kind: ViolationKind::StubDirection,
                    resource: None,
                });
                return;
            }
        }
    }

    /// E1：merge annotation 声明的共享区间须有 trunk 段实际贴合。
    fn check_merge_consistency(
        id: StableEdgeId,
        pts: &[Point],
        ann: &EdgeRouteAnnotation,
        out: &mut Vec<AuditViolation>,
    ) {
        'interval: for interval in &ann.merge_intervals {
            for w in pts.windows(2) {
                let (a, b) = (w[0], w[1]);
                let (horizontal, coord, t0, t1) = if (a.y - b.y).abs() <= AXIS_EPS {
                    (true, a.y, a.x.min(b.x), a.x.max(b.x))
                } else if (a.x - b.x).abs() <= AXIS_EPS {
                    (false, a.x, a.y.min(b.y), a.y.max(b.y))
                } else {
                    continue;
                };
                if horizontal == interval.horizontal
                    && (coord - interval.coord).abs() <= MERGE_COORD_TOL
                    && t1.min(interval.t0.max(interval.t1)) - t0.max(interval.t0.min(interval.t1))
                        > 0.0
                {
                    continue 'interval; // 该区间有段贴合。
                }
            }
            out.push(AuditViolation {
                edge: id,
                kind: ViolationKind::MergeInconsistent,
                resource: interval.group_key.clone(),
            });
            return;
        }
    }

    /// E1：穿越非端点节点内部（与 lint `edge_through_node` 同语义：
    /// 段数 > 2 时跳过首末段；0.5px 容差）。
    #[allow(clippy::too_many_arguments)]
    fn check_node_interior(
        id: StableEdgeId,
        pts: &[Point],
        from_id: &str,
        to_id: &str,
        ctx: &RouteAuditContext<'_>,
        node_ids: &[&String],
        out: &mut Vec<AuditViolation>,
    ) {
        let segment_count = pts.len() - 1;
        let skip_endpoints = segment_count > 2;
        for (seg_i, w) in pts.windows(2).enumerate() {
            if skip_endpoints && (seg_i == 0 || seg_i == segment_count - 1) {
                continue;
            }
            for node_id in node_ids {
                let node_id = node_id.as_str();
                if node_id == from_id || node_id == to_id {
                    continue;
                }
                let nl = &ctx.nodes[node_id];
                if crate::layout::geometry::Rect::from(nl).segment_crosses_interior(w[0], w[1], 0.5)
                {
                    out.push(AuditViolation {
                        edge: id,
                        kind: ViolationKind::ThroughNode,
                        resource: Some(node_id.to_string()),
                    });
                    return;
                }
            }
        }
    }

    /// E1：穿越无关分组内部（与 lint `edge_crosses_group_interior` 同语义）。
    #[allow(clippy::too_many_arguments)]
    fn check_group_interior(
        id: StableEdgeId,
        pts: &[Point],
        from_id: &str,
        to_id: &str,
        ctx: &RouteAuditContext<'_>,
        group_ids: &[&String],
        out: &mut Vec<AuditViolation>,
    ) {
        if ctx.groups.is_empty() {
            return;
        }
        let empty = HashSet::new();
        let from_related = ctx.related_groups.get(from_id).unwrap_or(&empty);
        let to_related = ctx.related_groups.get(to_id).unwrap_or(&empty);
        for gid in group_ids {
            if from_related.contains(gid.as_str()) || to_related.contains(gid.as_str()) {
                continue;
            }
            let gl = &ctx.groups[gid.as_str()];
            for w in pts.windows(2) {
                if crate::layout::routing::common::geom_obstacle::segment_pierces_group_interior(
                    w[0], w[1], gl,
                ) {
                    out.push(AuditViolation {
                        edge: id,
                        kind: ViolationKind::GroupInterior,
                        resource: Some(gid.to_string()),
                    });
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::materialize::GeometryMaterializer;
    use super::super::solution::{OrthogonalPath, RoutePath, RouteSolution, StraightPath};
    use crate::layout::geometry::Point;

    fn p(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    fn materialize(paths: Vec<RoutePath>) -> MaterializedRouteGeometry {
        let mut sol = RouteSolution::default();
        sol.paths = paths;
        GeometryMaterializer::materialize(&sol)
    }

    #[test]
    fn clean_geometry_passes() {
        let m = materialize(vec![
            RoutePath::Straight(StraightPath {
                start: p(0.0, 0.0),
                end: p(10.0, 0.0),
            }),
            RoutePath::Orthogonal(OrthogonalPath {
                points: vec![p(0.0, 0.0), p(0.0, 5.0), p(5.0, 5.0)],
            }),
        ]);
        let report = RouteAuditor::audit(&m);
        assert!(report.passed(), "clean geometry must pass: {report:?}");
        assert!(RouteAuditor::audit_and_advance(m).is_ok());
    }

    #[test]
    fn degenerate_straight_is_flagged() {
        let m = materialize(vec![RoutePath::Straight(StraightPath {
            start: p(3.0, 3.0),
            end: p(3.0, 3.0),
        })]);
        let report = RouteAuditor::audit(&m);
        assert!(!report.passed());
        assert_eq!(report.violations[0].kind, ViolationKind::DegenerateEndpoints);
    }

    #[test]
    fn non_finite_is_flagged() {
        let m = materialize(vec![RoutePath::Straight(StraightPath {
            start: p(0.0, 0.0),
            end: p(f64::NAN, 0.0),
        })]);
        let report = RouteAuditor::audit(&m);
        assert!(!report.passed());
        assert_eq!(report.violations[0].kind, ViolationKind::NonFinite);
    }

    #[test]
    fn undeclared_empty_polyline_is_too_few_points() {
        // Unresolved（solver 失败占位）不属声明性空边，仍是违规。
        let m = materialize(vec![RoutePath::Empty(
            super::super::solution::EmptyRouteReason::Unresolved,
        )]);
        let report = RouteAuditor::audit(&m);
        assert!(!report.passed());
        assert_eq!(report.violations[0].kind, ViolationKind::TooFewPoints);
        // 审计失败时不推进 typestate，返回原 materialized 供 repair。
        assert!(RouteAuditor::audit_and_advance(m).is_err());
    }

    #[test]
    fn declared_empty_polyline_passes_audit() {
        // Slice D2：声明性空边（Suppressed / MissingEndpoint / Degenerate）合法放行。
        use super::super::solution::EmptyRouteReason;
        for reason in [
            EmptyRouteReason::Suppressed,
            EmptyRouteReason::MissingEndpoint,
            EmptyRouteReason::Degenerate,
        ] {
            let m = materialize(vec![RoutePath::Empty(reason)]);
            let report = RouteAuditor::audit(&m);
            assert!(report.passed(), "声明性空边 {reason:?} 必须通过审计: {report:?}");
            assert!(RouteAuditor::audit_and_advance(m).is_ok());
        }
    }

    #[test]
    fn report_describe_reflects_state() {
        assert_eq!(AuditReport::default().describe(), "clean");
    }

    // ─── Slice E1：audit_extended ───

    use crate::layout::types::{GroupLayout, NodeLayout};
    use std::collections::HashMap;

    fn node(x: f64, y: f64, w: f64, h: f64) -> NodeLayout {
        NodeLayout {
            x,
            y,
            width: w,
            height: h,
            ..Default::default()
        }
    }

    fn ext_ctx<'a>(
        nodes: &'a HashMap<String, NodeLayout>,
        groups: &'a HashMap<String, GroupLayout>,
        endpoints: Vec<(String, String)>,
        orthogonal: bool,
    ) -> RouteAuditContext<'a> {
        RouteAuditContext {
            nodes,
            groups,
            edge_endpoints: endpoints,
            related_groups: HashMap::new(),
            orthogonal_family: orthogonal,
            annotations: None,
        }
    }

    #[test]
    fn extended_flags_non_orthogonal_segment() {
        let nodes = HashMap::new();
        let groups = HashMap::new();
        let m = materialize(vec![RoutePath::Orthogonal(OrthogonalPath {
            points: vec![p(0.0, 0.0), p(10.0, 10.0)],
        })]);
        let ctx = ext_ctx(&nodes, &groups, vec![], true);
        let report = RouteAuditor::audit_extended(&m, &ctx);
        assert_eq!(report.violations[0].kind, ViolationKind::NonOrthogonalSegment);
        // 非正交 family 不审轴对齐。
        let ctx2 = ext_ctx(&nodes, &groups, vec![], false);
        assert!(RouteAuditor::audit_extended(&m, &ctx2).passed());
    }

    #[test]
    fn extended_flags_through_node_and_resource() {
        let mut nodes = HashMap::new();
        nodes.insert("a".to_string(), node(0.0, 0.0, 20.0, 20.0));
        nodes.insert("b".to_string(), node(100.0, 0.0, 20.0, 20.0));
        nodes.insert("blocker".to_string(), node(50.0, 0.0, 20.0, 20.0));
        let groups = HashMap::new();
        // 单段直穿 blocker 中部（段数 ≤ 2 不跳首末段）。
        let m = materialize(vec![RoutePath::Orthogonal(OrthogonalPath {
            points: vec![p(20.0, 10.0), p(100.0, 10.0)],
        })]);
        let ctx = ext_ctx(
            &nodes,
            &groups,
            vec![("a".to_string(), "b".to_string())],
            true,
        );
        let report = RouteAuditor::audit_extended(&m, &ctx);
        let v = report
            .violations
            .iter()
            .find(|v| v.kind == ViolationKind::ThroughNode)
            .expect("必须报 ThroughNode");
        assert_eq!(v.resource.as_deref(), Some("blocker"));
        assert_eq!(
            v.kind.constraint_id(),
            Some(super::RouteConstraintId::EdgeThroughNode)
        );
    }

    #[test]
    fn extended_flags_group_interior_but_skips_related() {
        let mut nodes = HashMap::new();
        nodes.insert("a".to_string(), node(0.0, 0.0, 20.0, 20.0));
        nodes.insert("b".to_string(), node(200.0, 0.0, 20.0, 20.0));
        let mut groups = HashMap::new();
        groups.insert(
            "g".to_string(),
            GroupLayout {
                x: 60.0,
                y: -20.0,
                width: 60.0,
                height: 60.0,
                ..Default::default()
            },
        );
        let m = materialize(vec![RoutePath::Orthogonal(OrthogonalPath {
            points: vec![p(20.0, 10.0), p(200.0, 10.0)],
        })]);
        let mut ctx = ext_ctx(
            &nodes,
            &groups,
            vec![("a".to_string(), "b".to_string())],
            true,
        );
        let report = RouteAuditor::audit_extended(&m, &ctx);
        assert!(report
            .violations
            .iter()
            .any(|v| v.kind == ViolationKind::GroupInterior
                && v.resource.as_deref() == Some("g")));
        // 端点相关组不报。
        ctx.related_groups.insert(
            "a".to_string(),
            std::collections::HashSet::from(["g".to_string()]),
        );
        let report = RouteAuditor::audit_extended(&m, &ctx);
        assert!(!report
            .violations
            .iter()
            .any(|v| v.kind == ViolationKind::GroupInterior));
    }

    #[test]
    fn extended_flags_endpoint_off_boundary() {
        let mut nodes = HashMap::new();
        nodes.insert("a".to_string(), node(0.0, 0.0, 20.0, 20.0));
        nodes.insert("b".to_string(), node(100.0, 0.0, 20.0, 20.0));
        let groups = HashMap::new();
        // 起点远离 a 边界带（x=50 距 a 右缘 30px）。
        let m = materialize(vec![RoutePath::Orthogonal(OrthogonalPath {
            points: vec![p(50.0, 10.0), p(100.0, 10.0)],
        })]);
        let ctx = ext_ctx(
            &nodes,
            &groups,
            vec![("a".to_string(), "b".to_string())],
            true,
        );
        let report = RouteAuditor::audit_extended(&m, &ctx);
        assert!(report
            .violations
            .iter()
            .any(|v| v.kind == ViolationKind::EndpointBoundary
                && v.resource.as_deref() == Some("a")));
    }

    #[test]
    fn extended_clean_orthogonal_route_passes() {
        let mut nodes = HashMap::new();
        nodes.insert("a".to_string(), node(0.0, 0.0, 20.0, 20.0));
        nodes.insert("b".to_string(), node(100.0, 40.0, 20.0, 20.0));
        let groups = HashMap::new();
        let m = materialize(vec![RoutePath::Orthogonal(OrthogonalPath {
            points: vec![p(20.0, 10.0), p(60.0, 10.0), p(60.0, 50.0), p(100.0, 50.0)],
        })]);
        let ctx = ext_ctx(
            &nodes,
            &groups,
            vec![("a".to_string(), "b".to_string())],
            true,
        );
        let report = RouteAuditor::audit_extended(&m, &ctx);
        assert!(report.passed(), "干净正交路径必须通过: {report:?}");
    }
}
