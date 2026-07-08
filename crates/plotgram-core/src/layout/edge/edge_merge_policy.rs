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
    groups.sort();
    groups
}

/// 两条边是否允许共享路径 trunk（或视为同一语义 bundle）。
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
    g1.iter().any(|k| g2.contains(k))
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
}
