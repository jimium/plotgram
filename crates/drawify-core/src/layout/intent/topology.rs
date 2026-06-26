//! 拓扑意图校验。
//!
//! 在 `build_graph_with_overlay` 注入意图边之前，对 `LayoutIntentOverlay.topology`
//! 做以下校验：
//!
//! - **节点存在性**：意图引用的 `from` / `to` 必须存在于 `diagram.entities`。
//! - **矛盾去重**：`Below(A,B)` + `Above(A,B)`（等价于 `A→B` + `B→A`）保留先声明者，
//!   后者标记 `Conflicted`。完全重复（如两个 `Below(A,B)`）同样保留先声明者。
//! - **环检测**：注入意图边后若与真实边 + 已接受意图边构成环，标记 `Conflicted` 并跳过。
//!
//! 校验通过的意图返回为 [`ValidTopologyIntent`]，供 `build_graph_with_overlay` 注入。

use crate::ast::Diagram;
use crate::layout::intent::{IntentResult, IntentStatus, LayoutIntentOverlay, TopologyIntent};
use std::collections::{HashMap, HashSet};

/// 校验通过的拓扑意图（保留原始意图参数）。
#[derive(Debug, Clone)]
pub struct ValidTopologyIntent {
    /// 在 `overlay.topology` 中的原始索引。
    pub index: usize,
    /// 意图类型标签（`"below"` / `"above"`）。
    pub kind: &'static str,
    /// 原始意图的 `from` 节点 id（即 `Below(A,B)` / `Above(A,B)` 中的 A）。
    pub from: String,
    /// 原始意图的 `to` 节点 id（即 `Below(A,B)` / `Above(A,B)` 中的 B）。
    pub to: String,
}

impl ValidTopologyIntent {
    /// 返回注入 Sugiyama 图的有向边 `(from_edge, to_edge)`。
    ///
    /// - `Below(A,B)`：A 应在 B 下方 → 注入边 `B→A`（使 `rank(A) > rank(B)`）
    /// - `Above(A,B)`：A 应在 B 上方 → 注入边 `A→B`（使 `rank(A) < rank(B)`）
    pub fn edge(&self) -> (&str, &str) {
        match self.kind {
            "below" => (&self.to, &self.from), // B→A
            "above" => (&self.from, &self.to), // A→B
            _ => unreachable!("invalid kind"),
        }
    }
}

