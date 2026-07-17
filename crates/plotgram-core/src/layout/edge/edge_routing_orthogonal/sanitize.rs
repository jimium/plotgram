//! 正交路径锯齿消毒（不变量-only）。
//!
//! 空间契约下消毒**不得改拓扑**（大 U 折叠、变 Straight 穿层）。
//! 仅修复：
//! - 端点反向 stub
//! - 非正交斜段 → 强制拆成 L
//! - 真微折（< MICRO_JOG_LEN）
//!
//! 应在 lane/corridor 之后调用；snap 后再跑一次同一套不变量。
//!
//! P2：`collapse_micro_jogs`（含 overshoot 换角）经 `validate_route_edit` 守护；
//! 失败回退该步之前的折线。严格共线 `simplify_path` 不跑全量验证。

use super::path::{port_aware_elbow, port_outward};
use super::simplify::simplify_path;
use super::{EPS, PORT_CLEARANCE};
use crate::ast::Relation;
use crate::layout::edge::common::parallel_edges::build_parallel_aware_edge_labels_auto;
use crate::layout::edge::route_annotation::{
    annotate_edge_from_path, validate_route_edit, RouteAnnotationSet, RouteEditObstacleCtx,
    RouteEditValidateOpts,
};
use crate::layout::geometry::Point;
use crate::layout::{EdgeLayout, NodeLayout, PathGeometry, Port};
use std::collections::HashMap;

/// 短于该长度的折段视为「微折」，可折叠。
const MICRO_JOG_LEN: f64 = 24.0;

/// 路由后处理：消除反向 stub / 斜段 / 微折，并重建标签。
///
/// 路由内部（step 4g）调用保持**保守**（`merge_overshoot=false`），避免改动被
/// 后续 node/space-budget 反馈用于重定位节点而扰动全局布局；管线末尾几何冻结后
/// 调用启用 `merge_overshoot=true`，清理「冲过端口再折回」的 overshoot Z 折。
pub fn sanitize_orthogonal_edges(
    edges: &mut [EdgeLayout],
    relations: &[Relation],
    from_side: &[Port],
    to_side: &[Port],
) {
    sanitize_orthogonal_edges_ext(edges, relations, from_side, to_side, false);
}

/// 见 [`sanitize_orthogonal_edges`]；`merge_overshoot` 控制是否合并 overshoot Z 折。
pub fn sanitize_orthogonal_edges_ext(
    edges: &mut [EdgeLayout],
    relations: &[Relation],
    from_side: &[Port],
    to_side: &[Port],
    merge_overshoot: bool,
) {
    sanitize_orthogonal_edges_with_guard(
        edges,
        relations,
        from_side,
        to_side,
        merge_overshoot,
        None,
        None,
        None,
    );
}

/// 消毒 + 可选形状验证上下文（节点 / 冻结 Annotation）。
#[allow(clippy::too_many_arguments)]
pub fn sanitize_orthogonal_edges_with_guard(
    edges: &mut [EdgeLayout],
    relations: &[Relation],
    from_side: &[Port],
    to_side: &[Port],
    merge_overshoot: bool,
    annotations: Option<&RouteAnnotationSet>,
    nodes: Option<&HashMap<String, NodeLayout>>,
    sorted_node_ids: Option<&[String]>,
) {
    for (ei, edge) in edges.iter_mut().enumerate() {
        if edge.path_is_empty() {
            continue;
        }
        let mut points: Vec<Point> = edge.path_points().into_owned();
        if points.len() < 2 {
            continue;
        }
        let fs = from_side.get(ei).copied().unwrap_or(edge.from_port);
        let ts = to_side.get(ei).copied().unwrap_or(edge.to_port);

        let frozen = annotations.and_then(|set| set.get(ei)).cloned();
        // L5.1：D 末形状编辑挂穿障硬回退——仅拒「编辑前干净 → 编辑后穿节点」。
        // 已脏路径不在此清；清 through 走 refine dogleg（保组硬门禁）。
        let obstacle = match (nodes, sorted_node_ids, relations.get(ei)) {
            (Some(n), Some(ids), Some(rel)) => Some(RouteEditObstacleCtx {
                nodes: n,
                sorted_node_ids: ids,
                from_id: rel.from.as_str(),
                to_id: rel.to.as_str(),
            }),
            _ => None,
        };

        sanitize_polyline_ext_guarded(
            &mut points,
            fs,
            ts,
            merge_overshoot,
            frozen.as_ref(),
            ei,
            obstacle,
        );
        if points.len() < 2 {
            continue;
        }

        let labels = match relations.get(ei) {
            Some(rel) => build_parallel_aware_edge_labels_auto(rel, ei, relations, &points),
            None => Vec::new(),
        };

        let mut new_edge = EdgeLayout {
            geometry: PathGeometry::Polyline { points: Vec::new() },
            labels,
            from_port: fs,
            to_port: ts,
        };
        new_edge.set_polyline_points(points);
        *edge = new_edge;
    }
}

