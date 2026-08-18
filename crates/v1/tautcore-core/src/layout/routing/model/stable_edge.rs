//! `StableEdgeId` 与 `StableEdgeStore`：路由输入的稳定边身份。
//!
//! 现状：`LayoutResult.edges: Vec<EdgeLayout>` 与 `diagram.relations` 靠**下标隐式对应**，
//! 没有显式 id。路由问题模型（doc16 §4）要求边按稳定 `EdgeId` 连续引用，
//! 以支持可诊断的 problem signature 与后续增量路由。
//!
//! ## 两层身份（Slice F2a）
//!
//! - [`StableEdgeId`]：**solve 内 positional handle**，等于 `diagram.relations` 的声明
//!   序号；仅在一次布局-路由内稳定，**不得**跨版本持久。
//! - [`StableEdgeIdentity`]：跨版本持久 key（from/to/parallel_ordinal）。声明头部
//!   插入/删除其它 relation 后，既有边的 identity 不变，供增量路由 diff。
//!
//! 本模块只从**声明语义**抽取，不读取 `DiagramType`。

use crate::ast::{ArrowType, Diagram};
use serde::Serialize;
use std::collections::HashMap;

/// 稳定边标识：等于 `diagram.relations` 中的声明序号。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct StableEdgeId(pub usize);

impl StableEdgeId {
    /// 底层下标（与 `LayoutResult.edges` / `diagram.relations` 对应）。
    pub fn index(self) -> usize {
        self.0
    }
}

/// 跨版本持久边身份（Slice F2a）：同向端点对内按声明序编 ordinal。
///
/// 声明头部插入新 relation 不改变既有边的 identity（positional id 会变，
/// identity 不变）；平行边（同 from/to 多条）用 `parallel_ordinal` 区分。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct StableEdgeIdentity {
    pub from: String,
    pub to: String,
    /// 同向（from→to 完全相同）端点对内的声明序序号（第 n 条平行边）。
    pub parallel_ordinal: usize,
}

/// 跨版本 identity 匹配结果（全部确定性排序）。
#[derive(Debug, Clone, Default)]
pub struct EdgeIdentityDiff {
    /// (prev_idx, new_idx)：两版本中 identity 相同的边，按 new_idx 升序。
    pub retained: Vec<(usize, usize)>,
    /// 仅新版本存在的边下标，升序。
    pub added: Vec<usize>,
    /// 仅旧版本存在的边下标，升序。
    pub removed: Vec<usize>,
}

/// 单条边的稳定视图（只读，从声明语义抽取）。
///
/// 只携带路由问题模型需要的稳定信息；不含几何、不含定位。
#[derive(Debug, Clone, Serialize)]
pub struct StableEdge {
    pub id: StableEdgeId,
    /// 跨版本持久 key（Slice F2a）。
    pub identity: StableEdgeIdentity,
    pub from: String,
    pub to: String,
    pub arrow: ArrowType,
    /// 是否声明了中段标签（用于 label policy，不含定位）。
    pub has_mid_label: bool,
    /// 是否声明了箭头头部标签。
    pub has_head_label: bool,
    /// 是否声明了箭头尾部标签。
    pub has_tail_label: bool,
}

impl StableEdge {
    /// 自环：起止端点相同。
    pub fn is_self_loop(&self) -> bool {
        self.from == self.to
    }

    /// 端点无序规范化 key，用于识别平行边（相同端点对的多条边）。
    pub fn endpoint_key(&self) -> (String, String) {
        if self.from <= self.to {
            (self.from.clone(), self.to.clone())
        } else {
            (self.to.clone(), self.from.clone())
        }
    }

    /// 是否携带任意声明标签。
    pub fn has_any_label(&self) -> bool {
        self.has_mid_label || self.has_head_label || self.has_tail_label
    }
}

/// 稳定边存储：按 `StableEdgeId` 连续存储（下标即 id）。
#[derive(Debug, Clone, Default, Serialize)]
pub struct StableEdgeStore {
    edges: Vec<StableEdge>,
}

