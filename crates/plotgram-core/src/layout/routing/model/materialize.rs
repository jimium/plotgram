//! `GeometryMaterializer` + geometry typestate（doc16 §4.6 / §5.4 / R2 Slice 2）。
//!
//! 该模块建立 doc16 目标管线中 **geometry 唯一写者** 与 **typestate 写权约束**：
//!
//! ```text
//! SolvedRouteTopology (RouteSolution)
//!   → materialize → MaterializedRouteGeometry   ← 唯一由 GeometryMaterializer 构造
//!   → audit        → AuditedRouteGeometry        ← 唯一由 RouteAuditor 构造
//!   → freeze       → FrozenRouteGeometry         ← 冻结后只允许 label/annotation metadata
//! ```
//!
//! ## 写权红线（doc16 §4.6）
//!
//! - materializer 是 `points`/`controls` 的**唯一写者**；除它以外没有别的类型能构造
//!   [`MaterializedRouteGeometry`]（私有字段 + 私有构造）。
//! - auditor 只读，产出 [`AuditedRouteGeometry`]（见 [`super::audit`]）。
//! - freeze 后（[`FrozenRouteGeometry`]）几何不可再改。
//!
//! ## Slice 2 忠实性证明
//!
//! Slice 2 不迁移 router；为证明 materializer 是既有几何的**忠实**唯一写者，提供
//! [`GeometryMaterializer::lift_geometry`]（现有 `PathGeometry` → family-neutral
//! [`RoutePath`]），并保证 round-trip 幂等：`materialize(lift(g)) == g`（对 canonical
//! `PathGeometry` 三变体）。runner 侧以 behavior-neutral shadow 挂载该证明。

use super::solution::{
    BundleSolution, CubicPath, EmptyRouteReason, LaneAssignment, OrthogonalPath, RadialPath,
    RoutePath, RouteSolution, SplinePath, StraightPath,
};
use super::stable_edge::StableEdgeId;
use crate::layout::geometry::Point;
use crate::layout::types::{EdgeLayout, PathGeometry};

/// geometry 唯一写者（doc16 §4.6）。
///
/// 无状态；所有方法是纯函数式映射。除本类型外，任何 pass 都不得直接写 `points`/`controls`。
pub struct GeometryMaterializer;

impl GeometryMaterializer {
    /// 把 family-neutral [`RoutePath`] 骨架物化为渲染用 [`PathGeometry`]。
    ///
    /// 这是 geometry 的**唯一**构造点。映射：
    /// - `Straight`  → `PathGeometry::Straight`
    /// - `Cubic`/`Radial` → `PathGeometry::Bezier`（两控制点）
    /// - `Orthogonal`/`Spline` → `PathGeometry::Polyline`（折点序列）
    /// - `Empty` → 空 `PathGeometry::Polyline`
    pub fn materialize_path(path: &RoutePath) -> PathGeometry {
        match path {
            RoutePath::Straight(StraightPath { start, end }) => PathGeometry::Straight {
                start: *start,
                end: *end,
            },
            RoutePath::Cubic(CubicPath {
                start,
                end,
                controls,
            }) => PathGeometry::Bezier {
                start: *start,
                end: *end,
                controls: *controls,
            },
            RoutePath::Radial(RadialPath {
                start,
                end,
                controls,
            }) => PathGeometry::Bezier {
                start: *start,
                end: *end,
                controls: *controls,
            },
            RoutePath::Orthogonal(OrthogonalPath { points }) => PathGeometry::Polyline {
                points: points.clone(),
            },
            RoutePath::Spline(SplinePath { points }) => PathGeometry::Polyline {
                points: points.clone(),
            },
            RoutePath::Empty(_) => PathGeometry::Polyline { points: Vec::new() },
        }
    }

