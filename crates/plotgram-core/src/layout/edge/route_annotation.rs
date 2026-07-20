//! 旁路路由注解 + 形状编辑验证（共线方案 P2）。
//!
//! 不改写 [`crate::layout::EdgeLayout`]：在 C 末从路径冻结 stub / 受保护 trunk run，
//! 仅对「会改形状」的后处理（换角 / overshoot / 量化后简化）做 `validate_route_edit`，
//! 失败则回退 `before`。严格共线删点可跳过全量验证。

use crate::layout::edge::segment_pair::{MIN_SHARED_TRUNK_LEN, STUB_GUARD_LENGTH};
use crate::layout::geometry::Point;
use crate::layout::{EdgeLayout, NodeLayout, Port};
use serde::Serialize;
use std::collections::HashMap;

const EPS: f64 = 1.0;
/// 形状编辑验证专用的穿障垒量：独立于正交路由 `NODE_OBSTACLE_PAD`(=18)，
/// 此处故意取更紧的 4.0（仅用于 route-annotation 后处理校验，非路由避障同语）。
const NODE_PAD: f64 = 4.0;

/// 受保护的轴对齐 run（lane / trunk 坐标）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ProtectedRun {
    pub horizontal: bool,
    /// 水平段为 y，竖直段为 x
    pub coord: f64,
    pub t0: f64,
    pub t1: f64,
}

impl ProtectedRun {
    pub fn overlap_len(self, other_t0: f64, other_t1: f64) -> f64 {
        let a0 = self.t0.min(self.t1);
        let a1 = self.t0.max(self.t1);
        let b0 = other_t0.min(other_t1);
        let b1 = other_t0.max(other_t1);
        (a1.min(b1) - a0.max(b0)).max(0.0)
    }

    fn translate(&mut self, dx: f64, dy: f64) {
        let (cross, along) = if self.horizontal { (dy, dx) } else { (dx, dy) };
        self.coord += cross;
        self.t0 += along;
        self.t1 += along;
    }
}

/// 声明的 merge 共享区间（P2 可空；后续由 Classify / merge 组填充）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MergeInterval {
    pub horizontal: bool,
    pub coord: f64,
    pub t0: f64,
    pub t1: f64,
    /// 可选组键，便于调试
    pub group_key: Option<String>,
}

impl MergeInterval {
    fn translate(&mut self, dx: f64, dy: f64) {
        let (cross, along) = if self.horizontal { (dy, dx) } else { (dx, dy) };
        self.coord += cross;
        self.t0 += along;
        self.t1 += along;
    }
}

/// 单边旁路注解（C 末冻结）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EdgeRouteAnnotation {
    pub edge_index: usize,
    pub from_port: Port,
    pub to_port: Port,
    pub start: Point,
    pub end: Point,
    pub stub_start: Option<Point>,
    pub stub_end: Option<Point>,
    pub stub_guard_length: f64,
    pub protected_runs: Vec<ProtectedRun>,
    pub merge_intervals: Vec<MergeInterval>,
    pub degraded: Option<String>,
    /// S1：from 端 stub 轴坐标（Top/Bottom=x，Left/Right=y）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stub_occupancy_from: Option<f64>,
    /// S1：to 端 stub 轴坐标
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stub_occupancy_to: Option<f64>,
}

/// 全图旁路注解表（按边下标；缺失边在校验时按路径现推）。
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct RouteAnnotationSet {
    pub edges: Vec<EdgeRouteAnnotation>,
}

impl RouteAnnotationSet {
    pub fn get(&self, edge_index: usize) -> Option<&EdgeRouteAnnotation> {
        self.edges.iter().find(|a| a.edge_index == edge_index)
    }

    pub fn len(&self) -> usize {
        self.edges.len()
    }

    pub fn is_empty(&self) -> bool {
        self.edges.is_empty()
    }

    /// 与路径几何执行同一次全局平移，保持 annotation 的绝对坐标契约。
    pub fn translate(&mut self, dx: f64, dy: f64) {
        for ann in &mut self.edges {
            ann.start.x += dx;
            ann.start.y += dy;
            ann.end.x += dx;
            ann.end.y += dy;
            if let Some(point) = ann.stub_start.as_mut() {
                point.x += dx;
                point.y += dy;
            }
            if let Some(point) = ann.stub_end.as_mut() {
                point.x += dx;
                point.y += dy;
            }
            for run in &mut ann.protected_runs {
                run.translate(dx, dy);
            }
            for merge in &mut ann.merge_intervals {
                merge.translate(dx, dy);
            }
            if let Some(coord) = ann.stub_occupancy_from.as_mut() {
                *coord += port_cross_axis_delta(ann.from_port, dx, dy);
            }
            if let Some(coord) = ann.stub_occupancy_to.as_mut() {
                *coord += port_cross_axis_delta(ann.to_port, dx, dy);
            }
        }
    }
}