/// 对单条正交折线做不变量消毒（供测试与管道复用）。
pub fn sanitize_polyline(points: &mut Vec<Point>, from_side: Port, to_side: Port) {
    sanitize_polyline_ext(points, from_side, to_side, false);
}

/// 见 [`sanitize_polyline`]；`merge_overshoot` 控制是否合并 overshoot Z 折。
pub fn sanitize_polyline_ext(
    points: &mut Vec<Point>,
    from_side: Port,
    to_side: Port,
    merge_overshoot: bool,
) {
    sanitize_polyline_ext_guarded(points, from_side, to_side, merge_overshoot, None, 0, None);
}

/// 消毒单边；仅在 `merge_overshoot`（管线末激进清理）时挂形状验证。
/// router 内保守消毒不验证，避免回退改变边几何后经 space-budget 反馈扰动节点（node_fp）。
fn sanitize_polyline_ext_guarded(
    points: &mut Vec<Point>,
    from_side: Port,
    to_side: Port,
    merge_overshoot: bool,
    frozen: Option<&crate::layout::edge::EdgeRouteAnnotation>,
    edge_index: usize,
    obstacle: Option<RouteEditObstacleCtx<'_>>,
) {
    if points.len() < 2 {
        return;
    }
    let original = points.clone();
    let orig_end = *original.last().unwrap();
    let orig_start = original[0];
    let orig_len = polyline_length(&original);

    fix_endpoint_reverse_stub(points, true, from_side);
    fix_endpoint_reverse_stub(points, false, to_side);
    force_orthogonal(points);

    let before_shape = points.clone();
    // 校验注解必须来自**当前**几何：C 冻结的 start/end 在 space-budget / snap 后会过期，
    // 直接拿来 validate 会恒失败并回退，等效于关掉 overshoot 合并。
    let ann = if merge_overshoot {
        annotate_edge_from_path(&before_shape, from_side, to_side, edge_index).map(|mut a| {
            if let Some(frozen) = frozen {
                a.merge_intervals = frozen.merge_intervals.clone();
                a.degraded = frozen.degraded.clone();
            }
            a
        })
    } else {
        None
    };

    collapse_micro_jogs(points, merge_overshoot);
    *points = simplify_path(std::mem::take(points), true);
    ensure_outward_stub(points, true, from_side);
    ensure_outward_stub(points, false, to_side);
    repair_post_stub_inward(points, true, from_side);
    repair_post_stub_inward(points, false, to_side);
    *points = simplify_path(std::mem::take(points), true);

    if let Some(ref ann) = ann {
        if validate_route_edit(
            &before_shape,
            points,
            ann,
            obstacle,
            RouteEditValidateOpts::default(),
        )
        .is_err()
        {
            *points = before_shape;
        }
    }

    // 安全网：消毒不得丢掉起终点连通性（曾出现裁成 [anchor,stub] 导致「边起点丢失」）
    if points.len() < 2
        || !same_point(points[0], orig_start)
        || !same_point(*points.last().unwrap(), orig_end)
        || (orig_len > PORT_CLEARANCE * 4.0 && polyline_length(points) <= PORT_CLEARANCE + 1.0)
    {
        *points = original;
        force_orthogonal(points);
    }
}

fn same_point(a: Point, b: Point) -> bool {
    (a.x - b.x).abs() < 1.0 && (a.y - b.y).abs() < 1.0
}

fn polyline_length(points: &[Point]) -> f64 {
    points.windows(2).fold(0.0, |acc, w| {
        let dx = w[1].x - w[0].x;
        let dy = w[1].y - w[0].y;
        acc + (dx * dx + dy * dy).sqrt()
    })
}

