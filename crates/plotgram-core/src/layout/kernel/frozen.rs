//! 冻结节点布局：solver 完成后不可修改。
//!
//! 类型层面保证路由/refine 不会意外移动节点。
//! 使用 fingerprint 提供运行时守卫（debug_assert 级别）。

use crate::layout::types::NodeLayout;
use serde::Serialize;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// 冻结的节点布局：solver 完成后不可修改。
///
/// 类型层面保证路由/refine 不会意外移动节点。
/// - 只提供 `&NodeLayout` 只读访问，无 `&mut` 方法
/// - 创建时计算 fingerprint，用于运行时守卫
#[derive(Debug, Clone, Serialize)]
pub struct FrozenNodeLayout {
    inner: NodeLayout,
    fingerprint: u64,
}

impl FrozenNodeLayout {
    /// 冻结一个节点布局，计算 fingerprint。
    pub fn freeze(node: NodeLayout) -> Self {
        let fingerprint = Self::compute_fingerprint(&node);
        Self {
            inner: node,
            fingerprint,
        }
    }

    /// 只读访问内部节点布局。
    pub fn get(&self) -> &NodeLayout {
        &self.inner
    }

    /// 获取创建时的 fingerprint。
    pub fn fingerprint(&self) -> u64 {
        self.fingerprint
    }

    /// 验证当前状态是否与冻结时一致（运行时守卫）。
    ///
    /// 在 debug 构建中，路由/refine 完成后可调用此方法
    /// 确认节点未被意外修改。
    pub fn verify_integrity(&self) -> bool {
        Self::compute_fingerprint(&self.inner) == self.fingerprint
    }

    fn compute_fingerprint(node: &NodeLayout) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        let mut hasher = DefaultHasher::new();
        // 使用 to_bits 确保 f64 的确定性哈希
        node.x.to_bits().hash(&mut hasher);
        node.y.to_bits().hash(&mut hasher);
        node.width.to_bits().hash(&mut hasher);
        node.height.to_bits().hash(&mut hasher);
        hasher.finish()
    }
}

/// 冻结所有节点布局。
///
/// 在 solver 完成后调用，生成不可变的节点快照。
pub fn freeze_nodes(nodes: &HashMap<String, NodeLayout>) -> HashMap<String, FrozenNodeLayout> {
    nodes
        .iter()
        .map(|(id, node)| (id.clone(), FrozenNodeLayout::freeze(node.clone())))
        .collect()
}

/// 验证冻结快照与当前节点布局一致。
///
/// 返回被修改的节点 ID 列表（空表示全部一致）。
pub fn verify_frozen_integrity(
    frozen: &HashMap<String, FrozenNodeLayout>,
    current: &HashMap<String, NodeLayout>,
) -> Vec<String> {
    let mut modified = Vec::new();
    for (id, frozen_node) in frozen {
        if let Some(current_node) = current.get(id) {
            let expected_fp = frozen_node.fingerprint();
            let actual_fp = FrozenNodeLayout::compute_fingerprint(current_node);
            if expected_fp != actual_fp {
                modified.push(id.clone());
            }
        } else {
            // 节点被删除
            modified.push(id.clone());
        }
    }
    modified.sort();
    modified
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_freeze_and_verify() {
        let node = NodeLayout {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 50.0,
        };
        let frozen = FrozenNodeLayout::freeze(node.clone());
        assert!(frozen.verify_integrity());
        assert_eq!(frozen.get().x, 10.0);
        assert_eq!(frozen.get().y, 20.0);
    }

    #[test]
    fn test_verify_frozen_integrity_detects_modification() {
        let mut nodes = HashMap::new();
        nodes.insert(
            "a".to_string(),
            NodeLayout { x: 0.0, y: 0.0, width: 40.0, height: 30.0 },
        );
        nodes.insert(
            "b".to_string(),
            NodeLayout { x: 100.0, y: 0.0, width: 40.0, height: 30.0 },
        );

        let frozen = freeze_nodes(&nodes);

        // 未修改：一致
        assert!(verify_frozen_integrity(&frozen, &nodes).is_empty());

        // 修改节点 a
        let mut modified_nodes = nodes.clone();
        modified_nodes.get_mut("a").unwrap().x = 999.0;
        let diff = verify_frozen_integrity(&frozen, &modified_nodes);
        assert_eq!(diff, vec!["a".to_string()]);
    }

    #[test]
    fn test_fingerprint_deterministic() {
        let node = NodeLayout { x: 1.5, y: 2.5, width: 3.5, height: 4.5 };
        let f1 = FrozenNodeLayout::freeze(node.clone());
        let f2 = FrozenNodeLayout::freeze(node);
        assert_eq!(f1.fingerprint(), f2.fingerprint());
    }
}
