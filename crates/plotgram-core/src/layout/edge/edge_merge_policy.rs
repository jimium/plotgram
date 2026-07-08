//! 语义边合并策略：统一「两条边是否允许共享 trunk / corridor lane」的判定。
//!
//! 供 post-route bundling（`edge_bundling`）与架构图路由门控共用。
//! architecture 图：仅当两条边属于同一 `MergeGroup` 时才允许路径级合并；
//! flowchart 等图：几何 bundling 优先，不做语义拦截。

use crate::layout::edge::common::edge_geometry::canonical_pair;
use crate::types::DiagramType;

/// 合并组类型（用于调试与 lint）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeGroupKind {
    SameSourceFanOut,
    SameTargetFanIn,
    ParallelPair,
    SuperEdgePair,
}

/// 边的语义合并组键。两条边可共享 trunk 当且仅当至少拥有一个相同的 `MergeGroup`。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum MergeGroup {
    SameSourceFanOut { from_id: String },
    SameTargetFanIn { to_id: String },
    ParallelPair { a: String, b: String },
    SuperEdgePair { from_super: String, to_super: String },
}

impl MergeGroup {
    pub fn kind(&self) -> MergeGroupKind {
        match self {
            MergeGroup::SameSourceFanOut { .. } => MergeGroupKind::SameSourceFanOut,
            MergeGroup::SameTargetFanIn { .. } => MergeGroupKind::SameTargetFanIn,
            MergeGroup::ParallelPair { .. } => MergeGroupKind::ParallelPair,
            MergeGroup::SuperEdgePair { .. } => MergeGroupKind::SuperEdgePair,
        }
    }
}

/// 单条边的合并上下文（不依赖完整 `EdgeFeatures`，便于路由阶段复用）。
#[derive(Debug, Clone, Copy)]
pub struct EdgeMergeContext<'a> {
    pub from_id: &'a str,
    pub to_id: &'a str,
    pub edge_index: usize,
    pub from_leaf_group: Option<&'a str>,
    pub to_leaf_group: Option<&'a str>,
}

/// 该图类型是否启用语义合并门控（architecture 启用，flowchart 等不启用）。
pub fn requires_semantic_merge(diagram_type: DiagramType) -> bool {
    matches!(diagram_type, DiagramType::Architecture)
}

/// 列出一条边所属的全部合并组（确定性顺序）。
///
/// 当 `from_leaf_group` / `to_leaf_group` 均存在且不同时，追加 `SuperEdgePair`。
/// 注意：`SuperEdgePair` 仅用于 corridor lane 相邻分配，**不参与** trunk 合并判定
/// （见 `edges_may_share_trunk` 中的过滤）。
pub fn merge_groups_for_edge(ctx: &EdgeMergeContext<'_>) -> Vec<MergeGroup> {
    let mut groups = vec![
        MergeGroup::SameSourceFanOut {
            from_id: ctx.from_id.to_string(),
        },
        MergeGroup::SameTargetFanIn {
            to_id: ctx.to_id.to_string(),
        },
    ];
    let (a, b) = canonical_pair(ctx.from_id, ctx.to_id);
    groups.push(MergeGroup::ParallelPair {
        a: a.to_string(),
        b: b.to_string(),
    });
    if let (Some(fg), Some(tg)) = (ctx.from_leaf_group, ctx.to_leaf_group) {
        if fg != tg {
            let (sa, sb) = canonical_pair(fg, tg);
            groups.push(MergeGroup::SuperEdgePair {
                from_super: sa.to_string(),
                to_super: sb.to_string(),
            });
        }
    }
    groups.sort();
    groups
}

/// 两条边是否允许共享路径 trunk（或视为同一语义 bundle）。
///
/// `SuperEdgePair` 仅用于 corridor lane 分配，不参与 trunk 合并：即使两条边
/// 同属一个 `SuperEdgePair`（同 leaf 组对），也不因此允许共享 trunk。
pub fn edges_may_share_trunk(
    e1: &EdgeMergeContext<'_>,
    e2: &EdgeMergeContext<'_>,
    diagram_type: DiagramType,
) -> bool {
    if !requires_semantic_merge(diagram_type) {
        return true;
    }
    if e1.edge_index == e2.edge_index {
        return true;
    }
    let g1 = merge_groups_for_edge(e1);
    let g2 = merge_groups_for_edge(e2);
    g1.iter()
        .filter(|k| !matches!(k, MergeGroup::SuperEdgePair { .. }))
        .any(|k| g2.contains(k))
}

/// 从端点 id 构建最小上下文（无 leaf group，兼容 bundling 特征阶段）。
pub fn edge_merge_context<'a>(
    from_id: &'a str,
    to_id: &'a str,
    edge_index: usize,
) -> EdgeMergeContext<'a> {
    EdgeMergeContext {
        from_id,
        to_id,
        edge_index,
        from_leaf_group: None,
        to_leaf_group: None,
    }
}