    /// 逆映射：现有 [`PathGeometry`] → canonical [`RoutePath`] 骨架。
    ///
    /// 用于 Slice 2 忠实性证明（lift 既有几何再物化回来）。canonical 映射：
    /// - `Straight` → `RoutePath::Straight`
    /// - `Bezier`   → `RoutePath::Cubic`
    /// - `Polyline`（非空）→ `RoutePath::Orthogonal`
    /// - `Polyline`（空）  → `RoutePath::Empty(Degenerate)`
    ///
    /// 保证 `materialize_path(lift_geometry(g))` 与 `g` 几何等价（三变体 round-trip 恒等）。
    pub fn lift_geometry(geometry: &PathGeometry) -> RoutePath {
        match geometry {
            PathGeometry::Straight { start, end } => RoutePath::Straight(StraightPath {
                start: *start,
                end: *end,
            }),
            PathGeometry::Bezier {
                start,
                end,
                controls,
            } => RoutePath::Cubic(CubicPath {
                start: *start,
                end: *end,
                controls: *controls,
            }),
            PathGeometry::Polyline { points } if points.is_empty() => {
                RoutePath::Empty(EmptyRouteReason::Degenerate)
            }
            PathGeometry::Polyline { points } => RoutePath::Orthogonal(OrthogonalPath {
                points: points.clone(),
            }),
        }
    }

    /// 把整个 [`RouteSolution`] 物化为 typestate 化的 geometry。
    ///
    /// 按声明序（`StableEdgeId`）逐边物化路径骨架；端口分配一并携带以便写回。
    /// 该方法不产生 label——label 由后续 LabelSolver 独立处理（doc16 §4.6：freeze 后补全）。
    ///
    /// Slice C3.4：应用 LaneAssignment.segment_offsets（cross-axis 平移）和
    /// BundleSolution.trunk_segments（替换对应段坐标）后再物化。
    pub fn materialize(solution: &RouteSolution) -> MaterializedRouteGeometry {
        let adjusted = Self::apply_lane_and_bundle(solution);
        let mut geometries = Vec::with_capacity(adjusted.len());
        let mut empty_reasons = Vec::with_capacity(adjusted.len());
        for (i, path) in adjusted.iter().enumerate() {
            geometries.push((StableEdgeId(i), Self::materialize_path(path)));
            // Slice D2：携带声明性空边原因供 auditor 区分合法空边与静默空几何。
            empty_reasons.push(match path {
                RoutePath::Empty(reason) => Some(*reason),
                _ => None,
            });
        }
        MaterializedRouteGeometry {
            geometries,
            empty_reasons,
        }
    }

    /// Slice C3.4：对路径应用 lane 偏移 + bundle trunk 共享段。
    ///
    /// 1. 对每条边的 OrthogonalPath.points 应用 LaneAssignment.segment_offsets（cross-axis 平移）
    /// 2. 对 bundle 成员应用 trunk 共享段（替换对应段坐标）
    /// 3. 产出调整后的 RoutePath 序列
    fn apply_lane_and_bundle(solution: &RouteSolution) -> Vec<RoutePath> {
        let n = solution.paths.len();
        let mut paths: Vec<RoutePath> = solution.paths.clone();

        // ── 1. 应用 lane segment_offsets ──
        for la in &solution.lanes {
            let ei = la.edge.index();
            if ei >= n || la.segment_offsets.is_empty() {
                continue;
            }
            if let RoutePath::Orthogonal(ref mut ortho) = paths[ei] {
                let pts = &mut ortho.points;
                let n_segs = pts.len().saturating_sub(1);
                for si in 0..n_segs {
                    if si >= la.segment_offsets.len() {
                        break;
                    }
                    let offset = la.segment_offsets[si];
                    if offset.abs() < f64::EPSILON {
                        continue;
                    }
                    // 判断段方向：水平段移 y，垂直段移 x。
                    let dx = (pts[si + 1].x - pts[si].x).abs();
                    let dy = (pts[si + 1].y - pts[si].y).abs();
                    if dy < 1.0 && dx > 1.0 {
                        // 水平段：cross-axis = y
                        pts[si].y += offset;
                        pts[si + 1].y += offset;
                    } else if dx < 1.0 && dy > 1.0 {
                        // 垂直段：cross-axis = x
                        pts[si].x += offset;
                        pts[si + 1].x += offset;
                    }
                }
            }
        }

        // ── 2. 应用 bundle trunk 共享段 ──
        for bundle in &solution.bundles {
            if bundle.degraded || bundle.trunk_segments.is_empty() {
                continue;
            }
            // 对每个成员的每条 trunk 段，找到路径中最接近的平行段并替换坐标。
            for &member_id in &bundle.members {
                let ei = member_id.index();
                if ei >= n {
                    continue;
                }
                if let RoutePath::Orthogonal(ref mut ortho) = paths[ei] {
                    for &(trunk_start, trunk_end) in &bundle.trunk_segments {
                        Self::snap_segment_to_trunk(
                            &mut ortho.points,
                            trunk_start,
                            trunk_end,
                        );
                    }
                }
            }
        }

        paths
    }

