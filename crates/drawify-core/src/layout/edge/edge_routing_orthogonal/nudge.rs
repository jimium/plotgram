//! X-2: Segment Nudging 轻推后处理。
//!
//! X-1 多轮重路由后仍有残余重合时，对重合段做局部垂直平移（nudge），
//! 通过插入补偿折点保持路径正交，端点锚点不动。
//!
//! 设计原则：
//! - nudge 距离 ≤ EDGE_PARALLEL_GAP（小幅调整，不大幅变形）
//! - 只处理中段（非 stub），端点位置不变
//! - nudge 后检查不穿节点、不产生新重合
//! - 优雅降级：无法 nudge 时跳过，保留原路径

use super::*;
use crate::layout::geometry::{Point, Rect};
use crate::layout::group::GroupRoutingContext;
use crate::layout::{EdgeLayout, NodeLayout};
use std::collections::{HashMap, HashSet};

/// X-2: nudge 最大迭代轮次
const MAX_NUDGE_ROUNDS: usize = 3;

/// X-2: nudge 统计结果
#[derive(Default)]
pub struct NudgeStats {
    pub nudge_rounds: usize,
    pub nudged_segments: usize,
    pub nudge_failed: usize,
}

/// 检查一个段是否与任何节点相交（用于 nudge 碰撞检测）
fn segment_hits_node(a: Point, b: Point, nodes: &HashMap<String, NodeLayout>, sorted_node_ids: &[String]) -> bool {
    let seg_xmin = a.x.min(b.x) - NODE_OBSTACLE_PAD;
    let seg_xmax = a.x.max(b.x) + NODE_OBSTACLE_PAD;
    let seg_ymin = a.y.min(b.y) - NODE_OBSTACLE_PAD;
    let seg_ymax = a.y.max(b.y) + NODE_OBSTACLE_PAD;
    for node_id in sorted_node_ids {
        if let Some(nl) = nodes.get(node_id.as_str()) {
            if nl.x + nl.width < seg_xmin || nl.x > seg_xmax
                || nl.y + nl.height < seg_ymin || nl.y > seg_ymax {
                continue;
            }
            if Rect::from(nl).expanded(NODE_OBSTACLE_PAD).segment_crosses_interior(a, b, EPS) {
                return true;
            }
        }
    }
    false
}

/// 收集所有残余冲突段对（中段，非 stub）。
/// 返回按段长度降序排列的冲突段列表：(edge_index, seg_index, is_horizontal, gap)
fn collect_conflict_segments(
    edges: &[EdgeLayout],
    grid: &SegmentGrid,
    min_gap: f64,
) -> Vec<(usize, usize, bool, f64)> {
    let stub_guard = STUB_GUARD_LENGTH;
    let mut conflicts = Vec::new();
    let mut seen: HashSet<(usize, usize)> = HashSet::new();

    for ei in 0..edges.len() {
        if edges[ei].path_is_empty() {
            continue;
        }
        let points: Vec<Point> = edges[ei].path_points().into_owned();
        if points.len() < 2 {
            continue;
        }
        let n_segs = points.len() - 1;
        for (si, window) in points.windows(2).enumerate() {
            let is_stub = si == 0 || si == n_segs - 1;
            let seg_len = ((window[1].x - window[0].x).powi(2) + (window[1].y - window[0].y).powi(2)).sqrt();
            if is_stub && seg_len <= stub_guard + EPS {
                continue;
            }
            let seg = RoutedSegment {
                x1: window[0].x,
                y1: window[0].y,
                x2: window[1].x,
                y2: window[1].y,
                edge_index: ei,
            };
            let is_horizontal = (seg.y1 - seg.y2).abs() < EPS;
            let expand = min_gap + 4.0;
            for other in grid.query_overlapping(&seg, expand) {
                if other.edge_index == ei {
                    continue;
                }
                if let Some((kind, gap)) = segments_violate_spacing(&seg, other, min_gap) {
                    if matches!(kind, SpacingViolationKind::ExactOverlap | SpacingViolationKind::TightSpacing) {
                        if seen.insert((ei, si)) {
                            conflicts.push((ei, si, is_horizontal, gap));
                        }
                        break;
                    }
                }
            }
        }
    }

    // 按段长度降序排列（优先处理长重合段），边索引+段索引升序保证确定性
    conflicts.sort_by(|a, b| {
        let len_a = seg_len(edges, a.0, a.1);
        let len_b = seg_len(edges, b.0, b.1);
        len_b.partial_cmp(&len_a).unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(&b.0))
            .then(a.1.cmp(&b.1))
    });

    conflicts
}