/// 若端点第一段（或末端最后一段）沿端口外向为负，则切除「背向」折点并补正确 stub。
///
/// **不得**在找不到外向保留点时把路径裁成仅 stub——那会丢掉对端节点连接。
fn fix_endpoint_reverse_stub(points: &mut Vec<Point>, at_start: bool, side: Port) {
    if points.len() < 3 {
        return;
    }
    let (ox, oy) = port_outward(side);
    if at_start {
        let anchor = points[0];
        let dx = points[1].x - anchor.x;
        let dy = points[1].y - anchor.y;
        let proj = dx * ox + dy * oy;
        if proj >= -1.0 {
            return;
        }
        let mut keep_from = 1usize;
        while keep_from < points.len() {
            let p = points[keep_from];
            let fp = (p.x - anchor.x) * ox + (p.y - anchor.y) * oy;
            if fp >= PORT_CLEARANCE * 0.5 {
                break;
            }
            keep_from += 1;
        }
        // 全部点都在背向半平面：放弃裁剪，留给 stub_fix 翻端口重路由
        if keep_from >= points.len() {
            return;
        }
        let stub = Point::new(
            anchor.x + ox * PORT_CLEARANCE,
            anchor.y + oy * PORT_CLEARANCE,
        );
        let mut new_pts = vec![anchor, stub];
        let keep_pt = points[keep_from];
        if (stub.x - keep_pt.x).abs() > EPS && (stub.y - keep_pt.y).abs() > EPS {
            let elbow = port_aware_elbow(stub, keep_pt, side);
            if (elbow.x - stub.x).abs() > EPS || (elbow.y - stub.y).abs() > EPS {
                if (elbow.x - keep_pt.x).abs() > EPS || (elbow.y - keep_pt.y).abs() > EPS {
                    new_pts.push(elbow);
                }
            }
        }
        new_pts.extend_from_slice(&points[keep_from..]);
        *points = new_pts;
    } else {
        let last = points.len() - 1;
        let anchor = points[last];
        let prev = points[last - 1];
        let dx = prev.x - anchor.x;
        let dy = prev.y - anchor.y;
        let proj = dx * ox + dy * oy;
        if proj >= -1.0 {
            return;
        }
        let mut keep_to = last;
        while keep_to > 0 {
            let p = points[keep_to - 1];
            let fp = (p.x - anchor.x) * ox + (p.y - anchor.y) * oy;
            if fp >= PORT_CLEARANCE * 0.5 {
                break;
            }
            keep_to -= 1;
        }
        // keep_to==0 意味着起点也会被丢掉：放弃
        if keep_to == 0 {
            return;
        }
        let stub = Point::new(
            anchor.x + ox * PORT_CLEARANCE,
            anchor.y + oy * PORT_CLEARANCE,
        );
        let mut new_pts: Vec<Point> = points[..keep_to].to_vec();
        let keep_pt = points[keep_to - 1];
        if (keep_pt.x - stub.x).abs() > EPS && (keep_pt.y - stub.y).abs() > EPS {
            let elbow = port_aware_elbow(stub, keep_pt, side);
            if new_pts
                .last()
                .is_none_or(|p| (p.x - elbow.x).abs() > EPS || (p.y - elbow.y).abs() > EPS)
            {
                new_pts.push(elbow);
            }
        }
        new_pts.push(stub);
        new_pts.push(anchor);
        *points = new_pts;
    }
}

