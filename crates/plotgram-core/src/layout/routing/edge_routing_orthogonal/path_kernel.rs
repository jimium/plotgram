//! Phase 2 LexAStar 选路：短 stub → kernel ResourceGraph → 词典序 A*。

use super::context::{EndpointPair, OrthoRoutingContext};
use super::path::{port_outward, source_group_exit_stub_len, PathSelectStats};
use super::simplify::simplify_path;
use super::{NODE_OBSTACLE_PAD, PORT_CLEARANCE, EPS, SegmentGrid};
use crate::layout::geometry::{Point, Rect};
use crate::layout::kernel::route::{lex_astar, ResourceGraph, SolverStatus};

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
    let path = assemble_with_stubs(start, end, from_stub, to_stub, mid);

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
/// Phase 3：U 形穿组补丁已删；H2 由构图 + 联合约束保证，残余记债。
pub(crate) fn repair_group_interior_crossings(
    _edges: &mut [crate::layout::EdgeLayout],
    _diagram: &crate::ast::Diagram,
    _groups: &std::collections::HashMap<String, crate::layout::GroupLayout>,
    _group_ctx: &crate::layout::group::GroupRoutingContext,
    _sorted_group_ids: &[String],
    _grid: &mut SegmentGrid,
) -> usize {
    0
}