fn seg_len(edges: &[EdgeLayout], ei: usize, si: usize) -> f64 {
    if edges[ei].path_is_empty() {
        return 0.0;
    }
    let pts = edges[ei].path_points();
    if si + 1 >= pts.len() { return 0.0; }
    let dx = pts[si + 1].x - pts[si].x;
    let dy = pts[si + 1].y - pts[si].y;
    (dx * dx + dy * dy).sqrt()
}

/// 尝试对边 ei 的中段 si 施加 nudge。
/// 返回 nudge 后的新路径（points），失败返回 None。
fn try_nudge(
    edges: &[EdgeLayout],
    grid: &SegmentGrid,
    nodes: &HashMap<String, NodeLayout>,
    sorted_node_ids: &[String],
    ei: usize,
    si: usize,
    direction: f64,
    distance: f64,
    min_gap: f64,
) -> Option<Vec<Point>> {
    let old_pts: Vec<Point> = edges[ei].path_points().into_owned();
    // si 必须是中段（非首段、非末段），且前后有相邻段
    if si == 0 || si + 1 >= old_pts.len() {
        return None;
    }

    let p0 = old_pts[si];       // 段起点
    let p1 = old_pts[si + 1];   // 段终点
    let horiz = (p0.y - p1.y).abs() < EPS;
    let d = distance * direction.signum();

    let (np0, np1) = if horiz {
        (Point::new(p0.x, p0.y + d), Point::new(p1.x, p1.y + d))
    } else {
        (Point::new(p0.x + d, p0.y), Point::new(p1.x + d, p1.y))
    };

    // 三个新增/修改的段：p0→np0（补偿垂直段）、np0→np1（偏移后的平行段）、np1→p1（补偿垂直段）
    let new_segs: [(Point, Point); 3] = [(p0, np0), (np0, np1), (np1, p1)];

    // 检查1：新段不穿越节点
    for &(a, b) in &new_segs {
        if segment_hits_node(a, b, nodes, sorted_node_ids) {
            return None;
        }
    }

    // 检查2：新段不与其他边的段产生新的间距违规
    // 注：当前边 ei 的旧段仍在 grid 中，但我们跳过 edge_index == ei 的段
    for &(a, b) in &new_segs {
        let check_seg = RoutedSegment { x1: a.x, y1: a.y, x2: b.x, y2: b.y, edge_index: ei };
        let expand = min_gap + 4.0;
        for existing in grid.query_overlapping(&check_seg, expand) {
            if existing.edge_index == ei {
                continue;
            }
            if let Some((kind, _g)) = segments_violate_spacing(&check_seg, existing, min_gap) {
                if matches!(kind, SpacingViolationKind::ExactOverlap | SpacingViolationKind::TightSpacing) {
                    return None;
                }
            }
        }
    }

    // 构建新路径
    let mut new_pts: Vec<Point> = Vec::with_capacity(old_pts.len() + 2);
    new_pts.extend_from_slice(&old_pts[..=si]);
    new_pts.push(np0);
    new_pts.push(np1);
    new_pts.extend_from_slice(&old_pts[si + 1..]);
    // 注意：old_pts[si+1] 就是 p1，所以 new_pts 中有: ..., p0, np0, np1, p1, ...
    // 其中 p0 = old_pts[si], p1 = old_pts[si+1]
    // 实际序列是: old_pts[0..si+1] 包含 p0=old_pts[si], 然后 push np0, np1, 然后 old_pts[si+1..] 从 p1 开始
    // 所以段序列: ...→p0→np0→np1→p1→... — 正确！

    Some(new_pts)
}

