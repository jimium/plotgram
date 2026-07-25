//! `FrozenNodeStore` / `FrozenGroupStore`：路由输入的只读节点/分组快照。
//!
//! doc16 §16.1：节点须在 route 前 freeze，路由阶段禁止移动节点。
//! 本模块提供**借用型只读适配器**，包裹 `LayoutResult` 的 `nodes` / `groups`，
//! 只暴露 `&NodeLayout` / `&GroupLayout`，不提供任何 `&mut` 访问。
//!
//! 迭代一律走**按 id 升序的确定性顺序**（AGENTS.md §2：禁止依赖 HashMap 迭代顺序）。

use crate::layout::types::{GroupLayout, NodeLayout};
use std::collections::HashMap;

/// 只读节点存储：借用布局结果中的节点，路由阶段不可修改。
pub struct FrozenNodeStore<'a> {
    nodes: &'a HashMap<String, NodeLayout>,
}

impl<'a> FrozenNodeStore<'a> {
    pub fn new(nodes: &'a HashMap<String, NodeLayout>) -> Self {
        Self { nodes }
    }

    pub fn get(&self, id: &str) -> Option<&NodeLayout> {
        self.nodes.get(id)
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// 按 id 升序的确定性 (id, node) 列表。
    pub fn sorted_entries(&self) -> Vec<(&String, &NodeLayout)> {
        let mut entries: Vec<(&String, &NodeLayout)> = self.nodes.iter().collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));
        entries
    }
}

/// 只读分组存储：借用布局结果中的分组包围框。
pub struct FrozenGroupStore<'a> {
    groups: &'a HashMap<String, GroupLayout>,
}

impl<'a> FrozenGroupStore<'a> {
    pub fn new(groups: &'a HashMap<String, GroupLayout>) -> Self {
        Self { groups }
    }

    pub fn get(&self, id: &str) -> Option<&GroupLayout> {
        self.groups.get(id)
    }

    pub fn len(&self) -> usize {
        self.groups.len()
    }

    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    /// 按 id 升序的确定性 (id, group) 列表。
    pub fn sorted_entries(&self) -> Vec<(&String, &GroupLayout)> {
        let mut entries: Vec<(&String, &GroupLayout)> = self.groups.iter().collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));
        entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nodes() -> HashMap<String, NodeLayout> {
        let mut m = HashMap::new();
        m.insert("b".into(), NodeLayout { x: 10.0, y: 0.0, width: 4.0, height: 3.0 });
        m.insert("a".into(), NodeLayout { x: 0.0, y: 0.0, width: 4.0, height: 3.0 });
        m
    }

    #[test]
    fn sorted_entries_are_id_ascending() {
        let n = nodes();
        let store = FrozenNodeStore::new(&n);
        let ids: Vec<&str> = store.sorted_entries().iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b"]);
    }

    #[test]
    fn read_only_access() {
        let n = nodes();
        let store = FrozenNodeStore::new(&n);
        assert_eq!(store.len(), 2);
        assert!(store.get("a").is_some());
        assert!(store.get("missing").is_none());
    }
}
