//! 标签自动避让
//!
//! 在初始标签定位后，通过迭代式碰撞检测 + 最小位移策略
//! 自动消除标签-标签、标签-节点、标签-分组边框、标签-边路径的重叠。
//!
//! 核心语义：`EdgeLabelLayout.center` 为标签包围框的几何中心
//! （不再有"基线"与"中心"的歧义），所有 bbox 计算基于中心语义。
//!
//! 鲁棒性增强（P1-B）：
//! - **震荡检测**：记录每轮位移方向，方向反转且幅度相近时标记震荡并跳过
//! - **推开后回检**：推开后立即回检所有障碍，避免引入新重叠
//! - **统一标签尺寸**：`label_metrics` 提供统一的宽高估算
//!
//! 多标签支持（P2-1）：每条边可携带多个标签（中段/头部/尾部），
//! 避障以 `(edge_idx, label_idx)` 为最小单元独立处理每个标签。

use crate::layout::geometry::{Point, Rect};
use crate::layout::{EdgeLayout, GroupLayout, NodeLayout};
use crate::layout::constants::*;
use crate::layout::edge::common::edge_geometry::closest_point_on_path;
use std::collections::{HashMap, HashSet};

const EPS: f64 = 1e-6;

type LabelKey = (usize, usize);

pub fn label_metrics(text: &str) -> (f64, f64) {
    let width = estimate_label_width(text) + DEFAULT_LABEL_PADDING * 2.0;
    let height = DEFAULT_LABEL_FONT_SIZE + DEFAULT_LABEL_PADDING * 2.0;
    (width, height)
}