/// 保证端点存在沿外向的 stub 段（长度约 PORT_CLEARANCE）。
fn ensure_outward_stub(points: &mut Vec<Point>, at_start: bool, side: Port) {
    if points.len() < 2 {
        return;
    }
    let (ox, oy) = port_outward(side);
    if at_start {
        let anchor = points[0];
        let far_end = *points.last().unwrap();
        let nxt = points[1];
        let proj = (nxt.x - anchor.x) * ox + (nxt.y - anchor.y) * oy;
        if proj >= PORT_CLEARANCE * 0.5 {
            return;
        }
        let stub = Point::new(
            anchor.x + ox * PORT_CLEARANCE,
            anchor.y + oy * PORT_CLEARANCE,
        );
        let mut rest = points[1..].to_vec();
        // 丢掉仍在 stub 内侧的点
        while let Some(&p) = rest.first() {
            let fp = (p.x - anchor.x) * ox + (p.y - anchor.y) * oy;
            if fp >= PORT_CLEARANCE * 0.5 {
                break;
            }
            rest.remove(0);
        }
        // 若中间全被清掉，至少保留到原终点，禁止退化成 [anchor, stub]
        if rest.is_empty() {
            if same_point(anchor, far_end) {
                return;
            }
            let mut out = vec![anchor, stub];
            if (stub.x - far_end.x).abs() > EPS && (stub.y - far_end.y).abs() > EPS {
                let elbow = port_aware_elbow(stub, far_end, side);
                if (elbow.x - stub.x).abs() > EPS || (elbow.y - stub.y).abs() > EPS {
                    out.push(elbow);
                }
            }
            out.push(far_end);
            *points = out;
            return;
        }
        let mut out = vec![anchor, stub];
        if let Some(&p) = rest.first() {
            if (stub.x - p.x).abs() > EPS && (stub.y - p.y).abs() > EPS {
                let elbow = port_aware_elbow(stub, p, side);
                if (elbow.x - stub.x).abs() > EPS || (elbow.y - stub.y).abs() > EPS {
                    out.push(elbow);
                }
            }
            out.extend_from_slice(&rest);
        }
        *points = out;
    } else {
        let last = points.len() - 1;
        let anchor = points[last];
        let far_start = points[0];
        let prev = points[last - 1];
        let proj = (prev.x - anchor.x) * ox + (prev.y - anchor.y) * oy;
        if proj >= PORT_CLEARANCE * 0.5 {
            return;
        }
        let stub = Point::new(
            anchor.x + ox * PORT_CLEARANCE,
            anchor.y + oy * PORT_CLEARANCE,
        );
        let mut head = points[..last].to_vec();
        while let Some(&p) = head.last() {
            let fp = (p.x - anchor.x) * ox + (p.y - anchor.y) * oy;
            if fp >= PORT_CLEARANCE * 0.5 {
                break;
            }
            head.pop();
        }
        if head.is_empty() {
            // 禁止退化成 [stub, anchor] 丢掉起点
            if same_point(far_start, anchor) {
                return;
            }
            let mut out = vec![far_start];
            if (far_start.x - stub.x).abs() > EPS && (far_start.y - stub.y).abs() > EPS {
                let elbow = port_aware_elbow(stub, far_start, side);
                if (elbow.x - far_start.x).abs() > EPS || (elbow.y - far_start.y).abs() > EPS {
                    out.push(elbow);
                }
            }
            out.push(stub);
            out.push(anchor);
            *points = out;
            return;
        }
        let Some(&last_h) = head.last() else {
            return;
        };
        if (last_h.x - stub.x).abs() > EPS && (last_h.y - stub.y).abs() > EPS {
            let elbow = port_aware_elbow(stub, last_h, side);
            if (elbow.x - last_h.x).abs() > EPS || (elbow.y - last_h.y).abs() > EPS {
                head.push(elbow);
            }
        }
        head.push(stub);
        head.push(anchor);
        *points = head;
    }
}