fn port_cross_axis_delta(port: Port, dx: f64, dy: f64) -> f64 {
    match port {
        Port::Top | Port::Bottom => dx,
        Port::Left | Port::Right => dy,
    }
}

/// 形状编辑类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteEditKind {
    /// 严格共线删点：覆盖范围不变，可跳过全量验证
    StrictCollinear,
    /// 换角 / overshoot / 量化位移等
    ShapeChanging,
}

/// 验证失败原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteEditViolation {
    EndpointsChanged,
    NonOrthogonal,
    StubBroken,
    NodePenetration,
    CrossProtectedRun,
    DegeneratePath,
}

/// 可选穿障上下文。
#[derive(Debug, Clone, Copy)]
pub struct RouteEditObstacleCtx<'a> {
    pub nodes: &'a HashMap<String, NodeLayout>,
    pub sorted_node_ids: &'a [String],
    pub from_id: &'a str,
    pub to_id: &'a str,
}

/// 验证参数。
#[derive(Debug, Clone, Copy)]
pub struct RouteEditValidateOpts {
    /// 受保护 run 的 cross-axis 容差（量化后应用 `grid_step`）
    pub coord_tol: f64,
    pub edit_kind: RouteEditKind,
}

impl Default for RouteEditValidateOpts {
    fn default() -> Self {
        Self {
            coord_tol: 1.0,
            edit_kind: RouteEditKind::ShapeChanging,
        }
    }
}

fn port_outward(side: Port) -> (f64, f64) {
    match side {
        Port::Top => (0.0, -1.0),
        Port::Bottom => (0.0, 1.0),
        Port::Left => (-1.0, 0.0),
        Port::Right => (1.0, 0.0),
    }
}

fn same_point(a: Point, b: Point, tol: f64) -> bool {
    (a.x - b.x).abs() <= tol && (a.y - b.y).abs() <= tol
}

fn seg_len(a: Point, b: Point) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    (dx * dx + dy * dy).sqrt()
}

fn is_orthogonal_path(points: &[Point]) -> bool {
    points.windows(2).all(|w| {
        let dx = (w[1].x - w[0].x).abs();
        let dy = (w[1].y - w[0].y).abs();
        dx <= EPS || dy <= EPS
    })
}

fn stub_projection_ok(anchor: Point, stub: Point, side: Port) -> bool {
    let (ox, oy) = port_outward(side);
    let proj = (stub.x - anchor.x) * ox + (stub.y - anchor.y) * oy;
    proj >= STUB_GUARD_LENGTH * 0.25
}

fn segment_intersects_node(a: Point, b: Point, nl: &NodeLayout, pad: f64) -> bool {
    let left = nl.x - pad;
    let right = nl.x + nl.width + pad;
    let top = nl.y - pad;
    let bottom = nl.y + nl.height + pad;
    let xmin = a.x.min(b.x);
    let xmax = a.x.max(b.x);
    let ymin = a.y.min(b.y);
    let ymax = a.y.max(b.y);
    if xmax < left || xmin > right || ymax < top || ymin > bottom {
        return false;
    }
    // 轴对齐段：与矩形严格相交（含擦边）
    if (a.y - b.y).abs() <= EPS {
        let y = a.y;
        y >= top && y <= bottom && xmax >= left && xmin <= right
    } else if (a.x - b.x).abs() <= EPS {
        let x = a.x;
        x >= left && x <= right && ymax >= top && ymin <= bottom
    } else {
        true
    }
}