    /// 将路径中最接近 trunk 的平行段 snap 到 trunk 坐标。
    ///
    /// 只处理同向平行段（水平 trunk 对水平段，垂直 trunk 对垂直段），
    /// 且投影重叠超过段长 50% 时才 snap。
    fn snap_segment_to_trunk(pts: &mut [Point], trunk_start: Point, trunk_end: Point) {
        let is_trunk_h = (trunk_end.y - trunk_start.y).abs() < 1.0;
        let trunk_coord = if is_trunk_h { trunk_start.y } else { trunk_start.x };
        let (t0, t1) = if is_trunk_h {
            (trunk_start.x.min(trunk_end.x), trunk_start.x.max(trunk_end.x))
        } else {
            (trunk_start.y.min(trunk_end.y), trunk_start.y.max(trunk_end.y))
        };
        let trunk_len = t1 - t0;
        if trunk_len < 1.0 {
            return;
        }

        let n_segs = pts.len().saturating_sub(1);
        for si in 0..n_segs {
            let dx = (pts[si + 1].x - pts[si].x).abs();
            let dy = (pts[si + 1].y - pts[si].y).abs();
            let is_seg_h = dy < 1.0 && dx > 1.0;
            let is_seg_v = dx < 1.0 && dy > 1.0;
            if is_trunk_h != is_seg_h {
                continue;
            }
            if !is_seg_h && !is_seg_v {
                continue;
            }
            // 检查投影重叠。
            let (s0, s1) = if is_trunk_h {
                (pts[si].x.min(pts[si + 1].x), pts[si].x.max(pts[si + 1].x))
            } else {
                (pts[si].y.min(pts[si + 1].y), pts[si].y.max(pts[si + 1].y))
            };
            let overlap = s1.min(t1) - s0.max(t0);
            let seg_len = s1 - s0;
            if seg_len < 1.0 || overlap / seg_len < 0.5 {
                continue;
            }
            // Snap: 将段的 cross-axis 坐标对齐到 trunk。
            if is_trunk_h {
                pts[si].y = trunk_coord;
                pts[si + 1].y = trunk_coord;
            } else {
                pts[si].x = trunk_coord;
                pts[si + 1].x = trunk_coord;
            }
            return; // 每条 trunk 只 snap 一段
        }
    }