/// 从端点 id + leaf group 构建完整上下文（供 corridor lane 分配与 lint 使用）。
pub fn edge_merge_context_with_groups<'a>(
    from_id: &'a str,
    to_id: &'a str,
    edge_index: usize,
    from_leaf_group: Option<&'a str>,
    to_leaf_group: Option<&'a str>,
) -> EdgeMergeContext<'a> {
    EdgeMergeContext {
        from_id,
        to_id,
        edge_index,
        from_leaf_group,
        to_leaf_group,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx<'a>(
        from: &'a str,
        to: &'a str,
        idx: usize,
        from_g: Option<&'a str>,
        to_g: Option<&'a str>,
    ) -> EdgeMergeContext<'a> {
        EdgeMergeContext {
            from_id: from,
            to_id: to,
            edge_index: idx,
            from_leaf_group: from_g,
            to_leaf_group: to_g,
        }
    }

    #[test]
    fn unrelated_sources_do_not_share_trunk_on_architecture() {
        let a = ctx("auth_svc", "redis", 0, Some("private"), Some("data"));
        let b = ctx("biz_svc", "db_master", 1, Some("private"), Some("data"));
        assert!(
            !edges_may_share_trunk(&a, &b, DiagramType::Architecture),
            "不同源、不同无向对的边不应合并"
        );
    }

    #[test]
    fn same_source_fan_out_may_share_trunk() {
        let a = ctx("lb", "auth_svc", 0, None, None);
        let b = ctx("lb", "biz_svc", 1, None, None);
        assert!(edges_may_share_trunk(
            &a,
            &b,
            DiagramType::Architecture
        ));
    }

    #[test]
    fn parallel_pair_may_share_trunk() {
        let a = ctx("a", "b", 0, None, None);
        let b = ctx("b", "a", 1, None, None);
        assert!(edges_may_share_trunk(
            &a,
            &b,
            DiagramType::Architecture
        ));
    }

    #[test]
    fn same_leaf_group_pair_different_endpoints_do_not_share_trunk() {
        let a = ctx("auth_svc", "redis", 0, Some("private_subnet"), Some("data_subnet"));
        let b = ctx("biz_svc", "db_master", 1, Some("private_subnet"), Some("data_subnet"));
        assert!(
            !edges_may_share_trunk(&a, &b, DiagramType::Architecture),
            "同组对但不同源/宿的边仍不应合并"
        );
    }

    #[test]
    fn flowchart_allows_geometric_merge() {
        let a = ctx("auth_svc", "redis", 0, None, None);
        let b = ctx("biz_svc", "db_master", 1, None, None);
        assert!(edges_may_share_trunk(
            &a,
            &b,
            DiagramType::Flowchart
        ));
    }

    #[test]
    fn super_edge_pair_populated_from_leaf_groups() {
        let a = ctx("auth_svc", "redis", 0, Some("private_subnet"), Some("data_subnet"));
        let groups = merge_groups_for_edge(&a);
        assert!(
            groups
                .iter()
                .any(|g| matches!(g, MergeGroup::SuperEdgePair { from_super, to_super }
                    if from_super == "data_subnet" && to_super == "private_subnet")),
            "leaf group 均存在且不同时应填充 SuperEdgePair: {:?}",
            groups
        );
    }

    #[test]
    fn super_edge_pair_skipped_when_same_leaf_group() {
        let a = ctx("auth_svc", "biz_svc", 0, Some("private_subnet"), Some("private_subnet"));
        let groups = merge_groups_for_edge(&a);
        assert!(
            !groups
                .iter()
                .any(|g| matches!(g, MergeGroup::SuperEdgePair { .. })),
            "同 leaf group 不应填充 SuperEdgePair: {:?}",
            groups
        );
    }

    #[test]
    fn super_edge_pair_skipped_when_leaf_group_missing() {
        let a = ctx("auth_svc", "redis", 0, None, Some("data_subnet"));
        let groups = merge_groups_for_edge(&a);
        assert!(
            !groups
                .iter()
                .any(|g| matches!(g, MergeGroup::SuperEdgePair { .. })),
            "leaf group 缺失时不应填充 SuperEdgePair: {:?}",
            groups
        );
    }

    #[test]
    fn super_edge_pair_does_not_allow_trunk_sharing() {
        // 两条边同属一个 SuperEdgePair（同 leaf 组对），但不同源/宿
        let a = ctx("auth_svc", "redis", 0, Some("private_subnet"), Some("data_subnet"));
        let b = ctx("biz_svc", "db_master", 1, Some("private_subnet"), Some("data_subnet"));
        assert!(
            !edges_may_share_trunk(&a, &b, DiagramType::Architecture),
            "SuperEdgePair 不应允许 trunk 共享（仅用于 corridor lane）"
        );
    }

    #[test]
    fn super_edge_pair_key_is_canonical() {
        // 反向边应产生相同的 SuperEdgePair 键
        let a = ctx("redis", "auth_svc", 0, Some("data_subnet"), Some("private_subnet"));
        let groups_a = merge_groups_for_edge(&a);
        let b = ctx("auth_svc", "redis", 1, Some("private_subnet"), Some("data_subnet"));
        let groups_b = merge_groups_for_edge(&b);
        let key_a = groups_a
            .iter()
            .find_map(|g| match g {
                MergeGroup::SuperEdgePair { from_super, to_super } => {
                    Some((from_super.clone(), to_super.clone()))
                }
                _ => None,
            })
            .unwrap();
        let key_b = groups_b
            .iter()
            .find_map(|g| match g {
                MergeGroup::SuperEdgePair { from_super, to_super } => {
                    Some((from_super.clone(), to_super.clone()))
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(
            key_a, key_b,
            "反向边的 SuperEdgePair 键应规范化一致"
        );
    }
}
