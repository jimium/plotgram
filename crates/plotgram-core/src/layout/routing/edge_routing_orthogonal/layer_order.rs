//! 正交路由边序：有 Sugiyama rank 时分层批量路由，否则按连接度贪心。

use crate::ast::Relation;
use std::collections::HashMap;

pub(super) fn compute_node_degrees(relations: &[Relation]) -> HashMap<String, usize> {
    let mut degree = HashMap::new();
    for rel in relations {
        *degree.entry(rel.from.as_str().to_string()).or_insert(0) += 1;
        *degree.entry(rel.to.as_str().to_string()).or_insert(0) += 1;
    }
    degree
}

pub(super) fn edge_complexity(
    relations: &[Relation],
    index: usize,
    degree: &HashMap<String, usize>,
) -> usize {
    let rel = &relations[index];
    let from = degree.get(rel.from.as_str()).copied().unwrap_or(0);
    let to = degree.get(rel.to.as_str()).copied().unwrap_or(0);
    from.max(to)
}

fn edge_min_rank(
    relations: &[Relation],
    index: usize,
    ranks: &HashMap<String, usize>,
) -> usize {
    let rel = &relations[index];
    let from = ranks.get(rel.from.as_str()).copied().unwrap_or(0);
    let to = ranks.get(rel.to.as_str()).copied().unwrap_or(0);
    from.min(to)
}

/// 难度分量化键（毫分位）：避免 f64 直接 cmp 的不确定性。
fn edge_score_key(scores: Option<&[f64]>, index: usize) -> i64 {
    let s = scores.and_then(|v| v.get(index)).copied().unwrap_or(0.0);
    if !s.is_finite() {
        return 0;
    }
    (s * 1000.0).round() as i64
}

