//! Detect Channel-path suffix bundles (`bus_routing`).
//!
//! Shared track-id suffix = confluence fact (Atlas shape). Purely combinatorial;
//! no coordinates.

use std::collections::BTreeMap;

use super::substrate::TrackId;
use crate::layout::hierarchical::compose::bundle::{BundleKind, BundlePlan};

struct TrieNode {
    children: BTreeMap<TrackId, TrieNode>,
    edges: Vec<String>,
}

impl TrieNode {
    fn new() -> Self {
        Self {
            children: BTreeMap::new(),
            edges: Vec::new(),
        }
    }
}

/// Collect SharedCorridor bundles for paths with a shared suffix of length
/// ≥ `min_suffix`.
pub fn detect_corridor_bundles(
    paths: &[(String, &[TrackId])],
    min_suffix: usize,
) -> Vec<BundlePlan> {
    let mut root = TrieNode::new();
    let mut sorted: Vec<&(String, &[TrackId])> = paths.iter().collect();
    sorted.sort_by_key(|(e, _)| e.clone());
    for (edge, tracks) in sorted {
        let mut node = &mut root;
        for &t in tracks.iter().rev() {
            node = node.children.entry(t).or_insert_with(TrieNode::new);
        }
        node.edges.push(edge.clone());
    }

    let mut out = Vec::new();
    let mut suffix_rev: Vec<TrackId> = Vec::new();
    walk(&root, 0, min_suffix, &mut suffix_rev, &mut out);
    out.sort_by(|a, b| {
        b.shared_track_ids
            .len()
            .cmp(&a.shared_track_ids.len())
            .then_with(|| a.shared_track_ids.cmp(&b.shared_track_ids))
            .then_with(|| a.member_edges.cmp(&b.member_edges))
    });
    // Keep only maximal suffixes: drop a bundle whose members+tracks are
    // strictly covered by a longer one (avoid nested duplicate exemptions).
    let mut filtered = Vec::new();
    for b in &out {
        let dominated = filtered.iter().any(|kept: &BundlePlan| {
            kept.shared_track_ids.len() > b.shared_track_ids.len()
                && b.member_edges
                    .iter()
                    .all(|e| kept.member_edges.iter().any(|k| k == e))
                && b.shared_track_ids
                    .iter()
                    .rev()
                    .zip(kept.shared_track_ids.iter().rev())
                    .all(|(x, y)| x == y)
        });
        if !dominated {
            filtered.push(b.clone());
        }
    }
    filtered
}

fn walk(
    node: &TrieNode,
    depth: usize,
    min_suffix: usize,
    suffix_rev: &mut Vec<TrackId>,
    out: &mut Vec<BundlePlan>,
) {
    let subtree = collect_subtree_edges(node);
    if subtree.len() >= 2 && depth >= min_suffix {
        let mut suffix = suffix_rev.clone();
        suffix.reverse();
        let id = format!(
            "corridor:{}:{}",
            suffix
                .iter()
                .map(|t| t.0.to_string())
                .collect::<Vec<_>>()
                .join("-"),
            subtree.join("+")
        );
        out.push(BundlePlan {
            id,
            kind: BundleKind::SharedCorridor,
            member_edges: subtree,
            shared_track_ids: suffix.iter().map(|t| t.0).collect(),
        });
    }
    for (&track, child) in &node.children {
        suffix_rev.push(track);
        walk(child, depth + 1, min_suffix, suffix_rev, out);
        suffix_rev.pop();
    }
}

fn collect_subtree_edges(node: &TrieNode) -> Vec<String> {
    let mut edges = node.edges.clone();
    for child in node.children.values() {
        edges.extend(collect_subtree_edges(child));
    }
    edges.sort();
    edges.dedup();
    edges
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::substrate::TrackId;

    #[test]
    fn shared_suffix_forms_corridor_bundle() {
        let t0 = TrackId(0);
        let t1 = TrackId(1);
        let t2 = TrackId(2);
        let a = vec![t0, t1, t2];
        let b = vec![TrackId(9), t1, t2];
        let paths = [("e0".into(), a.as_slice()), ("e1".into(), b.as_slice())];
        let bundles = detect_corridor_bundles(&paths, 2);
        assert_eq!(bundles.len(), 1);
        assert_eq!(bundles[0].shared_track_ids, vec![1, 2]);
        assert_eq!(
            bundles[0].member_edges,
            vec!["e0".to_string(), "e1".to_string()]
        );
    }
}