fn path_penetrates_nodes(
    path: &[Point],
    from_id: &str,
    to_id: &str,
    nodes: &HashMap<String, NodeLayout>,
    sorted_node_ids: &[String],
) -> bool {
    if path.len() < 2 {
        return false;
    }
    let last_segment = path.len().saturating_sub(2);
    for (segment_index, window) in path.windows(2).enumerate() {
        let a = window[0];
        let b = window[1];
        let seg_xmin = a.x.min(b.x) - NODE_PAD;
        let seg_xmax = a.x.max(b.x) + NODE_PAD;
        let seg_ymin = a.y.min(b.y) - NODE_PAD;
        let seg_ymax = a.y.max(b.y) + NODE_PAD;
        for node_id in sorted_node_ids {
            let nid = node_id.as_str();
            let stub_exempt = (nid == from_id && segment_index == 0)
                || (nid == to_id && segment_index == last_segment);
            if stub_exempt {
                continue;
            }
            let Some(nl) = nodes.get(nid) else {
                continue;
            };
            if nl.x + nl.width < seg_xmin
                || nl.x > seg_xmax
                || nl.y + nl.height < seg_ymin
                || nl.y > seg_ymax
            {
                continue;
            }
            let pad = if nid == from_id || nid == to_id {
                0.0
            } else {
                NODE_PAD
            };
            if segment_intersects_node(a, b, nl, pad) {
                return true;
            }
        }
    }
    false
}

fn extract_protected_runs(points: &[Point]) -> Vec<ProtectedRun> {
    if points.len() < 3 {
        return Vec::new();
    }
    let last_seg = points.len() - 2;
    let mut runs = Vec::new();
    for (si, w) in points.windows(2).enumerate() {
        // 跳过端 stub 段
        if si == 0 || si == last_seg {
            continue;
        }
        let a = w[0];
        let b = w[1];
        let len = seg_len(a, b);
        if len + EPS < MIN_SHARED_TRUNK_LEN {
            continue;
        }
        let dx = (b.x - a.x).abs();
        let dy = (b.y - a.y).abs();
        if dx <= EPS && dy > EPS {
            runs.push(ProtectedRun {
                horizontal: false,
                coord: a.x,
                t0: a.y.min(b.y),
                t1: a.y.max(b.y),
            });
        } else if dy <= EPS && dx > EPS {
            runs.push(ProtectedRun {
                horizontal: true,
                coord: a.y,
                t0: a.x.min(b.x),
                t1: a.x.max(b.x),
            });
        }
    }
    runs
}

/// 从单边路径构建旁路注解（C 末或校验前回退用）。
pub fn annotate_edge_from_path(
    points: &[Point],
    from_port: Port,
    to_port: Port,
    edge_index: usize,
) -> Option<EdgeRouteAnnotation> {
    if points.len() < 2 {
        return None;
    }
    let start = points[0];
    let end = *points.last().unwrap();
    let stub_start = if points.len() >= 3 {
        Some(points[1])
    } else {
        None
    };
    let stub_end = if points.len() >= 4 {
        Some(points[points.len() - 2])
    } else if points.len() == 3 {
        Some(points[1])
    } else {
        None
    };
    Some(EdgeRouteAnnotation {
        edge_index,
        from_port,
        to_port,
        start,
        end,
        stub_start,
        stub_end,
        stub_guard_length: STUB_GUARD_LENGTH,
        protected_runs: extract_protected_runs(points),
        merge_intervals: Vec::new(),
        degraded: None,
        stub_occupancy_from: Some(if matches!(from_port, Port::Top | Port::Bottom) {
            start.x
        } else {
            start.y
        }),
        stub_occupancy_to: Some(if matches!(to_port, Port::Top | Port::Bottom) {
            end.x
        } else {
            end.y
        }),
    })
}

/// C 末冻结：为每条正交折线写旁路注解；可选附加 S3 merge_intervals / degraded 声明。
pub fn freeze_route_annotations_with_merges(
    edges: &[EdgeLayout],
    from_side: &[Port],
    to_side: &[Port],
    merge_intervals: Option<&HashMap<usize, Vec<MergeInterval>>>,
    degraded: Option<&HashMap<usize, String>>,
) -> RouteAnnotationSet {
    let mut out = RouteAnnotationSet::default();
    for (ei, edge) in edges.iter().enumerate() {
        if edge.is_bezier() || edge.path_len() < 2 {
            continue;
        }
        let points = edge.path_points();
        let fs = from_side.get(ei).copied().unwrap_or(edge.from_port);
        let ts = to_side.get(ei).copied().unwrap_or(edge.to_port);
        if let Some(mut ann) = annotate_edge_from_path(points.as_ref(), fs, ts, ei) {
            if let Some(map) = merge_intervals {
                if let Some(ivs) = map.get(&ei) {
                    ann.merge_intervals = ivs.clone();
                }
            }
            if let Some(map) = degraded {
                if let Some(reason) = map.get(&ei) {
                    ann.degraded = Some(reason.clone());
                }
            }
            out.edges.push(ann);
        }
    }
    out
}