/// 校验拓扑意图集合。
///
/// 返回 `(有效意图列表, 校验结果报告)`。有效意图按声明顺序排列，
/// 可直接用于 `build_graph_with_overlay` 的注入。报告中的 `IntentResult`
/// 按 `index` 升序排列，包含被跳过意图的 `NotFound` / `Conflicted` 状态。
///
/// 校验通过的有效意图不会出现在报告中（它们的 `Satisfied` 状态由布局后的
/// rank 比对阶段填充）。
pub fn validate_topology_intents(
    diagram: &Diagram,
    overlay: &LayoutIntentOverlay,
) -> (Vec<ValidTopologyIntent>, Vec<IntentResult>) {
    let mut valid = Vec::new();
    let mut results = Vec::new();

    let node_set: HashSet<&str> = diagram.entities.iter().map(|e| e.id.as_str()).collect();

    // 真实边邻接表（用于环检测的 DFS）
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for relation in &diagram.relations {
        adj.entry(relation.from.as_str())
            .or_default()
            .push(relation.to.as_str());
    }

    // 已接受的意图边方向集合（用于矛盾去重）
    let mut seen_pairs: HashSet<(String, String)> = HashSet::new();

    for (i, intent) in overlay.topology.iter().enumerate() {
        // 提取原始意图参数 (A, B) 与注入边方向
        // Below(A,B): A 应在 B 下方 → 注入边 B→A
        // Above(A,B): A 应在 B 上方 → 注入边 A→B
        let (orig_from, orig_to, kind, edge_from, edge_to) = match intent {
            TopologyIntent::Below { from, to } => {
                (from.as_str(), to.as_str(), "below", to.as_str(), from.as_str())
            }
            TopologyIntent::Above { from, to } => {
                (from.as_str(), to.as_str(), "above", from.as_str(), to.as_str())
            }
        };

        // 节点存在性
        if !node_set.contains(orig_from) {
            results.push(IntentResult {
                index: i,
                kind,
                status: IntentStatus::NotFound,
                message: Some(format!("node '{orig_from}' not found")),
            });
            continue;
        }
        if !node_set.contains(orig_to) {
            results.push(IntentResult {
                index: i,
                kind,
                status: IntentStatus::NotFound,
                message: Some(format!("node '{orig_to}' not found")),
            });
            continue;
        }

        // 自环检查（Below(A,A) 无意义）
        if orig_from == orig_to {
            results.push(IntentResult {
                index: i,
                kind,
                status: IntentStatus::Conflicted,
                message: Some(format!("self-loop intent: '{orig_from}' → '{orig_to}'")),
            });
            continue;
        }

        // 矛盾去重：检查同方向重复（基于注入边方向）
        let pair = (edge_from.to_string(), edge_to.to_string());
        if seen_pairs.contains(&pair) {
            results.push(IntentResult {
                index: i,
                kind,
                status: IntentStatus::Conflicted,
                message: Some("duplicate topology intent".into()),
            });
            continue;
        }
        // 矛盾去重：检查反方向冲突
        // Below(A,B) = edge B→A; Above(A,B) = edge A→B → 互为反方向 → 矛盾
        let reverse_pair = (edge_to.to_string(), edge_from.to_string());
        if seen_pairs.contains(&reverse_pair) {
            results.push(IntentResult {
                index: i,
                kind,
                status: IntentStatus::Conflicted,
                message: Some("contradicts earlier topology intent".into()),
            });
            continue;
        }

        // 环检测：添加 edge_from→edge_to 后是否产生环？
        // 即：图中是否已存在 edge_to →...→ edge_from 的路径？
        if has_path(&adj, edge_to, edge_from) {
            results.push(IntentResult {
                index: i,
                kind,
                status: IntentStatus::Conflicted,
                message: Some("creates cycle with existing edges".into()),
            });
            continue;
        }

        // 接受此意图
        seen_pairs.insert(pair);
        adj.entry(edge_from).or_default().push(edge_to);
        valid.push(ValidTopologyIntent {
            index: i,
            kind,
            from: orig_from.to_string(),
            to: orig_to.to_string(),
        });
    }

    (valid, results)
}

/// DFS 检查 `from` 到 `to` 是否存在路径。
fn has_path(adj: &HashMap<&str, Vec<&str>>, from: &str, to: &str) -> bool {
    if from == to {
        return true;
    }
    let mut visited = HashSet::new();
    let mut stack = vec![from];
    while let Some(node) = stack.pop() {
        if !visited.insert(node) {
            continue;
        }
        if let Some(neighbors) = adj.get(node) {
            for &succ in neighbors {
                if succ == to {
                    return true;
                }
                if !visited.contains(succ) {
                    stack.push(succ);
                }
            }
        }
    }
    false
}

