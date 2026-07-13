//! 标签候选位枚举 + 打分选优
//!
//! 在迭代推开之前，沿边路径生成有限候选框并打分，优先消除标签-节点硬冲突。

use crate::layout::constants::DEFAULT_LABEL_PERP_OFFSET;
use crate::layout::edge::common::edge_geometry::{closest_point_on_path, point_at_path_t};
use crate::layout::edge::common::label_avoidance::{
    aabb_overlap, label_bbox_overlaps_group_shell, segment_vs_aabb_intersect,
};
use crate::layout::geometry::Point;
use crate::layout::group::constants::GROUP_BORDER_SHELL_PAD;
use crate::layout::{EdgeLayout, GroupLayout, NodeLayout};
use crate::types::DiagramType;
use std::collections::HashMap;

use super::label_common::{build_edge_segments, collect_label_keys, sorted_node_obstacles};

const REJECT_SCORE: f64 = f64::INFINITY;
const LABEL_OVERLAP_PENALTY: f64 = 1000.0;
const FOREIGN_EDGE_PENALTY: f64 = 100.0;
const GROUP_OVERLAP_PENALTY: f64 = 50.0;
const PATH_DISTANCE_WEIGHT: f64 = 0.5;
const MIDPOINT_DISTANCE_WEIGHT: f64 = 0.1;
/// 长标签放在足够长的直线段上时的 whitespace 奖励（architecture Phase 4）。
const LONG_SEGMENT_WHITESPACE_BONUS: f64 = 25.0;
const LONG_LABEL_MIN_WIDTH: f64 = 48.0;
const LONG_SEGMENT_MIN_RATIO: f64 = 1.25;
/// 节点重叠：高有限惩罚（>LABEL_OVERLAP_PENALTY），按重叠面积加权。
/// 不用 INFINITY 以保证「所有候选都碰节点」时仍能选出重叠最小的候选，
/// 避免回退到原始冲突位置（label_avoidance Phase 2 不处理 label-node）。
const NODE_OVERLAP_PENALTY: f64 = 10000.0;
const NODE_OVERLAP_AREA_WEIGHT: f64 = 100.0;

/// 图种相关的标签候选打分策略（Phase 4 architecture 专项）。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LabelPlacementConfig {
    /// 偏好将较长标签放在有足够 whitespace 的直线段旁
    pub prefer_long_segment_whitespace: bool,
    /// 路径端点附近的标签与 group 边框壳层重叠时减轻惩罚
    pub soften_endpoint_group_shell: bool,
}

impl LabelPlacementConfig {
    pub fn for_diagram_type(diagram_type: DiagramType) -> Self {
        match diagram_type {
            DiagramType::Architecture | DiagramType::Flowchart => Self {
                prefer_long_segment_whitespace: true,
                soften_endpoint_group_shell: matches!(diagram_type, DiagramType::Architecture),
            },
            _ => Self::default(),
        }
    }
}

/// 沿边路径为所有标签做候选位放置（按边序贪心占位）。
pub fn place_all_labels_by_candidates(
    edges: &mut [EdgeLayout],
    nodes: &HashMap<String, NodeLayout>,
    groups: &HashMap<String, GroupLayout>,
) {
    place_all_labels_by_candidates_with_config(
        edges,
        nodes,
        groups,
        LabelPlacementConfig::default(),
    );
}