/// D 期改边后：按当前路径重建注解，保留既有 `merge_intervals` / `degraded`。
pub fn refresh_route_annotations_preserving_semantics(
    edges: &[EdgeLayout],
    from_side: &[Port],
    to_side: &[Port],
    previous: Option<&RouteAnnotationSet>,
) -> RouteAnnotationSet {
    let mut merges: HashMap<usize, Vec<MergeInterval>> = HashMap::new();
    let mut degraded: HashMap<usize, String> = HashMap::new();
    if let Some(prev) = previous {
        for ann in &prev.edges {
            if !ann.merge_intervals.is_empty() {
                merges.insert(ann.edge_index, ann.merge_intervals.clone());
            }
            if let Some(reason) = &ann.degraded {
                degraded.insert(ann.edge_index, reason.clone());
            }
        }
    }
    freeze_route_annotations_with_merges(
        edges,
        from_side,
        to_side,
        (!merges.is_empty()).then_some(&merges),
        (!degraded.is_empty()).then_some(&degraded),
    )
}

fn protected_runs_preserved(after: &[Point], runs: &[ProtectedRun], coord_tol: f64) -> bool {
    if runs.is_empty() {
        return true;
    }
    for run in runs {
        let need = (run.t1 - run.t0).abs() * 0.5;
        let mut covered = 0.0;
        for w in after.windows(2) {
            let a = w[0];
            let b = w[1];
            let dx = (b.x - a.x).abs();
            let dy = (b.y - a.y).abs();
            if run.horizontal {
                if dy > EPS || (a.y - run.coord).abs() > coord_tol {
                    continue;
                }
                covered += run.overlap_len(a.x, b.x);
            } else {
                if dx > EPS || (a.x - run.coord).abs() > coord_tol {
                    continue;
                }
                covered += run.overlap_len(a.y, b.y);
            }
        }
        if covered + EPS < need {
            return false;
        }
    }
    true
}

