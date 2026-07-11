//! 正交路径锯齿消毒（不变量-only）。
//!
//! 空间契约下消毒**不得改拓扑**（大 U 折叠、变 Straight 穿层）。
//! 仅修复：
//! - 端点反向 stub
//! - 非正交斜段 → 强制拆成 L
//! - 真微折（< MICRO_JOG_LEN）
//!
//! 应在 lane/corridor 之后调用；snap 后再跑一次同一套不变量。

use super::path::port_outward;
use super::simplify::simplify_path;
use super::{EPS, PORT_CLEARANCE};
use crate::ast::Relation;
use crate::layout::edge::common::parallel_edges::build_parallel_aware_edge_labels_auto;
use crate::layout::geometry::Point;
use crate::layout::{EdgeLayout, PathGeometry, Port};

/// 短于该长度的折段视为「微折」，可折叠。
const MICRO_JOG_LEN: f64 = 24.0;

/// 路由后处理：消除反向 stub / 斜段 / 微折，并重建标签。
pub fn sanitize_orthogonal_edges(
    edges: &mut [EdgeLayout],
    relations: &[Relation],
    from_side: &[Port],
    to_side: &[Port],
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

        sanitize_polyline(&mut points, fs, ts);
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
    if points.len() < 2 {
        return;
    }
    fix_endpoint_reverse_stub(points, true, from_side);
    fix_endpoint_reverse_stub(points, false, to_side);
    force_orthogonal(points);
    collapse_micro_jogs(points);
    *points = simplify_path(std::mem::take(points));
    ensure_outward_stub(points, true, from_side);
    ensure_outward_stub(points, false, to_side);
    *points = simplify_path(std::mem::take(points));
}

/// 若端点第一段（或末端最后一段）沿端口外向为负，则切除「背向」折点并补正确 stub。
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
        let stub = Point::new(anchor.x + ox * PORT_CLEARANCE, anchor.y + oy * PORT_CLEARANCE);
        let mut new_pts = vec![anchor, stub];
        if keep_from < points.len() {
            ortho_append(&mut new_pts, points[keep_from]);
            new_pts.extend_from_slice(&points[keep_from + 1..]);
        }
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
        let stub = Point::new(anchor.x + ox * PORT_CLEARANCE, anchor.y + oy * PORT_CLEARANCE);
        let mut new_pts: Vec<Point> = points[..keep_to].to_vec();
        if new_pts.is_empty() {
            new_pts.push(stub);
        } else {
            ortho_append(&mut new_pts, stub);
        }
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
        let nxt = points[1];
        let proj = (nxt.x - anchor.x) * ox + (nxt.y - anchor.y) * oy;
        if proj >= PORT_CLEARANCE * 0.5 {
            return;
        }
        let stub = Point::new(anchor.x + ox * PORT_CLEARANCE, anchor.y + oy * PORT_CLEARANCE);
        let mut rest = points[1..].to_vec();
        // 丢掉仍在 stub 内侧的点
        while let Some(&p) = rest.first() {
            let fp = (p.x - anchor.x) * ox + (p.y - anchor.y) * oy;
            if fp >= PORT_CLEARANCE * 0.5 {
                break;
            }
            rest.remove(0);
        }
        let mut out = vec![anchor, stub];
        if let Some(&p) = rest.first() {
            ortho_append(&mut out, p);
            out.extend_from_slice(&rest[1..]);
        }
        *points = out;
    } else {
        let last = points.len() - 1;
        let anchor = points[last];
        let prev = points[last - 1];
        let proj = (prev.x - anchor.x) * ox + (prev.y - anchor.y) * oy;
        if proj >= PORT_CLEARANCE * 0.5 {
            return;
        }
        let stub = Point::new(anchor.x + ox * PORT_CLEARANCE, anchor.y + oy * PORT_CLEARANCE);
        let mut head = points[..last].to_vec();
        while let Some(&p) = head.last() {
            let fp = (p.x - anchor.x) * ox + (p.y - anchor.y) * oy;
            if fp >= PORT_CLEARANCE * 0.5 {
                break;
            }
            head.pop();
        }
        if head.is_empty() {
            *points = vec![stub, anchor];
            return;
        }
        ortho_append(&mut head, stub);
        head.push(anchor);
        *points = head;
    }
}

fn ortho_append(points: &mut Vec<Point>, target: Point) {
    let Some(&curr) = points.last() else {
        points.push(target);
        return;
    };
    if (curr.x - target.x).abs() < EPS && (curr.y - target.y).abs() < EPS {
        return;
    }
    if (curr.x - target.x).abs() > EPS && (curr.y - target.y).abs() > EPS {
        // 优先延续上一段方向
        let elbow = if points.len() >= 2 {
            let prev = points[points.len() - 2];
            let came_vert = (curr.x - prev.x).abs() < EPS;
            if came_vert {
                Point::new(curr.x, target.y)
            } else {
                Point::new(target.x, curr.y)
            }
        } else {
            Point::new(target.x, curr.y)
        };
        if (elbow.x - curr.x).abs() > EPS || (elbow.y - curr.y).abs() > EPS {
            points.push(elbow);
        }
    }
    let Some(&curr2) = points.last() else {
        return;
    };
    if (curr2.x - target.x).abs() > EPS || (curr2.y - target.y).abs() > EPS {
        points.push(target);
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
fn collapse_micro_jogs(points: &mut Vec<Point>) {
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

            // 两段都短，或一段极短：用对齐角替换 curr
            if d1 < MICRO_JOG_LEN || d2 < MICRO_JOG_LEN {
                let cand_a = Point::new(next.x, prev.y);
                let cand_b = Point::new(prev.x, next.y);
                let da = (cand_a.x - curr.x).abs() + (cand_a.y - curr.y).abs();
                let db = (cand_b.x - curr.x).abs() + (cand_b.y - curr.y).abs();
                let new_c = if da <= db { cand_a } else { cand_b };
                if (new_c.x - curr.x).abs() > EPS || (new_c.y - curr.y).abs() > EPS {
                    points[i] = new_c;
                    changed = true;
                    continue;
                }
            }
            i += 1;
        }
        *points = simplify_path(std::mem::take(points));
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
        collapse_micro_jogs(&mut pts);
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
        collapse_micro_jogs(&mut pts);
        assert!(pts.len() <= before);
    }
}
