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

/// 确定性边路由顺序。
///
/// 有 `sugiyama_ranks` 时按端点最小 rank 升序分批（低层先占通道），
/// 层内按连接度降序；无 rank 时退化为连接度排序。
pub(super) fn compute_edge_order(
    relations: &[Relation],
    sugiyama_ranks: Option<&HashMap<String, usize>>,
    node_degree: &HashMap<String, usize>,
) -> Vec<usize> {
    let n = relations.len();
    let mut order: Vec<usize> = (0..n).collect();

    match sugiyama_ranks {
        Some(ranks) => {
            order.sort_by(|&a, &b| {
                edge_min_rank(relations, a, ranks)
                    .cmp(&edge_min_rank(relations, b, ranks))
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
                let da = edge_complexity(relations, a, node_degree);
                let db = edge_complexity(relations, b, node_degree);
                db.cmp(&da).then(a.cmp(&b))
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
        let order = compute_edge_order(&relations, Some(&ranks), &degree);

        assert_eq!(order, vec![1, 0]);
    }

    #[test]
    fn without_ranks_falls_back_to_degree_order() {
        let relations = vec![test_rel("hub", "a"), test_rel("b", "c")];
        let degree = compute_node_degrees(&relations);
        let order = compute_edge_order(&relations, None, &degree);

        assert_eq!(order, vec![0, 1]);
    }
}