/// X-2: 主入口——多轮 nudge 消除残余重合。
pub fn nudge_conflicting_segments(
    nodes: &HashMap<String, NodeLayout>,
    relations: &[crate::ast::Relation],
    from_side: &[Port],
    to_side: &[Port],
    edges: &mut Vec<EdgeLayout>,
    grid: &mut SegmentGrid,
    _cfg: &OrthoConfig,
    _group_ctx: &GroupRoutingContext,
    obstacles: &PreparedObstacles,
    ortho_stats: &mut crate::layout::OrthoDebugStats,
) -> NudgeStats {
    let mut stats = NudgeStats::default();
    let n = edges.len();
    if n < 2 {
        return stats;
    }

    for round in 0..MAX_NUDGE_ROUNDS {
        let conflicts = collect_conflict_segments(edges, grid, EDGE_PARALLEL_GAP);
        if conflicts.is_empty() {
            break;
        }
        stats.nudge_rounds = round + 1;

        let mut nudged_this_round: HashSet<usize> = HashSet::new();

        for &(ei, si, horiz, _gap) in &conflicts {
            if nudged_this_round.contains(&ei) {
                continue;
            }
            if edges[ei].path_is_empty() {
                continue;
            }
            // 重检：该段是否仍冲突？
            let pts: Vec<Point> = edges[ei].path_points().into_owned();
            if si + 1 >= pts.len() {
                continue;
            }
            let recheck = RoutedSegment {
                x1: pts[si].x, y1: pts[si].y,
                x2: pts[si+1].x, y2: pts[si+1].y,
                edge_index: ei,
            };
            let mut still_bad = false;
            let ex = EDGE_PARALLEL_GAP + 4.0;
            for existing in grid.query_overlapping(&recheck, ex) {
                if existing.edge_index == ei { continue; }
                if let Some((k, _)) = segments_violate_spacing(&recheck, existing, EDGE_PARALLEL_GAP) {
                    if matches!(k, SpacingViolationKind::ExactOverlap | SpacingViolationKind::TightSpacing) {
                        still_bad = true;
                        break;
                    }
                }
            }
            if !still_bad { continue; }

            // 选择 nudge 方向：水平段先试+Y（向上），垂直段先试-X（向左）
            // 交替选择方向以分散偏移方向
            let flip = (ei + si) % 2 == 1;
            let dirs: [f64; 2] = if horiz {
                if flip { [-1.0, 1.0] } else { [1.0, -1.0] }
            } else {
                if flip { [1.0, -1.0] } else { [-1.0, 1.0] }
            };

            let mut success = false;
            for &dir in &dirs {
                // 尝试全距离
                if let Some(new_pts) = try_nudge(
                    edges, grid, nodes, &obstacles.sorted_node_ids,
                    ei, si, dir, EDGE_PARALLEL_GAP, EDGE_PARALLEL_GAP,
                ) {
                    apply_nudge(edges, grid, relations, from_side, to_side, ei, new_pts);
                    nudged_this_round.insert(ei);
                    stats.nudged_segments += 1;
                    success = true;
                    break;
                }
            }

            if !success {
                stats.nudge_failed += 1;
            }
        }

        if nudged_this_round.is_empty() {
            break;
        }
    }

    ortho_stats.nudge_iterations = stats.nudge_rounds;
    ortho_stats.nudged_segments = stats.nudged_segments;
    ortho_stats.nudge_failed = stats.nudge_failed;
    stats
}

fn apply_nudge(
    edges: &mut Vec<EdgeLayout>,
    grid: &mut SegmentGrid,
    relations: &[crate::ast::Relation],
    from_side: &[Port],
    to_side: &[Port],
    ei: usize,
    new_pts: Vec<Point>,
) {
    grid.remove_by_edges(&[ei]);
    let labels = if new_pts.len() >= 2 {
        match relations.get(ei) {
            Some(rel) => {
                let middle_t = parse_label_t(rel);
                build_edge_labels(
                    rel, middle_t, Point::new(0.0, 0.0),
                    |t| point_at_path_t(&new_pts, t),
                )
            }
            None => Vec::new(),
        }
    } else { Vec::new() };
    grid.insert_path(&new_pts, ei);
    let mut edge = EdgeLayout {
        geometry: PathGeometry::Polyline { points: Vec::new() },
        labels,
        from_port: from_side[ei],
        to_port: to_side[ei],
    };
    edge.set_polyline_points(new_pts);
    edges[ei] = edge;
}