/// 出/入 stub 之后若下一段沿端口内向折回，跳过无法无穿模到达的点，并在 stub 外向坐标上转弯。
fn repair_post_stub_inward(points: &mut Vec<Point>, at_start: bool, side: Port) {
    if points.len() < 4 {
        return;
    }
    let (ox, oy) = port_outward(side);
    if at_start {
        let stub = points[1];
        // stub 须大致在外向 PORT_CLEARANCE 处
        let stub_proj = (stub.x - points[0].x) * ox + (stub.y - points[0].y) * oy;
        if stub_proj < PORT_CLEARANCE * 0.5 {
            return;
        }
        let nxt = points[2];
        let inward = (nxt.x - stub.x) * ox + (nxt.y - stub.y) * oy;
        if inward >= -1.0 {
            return;
        }
        // 跳过只能靠内向滑移到达的折点，直到能用合法肘点连接
        let mut idx = 2usize;
        while idx < points.len() {
            let t = points[idx];
            let direct_dx = t.x - stub.x;
            let direct_dy = t.y - stub.y;
            let dproj = direct_dx * ox + direct_dy * oy;
            let axis = direct_dx.abs() < EPS || direct_dy.abs() < EPS;
            if axis && dproj >= -1.0 {
                break;
            }
            let elbow = port_aware_elbow(stub, t, side);
            let edx = elbow.x - stub.x;
            let edy = elbow.y - stub.y;
            let eproj = edx * ox + edy * oy;
            if (edx.abs() > EPS || edy.abs() > EPS) && eproj >= -1.0 {
                break;
            }
            idx += 1;
        }
        if idx >= points.len() {
            return;
        }
        let t = points[idx];
        let mut out = vec![points[0], stub];
        if (t.x - stub.x).abs() > EPS && (t.y - stub.y).abs() > EPS {
            let elbow = port_aware_elbow(stub, t, side);
            if (elbow.x - stub.x).abs() > EPS || (elbow.y - stub.y).abs() > EPS {
                out.push(elbow);
            }
        }
        out.extend_from_slice(&points[idx..]);
        *points = out;
    } else {
        let last = points.len() - 1;
        let stub_i = last - 1;
        let stub = points[stub_i];
        let stub_proj = (stub.x - points[last].x) * ox + (stub.y - points[last].y) * oy;
        if stub_proj < PORT_CLEARANCE * 0.5 {
            return;
        }
        let prev = points[stub_i - 1];
        let inward = (prev.x - stub.x) * ox + (prev.y - stub.y) * oy;
        if inward >= -1.0 {
            return;
        }
        let mut idx = stub_i - 1;
        loop {
            let t = points[idx];
            let direct_dx = t.x - stub.x;
            let direct_dy = t.y - stub.y;
            let dproj = direct_dx * ox + direct_dy * oy;
            let axis = direct_dx.abs() < EPS || direct_dy.abs() < EPS;
            if axis && dproj >= -1.0 {
                break;
            }
            let elbow = port_aware_elbow(stub, t, side);
            let edx = elbow.x - stub.x;
            let edy = elbow.y - stub.y;
            let eproj = edx * ox + edy * oy;
            if (edx.abs() > EPS || edy.abs() > EPS) && eproj >= -1.0 {
                break;
            }
            if idx == 0 {
                return;
            }
            idx -= 1;
        }
        let t = points[idx];
        let mut head = points[..=idx].to_vec();
        if (t.x - stub.x).abs() > EPS && (t.y - stub.y).abs() > EPS {
            let elbow = port_aware_elbow(stub, t, side);
            if (elbow.x - t.x).abs() > EPS || (elbow.y - t.y).abs() > EPS {
                // 避免与 head 末点重复
                if head
                    .last()
                    .is_none_or(|p| (p.x - elbow.x).abs() > EPS || (p.y - elbow.y).abs() > EPS)
                {
                    head.push(elbow);
                }
            }
        }
        head.push(stub);
        head.push(points[last]);
        *points = head;
    }
}

/// 将非正交斜段拆成轴对齐 L 折。
fn force_orthogonal(points: &mut Vec<Point>) {
    if points.len() < 2 {
        return;
    }
    let mut out = vec![points[0]];
    for i in 1..points.len() {
        let curr = *out.last().unwrap();
        let next = points[i];
        let dx = next.x - curr.x;
        let dy = next.y - curr.y;
        if dx.abs() > EPS && dy.abs() > EPS {
            let elbow = if out.len() >= 2 {
                let prev = out[out.len() - 2];
                let came_vert = (curr.x - prev.x).abs() < EPS;
                if came_vert {
                    Point::new(curr.x, next.y)
                } else {
                    Point::new(next.x, curr.y)
                }
            } else {
                Point::new(next.x, curr.y)
            };
            if (elbow.x - curr.x).abs() > EPS || (elbow.y - curr.y).abs() > EPS {
                out.push(elbow);
            }
        }
        let last = *out.last().unwrap();
        if (last.x - next.x).abs() > EPS || (last.y - next.y).abs() > EPS {
            out.push(next);
        }
    }
    *points = out;
}

