//! Phase 2 LexAStar 选路：短 stub → kernel ResourceGraph → 词典序 A*。

use super::context::{EndpointPair, OrthoRoutingContext};
use super::path::{port_outward, source_group_exit_stub_len, PathSelectStats};
use super::simplify::simplify_path;
use super::{NODE_OBSTACLE_PAD, OrthoConfig, PORT_CLEARANCE, EPS, SegmentGrid};
use crate::layout::geometry::{Point, Rect};
use crate::layout::kernel::route::{lex_astar, ResourceGraph, SolverStatus};
use crate::layout::routing::config::RouteKernelKind;

/// LexAStar 轨：构造可搜索图并词典序搜索；失败时正交兜底（显式 degraded）。
pub(super) fn select_best_path_lex_astar(
    ctx: &OrthoRoutingContext<'_>,
    pair: &EndpointPair,
    stats: Option<&mut PathSelectStats>,
) -> Vec<Point> {
    let start = pair.from_anchor();
    let end = pair.to_anchor();
    let from_side = pair.from.side;
    let to_side = pair.to.side;
    let from_id = pair.from_id();
    let to_id = pair.to_id();

    let from_stub_len = source_group_exit_stub_len(ctx, from_id, to_id, from_side, start.x, start.y)
        .unwrap_or(PORT_CLEARANCE);
    let (fox, foy) = port_outward(from_side);
    let (tox, toy) = port_outward(to_side);
    let from_stub = Point::new(
        start.x + fox * from_stub_len,
        start.y + foy * from_stub_len,
    );
    let to_stub = Point::new(
        end.x + tox * PORT_CLEARANCE,
        end.y + toy * PORT_CLEARANCE,
    );

    let node_pad = NODE_OBSTACLE_PAD + 10.0;
    let mut node_obstacles: Vec<Rect> = Vec::new();
    for nid in &ctx.obstacles.sorted_node_ids {
        if nid == from_id || nid == to_id {
            continue;
        }
        if let Some(nl) = ctx.nodes.get(nid) {
            node_obstacles.push(
                Rect::new(nl.x, nl.y, nl.width, nl.height).expanded(node_pad),
            );
        }
    }
    // 确定性：按左上角排序
    node_obstacles.sort_by(|a, b| {
        a.x.partial_cmp(&b.x)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal))
    });

    let mut group_rects: Vec<(String, Rect)> = Vec::new();
    for gid in &ctx.obstacles.sorted_group_ids {
        if let Some(gl) = ctx.group_ctx.groups.get(gid) {
            group_rects.push((
                gid.clone(),
                Rect::new(gl.x, gl.y, gl.width, gl.height),
            ));
        }
    }

    let (graph, sid, eid) = ResourceGraph::build_for_query(
        from_stub,
        to_stub,
        from_side,
        to_side,
        from_id,
        to_id,
        &node_obstacles,
        &group_rects,
        Some(ctx.group_ctx),
    );

    let margin = 80.0;
    let occupied: Vec<(f64, f64, f64, f64)> = ctx
        .grid
        .query_bbox(
            start.x.min(end.x) - margin,
            start.y.min(end.y) - margin,
            start.x.max(end.x) + margin,
            start.y.max(end.y) + margin,
        )
        .iter()
        .map(|s| (s.x1, s.y1, s.x2, s.y2))
        .collect();

    let prefer_periphery = ctx.prefer_periphery;
    let result = lex_astar(&graph, sid, eid, prefer_periphery, &occupied);

    let mut degraded = result.status != SolverStatus::Converged;
    let mut mid = result.points;
    if mid.len() < 2 {
        mid = vec![from_stub, to_stub];
        degraded = true;
    }
    let mut path = assemble_with_stubs(start, end, from_stub, to_stub, mid);

    if let Some(s) = stats {
        s.candidate_count = 1;
        s.degraded = degraded;
        s.hard_filter_reject_count = 0;
    }
    path
}

