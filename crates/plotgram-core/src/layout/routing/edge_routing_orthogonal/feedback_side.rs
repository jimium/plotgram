//! 回环边（Greedy FAS 反转边）侧向通道分配。
//!
//! 路由前对反转边做左右（或上下）均衡分配，避免多条回环边挤在同一外通道。
//!
//! S4：另将「监控枢纽」入边（同目标被动入边 ≥ [`MONITOR_HUB_MIN_PASSIVE_IN`]）
//! 强制纳入侧通道，避免与业务 FanIn 主缝抢道。

use std::collections::HashMap;

use crate::ast::{ArrowType, Diagram, Relation};
use crate::layout::engines::common::acyclic::greedy_fas;
use crate::layout::{NodeLayout, Port};

/// 同侧回环边超过此阈值时溢出到另一侧。
pub const MAX_SAME_SIDE_FEEDBACK: usize = 3;

/// 单条回环边的侧向分配结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeedbackSideHint {
    pub from_side: Port,
    pub to_side: Port,
    pub lane: usize,
}

/// 回环边侧向分配表（edge_index → hint）。
#[derive(Debug, Clone, Default)]
pub struct FeedbackSideAssignment {
    pub hints: HashMap<usize, FeedbackSideHint>,
}

/// 为 Greedy FAS 反转边、以及跨多层的长前向边分配侧向通道。
///
/// TB 布局走 Left/Right 外通道；LR 布局走 Top/Bottom 外通道。
/// 自环边跳过（由自环路由单独处理）。
///
/// 长前向边（rank_span ≥ [`LONG_SPAN_SIDE_THRESHOLD`]）若仍走 Bottom↔Top，
/// 会穿过中间层节点列（如 warning→healthy）；强制侧通道与回环边一致绕行。
pub fn assign_feedback_sides(
    diagram: &Diagram,
    relations: &[Relation],
    nodes: &HashMap<String, NodeLayout>,
    ranks: Option<&HashMap<String, usize>>,
    horizontal: bool,
) -> FeedbackSideAssignment {
    let reversed = reversed_edge_indices(diagram, relations);
    let mut reversed_set: HashMap<usize, ()> = HashMap::new();
    for &i in &reversed {
        reversed_set.insert(i, ());
    }

    let graph_center = graph_center(nodes, horizontal);
    let mut left_bucket: Vec<(usize, usize)> = Vec::new();
    let mut right_bucket: Vec<(usize, usize)> = Vec::new();

    // 候选：FAS 反转边 + 长跨度前向边
    // S4：监控枢纽边不在此强制改端口（顶置 hub 强行 L/R 会增交叉）；
    // 仅由 run 侧并入 feedback_edge_set 延后路由 + 外环/干线评分。
    let mut candidates: Vec<usize> = reversed;
    for (i, rel) in relations.iter().enumerate() {
        if reversed_set.contains_key(&i) {
            continue;
        }
        if rel.from.as_str() == rel.to.as_str() {
            continue;
        }
        let Some(from_nl) = nodes.get(rel.from.as_str()) else {
            continue;
        };
        let Some(to_nl) = nodes.get(rel.to.as_str()) else {
            continue;
        };
        let span = rank_span_for_edge(rel, ranks, from_nl, to_nl, horizontal);
        if span >= LONG_SPAN_SIDE_THRESHOLD {
            candidates.push(i);
        }
    }
    candidates.sort();
    candidates.dedup();

    for &edge_index in &candidates {
        let rel = &relations[edge_index];
        if rel.from.as_str() == rel.to.as_str() {
            continue;
        }
        let Some(from_nl) = nodes.get(rel.from.as_str()) else {
            continue;
        };
        let Some(to_nl) = nodes.get(rel.to.as_str()) else {
            continue;
        };

        let rank_span = rank_span_for_edge(rel, ranks, from_nl, to_nl, horizontal);
        // 相邻层且投影重叠：走几何正对端口，不进侧通道桶。
        // R1：rank_span==1 时正对候选须 path_is_clean；不干净则仍进侧通道。
        // Phase A：长跨度边也检查正对路径是否干净；干净则不强制侧通道（无中间节点需绕行）。
        let opposite_clean = opposite_channel_path_is_clean(
            from_nl,
            to_nl,
            rel.from.as_str(),
            rel.to.as_str(),
            nodes,
            horizontal,
        );
        let proj_overlap = projections_overlap_on_cross_axis(from_nl, to_nl, horizontal);
        if feedback_dbg_enabled() {
            eprintln!(
                "[feedback_dbg] edge[{}] {} -> {} | span={} opposite_clean={} proj_overlap={}",
                edge_index, rel.from, rel.to, rank_span, opposite_clean, proj_overlap
            );
        }
        if rank_span < LONG_SPAN_SIDE_THRESHOLD {
            if prefers_opposite_ports_over_side_channel(from_nl, to_nl, rank_span, horizontal)
                && opposite_clean
            {
                if feedback_dbg_enabled() {
                    eprintln!("[feedback_dbg]   -> SKIP (short-span opposite)");
                }
                continue;
            }
        } else if opposite_clean && proj_overlap {
            // 长跨度但正对路径干净且节点在交叉轴投影重叠 → 不强制侧通道
            if feedback_dbg_enabled() {
                eprintln!("[feedback_dbg]   -> SKIP (long-span opposite clean+overlap)");
            }
            continue;
        }

        let centroid = if horizontal {
            (from_nl.y + from_nl.height / 2.0 + to_nl.y + to_nl.height / 2.0) / 2.0
        } else {
            (from_nl.x + from_nl.width / 2.0 + to_nl.x + to_nl.width / 2.0) / 2.0
        };

        if centroid < graph_center {
            left_bucket.push((edge_index, rank_span));
        } else {
            right_bucket.push((edge_index, rank_span));
        }
    }

    left_bucket.sort_by_key(|(idx, span)| (*span, *idx));
    right_bucket.sort_by_key(|(idx, span)| (*span, *idx));

    balance_buckets(&mut left_bucket, &mut right_bucket);
    // 已知代价修复：仅 2 条回环且挤在同侧时，强制分左右，避免长通道重叠增交叉。
    split_paired_feedback_sides(&mut left_bucket, &mut right_bucket);

    let mut hints = HashMap::new();
    assign_bucket_hints(&mut hints, &left_bucket, detour_side(horizontal, true), horizontal);
    assign_bucket_hints(&mut hints, &right_bucket, detour_side(horizontal, false), horizontal);

    FeedbackSideAssignment { hints }
}