/// 折叠短正交微折与单臂短台阶。
///
/// `merge_overshoot=true` 时，用「相邻段共线合并分数」选择对齐角，可消除
/// 「冲过端口再折回」的 overshoot Z 折；false 时沿用最近距离启发式（保守）。
fn collapse_micro_jogs(points: &mut Vec<Point>, merge_overshoot: bool) {
    if points.len() < 4 {
        return;
    }
    let mut changed = true;
    let mut guard = 0;
    while changed && guard < 8 {
        changed = false;
        guard += 1;
        let mut i = 1;
        // 允许改到倒数第二个折点（保留最终锚点）；stub 由 ensure_outward_stub 补回
        while i + 1 < points.len() {
            let prev = points[i - 1];
            let curr = points[i];
            let next = points[i + 1];
            let d1 = seg_len(prev, curr);
            let d2 = seg_len(curr, next);
            if d1.min(d2) >= MICRO_JOG_LEN {
                i += 1;
                continue;
            }

            // 共线：直接删 curr
            if (prev.x - next.x).abs() < EPS || (prev.y - next.y).abs() < EPS {
                // 避免删掉唯一外向 stub：若 curr 是 index 1 且 d1≈PORT_CLEARANCE 且 next 不在外向延长线上
                // 共线时删掉安全
                points.remove(i);
                changed = true;
                continue;
            }

            // 两段都短，或一段极短：用对齐角替换 curr。
            //
            // 两个候选角 cand_a/cand_b 端点相同、都合法；关键是选能与相邻段**共线合并**
            // 的那个，从而让后续 simplify 删点、消除「冲过端口再折回」的 overshoot Z 折。
            // 仅按「离 curr 最近」选会在 cand==curr 时死锁（overshoot 永不消除），
            // 且可能把本可合并的直段翻成镜像 L。改为优先合并分数，平局再退回最近距离。
            if d1 < MICRO_JOG_LEN || d2 < MICRO_JOG_LEN {
                let cand_a = Point::new(next.x, prev.y);
                let cand_b = Point::new(prev.x, next.y);
                let before = if i >= 2 { Some(points[i - 2]) } else { None };
                let after = if i + 2 < points.len() {
                    Some(points[i + 2])
                } else {
                    None
                };
                // cand_a：prev→cand_a 沿 y=prev.y（水平），cand_a→next 沿 x=next.x（竖直）
                let mut score_a = 0i32;
                if before.is_some_and(|b| (b.y - prev.y).abs() < EPS) {
                    score_a += 1;
                }
                if after.is_some_and(|a| (a.x - next.x).abs() < EPS) {
                    score_a += 1;
                }
                // cand_b：prev→cand_b 沿 x=prev.x（竖直），cand_b→next 沿 y=next.y（水平）
                let mut score_b = 0i32;
                if before.is_some_and(|b| (b.x - prev.x).abs() < EPS) {
                    score_b += 1;
                }
                if after.is_some_and(|a| (a.y - next.y).abs() < EPS) {
                    score_b += 1;
                }

                let da = (cand_a.x - curr.x).abs() + (cand_a.y - curr.y).abs();
                let db = (cand_b.x - curr.x).abs() + (cand_b.y - curr.y).abs();
                let new_c = if merge_overshoot && score_a != score_b {
                    if score_a > score_b {
                        cand_a
                    } else {
                        cand_b
                    }
                } else if da <= db {
                    cand_a
                } else {
                    cand_b
                };
                if (new_c.x - curr.x).abs() > EPS || (new_c.y - curr.y).abs() > EPS {
                    points[i] = new_c;
                    changed = true;
                    continue;
                }
            }
            i += 1;
        }
        *points = simplify_path(std::mem::take(points), false);
    }
}

fn seg_len(a: Point, b: Point) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    (dx * dx + dy * dy).sqrt()
}

/// 首段是否沿端口外向反向（供检测与测试）。
pub fn first_segment_is_reverse(points: &[Point], side: Port) -> bool {
    if points.len() < 2 {
        return false;
    }
    let (ox, oy) = port_outward(side);
    let dx = points[1].x - points[0].x;
    let dy = points[1].y - points[0].y;
    dx * ox + dy * oy < -1.0
}

