//! 回环边（Greedy FAS 反转边）侧向通道分配。
//!
//! 路由前对反转边做左右（或上下）均衡分配，避免多条回环边挤在同一外通道。

use std::collections::HashMap;

use crate::ast::{Diagram, Relation};
use crate::layout::node::common::acyclic::greedy_fas;
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

/// 为 Greedy FAS 反转边分配侧向通道。
///
/// TB 布局走 Left/Right 外通道；LR 布局走 Top/Bottom 外通道。
/// 自环边跳过（由自环路由单独处理）。
pub fn assign_feedback_sides(
    diagram: &Diagram,
    relations: &[Relation],
    nodes: &HashMap<String, NodeLayout>,
    ranks: Option<&HashMap<String, usize>>,
    horizontal: bool,
) -> FeedbackSideAssignment {
    let reversed = reversed_edge_indices(diagram, relations);
    if reversed.is_empty() {
        return FeedbackSideAssignment::default();
    }

    let graph_center = graph_center(nodes, horizontal);
    let mut left_bucket: Vec<(usize, usize)> = Vec::new();
    let mut right_bucket: Vec<(usize, usize)> = Vec::new();

    for &edge_index in &reversed {
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

        let centroid = if horizontal {
            (from_nl.y + from_nl.height / 2.0 + to_nl.y + to_nl.height / 2.0) / 2.0
        } else {
            (from_nl.x + from_nl.width / 2.0 + to_nl.x + to_nl.width / 2.0) / 2.0
        };

        let rank_span = rank_span_for_edge(rel, ranks, from_nl, to_nl, horizontal);
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

fn graph_center(nodes: &HashMap<String, NodeLayout>, horizontal: bool) -> f64 {
    if nodes.is_empty() {
        return 0.0;
    }
    let sum: f64 = nodes
        .values()
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
}