/// 跨此层数及以上的边优先走侧通道（与回环边同策略）。
pub const LONG_SPAN_SIDE_THRESHOLD: usize = 2;

/// 同目标被动（`-->`）入边达到此数时视为监控枢纽，强制侧通道。
pub const MONITOR_HUB_MIN_PASSIVE_IN: usize = 3;

/// 汇聚到监控枢纽的被动入边下标（确定性：按 hub id、边下标排序）。
pub fn monitor_hub_edge_indices(relations: &[Relation]) -> Vec<usize> {
    let mut inbound: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, rel) in relations.iter().enumerate() {
        if rel.arrow != ArrowType::Passive {
            continue;
        }
        if rel.from.as_str() == rel.to.as_str() {
            continue;
        }
        inbound.entry(rel.to.as_str()).or_default().push(i);
    }
    let mut hubs: Vec<&str> = inbound
        .iter()
        .filter(|(_, edges)| edges.len() >= MONITOR_HUB_MIN_PASSIVE_IN)
        .map(|(id, _)| *id)
        .collect();
    hubs.sort_unstable();
    let mut out = Vec::new();
    for hub in hubs {
        if let Some(edges) = inbound.get(hub) {
            out.extend(edges.iter().copied());
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// S4.x：同排被堵死时，监控边改走朝向枢纽的正对端口（TB：Top↔Bottom）。
///
/// 中排节点（如 postgres）左右都有同排邻居时，L/R 侧廊必须在 NODE_GAP(32) 内
/// 竖向离排，但 `NODE_OBSTACLE_PAD`(18)×2 已超过缝宽 → 必然穿模。此时改 Top/Bottom
/// 先进入层间，再由外环候选绕开中轴。
pub fn apply_monitor_hub_escape_ports(
    relations: &[Relation],
    nodes: &HashMap<String, NodeLayout>,
    from_side: &mut [Port],
    to_side: &mut [Port],
    horizontal: bool,
) {
    if horizontal {
        return;
    }
    const MIN_SIDE_ESCAPE_GAP: f64 = 36.0; // ≥ 2 * NODE_OBSTACLE_PAD
    for ei in monitor_hub_edge_indices(relations) {
        if ei >= relations.len() || ei >= from_side.len() {
            continue;
        }
        let rel = &relations[ei];
        let Some(from_nl) = nodes.get(rel.from.as_str()) else {
            continue;
        };
        let Some(to_nl) = nodes.get(rel.to.as_str()) else {
            continue;
        };
        let sy = from_nl.y + from_nl.height * 0.5;
        let gap_left = same_row_clearance_toward(from_nl, rel.from.as_str(), sy, nodes, true);
        let gap_right = same_row_clearance_toward(from_nl, rel.from.as_str(), sy, nodes, false);
        let blocked_left = gap_left < MIN_SIDE_ESCAPE_GAP;
        let blocked_right = gap_right < MIN_SIDE_ESCAPE_GAP;
        let blocked = match from_side[ei] {
            Port::Left => blocked_left,
            Port::Right => blocked_right,
            _ => blocked_left && blocked_right,
        };
        if !blocked {
            continue;
        }
        let from_cy = from_nl.y + from_nl.height * 0.5;
        let to_cy = to_nl.y + to_nl.height * 0.5;
        if to_cy < from_cy - 1.0 {
            from_side[ei] = Port::Top;
            to_side[ei] = Port::Bottom;
        } else if to_cy > from_cy + 1.0 {
            from_side[ei] = Port::Bottom;
            to_side[ei] = Port::Top;
        }
    }
}

/// 同排朝向 `left`/`right` 最近邻的外缘间隙；无邻居则视为足够宽。
fn same_row_clearance_toward(
    from_nl: &NodeLayout,
    from_id: &str,
    sy: f64,
    nodes: &HashMap<String, NodeLayout>,
    toward_left: bool,
) -> f64 {
    let mut best = f64::INFINITY;
    for (id, n) in nodes {
        if id == from_id {
            continue;
        }
        if n.y - 1.0 > sy || n.y + n.height + 1.0 < sy {
            continue;
        }
        if toward_left {
            if n.x + n.width <= from_nl.x + 1.0 {
                best = best.min(from_nl.x - (n.x + n.width));
            }
        } else if n.x >= from_nl.x + from_nl.width - 1.0 {
            best = best.min(n.x - (from_nl.x + from_nl.width));
        }
    }
    if best.is_finite() {
        best
    } else {
        f64::INFINITY
    }
}

fn reversed_edge_indices(diagram: &Diagram, relations: &[Relation]) -> Vec<usize> {
    let mut nodes: Vec<String> = diagram
        .entities
        .iter()
        .map(|e| e.id.as_str().to_string())
        .collect();
    nodes.sort();

    let mut out_neighbors: HashMap<String, Vec<String>> = HashMap::new();
    let mut in_neighbors: HashMap<String, Vec<String>> = HashMap::new();
    for rel in relations {
        let from = rel.from.as_str().to_string();
        let to = rel.to.as_str().to_string();
        out_neighbors.entry(from.clone()).or_default().push(to.clone());
        in_neighbors.entry(to).or_default().push(from);
    }
    for list in out_neighbors.values_mut() {
        list.sort();
    }
    for list in in_neighbors.values_mut() {
        list.sort();
    }

    let reversed = greedy_fas(&nodes, &out_neighbors, &in_neighbors);
    let mut indices: Vec<usize> = relations
        .iter()
        .enumerate()
        .filter_map(|(i, rel)| {
            let key = (rel.from.as_str().to_string(), rel.to.as_str().to_string());
            reversed.contains(&key).then_some(i)
        })
        .collect();
    indices.sort();
    indices
}

/// 临时调试开关：PLOTGRAM_FEEDBACK_DEBUG=1 时打印回环边侧通道决策明细。
fn feedback_dbg_enabled() -> bool {
    std::env::var("PLOTGRAM_FEEDBACK_DEBUG")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn graph_center(nodes: &HashMap<String, NodeLayout>, horizontal: bool) -> f64 {
    if nodes.is_empty() {
        return 0.0;
    }
    let mut ids: Vec<&String> = nodes.keys().collect();
    ids.sort();
    let sum: f64 = ids
        .into_iter()
        .filter_map(|id| nodes.get(id))
        .map(|n| {
            if horizontal {
                n.y + n.height / 2.0
            } else {
                n.x + n.width / 2.0
            }
        })
        .sum();
    sum / nodes.len() as f64
}

fn rank_span_for_edge(
    rel: &Relation,
    ranks: Option<&HashMap<String, usize>>,
    from_nl: &NodeLayout,
    to_nl: &NodeLayout,
    horizontal: bool,
) -> usize {
    if let Some(ranks) = ranks {
        let from_rank = ranks.get(rel.from.as_str()).copied().unwrap_or(0);
        let to_rank = ranks.get(rel.to.as_str()).copied().unwrap_or(0);
        return from_rank.abs_diff(to_rank).max(1);
    }
    let dist = if horizontal {
        (from_nl.x - to_nl.x).abs()
    } else {
        (from_nl.y - to_nl.y).abs()
    };
    ((dist / 48.0).round() as usize).max(1)
}

fn balance_buckets(left: &mut Vec<(usize, usize)>, right: &mut Vec<(usize, usize)>) {
    if left.len() > MAX_SAME_SIDE_FEEDBACK {
        let overflow = left.len() - MAX_SAME_SIDE_FEEDBACK;
        let moved: Vec<_> = left.drain(left.len() - overflow..).collect();
        right.extend(moved);
        right.sort_by_key(|(idx, span)| (*span, *idx));
    }
    if right.len() > MAX_SAME_SIDE_FEEDBACK {
        let overflow = right.len() - MAX_SAME_SIDE_FEEDBACK;
        let moved: Vec<_> = right.drain(right.len() - overflow..).collect();
        left.extend(moved);
        left.sort_by_key(|(idx, span)| (*span, *idx));
    }
}

/// 恰好 2 条回环挤在同一侧时，把 span 更大的一条挪到对侧。
fn split_paired_feedback_sides(
    left: &mut Vec<(usize, usize)>,
    right: &mut Vec<(usize, usize)>,
) {
    if left.is_empty() && right.len() == 2 {
        let moved = right.remove(1);
        left.push(moved);
        left.sort_by_key(|(idx, span)| (*span, *idx));
    } else if right.is_empty() && left.len() == 2 {
        let moved = left.remove(1);
        right.push(moved);
        right.sort_by_key(|(idx, span)| (*span, *idx));
    }
}

fn detour_side(horizontal: bool, low_side: bool) -> Port {
    if horizontal {
        if low_side {
            Port::Top
        } else {
            Port::Bottom
        }
    } else if low_side {
        Port::Left
    } else {
        Port::Right
    }
}

fn same_side_ports(side: Port) -> (Port, Port) {
    (side, side)
}

/// 相邻层 + 切线方向投影重叠（或间隙 < 16px）时，反馈边应走正对端口而非侧通道。
const OPPOSITE_PORT_GAP_THRESHOLD: f64 = 16.0;

/// 检查两节点在交叉轴上的投影是否重叠（TB 看 x 轴，LR 看 y 轴）。
/// 不受 rank_span 限制，用于长跨度边的正对路径判断。
fn projections_overlap_on_cross_axis(from_nl: &NodeLayout, to_nl: &NodeLayout, horizontal: bool) -> bool {
    if horizontal {
        let a0 = from_nl.y;
        let a1 = from_nl.y + from_nl.height;
        let b0 = to_nl.y;
        let b1 = to_nl.y + to_nl.height;
        (a1.min(b1) - a0.max(b0)) > 0.0
    } else {
        let a0 = from_nl.x;
        let a1 = from_nl.x + from_nl.width;
        let b0 = to_nl.x;
        let b1 = to_nl.x + to_nl.width;
        (a1.min(b1) - a0.max(b0)) > 0.0
    }
}

fn prefers_opposite_ports_over_side_channel(
    from_nl: &NodeLayout,
    to_nl: &NodeLayout,
    rank_span: usize,
    horizontal: bool,
) -> bool {
    if rank_span > 1 {
        return false;
    }
    let (overlap, gap) = if horizontal {
        // LR：切线为 y
        let a0 = from_nl.y;
        let a1 = from_nl.y + from_nl.height;
        let b0 = to_nl.y;
        let b1 = to_nl.y + to_nl.height;
        let overlap = (a1.min(b1) - a0.max(b0)).max(0.0);
        let gap = if a1 < b0 {
            b0 - a1
        } else if b1 < a0 {
            a0 - b1
        } else {
            0.0
        };
        (overlap, gap)
    } else {
        // TB：切线为 x
        let a0 = from_nl.x;
        let a1 = from_nl.x + from_nl.width;
        let b0 = to_nl.x;
        let b1 = to_nl.x + to_nl.width;
        let overlap = (a1.min(b1) - a0.max(b0)).max(0.0);
        let gap = if a1 < b0 {
            b0 - a1
        } else if b1 < a0 {
            a0 - b1
        } else {
            0.0
        };
        (overlap, gap)
    };
    overlap > 0.0 || gap < OPPOSITE_PORT_GAP_THRESHOLD
}

/// R1：构造正对端口的简易正交路径，检查是否穿第三方节点。
fn opposite_channel_path_is_clean(
    from_nl: &NodeLayout,
    to_nl: &NodeLayout,
    from_id: &str,
    to_id: &str,
    nodes: &HashMap<String, NodeLayout>,
    horizontal: bool,
) -> bool {
    use crate::layout::geometry::{Point, Rect};

    let (sx, sy, ex, ey) = if horizontal {
        // LR：Right → Left
        (
            from_nl.x + from_nl.width,
            from_nl.y + from_nl.height / 2.0,
            to_nl.x,
            to_nl.y + to_nl.height / 2.0,
        )
    } else {
        // TB：Bottom → Top
        (
            from_nl.x + from_nl.width / 2.0,
            from_nl.y + from_nl.height,
            to_nl.x + to_nl.width / 2.0,
            to_nl.y,
        )
    };
    let start = Point::new(sx, sy);
    let end = Point::new(ex, ey);
    let path = if (sx - ex).abs() < 1e-6 || (sy - ey).abs() < 1e-6 {
        vec![start, end]
    } else if horizontal {
        vec![start, Point::new(ex, sy), end]
    } else {
        vec![start, Point::new(sx, ey), end]
    };
    const PAD: f64 = 18.0; // 对齐 NODE_OBSTACLE_PAD
    let mut sorted_ids: Vec<&String> = nodes.keys().collect();
    sorted_ids.sort();
    for window in path.windows(2) {
        let a = window[0];
        let b = window[1];
        for id in &sorted_ids {
            if id.as_str() == from_id || id.as_str() == to_id {
                continue;
            }
            let nl = &nodes[*id];
            let rect = Rect::new(
                nl.x - PAD,
                nl.y - PAD,
                nl.width + 2.0 * PAD,
                nl.height + 2.0 * PAD,
            );
            if rect.intersects_segment(a, b, 0.0) {
                return false;
            }
        }
    }
    true
}

fn assign_bucket_hints(
    hints: &mut HashMap<usize, FeedbackSideHint>,
    bucket: &[(usize, usize)],
    side: Port,
    _horizontal: bool,
) {
    let (from_side, to_side) = same_side_ports(side);
    for (lane, (edge_index, _)) in bucket.iter().enumerate() {
        hints.insert(
            *edge_index,
            FeedbackSideHint {
                from_side,
                to_side,
                lane,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{ArrowType, AttributeMap, Diagram, Entity, Identifier, SourceInfo, Span};
    use crate::types::DiagramType;

    fn node(id: &str, x: f64, y: f64) -> (String, NodeLayout) {
        (
            id.to_string(),
            NodeLayout {
                x,
                y,
                width: 80.0,
                height: 40.0,
                ..Default::default()
            },
        )
    }

    #[test]
    fn balances_overflow_to_other_side() {
        let span = Span::dummy();
        let relations = vec![
            Relation {
                from: Identifier::new_unchecked("a"),
                to: Identifier::new_unchecked("b"),
                arrow: ArrowType::Active,
                label: None,
                head_label: None,
                tail_label: None,
                attributes: AttributeMap::default(),
                span,
            },
            Relation {
                from: Identifier::new_unchecked("c"),
                to: Identifier::new_unchecked("d"),
                arrow: ArrowType::Active,
                label: None,
                head_label: None,
                tail_label: None,
                attributes: AttributeMap::default(),
                span,
            },
            Relation {
                from: Identifier::new_unchecked("e"),
                to: Identifier::new_unchecked("f"),
                arrow: ArrowType::Active,
                label: None,
                head_label: None,
                tail_label: None,
                attributes: AttributeMap::default(),
                span,
            },
            Relation {
                from: Identifier::new_unchecked("g"),
                to: Identifier::new_unchecked("h"),
                arrow: ArrowType::Active,
                label: None,
                head_label: None,
                tail_label: None,
                attributes: AttributeMap::default(),
                span,
            },
        ];
        let diagram = Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: vec![],
            entities: vec![
                Entity {
                    id: Identifier::new_unchecked("a"),
                    label: "a".into(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span,
                },
                Entity {
                    id: Identifier::new_unchecked("b"),
                    label: "b".into(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span,
                },
                Entity {
                    id: Identifier::new_unchecked("c"),
                    label: "c".into(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span,
                },
                Entity {
                    id: Identifier::new_unchecked("d"),
                    label: "d".into(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span,
                },
                Entity {
                    id: Identifier::new_unchecked("e"),
                    label: "e".into(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span,
                },
                Entity {
                    id: Identifier::new_unchecked("f"),
                    label: "f".into(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span,
                },
                Entity {
                    id: Identifier::new_unchecked("g"),
                    label: "g".into(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span,
                },
                Entity {
                    id: Identifier::new_unchecked("h"),
                    label: "h".into(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span,
                },
            ],
            relations: relations.clone(),
            groups: vec![],
            constraints: vec![],
            style_decls: vec![],
            source_info: SourceInfo {
                file: None,
                line_count: 1,
            },
            ..Default::default()
        };

        let nodes = HashMap::from([
            node("a", 10.0, 10.0),
            node("b", 10.0, 100.0),
            node("c", 20.0, 10.0),
            node("d", 20.0, 100.0),
            node("e", 30.0, 10.0),
            node("f", 30.0, 100.0),
            node("g", 400.0, 10.0),
            node("h", 400.0, 100.0),
        ]);

        let assignment = assign_feedback_sides(&diagram, &relations, &nodes, None, false);
        let left_count = assignment
            .hints
            .values()
            .filter(|h| h.from_side == Port::Left)
            .count();
        let right_count = assignment
            .hints
            .values()
            .filter(|h| h.from_side == Port::Right)
            .count();
        assert!(left_count <= MAX_SAME_SIDE_FEEDBACK);
        assert!(right_count <= MAX_SAME_SIDE_FEEDBACK);
    }

    #[test]
    fn monitor_hub_detects_passive_inbound_cluster() {
        let span = Span::dummy();
        let relations = vec![
            Relation {
                from: Identifier::new_unchecked("a"),
                to: Identifier::new_unchecked("hub"),
                arrow: ArrowType::Passive,
                label: None,
                head_label: None,
                tail_label: None,
                attributes: AttributeMap::default(),
                span,
            },
            Relation {
                from: Identifier::new_unchecked("b"),
                to: Identifier::new_unchecked("hub"),
                arrow: ArrowType::Passive,
                label: None,
                head_label: None,
                tail_label: None,
                attributes: AttributeMap::default(),
                span,
            },
            Relation {
                from: Identifier::new_unchecked("c"),
                to: Identifier::new_unchecked("hub"),
                arrow: ArrowType::Passive,
                label: None,
                head_label: None,
                tail_label: None,
                attributes: AttributeMap::default(),
                span,
            },
            Relation {
                from: Identifier::new_unchecked("d"),
                to: Identifier::new_unchecked("other"),
                arrow: ArrowType::Passive,
                label: None,
                head_label: None,
                tail_label: None,
                attributes: AttributeMap::default(),
                span,
            },
        ];
        let hub = monitor_hub_edge_indices(&relations);
        assert_eq!(hub, vec![0, 1, 2]);
    }
}