fn assemble_with_stubs(
    start: Point,
    end: Point,
    from_stub: Point,
    to_stub: Point,
    mid: Vec<Point>,
) -> Vec<Point> {
    let mut path = Vec::with_capacity(mid.len() + 2);
    path.push(start);
    if (path[0].x - mid[0].x).abs() > EPS || (path[0].y - mid[0].y).abs() > EPS {
        if (mid[0].x - from_stub.x).abs() > EPS || (mid[0].y - from_stub.y).abs() > EPS {
            path.push(from_stub);
        }
    }
    path.extend(mid);
    let last = *path.last().unwrap();
    if (last.x - end.x).abs() > EPS || (last.y - end.y).abs() > EPS {
        if (last.x - to_stub.x).abs() > EPS || (last.y - to_stub.y).abs() > EPS {
            path.push(to_stub);
        }
        path.push(end);
    }
    simplify_path(path, false)
}

/// 是否走 LexAStar 内核（默认 true）。
#[inline]
pub(super) fn use_lex_astar(cfg: &OrthoConfig) -> bool {
    matches!(cfg.routing.route_kernel, RouteKernelKind::LexAStar)
}

/// Phase 2 红线修复：对 lint 语义下仍穿无关组的边，用外框 U 形替换。
///
/// 豁免口径与 `edge_crosses_group_interior` 对齐（diagram entity group_id + ancestors），
/// 避免 `path_avoids_group_interiors`（GroupRoutingContext）过宽豁免导致漏修。
pub(crate) fn repair_group_interior_crossings(
    edges: &mut [crate::layout::EdgeLayout],
    diagram: &crate::ast::Diagram,
    groups: &std::collections::HashMap<String, crate::layout::GroupLayout>,
    _group_ctx: &crate::layout::group::GroupRoutingContext,
    _sorted_group_ids: &[String],
    grid: &mut SegmentGrid,
) -> usize {
    use crate::layout::quality::lint::{
        GroupInteriorMaps, edge_crosses_group_interior_with_maps,
    };
    use crate::layout::LayoutResult;

    if groups.is_empty() {
        return 0;
    }
    let maps = GroupInteriorMaps::new(diagram);
    let mut probe = LayoutResult {
        nodes: std::collections::HashMap::new(),
        edges: edges.to_vec(),
        groups: groups.clone(),
        total_width: 0.0,
        total_height: 0.0,
        hints: Default::default(),
    };

    let mut repaired = 0usize;
    for ei in 0..edges.len() {
        if edges[ei].path_is_empty() {
            continue;
        }
        if !edge_crosses_group_interior_with_maps(diagram, &probe, ei, &maps) {
            continue;
        }        let pts: Vec<Point> = edges[ei].path_points().into_owned();
        let start = pts[0];
        let end = *pts.last().unwrap();
        let from_id = diagram.relations[ei].from.as_str();
        let to_id = diagram.relations[ei].to.as_str();
        let mut lint_sorted: Vec<String> = groups.keys().cloned().collect();
        lint_sorted.sort();
        let Some(outer) =
            outer_u_path_lint_aware(start, end, from_id, to_id, &maps, groups, &lint_sorted)
        else {
            continue;
        };
        edges[ei].set_polyline_points(outer.clone());
        probe.edges[ei].set_polyline_points(outer.clone());
        if edge_crosses_group_interior_with_maps(diagram, &probe, ei, &maps) {
            edges[ei].set_polyline_points(pts);
            probe.edges[ei].set_polyline_points(edges[ei].path_points().into_owned());
            continue;
        }
        grid.remove_by_edges(&[ei]);
        grid.insert_path(&outer, ei);
        repaired += 1;
    }
    repaired
}