pub fn resolve_label_overlaps(
    edges: &mut [EdgeLayout],
    nodes: &HashMap<String, NodeLayout>,
    groups: &HashMap<String, GroupLayout>,
) {
    let label_keys: Vec<LabelKey> = edges
        .iter()
        .enumerate()
        .flat_map(|(i, e)| {
            if e.path_len() < 2 {
                Vec::new()
            } else {
                (0..e.labels.len()).map(move |li| (i, li)).collect()
            }
        })
        .collect();

    if label_keys.is_empty() {
        return;
    }

    let initial_positions: HashMap<LabelKey, Point> = label_keys
        .iter()
        .filter_map(|&k| edges[k.0].label_pos_at(k.1).map(|p| (k, p)))
        .collect();

    let edge_segments: Vec<Vec<(Point, Point)>> = edges
        .iter()
        .map(|e| {
            if e.path_len() < 2 {
                return Vec::new();
            }
            let path = e.path_points().into_owned();
            path.windows(2).map(|w| (w[0], w[1])).collect()
        })
        .collect();

    let node_obstacles: Vec<(f64, f64, f64, f64)> = nodes
        .values()
        .map(|nl| (nl.x, nl.y, nl.x + nl.width, nl.y + nl.height))
        .collect();
    let group_obstacles: Vec<(f64, f64, f64, f64)> = groups
        .values()
        .map(|gl| (gl.x, gl.y, gl.x + gl.width, gl.y + gl.height))
        .collect();

    let mut last_delta: HashMap<LabelKey, Point> = HashMap::new();
    let mut oscillating: HashSet<LabelKey> = HashSet::new();

    for _ in 0..DEFAULT_MAX_LABEL_ITERATIONS {
        let prev_positions: HashMap<LabelKey, Point> = label_keys
            .iter()
            .filter(|&&k| !oscillating.contains(&k))
            .filter_map(|&k| edges[k.0].label_pos_at(k.1).map(|p| (k, p)))
            .collect();

        let mut moved = false;

        for a in 0..label_keys.len() {
            for b in (a + 1)..label_keys.len() {
                let ka = label_keys[a];
                let kb = label_keys[b];
                if oscillating.contains(&ka) || oscillating.contains(&kb) {
                    continue;
                }

                let bbox_a = match edges[ka.0].label_bbox_at(ka.1) {
                    Some(b) => b,
                    None => continue,
                };
                let bbox_b = match edges[kb.0].label_bbox_at(kb.1) {
                    Some(b) => b,
                    None => continue,
                };

                if let Some((dx, dy)) = aabb_overlap(&bbox_a, &bbox_b) {
                    let pos_a = match edges[ka.0].label_pos_at(ka.1) {
                        Some(p) => p,
                        None => continue,
                    };
                    let pos_b = match edges[kb.0].label_pos_at(kb.1) {
                        Some(p) => p,
                        None => continue,
                    };
                    let (mut new_a, mut new_b) = (pos_a, pos_b);
                    if dx < dy {
                        let shift = (dx + DEFAULT_MIN_SEPARATION) / 2.0;
                        if pos_a.x < pos_b.x {
                            new_a.x -= shift;
                            new_b.x += shift;
                        } else {
                            new_a.x += shift;
                            new_b.x -= shift;
                        }
                    } else {
                        let shift = (dy + DEFAULT_MIN_SEPARATION) / 2.0;
                        if pos_a.y < pos_b.y {
                            new_a.y -= shift;
                            new_b.y += shift;
                        } else {
                            new_a.y += shift;
                            new_b.y -= shift;
                        }
                    }
                    edges[ka.0].set_label_pos_at(ka.1, new_a);
                    edges[kb.0].set_label_pos_at(kb.1, new_b);
                    moved = true;
                }
            }
        }

        for &k in &label_keys {
            if oscillating.contains(&k) {
                continue;
            }
            let mut pos = match edges[k.0].label_pos_at(k.1) {
                Some(p) => p,
                None => continue,
            };
            let mut bbox = match edges[k.0].label_bbox_at(k.1) {
                Some(b) => b,
                None => continue,
            };

            for &obstacle in &node_obstacles {
                if push_label_from_obstacle_safe(
                    &mut pos,
                    &mut bbox,
                    obstacle,
                    &node_obstacles,
                ) {
                    edges[k.0].set_label_pos_at(k.1, pos);
                    moved = true;
                }
            }
        }

        for &k in &label_keys {
            if oscillating.contains(&k) {
                continue;
            }
            let mut pos = match edges[k.0].label_pos_at(k.1) {
                Some(p) => p,
                None => continue,
            };
            let mut bbox = match edges[k.0].label_bbox_at(k.1) {
                Some(b) => b,
                None => continue,
            };

            for &obstacle in &group_obstacles {
                if push_label_from_obstacle_safe(
                    &mut pos,
                    &mut bbox,
                    obstacle,
                    &group_obstacles,
                ) {
                    edges[k.0].set_label_pos_at(k.1, pos);
                    moved = true;
                }
            }
        }

        for &k in &label_keys {
            if oscillating.contains(&k) {
                continue;
            }
            let mut pos = match edges[k.0].label_pos_at(k.1) {
                Some(p) => p,
                None => continue,
            };
            let mut bbox = match edges[k.0].label_bbox_at(k.1) {
                Some(b) => b,
                None => continue,
            };

            for (_j, segs) in edge_segments.iter().enumerate() {
                if segs.is_empty() {
                    continue;
                }
                for &(p1, p2) in segs {
                    if segment_vs_aabb_intersect(p1, p2, bbox) {
                        if push_label_from_segment(&mut pos, &mut bbox, p1, p2) {
                            edges[k.0].set_label_pos_at(k.1, pos);
                            moved = true;
                        }
                        break;
                    }
                }
            }
        }

        for &k in &label_keys {
            if oscillating.contains(&k) {
                continue;
            }
            let curr = match edges[k.0].label_pos_at(k.1) {
                Some(p) => p,
                None => continue,
            };
            if let Some(&prev) = prev_positions.get(&k) {
                let delta = Point::new(curr.x - prev.x, curr.y - prev.y);
                let curr_mag = (delta.x * delta.x + delta.y * delta.y).sqrt();
                if curr_mag > EPS {
                    if let Some(&last) = last_delta.get(&k) {
                        let dot = last.x * delta.x + last.y * delta.y;
                        let last_mag = (last.x * last.x + last.y * last.y).sqrt();
                        if dot < 0.0 && last_mag > EPS {
                            let ratio = last_mag.min(curr_mag) / last_mag.max(curr_mag);
                            if ratio > 0.5 {
                                oscillating.insert(k);
                            }
                        }
                    }
                    last_delta.insert(k, delta);
                }
            }
        }

        if !moved {
            break;
        }
    }

    assign_leader_lines(edges, &initial_positions);
}