impl StableEdgeStore {
    /// 从 `Diagram.relations` 构造。只读声明语义，不读取 `DiagramType`。
    ///
    /// identity 的 `parallel_ordinal` 按同向 (from, to) 对内的声明序编号，
    /// 确定性不依赖 HashMap 迭代序（逐条递增计数）。
    pub fn from_diagram(diagram: &Diagram) -> Self {
        let mut ordinal_counter: HashMap<(String, String), usize> = HashMap::new();
        let edges = diagram
            .relations
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let from = r.from.as_str().to_string();
                let to = r.to.as_str().to_string();
                let counter = ordinal_counter
                    .entry((from.clone(), to.clone()))
                    .or_insert(0);
                let parallel_ordinal = *counter;
                *counter += 1;
                StableEdge {
                    id: StableEdgeId(i),
                    identity: StableEdgeIdentity {
                        from: from.clone(),
                        to: to.clone(),
                        parallel_ordinal,
                    },
                    from,
                    to,
                    arrow: r.arrow.clone(),
                    has_mid_label: r.label.is_some(),
                    has_head_label: r.head_label.is_some(),
                    has_tail_label: r.tail_label.is_some(),
                }
            })
            .collect();
        Self { edges }
    }

    pub fn len(&self) -> usize {
        self.edges.len()
    }

    pub fn is_empty(&self) -> bool {
        self.edges.is_empty()
    }

    pub fn get(&self, id: StableEdgeId) -> Option<&StableEdge> {
        self.edges.get(id.0)
    }

    pub fn iter(&self) -> std::slice::Iter<'_, StableEdge> {
        self.edges.iter()
    }

    pub fn as_slice(&self) -> &[StableEdge] {
        &self.edges
    }

    /// Slice F2a：跨版本 identity 匹配（供增量路由 diff）。
    ///
    /// 以 [`StableEdgeIdentity`] 为 key 对齐两版本：同 identity → retained；
    /// 仅新版存在 → added；仅旧版存在 → removed。三个集合均确定性排序。
    pub fn match_identities(&self, prev: &StableEdgeStore) -> EdgeIdentityDiff {
        let prev_by_identity: HashMap<&StableEdgeIdentity, usize> = prev
            .edges
            .iter()
            .enumerate()
            .map(|(i, e)| (&e.identity, i))
            .collect();

        let mut diff = EdgeIdentityDiff::default();
        let mut matched_prev = vec![false; prev.edges.len()];
        for (new_idx, edge) in self.edges.iter().enumerate() {
            match prev_by_identity.get(&edge.identity) {
                Some(&prev_idx) => {
                    diff.retained.push((prev_idx, new_idx));
                    matched_prev[prev_idx] = true;
                }
                None => diff.added.push(new_idx),
            }
        }
        for (prev_idx, matched) in matched_prev.iter().enumerate() {
            if !matched {
                diff.removed.push(prev_idx);
            }
        }
        // retained 按 new_idx 升序（枚举序已保证）；added/removed 枚举即升序。
        diff
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Diagram, Identifier, Relation, Span, AttributeMap, SourceInfo};
    use crate::types::DiagramType;

    fn rel(from: &str, to: &str, arrow: ArrowType, label: Option<&str>) -> Relation {
        Relation {
            from: Identifier::new_unchecked(from),
            to: Identifier::new_unchecked(to),
            arrow,
            label: label.map(|s| s.to_string()),
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span: Span::dummy(),
        }
    }

    fn diagram_with(relations: Vec<Relation>) -> Diagram {
        let mut d = Diagram::new(DiagramType::Flowchart, SourceInfo::default());
        d.relations = relations;
        d
    }

    #[test]
    fn stable_id_equals_declaration_order() {
        let d = diagram_with(vec![
            rel("a", "b", ArrowType::Active, None),
            rel("b", "c", ArrowType::Active, Some("x")),
        ]);
        let store = StableEdgeStore::from_diagram(&d);
        assert_eq!(store.len(), 2);
        assert_eq!(store.get(StableEdgeId(0)).unwrap().from, "a");
        assert_eq!(store.get(StableEdgeId(1)).unwrap().to, "c");
        assert!(store.get(StableEdgeId(1)).unwrap().has_mid_label);
    }

    #[test]
    fn identity_survives_head_insertion() {
        // v1: [a→b, b→c]；v2 在声明头部插入 x→y。
        let v1 = StableEdgeStore::from_diagram(&diagram_with(vec![
            rel("a", "b", ArrowType::Active, None),
            rel("b", "c", ArrowType::Active, None),
        ]));
        let v2 = StableEdgeStore::from_diagram(&diagram_with(vec![
            rel("x", "y", ArrowType::Active, None),
            rel("a", "b", ArrowType::Active, None),
            rel("b", "c", ArrowType::Active, None),
        ]));
        let diff = v2.match_identities(&v1);
        // positional id 变了（1、2），identity 不变 → retained。
        assert_eq!(diff.retained, vec![(0, 1), (1, 2)]);
        assert_eq!(diff.added, vec![0]);
        assert!(diff.removed.is_empty());
    }

    #[test]
    fn parallel_edge_ordinal_is_stable() {
        // 两条 a→b 平行边按声明序编 ordinal 0/1。
        let v1 = StableEdgeStore::from_diagram(&diagram_with(vec![
            rel("a", "b", ArrowType::Active, Some("first")),
            rel("a", "b", ArrowType::Active, Some("second")),
        ]));
        assert_eq!(v1.get(StableEdgeId(0)).unwrap().identity.parallel_ordinal, 0);
        assert_eq!(v1.get(StableEdgeId(1)).unwrap().identity.parallel_ordinal, 1);

        // 头部插入无关边后，平行边 ordinal 与匹配关系保持稳定。
        let v2 = StableEdgeStore::from_diagram(&diagram_with(vec![
            rel("m", "n", ArrowType::Active, None),
            rel("a", "b", ArrowType::Active, Some("first")),
            rel("a", "b", ArrowType::Active, Some("second")),
        ]));
        assert_eq!(v2.get(StableEdgeId(1)).unwrap().identity.parallel_ordinal, 0);
        assert_eq!(v2.get(StableEdgeId(2)).unwrap().identity.parallel_ordinal, 1);
        let diff = v2.match_identities(&v1);
        assert_eq!(diff.retained, vec![(0, 1), (1, 2)]);
        assert_eq!(diff.added, vec![0]);
        assert!(diff.removed.is_empty());
    }

    #[test]
    fn removed_edge_reported_deterministically() {
        let v1 = StableEdgeStore::from_diagram(&diagram_with(vec![
            rel("a", "b", ArrowType::Active, None),
            rel("b", "c", ArrowType::Active, None),
            rel("c", "d", ArrowType::Active, None),
        ]));
        let v2 = StableEdgeStore::from_diagram(&diagram_with(vec![
            rel("b", "c", ArrowType::Active, None),
        ]));
        let diff = v2.match_identities(&v1);
        assert_eq!(diff.retained, vec![(1, 0)]);
        assert!(diff.added.is_empty());
        assert_eq!(diff.removed, vec![0, 2]);
    }

    #[test]
    fn self_loop_and_endpoint_key() {
        let d = diagram_with(vec![
            rel("a", "a", ArrowType::Active, None),
            rel("c", "b", ArrowType::Active, None),
        ]);
        let store = StableEdgeStore::from_diagram(&d);
        assert!(store.get(StableEdgeId(0)).unwrap().is_self_loop());
        // 端点无序规范化：c->b 与 b->c 相同 key
        assert_eq!(
            store.get(StableEdgeId(1)).unwrap().endpoint_key(),
            ("b".to_string(), "c".to_string())
        );
    }
}
