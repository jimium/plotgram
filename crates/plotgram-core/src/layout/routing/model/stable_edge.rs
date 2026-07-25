//! `StableEdgeId` 与 `StableEdgeStore`：路由输入的稳定边身份。
//!
//! 现状：`LayoutResult.edges: Vec<EdgeLayout>` 与 `diagram.relations` 靠**下标隐式对应**，
//! 没有显式 id。路由问题模型（doc16 §4）要求边按稳定 `EdgeId` 连续引用，
//! 以支持可诊断的 problem signature 与后续增量路由。
//!
//! `StableEdgeId` 等于 `diagram.relations` 的声明序号；relations 顺序在 parse 后固定，
//! 故该 id 在一次布局-路由内稳定。本模块只从**声明语义**抽取，不读取 `DiagramType`。

use crate::ast::{ArrowType, Diagram};
use serde::Serialize;

/// 稳定边标识：等于 `diagram.relations` 中的声明序号。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct StableEdgeId(pub usize);

impl StableEdgeId {
    /// 底层下标（与 `LayoutResult.edges` / `diagram.relations` 对应）。
    pub fn index(self) -> usize {
        self.0
    }
}

/// 单条边的稳定视图（只读，从声明语义抽取）。
///
/// 只携带路由问题模型需要的稳定信息；不含几何、不含定位。
#[derive(Debug, Clone, Serialize)]
pub struct StableEdge {
    pub id: StableEdgeId,
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
    pub fn from_diagram(diagram: &Diagram) -> Self {
        let edges = diagram
            .relations
            .iter()
            .enumerate()
            .map(|(i, r)| StableEdge {
                id: StableEdgeId(i),
                from: r.from.as_str().to_string(),
                to: r.to.as_str().to_string(),
                arrow: r.arrow.clone(),
                has_mid_label: r.label.is_some(),
                has_head_label: r.head_label.is_some(),
                has_tail_label: r.tail_label.is_some(),
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