/// 确定性边路由顺序。
///
/// 有 `sugiyama_ranks` 时按端点最小 rank 升序分批（低层先占通道），
/// 层内：非 feedback 先于 feedback（避免回环边抢占前向通道），再按连接度降序。
/// 无 rank 时退化为连接度排序。
///
/// `difficulty_scores`：可选边级难度（与 relations 下标对齐）；同 rank 内高分略提前占道。
/// 可用 `PLOTGRAM_EDGE_ORDER_SCORE=0` 关闭（调用方不传即可）。
///
/// 可传入 feedback（回环）边集合以延后路由，以及可选 `difficulty_scores`（Phase 2 soft 提前）。
pub(super) fn compute_edge_order_with_feedback(
    relations: &[Relation],
    sugiyama_ranks: Option<&HashMap<String, usize>>,
    node_degree: &HashMap<String, usize>,
    feedback_edges: Option<&std::collections::HashSet<usize>>,
    difficulty_scores: Option<&[f64]>,
) -> Vec<usize> {
    let n = relations.len();
    let mut order: Vec<usize> = (0..n).collect();
    let is_feedback = |idx: usize| -> bool {
        feedback_edges.is_some_and(|s| s.contains(&idx))
    };

    match sugiyama_ranks {
        Some(ranks) => {
            // 全局：非 feedback 先于 feedback，再按 min_rank / 难度分 / 连接度。
            // 避免长回环（低 min_rank）抢占高层前向边通道。
            // 难度分：同层内高难边略提前占廊（soft；不推翻 feedback/rank 主序）。
            order.sort_by(|&a, &b| {
                is_feedback(a)
                    .cmp(&is_feedback(b))
                    .then_with(|| {
                        edge_min_rank(relations, a, ranks)
                            .cmp(&edge_min_rank(relations, b, ranks))
                    })
                    .then_with(|| {
                        edge_score_key(difficulty_scores, b)
                            .cmp(&edge_score_key(difficulty_scores, a))
                    })
                    .then_with(|| {
                        let da = edge_complexity(relations, a, node_degree);
                        let db = edge_complexity(relations, b, node_degree);
                        db.cmp(&da)
                    })
                    .then(a.cmp(&b))
            });
        }
        None => {
            order.sort_by(|&a, &b| {
                is_feedback(a)
                    .cmp(&is_feedback(b))
                    .then_with(|| {
                        edge_score_key(difficulty_scores, b)
                            .cmp(&edge_score_key(difficulty_scores, a))
                    })
                    .then_with(|| {
                        let da = edge_complexity(relations, a, node_degree);
                        let db = edge_complexity(relations, b, node_degree);
                        db.cmp(&da)
                    })
                    .then(a.cmp(&b))
            });
        }
    }

    order
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{ArrowType, AttributeMap, Identifier, Relation, Span};

    fn test_rel(from: &str, to: &str) -> Relation {
        Relation {
            from: Identifier::new_unchecked(from),
            to: Identifier::new_unchecked(to),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span: Span::dummy(),
        }
    }

    #[test]
    fn layered_order_routes_lower_ranks_first() {
        let relations = vec![test_rel("c", "d"), test_rel("a", "b")];
        let mut ranks = HashMap::new();
        ranks.insert("a".into(), 0);
        ranks.insert("b".into(), 1);
        ranks.insert("c".into(), 2);
        ranks.insert("d".into(), 3);

        let degree = compute_node_degrees(&relations);
        let order = compute_edge_order_with_feedback(&relations, Some(&ranks), &degree, None, None);

        assert_eq!(order, vec![1, 0]);
    }

    #[test]
    fn feedback_edges_deferred_globally_even_with_lower_min_rank() {
        // feedback c→a (min_rank=0) 应排在前向 a→b (min_rank=0) 与 d→e (min_rank=2) 之后
        let relations = vec![
            test_rel("c", "a"), // 0 feedback
            test_rel("a", "b"), // 1 forward
            test_rel("d", "e"), // 2 forward higher rank
        ];
        let mut ranks = HashMap::new();
        ranks.insert("a".into(), 0);
        ranks.insert("b".into(), 1);
        ranks.insert("c".into(), 2);
        ranks.insert("d".into(), 2);
        ranks.insert("e".into(), 3);
        let degree = compute_node_degrees(&relations);
        let mut feedback = std::collections::HashSet::new();
        feedback.insert(0);
        let order = compute_edge_order_with_feedback(
            &relations,
            Some(&ranks),
            &degree,
            Some(&feedback),
            None,
        );
        assert_eq!(order.last().copied(), Some(0), "feedback should be last");
        assert!(order[..2].contains(&1) && order[..2].contains(&2));
    }

    #[test]
    fn feedback_edges_deferred_within_same_min_rank() {
        // a→b (rank 0→1) 与 c→a (feedback, min_rank=0)：前向应先于 feedback
        let relations = vec![test_rel("c", "a"), test_rel("a", "b")];
        let mut ranks = HashMap::new();
        ranks.insert("a".into(), 0);
        ranks.insert("b".into(), 1);
        ranks.insert("c".into(), 2);
        let degree = compute_node_degrees(&relations);
        let mut feedback = std::collections::HashSet::new();
        feedback.insert(0); // c→a
        let order = compute_edge_order_with_feedback(
            &relations,
            Some(&ranks),
            &degree,
            Some(&feedback),
            None,
        );
        assert_eq!(order, vec![1, 0], "forward a→b before feedback c→a");
    }

    #[test]
    fn without_ranks_falls_back_to_degree_order() {
        let relations = vec![test_rel("hub", "a"), test_rel("b", "c")];
        let degree = compute_node_degrees(&relations);
        let order = compute_edge_order_with_feedback(&relations, None, &degree, None, None);

        assert_eq!(order, vec![0, 1]);
    }

    #[test]
    fn higher_difficulty_score_routes_earlier_within_same_rank() {
        // 同 min_rank、同 degree：高分边应更靠前
        let relations = vec![
            test_rel("a", "b"), // 0 score low
            test_rel("c", "d"), // 1 score high
        ];
        let mut ranks = HashMap::new();
        ranks.insert("a".into(), 0);
        ranks.insert("b".into(), 1);
        ranks.insert("c".into(), 0);
        ranks.insert("d".into(), 1);
        let degree = compute_node_degrees(&relations);
        let scores = [1.0_f64, 3.5_f64];
        let order = compute_edge_order_with_feedback(
            &relations,
            Some(&ranks),
            &degree,
            None,
            Some(&scores),
        );
        assert_eq!(order, vec![1, 0]);
    }

    #[test]
    fn difficulty_score_does_not_override_feedback_deferral() {
        let relations = vec![
            test_rel("c", "a"), // 0 feedback, 极高分
            test_rel("a", "b"), // 1 forward, 低分
        ];
        let mut ranks = HashMap::new();
        ranks.insert("a".into(), 0);
        ranks.insert("b".into(), 1);
        ranks.insert("c".into(), 2);
        let degree = compute_node_degrees(&relations);
        let mut feedback = std::collections::HashSet::new();
        feedback.insert(0);
        let scores = [9.0_f64, 0.1_f64];
        let order = compute_edge_order_with_feedback(
            &relations,
            Some(&ranks),
            &degree,
            Some(&feedback),
            Some(&scores),
        );
        assert_eq!(order, vec![1, 0]);
    }
}