fn outer_u_path_lint_aware(
    start: Point,
    end: Point,
    from_id: &str,
    to_id: &str,
    maps: &crate::layout::quality::lint::GroupInteriorMaps,
    groups: &std::collections::HashMap<String, crate::layout::GroupLayout>,
    sorted_group_ids: &[String],
) -> Option<Vec<Point>> {
    let from_rel = maps.related_groups(from_id);
    let to_rel = maps.related_groups(to_id);
    let mut x_lo = start.x.min(end.x);
    let mut x_hi = start.x.max(end.x);
    let mut y_lo = start.y.min(end.y);
    let mut y_hi = start.y.max(end.y);
    let mut any = false;
    for gid in sorted_group_ids {
        if from_rel.contains(gid) || to_rel.contains(gid) {
            continue;
        }
        let Some(gl) = groups.get(gid) else {
            continue;
        };
        if gl.width <= 0.0 || gl.height <= 0.0 {
            continue;
        }
        any = true;
        x_lo = x_lo.min(gl.x);
        y_lo = y_lo.min(gl.y);
        x_hi = x_hi.max(gl.x + gl.width);
        y_hi = y_hi.max(gl.y + gl.height);
    }
    if !any {
        // 无「无关组」却被 lint 判穿：扩大端点 bbox 外绕
        let pad = 80.0;
        x_lo -= pad;
        x_hi += pad;
        y_lo -= pad;
        y_hi += pad;
    }    for gid in sorted_group_ids {
        if let Some(gl) = groups.get(gid) {        }
    }
    let pad = 40.0;
    let left = x_lo - pad;
    let right = x_hi + pad;
    let top = y_lo - pad;
    let bottom = y_hi + pad;
    let mut candidates = vec![
        simplify_path(
            vec![
                start,
                Point::new(left, start.y),
                Point::new(left, end.y),
                end,
            ],
            false,
        ),
        simplify_path(
            vec![
                start,
                Point::new(right, start.y),
                Point::new(right, end.y),
                end,
            ],
            false,
        ),
        simplify_path(
            vec![
                start,
                Point::new(start.x, top),
                Point::new(end.x, top),
                end,
            ],
            false,
        ),
        simplify_path(
            vec![
                start,
                Point::new(start.x, bottom),
                Point::new(end.x, bottom),
                end,
            ],
            false,
        ),
    ];
    // 外环四角绕行（U 形仍穿组时）
    for (cx, cy) in [
        (left, top),
        (right, top),
        (left, bottom),
        (right, bottom),
    ] {
        candidates.push(simplify_path(
            vec![
                start,
                Point::new(cx, start.y),
                Point::new(cx, cy),
                Point::new(end.x, cy),
                end,
            ],
            false,
        ));
        candidates.push(simplify_path(
            vec![
                start,
                Point::new(start.x, cy),
                Point::new(cx, cy),
                Point::new(cx, end.y),
                end,
            ],
            false,
        ));
    }
    // 更大外扩再试一档
    let pad2 = 120.0;
    let left2 = x_lo - pad2;
    let right2 = x_hi + pad2;
    let top2 = y_lo - pad2;
    let bottom2 = y_hi + pad2;
    for (cx, cy) in [
        (left2, top2),
        (right2, top2),
        (left2, bottom2),
        (right2, bottom2),
    ] {
        candidates.push(simplify_path(
            vec![
                start,
                Point::new(cx, start.y),
                Point::new(cx, cy),
                Point::new(end.x, cy),
                end,
            ],
            false,
        ));
    }

    let mut best: Option<(f64, Vec<Point>)> = None;
    for cand in candidates {
        // 用几何 pierce 粗检：任一无关组
        let pierces = sorted_group_ids.iter().any(|gid| {
            if from_rel.contains(gid) || to_rel.contains(gid) {
                return false;
            }
            let Some(gl) = groups.get(gid) else {
                return false;
            };
            cand.windows(2).any(|w| {
                crate::layout::routing::common::geom_obstacle::segment_pierces_group_interior(
                    w[0], w[1], gl,
                )
            })
        });
        if pierces {
            continue;
        }
        let len: f64 = cand
            .windows(2)
            .map(|w| (w[0].x - w[1].x).abs() + (w[0].y - w[1].y).abs())
            .sum();
        if best.as_ref().map_or(true, |(bl, _)| len < *bl) {
            best = Some((len, cand));
        }
    }
    best.map(|(_, p)| p)
}