/// 验证形状编辑：失败应由调用方回退 `before`。
pub fn validate_route_edit(
    before: &[Point],
    after: &[Point],
    ann: &EdgeRouteAnnotation,
    obstacle: Option<RouteEditObstacleCtx<'_>>,
    opts: RouteEditValidateOpts,
) -> Result<(), RouteEditViolation> {
    if matches!(opts.edit_kind, RouteEditKind::StrictCollinear) {
        return Ok(());
    }
    if after.len() < 2 {
        return Err(RouteEditViolation::DegeneratePath);
    }
    if !same_point(after[0], ann.start, EPS) || !same_point(*after.last().unwrap(), ann.end, EPS) {
        return Err(RouteEditViolation::EndpointsChanged);
    }
    // 编辑不得丢掉连通性：端点相对 before 也需稳定
    if !before.is_empty()
        && (!same_point(after[0], before[0], EPS)
            || !same_point(*after.last().unwrap(), *before.last().unwrap(), EPS))
    {
        return Err(RouteEditViolation::EndpointsChanged);
    }
    if !is_orthogonal_path(after) {
        return Err(RouteEditViolation::NonOrthogonal);
    }

    if let Some(stub) = after.get(1) {
        if !stub_projection_ok(after[0], *stub, ann.from_port) {
            return Err(RouteEditViolation::StubBroken);
        }
    }
    if after.len() >= 3 {
        let stub = after[after.len() - 2];
        if !stub_projection_ok(after[after.len() - 1], stub, ann.to_port) {
            return Err(RouteEditViolation::StubBroken);
        }
    }

    // 仅校验**显式声明**的 merge 区间。启发式 protected_runs（中段长 trunk）
    // 仍写入 Annotation 供观测 / P4，但不作为 sanitize 换角的硬回退条件——
    // 否则会误杀合法微折折叠，导致紧间距严重度回升（P2 门禁回归）。
    if !ann.merge_intervals.is_empty() {
        let as_runs: Vec<ProtectedRun> = ann
            .merge_intervals
            .iter()
            .map(|m| ProtectedRun {
                horizontal: m.horizontal,
                coord: m.coord,
                t0: m.t0,
                t1: m.t1,
            })
            .collect();
        if !protected_runs_preserved(after, &as_runs, opts.coord_tol) {
            return Err(RouteEditViolation::CrossProtectedRun);
        }
    }

    if let Some(obs) = obstacle {
        let before_dirty = path_penetrates_nodes(
            before,
            obs.from_id,
            obs.to_id,
            obs.nodes,
            obs.sorted_node_ids,
        );
        let after_dirty = path_penetrates_nodes(
            after,
            obs.from_id,
            obs.to_id,
            obs.nodes,
            obs.sorted_node_ids,
        );
        if !before_dirty && after_dirty {
            return Err(RouteEditViolation::NodePenetration);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn horiz_stair() -> Vec<Point> {
        vec![
            Point::new(0.0, 0.0),
            Point::new(16.0, 0.0),
            Point::new(16.0, 40.0),
            Point::new(80.0, 40.0),
            Point::new(80.0, 80.0),
            Point::new(96.0, 80.0),
        ]
    }

    #[test]
    fn annotate_serializes_key_fields() {
        let pts = horiz_stair();
        let ann = annotate_edge_from_path(&pts, Port::Right, Port::Left, 3).unwrap();
        assert_eq!(ann.edge_index, 3);
        assert!(ann.stub_start.is_some());
        assert!(ann.stub_end.is_some());
        assert!(!ann.protected_runs.is_empty());
        let json = serde_json::to_string(&ann).expect("serialize");
        assert!(json.contains("\"protected_runs\""));
        assert!(json.contains("\"stub_guard_length\""));
    }

    #[test]
    fn validate_rejects_crossing_explicit_merge_interval() {
        let pts = horiz_stair();
        let mut ann = annotate_edge_from_path(&pts, Port::Right, Port::Left, 0).unwrap();
        ann.merge_intervals = vec![MergeInterval {
            horizontal: true,
            coord: 40.0,
            t0: 16.0,
            t1: 80.0,
            group_key: Some("g".into()),
        }];
        // 把中间水平 trunk 从 y=40 拉到 y=0（越过声明的 merge 区间）
        let mut after = pts.clone();
        after[2] = Point::new(16.0, 0.0);
        after[3] = Point::new(80.0, 0.0);
        let err = validate_route_edit(&pts, &after, &ann, None, RouteEditValidateOpts::default());
        assert_eq!(err, Err(RouteEditViolation::CrossProtectedRun));
    }

    #[test]
    fn strict_collinear_skips_validation() {
        let pts = horiz_stair();
        let ann = annotate_edge_from_path(&pts, Port::Right, Port::Left, 0).unwrap();
        let after = vec![
            pts[0],
            pts[1],
            Point::new(50.0, 40.0), // 共线中点可删，这里故意换成穿障形状也不检
            pts[3],
            pts[4],
            pts[5],
        ];
        assert!(validate_route_edit(
            &pts,
            &after,
            &ann,
            None,
            RouteEditValidateOpts {
                edit_kind: RouteEditKind::StrictCollinear,
                ..Default::default()
            },
        )
        .is_ok());
    }

    #[test]
    fn validate_rejects_new_node_penetration() {
        let pts = vec![
            Point::new(0.0, 50.0),
            Point::new(16.0, 50.0),
            Point::new(100.0, 50.0),
            Point::new(116.0, 50.0),
        ];
        let ann = annotate_edge_from_path(&pts, Port::Right, Port::Left, 0).unwrap();
        let mut nodes = HashMap::new();
        nodes.insert(
            "obs".into(),
            NodeLayout {
                x: 40.0,
                y: 40.0,
                width: 20.0,
                height: 20.0,
            },
        );
        nodes.insert(
            "a".into(),
            NodeLayout {
                x: -30.0,
                y: 40.0,
                width: 30.0,
                height: 20.0,
            },
        );
        nodes.insert(
            "b".into(),
            NodeLayout {
                x: 116.0,
                y: 40.0,
                width: 30.0,
                height: 20.0,
            },
        );
        let ids = vec!["a".into(), "obs".into(), "b".into()];
        // before 绕开障碍
        let before = vec![
            Point::new(0.0, 50.0),
            Point::new(16.0, 50.0),
            Point::new(16.0, 10.0),
            Point::new(100.0, 10.0),
            Point::new(100.0, 50.0),
            Point::new(116.0, 50.0),
        ];
        let mut ann2 = annotate_edge_from_path(&before, Port::Right, Port::Left, 0).unwrap();
        ann2.protected_runs.clear(); // 本测只覆盖穿障
        let after = pts.clone(); // 水平穿过 obs
        let obs = RouteEditObstacleCtx {
            nodes: &nodes,
            sorted_node_ids: &ids,
            from_id: "a",
            to_id: "b",
        };
        assert_eq!(
            validate_route_edit(
                &before,
                &after,
                &ann2,
                Some(obs),
                RouteEditValidateOpts::default()
            ),
            Err(RouteEditViolation::NodePenetration)
        );
        let _ = ann;
    }
}