fn assign_leader_lines(
    edges: &mut [EdgeLayout],
    initial_positions: &HashMap<LabelKey, Point>,
) {
    for (edge_idx, edge) in edges.iter_mut().enumerate() {
        if edge.path_len() < 2 {
            continue;
        }
        let path = edge.path_points().into_owned();
        for (label_idx, label) in edge.labels.iter_mut().enumerate() {
            let key = (edge_idx, label_idx);
            let initial = initial_positions.get(&key);
            let was_moved = match initial {
                Some(&init) => {
                    let dx = label.center.x - init.x;
                    let dy = label.center.y - init.y;
                    (dx * dx + dy * dy).sqrt() > DEFAULT_LEADER_LINE_THRESHOLD
                }
                None => false,
            };
            if !was_moved {
                label.leader_to = None;
                continue;
            }

            let (closest, dist) = closest_point_on_path(&path, label.center);
            if dist > DEFAULT_LEADER_LINE_THRESHOLD {
                label.leader_to = Some(closest);
            } else {
                label.leader_to = None;
            }
        }
    }
}

fn push_label_from_obstacle(
    label_pos: &mut Point,
    bbox: &mut (f64, f64, f64, f64),
    obstacle: (f64, f64, f64, f64),
) -> bool {
    if let Some((dx, dy)) = aabb_overlap(bbox, &obstacle) {
        if dx < dy {
            let shift = dx + DEFAULT_MIN_SEPARATION;
            let cx = (obstacle.0 + obstacle.2) / 2.0;
            if label_pos.x < cx {
                label_pos.x -= shift;
            } else {
                label_pos.x += shift;
            }
        } else {
            let shift = dy + DEFAULT_MIN_SEPARATION;
            let cy = (obstacle.1 + obstacle.3) / 2.0;
            if label_pos.y < cy {
                label_pos.y -= shift;
            } else {
                label_pos.y += shift;
            }
        }
        *bbox = label_bbox_from_pos(*label_pos, bbox);
        true
    } else {
        false
    }
}

fn push_label_from_obstacle_safe(
    label_pos: &mut Point,
    bbox: &mut (f64, f64, f64, f64),
    obstacle: (f64, f64, f64, f64),
    all_obstacles: &[(f64, f64, f64, f64)],
) -> bool {
    let original_pos = *label_pos;
    let original_bbox = *bbox;
    if push_label_from_obstacle(label_pos, bbox, obstacle) {
        for other in all_obstacles {
            if aabb_overlap(bbox, other).is_some() {
                *label_pos = original_pos;
                *bbox = original_bbox;
                return false;
            }
        }
        true
    } else {
        false
    }
}

fn label_bbox_from_pos(center: Point, prev: &(f64, f64, f64, f64)) -> (f64, f64, f64, f64) {
    let w = prev.2 - prev.0;
    let h = prev.3 - prev.1;
    (center.x - w / 2.0, center.y - h / 2.0, center.x + w / 2.0, center.y + h / 2.0)
}

pub fn estimate_label_width(text: &str) -> f64 {
    let mut width = 0.0;
    for ch in text.chars() {
        width += if ch.is_ascii() {
            DEFAULT_ASCII_CHAR_WIDTH
        } else {
            DEFAULT_CJK_CHAR_WIDTH
        };
    }
    width
}

pub fn label_bbox(el: &EdgeLayout, _text: &str) -> (f64, f64, f64, f64) {
    el.label_bbox()
}

pub fn aabb_overlap(
    a: &(f64, f64, f64, f64),
    b: &(f64, f64, f64, f64),
) -> Option<(f64, f64)> {
    let overlap_x = (a.2.min(b.2) - a.0.max(b.0)).max(0.0);
    let overlap_y = (a.3.min(b.3) - a.1.max(b.1)).max(0.0);
    if overlap_x > 0.0 && overlap_y > 0.0 {
        Some((overlap_x, overlap_y))
    } else {
        None
    }
}

fn segment_vs_aabb_intersect(
    p1: Point,
    p2: Point,
    bbox: (f64, f64, f64, f64),
) -> bool {
    Rect::new(bbox.0, bbox.1, bbox.2 - bbox.0, bbox.3 - bbox.1).intersects_segment(p1, p2, 0.0)
}