/// 带图种策略的候选位放置。
pub fn place_all_labels_by_candidates_with_config(
    edges: &mut [EdgeLayout],
    nodes: &HashMap<String, NodeLayout>,
    groups: &HashMap<String, GroupLayout>,
    config: LabelPlacementConfig,
) {
    let label_keys = collect_label_keys(edges);
    if label_keys.is_empty() {
        return;
    }

    let node_obstacles: Vec<(f64, f64, f64, f64)> = sorted_node_obstacles(nodes);
    let edge_segments = build_edge_segments(edges);

    let mut placed_bboxes: Vec<(f64, f64, f64, f64)> = Vec::new();

    for key in label_keys {
        let (edge_idx, label_idx) = key;
        let path = edges[edge_idx].path_points().into_owned();
        if path.len() < 2 {
            continue;
        }

        let label = &edges[edge_idx].labels[label_idx];
        let size = label.size;
        if size.0 <= 0.0 || size.1 <= 0.0 {
            continue;
        }

        let current = label.center;
        let current_bbox = bbox_from_center(current, size);
        let preferred_t = preferred_t_for_label(label_idx, &path, current);

        let has_conflict = placement_has_conflict(
            current_bbox,
            edge_idx,
            &path,
            &placed_bboxes,
            &node_obstacles,
            &edge_segments,
        );

        if has_conflict {
            let candidates = generate_candidates(&path, preferred_t, size);
            let ctx = CandidateScoringContext {
                edge_idx,
                path: &path,
                preferred_t,
                label_size: size,
                placed_bboxes: &placed_bboxes,
                node_obstacles: &node_obstacles,
                groups,
                edge_segments: &edge_segments,
                config,
            };
            // 同时为当前位置打分，确保候选比当前位置更好才移动（避免劣化）
            let current_score = score_candidate(current, current_bbox, &ctx);
            let best = candidates
                .into_iter()
                .map(|center| {
                    let bbox = bbox_from_center(center, size);
                    let score = score_candidate(center, bbox, &ctx);
                    (score, center)
                })
                .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

            if let Some((score, center)) = best {
                // 候选优于当前位置才移动；节点重叠现为有限惩罚，
                // 保证「全候选碰节点」时仍选出最小重叠候选（优于原始冲突位置）。
                if score < current_score {
                    edges[edge_idx].set_label_pos_at(label_idx, center);
                    placed_bboxes.push(bbox_from_center(center, size));
                    continue;
                }
            }
        }

        if let Some(pos) = edges[edge_idx].label_pos_at(label_idx) {
            placed_bboxes.push(bbox_from_center(pos, size));
        }
    }
}

fn placement_has_conflict(
    bbox: (f64, f64, f64, f64),
    edge_idx: usize,
    path: &[Point],
    placed_bboxes: &[(f64, f64, f64, f64)],
    node_obstacles: &[(f64, f64, f64, f64)],
    edge_segments: &[Vec<(Point, Point)>],
) -> bool {
    for node_bbox in node_obstacles {
        if aabb_overlap(&bbox, node_bbox).is_some() {
            return true;
        }
    }
    for placed in placed_bboxes {
        if aabb_overlap(&bbox, placed).is_some() {
            return true;
        }
    }
    if let Some(segs) = edge_segments.get(edge_idx) {
        for &(p1, p2) in segs {
            if segment_vs_aabb_intersect(p1, p2, bbox) {
                return true;
            }
        }
    }
    for (seg_edge_idx, segs) in edge_segments.iter().enumerate() {
        if seg_edge_idx == edge_idx {
            continue;
        }
        for &(p1, p2) in segs {
            if segment_vs_aabb_intersect(p1, p2, bbox) {
                return true;
            }
        }
    }
    let _ = path;
    false
}

fn preferred_t_for_label(label_idx: usize, path: &[Point], current: Point) -> f64 {
    let anchor_t = match label_idx {
        0 => 0.5,
        1 => 0.15,
        _ => 0.85,
    };
    let (closest, dist) = closest_point_on_path(path, current);
    if dist.is_finite() && dist < 80.0 {
        let total_len: f64 = path
            .windows(2)
            .map(|w| {
                let dx = w[1].x - w[0].x;
                let dy = w[1].y - w[0].y;
                (dx * dx + dy * dy).sqrt()
            })
            .sum();
        if total_len > 1e-6 {
            let mut accum = 0.0;
            for w in path.windows(2) {
                let seg_len = {
                    let dx = w[1].x - w[0].x;
                    let dy = w[1].y - w[0].y;
                    (dx * dx + dy * dy).sqrt()
                };
                let seg_dist = {
                    let dx = closest.x - w[0].x;
                    let dy = closest.y - w[0].y;
                    (dx * dx + dy * dy).sqrt()
                };
                if seg_dist <= seg_len + 1.0 {
                    let local_t = if seg_len > 1e-6 {
                        seg_dist / seg_len
                    } else {
                        0.0
                    };
                    let t = (accum + seg_len * local_t) / total_len;
                    return t.clamp(0.05, 0.95).mul_add(0.35, anchor_t * 0.65);
                }
                accum += seg_len;
            }
        }
    }
    anchor_t
}