/// 路径是否含非正交段。
pub fn has_non_orthogonal_segment(points: &[Point]) -> bool {
    points.windows(2).any(|w| {
        let dx = (w[1].x - w[0].x).abs();
        let dy = (w[1].y - w[0].y).abs();
        dx > EPS && dy > EPS
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Port;

    #[test]
    fn fixes_bottom_port_upward_micro_stub() {
        let mut pts = vec![
            Point::new(177.0, 814.0),
            Point::new(177.0, 796.0),
            Point::new(150.0, 796.0),
            Point::new(150.0, 900.0),
            Point::new(177.0, 998.0),
        ];
        assert!(first_segment_is_reverse(&pts, Port::Bottom));
        fix_endpoint_reverse_stub(&mut pts, true, Port::Bottom);
        assert!(!first_segment_is_reverse(&pts, Port::Bottom));
        assert!(pts[1].y > pts[0].y);
        assert!(pts.iter().skip(1).all(|p| p.y >= pts[0].y - 1.0));
    }

    #[test]
    fn force_orthogonal_splits_diagonal() {
        let mut pts = vec![
            Point::new(126.0, 892.0),
            Point::new(180.0, 899.0),
            Point::new(180.0, 998.0),
        ];
        force_orthogonal(&mut pts);
        assert!(!has_non_orthogonal_segment(&pts));
        assert!(pts.len() >= 3);
    }

    #[test]
    fn sanitize_preserves_intentional_detour() {
        // 空间契约：消毒不得折叠绕障 U 折
        let mut pts = vec![
            Point::new(176.5, 814.0),
            Point::new(176.5, 830.0),
            Point::new(150.0, 830.0),
            Point::new(150.0, 899.0),
            Point::new(176.5, 899.0),
            Point::new(176.5, 998.0),
        ];
        sanitize_polyline(&mut pts, Port::Bottom, Port::Top);
        assert!(!has_non_orthogonal_segment(&pts));
        assert!(!first_segment_is_reverse(&pts, Port::Bottom));
        // 绕行通道应保留（x≈150）
        assert!(
            pts.iter().any(|p| (p.x - 150.0).abs() < 1.0),
            "detour should be preserved, pts={:?}",
            pts
        );
    }

    #[test]
    fn collapse_read_data_end_stair() {
        let mut pts = vec![
            Point::new(181.0, 814.0),
            Point::new(181.0, 830.0),
            Point::new(190.0, 830.0),
            Point::new(190.0, 916.0),
            Point::new(326.0, 916.0),
            Point::new(326.0, 935.0),
            Point::new(316.0, 935.0),
            Point::new(316.0, 998.0),
        ];
        sanitize_polyline(&mut pts, Port::Bottom, Port::Top);
        assert!(!has_non_orthogonal_segment(&pts));
        assert!(!first_segment_is_reverse(&pts, Port::Bottom));
    }

    #[test]
    fn sanitize_batch_write_diagonal() {
        let mut pts = vec![
            Point::new(305.0, 724.0),
            Point::new(305.0, 740.0),
            Point::new(278.0, 740.0),
            Point::new(278.0, 880.0),
            Point::new(158.0, 880.0),
            Point::new(158.0, 892.0),
            Point::new(126.0, 892.0),
            Point::new(180.0, 899.0),
            Point::new(180.0, 998.0),
        ];
        sanitize_polyline(&mut pts, Port::Bottom, Port::Top);
        assert!(
            !has_non_orthogonal_segment(&pts),
            "diagonal remains: {:?}",
            pts
        );
        assert!(!first_segment_is_reverse(&pts, Port::Bottom));
    }

    #[test]
    fn collapse_short_collinear_jog() {
        let mut pts = vec![
            Point::new(0.0, 0.0),
            Point::new(0.0, 50.0),
            Point::new(0.0, 58.0),
            Point::new(0.0, 100.0),
        ];
        collapse_micro_jogs(&mut pts, false);
        assert!(
            pts.len() <= 3,
            "collinear micro jog should collapse, got {} points: {:?}",
            pts.len(),
            pts
        );
    }

    #[test]
    fn collapse_short_z_replaces_corner() {
        let mut pts = vec![
            Point::new(0.0, 0.0),
            Point::new(0.0, 40.0),
            Point::new(8.0, 40.0),
            Point::new(8.0, 48.0),
            Point::new(8.0, 100.0),
        ];
        let before = pts.len();
        collapse_micro_jogs(&mut pts, true);
        assert!(pts.len() <= before);
    }
}