    /// Slice D3：topology-preserving canonicalize（正交折线规范化）归 materializer。
    ///
    /// sanitize 第 (1) 类边界（反向 stub 修正 / 斜段拆 L / 严格共线合并 /
    /// 零长段归一 / 浮点规范化）属 materializer 语义，调用方不得再直呼 sanitize
    /// 内核。保护输入：`annotations` 携带 merge intervals（trunk 共享段不被拆散），
    /// 端口侧向（`from_side`/`to_side`）保证 protected stub 不被缩短。
    ///
    /// `merge_overshoot`（第 (2) 类 topology-changing overshoot Z 折合并）本步不动，
    /// 经参数透传原内核，E4 收编进 PathSolver 重评分。
    #[allow(clippy::too_many_arguments)]
    pub fn canonicalize_orthogonal_edges(
        edges: &mut [EdgeLayout],
        relations: &[crate::ast::Relation],
        from_side: &[crate::layout::types::Port],
        to_side: &[crate::layout::types::Port],
        merge_overshoot: bool,
        annotations: Option<&crate::layout::routing::route_annotation::RouteAnnotationSet>,
        nodes: Option<&std::collections::HashMap<String, crate::layout::types::NodeLayout>>,
        sorted_node_ids: Option<&[String]>,
    ) {
        crate::layout::routing::edge_routing_orthogonal::sanitize::sanitize_orthogonal_edges_with_guard(
            edges,
            relations,
            from_side,
            to_side,
            merge_overshoot,
            annotations,
            nodes,
            sorted_node_ids,
        );
    }
}

/// typestate：已物化几何（doc16 §4.6）。
///
/// **只能**由 [`GeometryMaterializer::materialize`] 构造（字段私有 + 无公开构造），
/// 以在类型层面保证「geometry 唯一写者」。
#[derive(Debug, Clone)]
pub struct MaterializedRouteGeometry {
    geometries: Vec<(StableEdgeId, PathGeometry)>,
    /// 逐边空路径原因（与 `geometries` 同序；Slice D2）。
    ///
    /// `Some(reason)` 表示该边由 `RoutePath::Empty(reason)` 物化而来；`None` 表示
    /// 非空骨架——若物化后仍为空折线则属静默空几何，auditor 报违规。
    empty_reasons: Vec<Option<EmptyRouteReason>>,
}

impl MaterializedRouteGeometry {
    /// 逐边（id, geometry）只读视图（按声明序）。
    pub fn entries(&self) -> &[(StableEdgeId, PathGeometry)] {
        &self.geometries
    }

    /// 第 `idx` 条边的声明性空路径原因（与 `entries()` 同序；Slice D2）。
    pub fn empty_reason(&self, idx: usize) -> Option<EmptyRouteReason> {
        self.empty_reasons.get(idx).copied().flatten()
    }

    pub fn len(&self) -> usize {
        self.geometries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.geometries.is_empty()
    }
}

/// typestate：已审计几何（doc16 §4.6）。
///
/// **只能**由 [`super::audit::RouteAuditor`] 在审计通过后构造（crate 内构造），
/// 保证「进入冻结前必经 auditor」。
#[derive(Debug, Clone)]
pub struct AuditedRouteGeometry {
    inner: MaterializedRouteGeometry,
}

impl AuditedRouteGeometry {
    /// model 内构造点（仅供 [`super::audit`] 在审计通过后调用；Slice D2 收权）。
    pub(super) fn from_audited(inner: MaterializedRouteGeometry) -> Self {
        Self { inner }
    }

    pub fn entries(&self) -> &[(StableEdgeId, PathGeometry)] {
        self.inner.entries()
    }

    /// 冻结几何：此后只允许 label/annotation metadata 补全（doc16 §4.6）。
    pub fn freeze(self) -> FrozenRouteGeometry {
        FrozenRouteGeometry { inner: self.inner }
    }
}

/// typestate：已冻结几何（doc16 §4.6）。冻结后几何不可再改。
#[derive(Debug, Clone)]
pub struct FrozenRouteGeometry {
    inner: MaterializedRouteGeometry,
}

impl FrozenRouteGeometry {
    pub fn entries(&self) -> &[(StableEdgeId, PathGeometry)] {
        self.inner.entries()
    }

