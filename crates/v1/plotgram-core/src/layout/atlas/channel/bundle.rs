//! 合流检测：共享 track 序列后缀 = bundle（22 号文 §5.1 I.6）。
//!
//! **合流是拓扑事实，不是几何巧合**：今天的 `semantic_trunk_merge` 要在几何上找共线段，
//! Atlas 里「共享 track 序列后缀」就是合流的定义——纯组合判定，不依赖坐标。
//!
//! 算法：对每条边的 track 序列建**反向后缀 trie**，trie 节点 = 一个共享后缀；
//! 节点覆盖的边数 ≥ 2 且后缀长度 ≥ `min_suffix` 即报告为一个 bundle。

use super::substrate::{EdgeId, TrackId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 一个合流束：≥2 条边共享的 track 序列后缀。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bundle {
    /// 共享此后缀的边（升序）。
    pub edges: Vec<EdgeId>,
    /// 共享的 track 后缀（正序：从分叉点到终点）。
    pub suffix: Vec<TrackId>,
}

struct TrieNode {
    children: BTreeMap<TrackId, TrieNode>,
    /// 以当前后缀结尾的边。
    edges: Vec<EdgeId>,
}

impl TrieNode {
    fn new() -> Self {
        Self {
            children: BTreeMap::new(),
            edges: Vec::new(),
        }
    }
}

/// 后缀 trie：根 = 空后缀，沿 track 序列**逆序**插入。
struct SuffixTrie {
    root: TrieNode,
}

impl SuffixTrie {
    fn new() -> Self {
        Self {
            root: TrieNode::new(),
        }
    }

    fn insert(&mut self, edge: EdgeId, tracks: &[TrackId]) {
        let mut node = &mut self.root;
        // 逆序插入：先终点 track，使 trie 深度 = 后缀长度
        for &t in tracks.iter().rev() {
            node = node.children.entry(t).or_insert_with(TrieNode::new);
        }
        node.edges.push(edge);
    }

    /// 收集全部 bundle：节点覆盖边数 ≥ 2 且深度 ≥ min_suffix。
    fn collect(&self, min_suffix: usize) -> Vec<Bundle> {
        let mut out = Vec::new();
        let mut suffix_rev: Vec<TrackId> = Vec::new();
        Self::walk(&self.root, 0, min_suffix, &mut suffix_rev, &mut out);
        // 确定性输出：按 (后缀长度降序, 后缀内容, 首条边) 排
        out.sort_by(|a, b| {
            b.suffix
                .len()
                .cmp(&a.suffix.len())
                .then_with(|| a.suffix.cmp(&b.suffix))
                .then_with(|| a.edges.cmp(&b.edges))
        });
        out
    }

    fn walk(
        node: &TrieNode,
        depth: usize,
        min_suffix: usize,
        suffix_rev: &mut Vec<TrackId>,
        out: &mut Vec<Bundle>,
    ) {
        // 子树内全部边（当前节点终止边 + 所有后代终止边）
        let subtree_edges = collect_subtree_edges(node);
        if subtree_edges.len() >= 2 && depth >= min_suffix {
            let mut suffix = suffix_rev.clone();
            suffix.reverse();
            out.push(Bundle {
                edges: subtree_edges,
                suffix,
            });
        }
        for (&track, child) in &node.children {
            suffix_rev.push(track);
            Self::walk(child, depth + 1, min_suffix, suffix_rev, out);
            suffix_rev.pop();
        }
    }
}

fn collect_subtree_edges(node: &TrieNode) -> Vec<EdgeId> {
    let mut edges = node.edges.clone();
    for child in node.children.values() {
        edges.extend(collect_subtree_edges(child));
    }
    edges.sort_unstable();
    edges.dedup();
    edges
}

/// 检测一批边路径中的合流束。
///
/// `paths`：`(edge_id, track 序列)`；`min_suffix`：构成 bundle 的最小共享后缀长度。
/// 返回全部层级共享后缀（最长后缀优先）；消费方按需取最具体的一层。
pub fn detect_bundles(paths: &[(EdgeId, &[TrackId])], min_suffix: usize) -> Vec<Bundle> {
    let mut trie = SuffixTrie::new();
    // 按 edge_id 升序插入，保证同结构输入确定性
    let mut sorted: Vec<&(EdgeId, &[TrackId])> = paths.iter().collect();
    sorted.sort_by_key(|(e, _)| *e);
    for &(edge, tracks) in sorted {
        trie.insert(edge, tracks);
    }
    trie.collect(min_suffix)
}