fn generate_candidates(path: &[Point], preferred_t: f64, size: (f64, f64)) -> Vec<Point> {
    // 覆盖短边场景：端点附近 (0.15/0.85) 增加候选，中段保持 0.3/0.5/0.7
    let mut ts = vec![0.15, 0.3, 0.5, 0.7, 0.85, preferred_t];
    ts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    ts.dedup_by(|a, b| (*a - *b).abs() < 0.05);

    // 外偏需能覆盖半标签宽，否则 fan 组看得见冲突却移不动
    let half_w = (size.0 * 0.5 + 6.0).max(DEFAULT_LABEL_PERP_OFFSET * 3.0);
    let distances = [
        DEFAULT_LABEL_PERP_OFFSET,
        DEFAULT_LABEL_PERP_OFFSET * 2.0,
        DEFAULT_LABEL_PERP_OFFSET * 3.0,
        half_w,
    ];

    let mut candidates = Vec::new();
    for t in ts {
        let (normal, anchor) = normal_at_path_t(path, t);
        for sign in [1.0, -1.0] {
            for &dist in &distances {
                let offset = dist * sign;
                candidates.push(Point::new(
                    anchor.x + normal.x * offset,
                    anchor.y + normal.y * offset,
                ));
            }
        }
    }
    candidates
}

fn normal_at_path_t(path: &[Point], t: f64) -> (Point, Point) {
    let t0 = (t - 0.01).max(0.0);
    let t1 = (t + 0.01).min(1.0);
    let p0 = point_at_path_t(path, t0);
    let p1 = point_at_path_t(path, t1);
    let dx = p1.x - p0.x;
    let dy = p1.y - p0.y;
    let len = (dx * dx + dy * dy).sqrt().max(1e-9);
    let normal = Point::new(-dy / len, dx / len);
    (normal, point_at_path_t(path, t))
}

fn bbox_from_center(center: Point, size: (f64, f64)) -> (f64, f64, f64, f64) {
    let (w, h) = size;
    (
        center.x - w / 2.0,
        center.y - h / 2.0,
        center.x + w / 2.0,
        center.y + h / 2.0,
    )
}

/// Shared scoring context for label candidate evaluation.
/// Constructed once per edge, reused across all candidates.
struct CandidateScoringContext<'a> {
    edge_idx: usize,
    path: &'a [Point],
    preferred_t: f64,
    label_size: (f64, f64),
    placed_bboxes: &'a [(f64, f64, f64, f64)],
    node_obstacles: &'a [(f64, f64, f64, f64)],
    groups: &'a HashMap<String, GroupLayout>,
    edge_segments: &'a [Vec<(Point, Point)>],
    config: LabelPlacementConfig,
}

fn score_candidate(
    center: Point,
    bbox: (f64, f64, f64, f64),
    ctx: &CandidateScoringContext,
) -> f64 {
    // 节点重叠：高有限惩罚（非 INFINITY），按重叠面积加权。
    // 保证「全部候选都碰节点」时仍能选出最小重叠候选，而非回退原始冲突位置。
    let mut score = 0.0;
    for node_bbox in ctx.node_obstacles {
        if let Some((ox, oy)) = aabb_overlap(&bbox, node_bbox) {
            score += NODE_OVERLAP_PENALTY + ox * oy * NODE_OVERLAP_AREA_WEIGHT;
        }
    }

    for placed in ctx.placed_bboxes {
        if let Some((ox, oy)) = aabb_overlap(&bbox, placed) {
            score += LABEL_OVERLAP_PENALTY + ox * oy;
        }
    }

    for id in {
        let mut ids: Vec<&String> = ctx.groups.keys().collect();
        ids.sort();
        ids
    } {
        if label_bbox_overlaps_group_shell(&bbox, &ctx.groups[id], GROUP_BORDER_SHELL_PAD) {
            let near_endpoint = ctx.config.soften_endpoint_group_shell
                && path_t_for_center(ctx.path, center).is_some_and(|t| t <= 0.25 || t >= 0.75);
            let penalty = if near_endpoint {
                GROUP_OVERLAP_PENALTY * 0.2
            } else {
                GROUP_OVERLAP_PENALTY
            };
            score += penalty;
        }
    }

    for (seg_edge_idx, segs) in ctx.edge_segments.iter().enumerate() {
        if seg_edge_idx == ctx.edge_idx {
            continue;
        }
        for &(p1, p2) in segs {
            if segment_vs_aabb_intersect(p1, p2, bbox) {
                score += FOREIGN_EDGE_PENALTY;
                break;
            }
        }
    }

    let (_, path_dist) = closest_point_on_path(ctx.path, center);
    score += path_dist * PATH_DISTANCE_WEIGHT;

    let midpoint = point_at_path_t(ctx.path, 0.5);
    let mid_dist = ((center.x - midpoint.x).powi(2) + (center.y - midpoint.y).powi(2)).sqrt();
    score += mid_dist * MIDPOINT_DISTANCE_WEIGHT;

    let preferred_point = point_at_path_t(ctx.path, ctx.preferred_t);
    let pref_dist =
        ((center.x - preferred_point.x).powi(2) + (center.y - preferred_point.y).powi(2)).sqrt();
    score += pref_dist * 0.05;

    if ctx.config.prefer_long_segment_whitespace && ctx.label_size.0 >= LONG_LABEL_MIN_WIDTH {
        let seg_len = segment_length_at_center(ctx.path, center);
        if seg_len >= ctx.label_size.0 * LONG_SEGMENT_MIN_RATIO {
            let bonus = (seg_len / ctx.label_size.0).min(3.0) * LONG_SEGMENT_WHITESPACE_BONUS;
            score -= bonus;
        }
    }

    score
}