/// 比对布局后的 rank 映射，判断拓扑意图是否被满足。
///
/// `Below(A,B)` 满足当且仅当 `rank(A) > rank(B)`（A 在 B 下游）。
/// `Above(A,B)` 满足当且仅当 `rank(A) < rank(B)`（A 在 B 上游）。
///
/// `ValidTopologyIntent.from` / `.to` 存储原始意图参数 (A, B)，
/// 与 `kind` 共同决定满足条件。
///
/// 返回每个有效意图的 `IntentResult`（`Satisfied` 或 `Partial`）。
pub fn evaluate_topology_satisfaction(
    valid: &[ValidTopologyIntent],
    ranks: &HashMap<String, usize>,
) -> Vec<IntentResult> {
    valid
        .iter()
        .map(|v| {
            let from_rank = ranks.get(&v.from);
            let to_rank = ranks.get(&v.to);
            let status = match (from_rank, to_rank) {
                (Some(fr), Some(tr)) => {
                    let satisfied = match v.kind {
                        "below" => *fr > *tr, // rank(A) > rank(B)
                        "above" => *fr < *tr, // rank(A) < rank(B)
                        _ => false,
                    };
                    if satisfied {
                        IntentStatus::Satisfied
                    } else {
                        IntentStatus::Partial
                    }
                }
                _ => IntentStatus::NotFound,
            };
            IntentResult {
                index: v.index,
                kind: v.kind,
                status,
                message: if status == IntentStatus::Partial {
                    Some(format!(
                        "rank({}) = {:?}, rank({}) = {:?}; intent not fully satisfied",
                        v.from,
                        from_rank,
                        v.to,
                        to_rank
                    ))
                } else {
                    None
                },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        ArrowType, AttributeMap, Diagram, Entity, Identifier, Relation, SourceInfo, Span,
    };
    use crate::types::DiagramType;

    fn make_diagram(entities: &[&str], relations: &[(&str, &str)]) -> Diagram {
        let span = Span::dummy();
        Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: vec![],
            entities: entities
                .iter()
                .map(|id| Entity {
                    id: Identifier::new_unchecked(id),
                    label: id.to_string(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span,
                })
                .collect(),
            relations: relations
                .iter()
                .map(|(from, to)| Relation {
                    from: Identifier::new_unchecked(from),
                    to: Identifier::new_unchecked(to),
                    arrow: ArrowType::Active,
                    label: None,
                    head_label: None,
                    tail_label: None,
                    attributes: AttributeMap::default(),
                    span,
                })
                .collect(),
            groups: vec![],
            style_decls: vec![],
            source_info: SourceInfo {
                file: None,
                line_count: 1,
            },
            ..Default::default()
        }
    }

    #[test]
    fn valid_below_intent_passes() {
        // 无真实边；Below(A,B) → edge B→A，不构成环
        let diagram = make_diagram(&["a", "b"], &[]);
        let overlay = LayoutIntentOverlay {
            topology: vec![TopologyIntent::Below {
                from: "a".into(),
                to: "b".into(),
            }],
            geometric: vec![],
        };

        let (valid, results) = validate_topology_intents(&diagram, &overlay);
        assert_eq!(valid.len(), 1);
        assert_eq!(valid[0].from, "a");
        assert_eq!(valid[0].to, "b");
        assert!(results.is_empty());
    }

    #[test]
    fn above_intent_resolves_to_reverse_edge() {
        let diagram = make_diagram(&["a", "b"], &[]);
        let overlay = LayoutIntentOverlay {
            topology: vec![TopologyIntent::Above {
                from: "a".into(),
                to: "b".into(),
            }],
            geometric: vec![],
        };

        let (valid, results) = validate_topology_intents(&diagram, &overlay);
        assert_eq!(valid.len(), 1);
        // Above(A,B): from=A, to=B (原始意图参数)
        assert_eq!(valid[0].from, "a");
        assert_eq!(valid[0].to, "b");
        // 注入边方向: A→B
        let (edge_from, edge_to) = valid[0].edge();
        assert_eq!(edge_from, "a");
        assert_eq!(edge_to, "b");
        assert!(results.is_empty());
    }

    #[test]
    fn nonexistent_node_marked_not_found() {
        let diagram = make_diagram(&["a", "b"], &[("a", "b")]);
        let overlay = LayoutIntentOverlay {
            topology: vec![TopologyIntent::Below {
                from: "a".into(),
                to: "ghost".into(),
            }],
            geometric: vec![],
        };

        let (valid, results) = validate_topology_intents(&diagram, &overlay);
        assert!(valid.is_empty());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, IntentStatus::NotFound);
        assert!(results[0].message.as_deref().unwrap().contains("ghost"));
    }

    #[test]
    fn duplicate_intent_marked_conflicted() {
        let diagram = make_diagram(&["a", "b"], &[]);
        let overlay = LayoutIntentOverlay {
            topology: vec![
                TopologyIntent::Below { from: "a".into(), to: "b".into() },
                TopologyIntent::Below { from: "a".into(), to: "b".into() },
            ],
            geometric: vec![],
        };

        let (valid, results) = validate_topology_intents(&diagram, &overlay);
        assert_eq!(valid.len(), 1);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, IntentStatus::Conflicted);
    }

    #[test]
    fn contradictory_below_above_marked_conflicted() {
        let diagram = make_diagram(&["a", "b"], &[]);
        // Below(A,B) = B→A; Above(A,B) = A→B → contradiction
        let overlay = LayoutIntentOverlay {
            topology: vec![
                TopologyIntent::Below { from: "a".into(), to: "b".into() },
                TopologyIntent::Above { from: "a".into(), to: "b".into() },
            ],
            geometric: vec![],
        };

        let (valid, results) = validate_topology_intents(&diagram, &overlay);
        assert_eq!(valid.len(), 1);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, IntentStatus::Conflicted);
        assert_eq!(valid[0].from, "a");
        assert_eq!(valid[0].to, "b");
    }

    #[test]
    fn cycle_with_real_edges_marked_conflicted() {
        // Real: A→B. Intent: Below(A,B) → edge B→A. Creates cycle A→B→A.
        let diagram = make_diagram(&["a", "b"], &[("a", "b")]);
        let overlay = LayoutIntentOverlay {
            topology: vec![TopologyIntent::Below {
                from: "a".into(),
                to: "b".into(),
            }],
            geometric: vec![],
        };

        let (valid, results) = validate_topology_intents(&diagram, &overlay);
        assert!(valid.is_empty());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, IntentStatus::Conflicted);
        assert!(results[0].message.as_deref().unwrap().contains("cycle"));
    }

    #[test]
    fn cycle_among_intents_marked_conflicted() {
        let diagram = make_diagram(&["a", "b", "c"], &[]);
        // Below(A,B) = B→A, Below(B,C) = C→B, Below(C,A) = A→C → cycle
        let overlay = LayoutIntentOverlay {
            topology: vec![
                TopologyIntent::Below { from: "a".into(), to: "b".into() },
                TopologyIntent::Below { from: "b".into(), to: "c".into() },
                TopologyIntent::Below { from: "c".into(), to: "a".into() },
            ],
            geometric: vec![],
        };

        let (valid, results) = validate_topology_intents(&diagram, &overlay);
        assert_eq!(valid.len(), 2);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, IntentStatus::Conflicted);
        assert_eq!(results[0].index, 2);
    }

    #[test]
    fn self_loop_marked_conflicted() {
        let diagram = make_diagram(&["a"], &[]);
        let overlay = LayoutIntentOverlay {
            topology: vec![TopologyIntent::Below {
                from: "a".into(),
                to: "a".into(),
            }],
            geometric: vec![],
        };

        let (valid, results) = validate_topology_intents(&diagram, &overlay);
        assert!(valid.is_empty());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, IntentStatus::Conflicted);
    }

    #[test]
    fn satisfaction_satisfied_when_rank_correct() {
        let valid = vec![ValidTopologyIntent {
            index: 0,
            kind: "below",
            from: "a".into(),
            to: "b".into(),
        }];
        let ranks = HashMap::from([("a".to_string(), 2), ("b".to_string(), 1)]);
        let results = evaluate_topology_satisfaction(&valid, &ranks);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, IntentStatus::Satisfied);
    }

    #[test]
    fn satisfaction_partial_when_rank_wrong() {
        let valid = vec![ValidTopologyIntent {
            index: 0,
            kind: "below",
            from: "a".into(),
            to: "b".into(),
        }];
        // rank(A) = 1, rank(B) = 2 → A is above B, but below intent wants A below B
        let ranks = HashMap::from([("a".to_string(), 1), ("b".to_string(), 2)]);
        let results = evaluate_topology_satisfaction(&valid, &ranks);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, IntentStatus::Partial);
    }

    #[test]
    fn satisfaction_above_checked_correctly() {
        let valid = vec![ValidTopologyIntent {
            index: 0,
            kind: "above",
            from: "a".into(), // Above(A,B): from=A, to=B
            to: "b".into(),
        }];
        // Above(A,B) wants A above B → rank(A) < rank(B)
        let ranks = HashMap::from([("a".to_string(), 0), ("b".to_string(), 1)]);
        let results = evaluate_topology_satisfaction(&valid, &ranks);
        assert_eq!(results[0].status, IntentStatus::Satisfied);
    }
}