fn push_label_from_segment(
    label_pos: &mut Point,
    bbox: &mut (f64, f64, f64, f64),
    p1: Point,
    p2: Point,
) -> bool {
    let dx = p2.x - p1.x;
    let dy = p2.y - p1.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-9 {
        return false;
    }

    let nx = -dy / len;
    let ny = dx / len;

    let to_label_x = label_pos.x - p1.x;
    let to_label_y = label_pos.y - p1.y;
    let side = to_label_x * nx + to_label_y * ny;
    let sign = if side >= 0.0 { 1.0 } else { -1.0 };

    let w = bbox.2 - bbox.0;
    let h = bbox.3 - bbox.1;
    let half_extent = (w * nx.abs() + h * ny.abs()) / 2.0;
    let push_dist = half_extent + DEFAULT_MIN_SEPARATION;

    label_pos.x += sign * nx * push_dist;
    label_pos.y += sign * ny * push_dist;

    *bbox = label_bbox_from_pos(*label_pos, bbox);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{EdgeLabelLayout, PathGeometry, Port};

    fn labeled_edge(label_center: Point) -> EdgeLayout {
        EdgeLayout {
            geometry: PathGeometry::Straight {
                start: Point::new(0.0, 0.0),
                end: Point::new(100.0, 0.0),
            },
            labels: vec![EdgeLabelLayout::new("hello", label_center)],
            from_port: Port::Bottom,
            to_port: Port::Top,
        }
    }

    #[test]
    fn label_avoids_group_border_overlap() {
        let mut edges = vec![labeled_edge(Point::new(200.0, 430.0))];
        let nodes = HashMap::new();
        let mut groups = HashMap::new();
        groups.insert(
            "backend".to_string(),
            GroupLayout {
                x: 12.0,
                y: 174.0,
                width: 368.0,
                height: 256.0,
                ..Default::default()
            },
        );

        resolve_label_overlaps(&mut edges, &nodes, &groups);

        let bbox = edges[0].label_bbox();
        let group_bbox = (12.0, 174.0, 380.0, 430.0);
        assert!(
            aabb_overlap(&bbox, &group_bbox).is_none(),
            "label bbox {:?} should not overlap group {:?}",
            bbox,
            group_bbox
        );
    }

    #[test]
    fn segment_vs_aabb_basic_intersect() {
        let bbox = (0.0, 0.0, 100.0, 20.0);
        assert!(segment_vs_aabb_intersect(Point::new(-10.0, 10.0), Point::new(110.0, 10.0), bbox));
        assert!(segment_vs_aabb_intersect(Point::new(50.0, -10.0), Point::new(50.0, 30.0), bbox));
        assert!(!segment_vs_aabb_intersect(Point::new(0.0, -10.0), Point::new(100.0, -10.0), bbox));
        assert!(!segment_vs_aabb_intersect(Point::new(110.0, 0.0), Point::new(120.0, 20.0), bbox));
        assert!(segment_vs_aabb_intersect(Point::new(-10.0, -10.0), Point::new(110.0, 30.0), bbox));
        assert!(segment_vs_aabb_intersect(Point::new(50.0, 10.0), Point::new(200.0, 10.0), bbox));
    }

    #[test]
    fn label_pushed_away_from_own_edge() {
        let mut edges = vec![labeled_edge(Point::new(50.0, 0.0))];
        let nodes = HashMap::new();
        let groups = HashMap::new();

        let bbox_before = edges[0].label_bbox();
        assert!(segment_vs_aabb_intersect(
            Point::new(0.0, 0.0),
            Point::new(100.0, 0.0),
            bbox_before
        ));

        resolve_label_overlaps(&mut edges, &nodes, &groups);

        let bbox_after = edges[0].label_bbox();
        assert!(
            !segment_vs_aabb_intersect(Point::new(0.0, 0.0), Point::new(100.0, 0.0), bbox_after),
            "label should not intersect edge after avoidance, bbox={:?}",
            bbox_after
        );
    }

    #[test]
    fn label_pushed_away_from_other_edge() {
        let mut edges = vec![
            labeled_edge(Point::new(50.0, 0.0)),
            EdgeLayout {
                geometry: PathGeometry::Straight {
                    start: Point::new(0.0, 0.0),
                    end: Point::new(100.0, 0.0),
                },
                labels: vec![],
                from_port: Port::Bottom,
                to_port: Port::Top,
            },
        ];
        let nodes = HashMap::new();
        let groups = HashMap::new();

        let bbox_before = edges[0].label_bbox();
        assert!(segment_vs_aabb_intersect(
            Point::new(0.0, 0.0),
            Point::new(100.0, 0.0),
            bbox_before
        ));

        resolve_label_overlaps(&mut edges, &nodes, &groups);

        let bbox_after = edges[0].label_bbox();
        assert!(
            !segment_vs_aabb_intersect(Point::new(0.0, 0.0), Point::new(100.0, 0.0), bbox_after),
            "label should not intersect edge 1 after avoidance, bbox={:?}",
            bbox_after
        );
    }

    #[test]
    fn label_not_pushed_when_no_intersection() {
        let mut edges = vec![labeled_edge(Point::new(50.0, 100.0))];
        let nodes = HashMap::new();
        let groups = HashMap::new();

        let pos_before = edges[0].label_pos();
        resolve_label_overlaps(&mut edges, &nodes, &groups);
        let pos_after = edges[0].label_pos();

        assert_eq!(
            pos_before, pos_after,
            "label should not move when not intersecting any edge"
        );
    }

    #[test]
    fn label_metrics_returns_expected_size() {
        let (w, h) = label_metrics("hello");
        assert_eq!(w, 6.5 * 5.0 + DEFAULT_LABEL_PADDING * 2.0);
        assert_eq!(h, DEFAULT_LABEL_FONT_SIZE + DEFAULT_LABEL_PADDING * 2.0);

        let (w_cjk, _) = label_metrics("标签");
        assert_eq!(w_cjk, 11.0 * 2.0 + DEFAULT_LABEL_PADDING * 2.0);

        let (w_mix, _) = label_metrics("a标");
        assert_eq!(w_mix, 6.5 + 11.0 + DEFAULT_LABEL_PADDING * 2.0);
    }

    #[test]
    fn push_label_from_obstacle_safe_rejects_push_into_other_obstacle() {
        let mut label_pos = Point::new(95.0, 15.0);
        let mut bbox = (90.0, 5.0, 100.0, 25.0);
        let obstacle_a = (0.0, 0.0, 100.0, 50.0);
        let obstacle_b = (110.0, 0.0, 210.0, 50.0);
        let all_obstacles = vec![obstacle_a, obstacle_b];

        let original_pos = label_pos;
        let result = push_label_from_obstacle_safe(
            &mut label_pos,
            &mut bbox,
            obstacle_a,
            &all_obstacles,
        );

        assert!(!result, "push should be rejected (would enter obstacle B)");
        assert_eq!(
            label_pos, original_pos,
            "label should not move when push is rejected"
        );
    }

    #[test]
    fn push_label_from_obstacle_safe_allows_safe_push() {
        let mut label_pos = Point::new(95.0, 15.0);
        let mut bbox = (90.0, 5.0, 100.0, 25.0);
        let obstacle_a = (0.0, 0.0, 100.0, 50.0);
        let obstacle_b = (500.0, 500.0, 600.0, 550.0);
        let all_obstacles = vec![obstacle_a, obstacle_b];

        let original_pos = label_pos;
        let result = push_label_from_obstacle_safe(
            &mut label_pos,
            &mut bbox,
            obstacle_a,
            &all_obstacles,
        );

        assert!(result, "push should be accepted when no new overlap");
        assert_ne!(label_pos, original_pos, "label should move");
    }

    #[test]
    fn label_between_two_nodes_not_pushed_into_other() {
        let mut edges = vec![labeled_edge(Point::new(50.0, 25.0))];
        let mut nodes = HashMap::new();
        nodes.insert(
            "left".to_string(),
            NodeLayout {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 50.0,
                ..Default::default()
            },
        );
        nodes.insert(
            "right".to_string(),
            NodeLayout {
                x: 100.0,
                y: 0.0,
                width: 100.0,
                height: 50.0,
                ..Default::default()
            },
        );
        let groups = HashMap::new();

        resolve_label_overlaps(&mut edges, &nodes, &groups);

        let bbox = edges[0].label_bbox();
        let right_node = (100.0, 0.0, 200.0, 50.0);
        assert!(
            aabb_overlap(&bbox, &right_node).is_none(),
            "label should not be pushed into right node, bbox={:?}",
            bbox
        );
    }

    #[test]
    fn oscillating_label_does_not_infinite_loop() {
        let mut edges = vec![labeled_edge(Point::new(50.0, 0.0))];
        let mut nodes = HashMap::new();
        nodes.insert(
            "blocker".to_string(),
            NodeLayout {
                x: 30.0,
                y: -30.0,
                width: 40.0,
                height: 25.0,
                ..Default::default()
            },
        );
        let groups = HashMap::new();

        let pos_initial = edges[0].label_pos();
        resolve_label_overlaps(&mut edges, &nodes, &groups);
        let pos_final = edges[0].label_pos();

        let _ = pos_initial;
        let _ = pos_final;
    }

    #[test]
    fn leader_line_set_when_label_pushed_from_edge() {
        let mut edges = vec![labeled_edge(Point::new(50.0, 0.0))];
        let nodes = HashMap::new();
        let groups = HashMap::new();

        assert!(edges[0].labels[0].leader_to.is_none());

        resolve_label_overlaps(&mut edges, &nodes, &groups);

        let label = &edges[0].labels[0];
        assert!(
            label.leader_to.is_some(),
            "leader_to should be set after label is pushed from edge"
        );
        let leader = label.leader_to.unwrap();
        assert!(
            leader.y.abs() < 1.0,
            "leader_to should be on edge path (y≈0), got y={}",
            leader.y
        );
    }

    #[test]
    fn leader_line_none_when_label_not_displaced() {
        let mut edges = vec![labeled_edge(Point::new(50.0, 100.0))];
        let nodes = HashMap::new();
        let groups = HashMap::new();

        resolve_label_overlaps(&mut edges, &nodes, &groups);

        assert!(
            edges[0].labels[0].leader_to.is_none(),
            "leader_to should be None when label is not displaced"
        );
    }

    #[test]
    fn leader_line_points_to_closest_edge_point() {
        let mut edges = vec![labeled_edge(Point::new(50.0, 20.0))];
        let nodes = HashMap::new();
        let groups = HashMap::new();

        resolve_label_overlaps(&mut edges, &nodes, &groups);

        let label = &edges[0].labels[0];
        if let Some(leader) = label.leader_to {
            assert!(
                leader.y.abs() < 5.0,
                "leader_to y should be near edge path, got {}",
                leader.y
            );
        }
    }

    fn multi_label_edge(centers: &[Point]) -> EdgeLayout {
        let labels = centers
            .iter()
            .map(|&c| EdgeLabelLayout::new("L", c))
            .collect();
        EdgeLayout {
            geometry: PathGeometry::Straight {
                start: Point::new(0.0, 0.0),
                end: Point::new(100.0, 0.0),
            },
            labels,
            from_port: Port::Bottom,
            to_port: Port::Top,
        }
    }

    #[test]
    fn multi_label_same_edge_labels_avoid_each_other() {
        let mut edges = vec![multi_label_edge(&[Point::new(50.0, 0.0), Point::new(50.0, 0.0)])];
        let nodes = HashMap::new();
        let groups = HashMap::new();

        resolve_label_overlaps(&mut edges, &nodes, &groups);

        let bbox0 = edges[0].label_bbox_at(0).unwrap();
        let bbox1 = edges[0].label_bbox_at(1).unwrap();
        assert!(
            aabb_overlap(&bbox0, &bbox1).is_none(),
            "two labels on same edge should not overlap after avoidance, bbox0={:?} bbox1={:?}",
            bbox0,
            bbox1
        );
    }

    #[test]
    fn multi_label_independent_displacement() {
        let mut edges = vec![multi_label_edge(&[Point::new(50.0, 0.0), Point::new(50.0, 100.0)])];
        let nodes = HashMap::new();
        let groups = HashMap::new();

        resolve_label_overlaps(&mut edges, &nodes, &groups);

        assert!(
            edges[0].labels[0].leader_to.is_some(),
            "displaced label should have leader_to"
        );
        assert!(
            edges[0].labels[1].leader_to.is_none(),
            "non-displaced label should not have leader_to"
        );
    }

    #[test]
    fn multi_label_cross_edge_collision() {
        let mut edges = vec![
            multi_label_edge(&[Point::new(50.0, 0.0)]),
            multi_label_edge(&[Point::new(50.0, 0.0)]),
        ];
        let nodes = HashMap::new();
        let groups = HashMap::new();

        resolve_label_overlaps(&mut edges, &nodes, &groups);

        let bbox0 = edges[0].label_bbox_at(0).unwrap();
        let bbox1 = edges[1].label_bbox_at(0).unwrap();
        assert!(
            aabb_overlap(&bbox0, &bbox1).is_none(),
            "cross-edge labels should not overlap after avoidance"
        );
    }

    #[test]
    fn multi_label_count_preserved() {
        let mut edges = vec![multi_label_edge(&[
            Point::new(20.0, 0.0),
            Point::new(50.0, 0.0),
            Point::new(80.0, 0.0),
        ])];
        let nodes = HashMap::new();
        let groups = HashMap::new();

        resolve_label_overlaps(&mut edges, &nodes, &groups);

        assert_eq!(
            edges[0].label_count(),
            3,
            "label count should be preserved after avoidance"
        );
    }
}