fn path_t_for_center(path: &[Point], center: Point) -> Option<f64> {
    let total_len: f64 = path
        .windows(2)
        .map(|w| segment_length(w[0], w[1]))
        .sum();
    if total_len <= 1e-6 {
        return None;
    }
    let (closest, _) = closest_point_on_path(path, center);
    let mut accum = 0.0;
    for w in path.windows(2) {
        let seg_len = segment_length(w[0], w[1]);
        let seg_dist = segment_length(w[0], closest);
        if seg_dist <= seg_len + 1.0 {
            let local_t = if seg_len > 1e-6 {
                seg_dist / seg_len
            } else {
                0.0
            };
            return Some(((accum + seg_len * local_t) / total_len).clamp(0.0, 1.0));
        }
        accum += seg_len;
    }
    None
}

fn segment_length(p0: Point, p1: Point) -> f64 {
    let dx = p1.x - p0.x;
    let dy = p1.y - p0.y;
    (dx * dx + dy * dy).sqrt()
}

/// 标签中心最近路径段的长度（用于 whitespace 奖励）。
fn segment_length_at_center(path: &[Point], center: Point) -> f64 {
    if path.len() < 2 {
        return 0.0;
    }
    let (closest, _) = closest_point_on_path(path, center);
    let mut best: f64 = 0.0;
    for w in path.windows(2) {
        let seg_len = segment_length(w[0], w[1]);
        let d0 = segment_length(w[0], closest);
        let d1 = segment_length(w[1], closest);
        if d0 + d1 <= seg_len + 2.0 {
            best = best.max(seg_len);
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{EdgeLabelLayout, PathGeometry, Port};
    use crate::types::DiagramType;

    fn labeled_edge(center: Point) -> EdgeLayout {
        EdgeLayout {
            geometry: PathGeometry::Straight {
                start: Point::new(0.0, 0.0),
                end: Point::new(200.0, 0.0),
            },
            labels: vec![EdgeLabelLayout::new("测试标签", center)],
            from_port: Port::Bottom,
            to_port: Port::Top,
        }
    }

    #[test]
    fn architecture_config_prefers_long_segment_whitespace() {
        let cfg = LabelPlacementConfig::for_diagram_type(DiagramType::Architecture);
        assert!(cfg.prefer_long_segment_whitespace);
        assert!(cfg.soften_endpoint_group_shell);

        let flow = LabelPlacementConfig::for_diagram_type(DiagramType::Flowchart);
        assert!(flow.prefer_long_segment_whitespace);
        assert!(!flow.soften_endpoint_group_shell);
    }

    #[test]
    fn candidate_skips_clean_placement() {
        let mut edges = vec![labeled_edge(Point::new(50.0, 100.0))];
        let nodes = HashMap::new();
        let groups = HashMap::new();

        let before = edges[0].label_pos();
        place_all_labels_by_candidates(&mut edges, &nodes, &groups);
        let after = edges[0].label_pos();

        assert_eq!(
            before, after,
            "clean label should not be moved by candidate placement"
        );
    }

    #[test]
    fn candidate_separates_two_labels() {
        let mut edges = vec![
            labeled_edge(Point::new(100.0, 0.0)),
            labeled_edge(Point::new(100.0, 0.0)),
        ];
        let nodes = HashMap::new();
        let groups = HashMap::new();

        place_all_labels_by_candidates(&mut edges, &nodes, &groups);

        let b0 = edges[0].label_bbox();
        let b1 = edges[1].label_bbox();
        assert!(
            aabb_overlap(&b0, &b1).is_none(),
            "labels should be separated, b0={b0:?} b1={b1:?}"
        );
    }

    #[test]
    fn bbox_from_center_contains_center() {
        let (w, h) = crate::layout::edge::common::label_avoidance::label_metrics("ab");
        let bbox = bbox_from_center(Point::new(10.0, 20.0), (w, h));
        assert!(bbox.0 <= 10.0 && bbox.2 >= 10.0);
        assert!(bbox.1 <= 20.0 && bbox.3 >= 20.0);
    }
}