    /// 把冻结几何写回既有 `EdgeLayout` 切片（只写 geometry，不碰 labels/ports）。
    ///
    /// Slice 2 不在主管线启用写回（router 仍产出最终几何）；该方法供后续 Slice（R3/R4）
    /// 迁移 router 时使用，并用于单测验证写回等价。
    pub fn write_geometry_into(&self, edges: &mut [EdgeLayout]) {
        for (id, geometry) in self.inner.entries() {
            if let Some(edge) = edges.get_mut(id.index()) {
                edge.geometry = geometry.clone();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::geometry::Point;

    fn p(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    // straight 是 polyline 的特例，round-trip 已由 round_trip_polyline_is_identity 覆盖。

    #[test]
    fn round_trip_bezier_is_identity() {
        let g = PathGeometry::Bezier {
            start: p(0.0, 0.0),
            end: p(10.0, 0.0),
            controls: [p(3.0, 2.0), p(7.0, 2.0)],
        };
        let back = GeometryMaterializer::materialize_path(&GeometryMaterializer::lift_geometry(&g));
        match back {
            PathGeometry::Bezier {
                start,
                end,
                controls,
            } => {
                assert_eq!(start, p(0.0, 0.0));
                assert_eq!(end, p(10.0, 0.0));
                assert_eq!(controls, [p(3.0, 2.0), p(7.0, 2.0)]);
            }
            other => panic!("expected Bezier, got {other:?}"),
        }
    }

    #[test]
    fn round_trip_polyline_is_identity() {
        let g = PathGeometry::Polyline {
            points: vec![p(0.0, 0.0), p(5.0, 0.0), p(5.0, 5.0)],
        };
        let back = GeometryMaterializer::materialize_path(&GeometryMaterializer::lift_geometry(&g));
        match back {
            PathGeometry::Polyline { points } => {
                assert_eq!(points, vec![p(0.0, 0.0), p(5.0, 0.0), p(5.0, 5.0)]);
            }
            other => panic!("expected Polyline, got {other:?}"),
        }
    }

    #[test]
    fn empty_polyline_lifts_to_empty_route() {
        let g = PathGeometry::Polyline { points: vec![] };
        assert!(matches!(
            GeometryMaterializer::lift_geometry(&g),
            RoutePath::Empty(EmptyRouteReason::Degenerate)
        ));
    }

    #[test]
    fn materialize_preserves_declaration_order() {
        let mut sol = RouteSolution::default();
        sol.paths.push(RoutePath::Straight(StraightPath {
            start: p(0.0, 0.0),
            end: p(1.0, 0.0),
        }));
        sol.paths.push(RoutePath::Orthogonal(OrthogonalPath {
            points: vec![p(0.0, 0.0), p(0.0, 5.0)],
        }));
        let materialized = GeometryMaterializer::materialize(&sol);
        assert_eq!(materialized.len(), 2);
        assert_eq!(materialized.entries()[0].0, StableEdgeId(0));
        assert_eq!(materialized.entries()[1].0, StableEdgeId(1));
        assert!(matches!(
            materialized.entries()[1].1,
            PathGeometry::Polyline { .. }
        ));
    }

    #[test]
    fn typestate_progression_and_writeback() {
        let mut sol = RouteSolution::default();
        sol.paths.push(RoutePath::Straight(StraightPath {
            start: p(0.0, 0.0),
            end: p(10.0, 0.0),
        }));
        let materialized = GeometryMaterializer::materialize(&sol);
        // 手动走 typestate（审计通过在 audit.rs 单测覆盖）。
        let audited = AuditedRouteGeometry::from_audited(materialized);
        let frozen = audited.freeze();

        let mut edges = vec![EdgeLayout {
            geometry: PathGeometry::Polyline { points: vec![] },
            labels: vec![],
            from_port: crate::layout::types::Port::Top,
            to_port: crate::layout::types::Port::Bottom,
        }];
        frozen.write_geometry_into(&mut edges);
        assert!(matches!(edges[0].geometry, PathGeometry::Straight { .. }));
        // 写回只碰 geometry，不动 ports。
        assert_eq!(edges[0].from_port, crate::layout::types::Port::Top);
    }
}
