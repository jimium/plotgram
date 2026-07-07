//! Step 3: 边聚类（确定性连通分量聚类）
//!
//! 详见 `docs/architecture/布局优化/edge-bundling-research.md` §4.5。
//!
//! ## 算法
//!
//! 1. 将所有边作为图节点
//! 2. 兼容边对之间连边（权重 = compatibility 分数）
//! 3. 按权重从高到低处理边对，用并查集（Union-Find）合并连通分量
//! 4. 聚类大小 ≥ 2 且 ≤ `max_bundle_size` 时形成 bundle candidate
//! 5. 超过 `max_bundle_size` 的 bundle 按目标区域子分
//!
//! ## 确定性保证
//!
//! - 并查集合并按权重降序、权重相同按最小 edge_index 升序
//! - 所有排序使用显式全序 key（含 tiebreaker）
//! - 不依赖 HashMap 迭代顺序

use std::collections::BTreeMap;

use super::compatibility::{compute_compatibility, CompatibilityBucket, EdgeFeatures};
use super::types::{BundlingConfig, EdgeBundlingDebugStats};

/// 桶内全量两两比对的边数上限；更大桶改用 from_id / to_id 子分组。
const FULL_PAIRWISE_THRESHOLD: usize = 16;

/// 聚类候选：一组可捆绑的边索引。
#[derive(Debug, Clone)]
pub struct BundleCandidate {
    /// 包含的边索引列表（已排序，升序）
    pub edges: Vec<usize>,
}

/// 并查集（Union-Find）数据结构。
///
/// 路径压缩 + 按秩合并，保证近 O(α(n)) 复杂度。
struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            rank: vec![0; n],
        }
    }

    fn find(&mut self, x: usize) -> usize {
        let mut root = x;
        while self.parent[root] != root {
            root = self.parent[root];
        }
        // 路径压缩
        let mut cur = x;
        while self.parent[cur] != root {
            let next = self.parent[cur];
            self.parent[cur] = root;
            cur = next;
        }
        root
    }

    fn union(&mut self, x: usize, y: usize) {
        let rx = self.find(x);
        let ry = self.find(y);
        if rx == ry {
            return;
        }
        // 按秩合并
        match self.rank[rx].cmp(&self.rank[ry]) {
            std::cmp::Ordering::Less => self.parent[rx] = ry,
            std::cmp::Ordering::Greater => self.parent[ry] = rx,
            std::cmp::Ordering::Equal => {
                self.parent[ry] = rx;
                self.rank[rx] += 1;
            }
        }
    }
}

/// 兼容边对（含分数和确定性排序键）。
struct CompatiblePair {
    e1: usize,
    e2: usize,
    score: f64,
}

/// 对所有边进行聚类，返回 bundle candidate 列表。
///
/// 聚类流程：
/// 1. 按 `CompatibilityBucket` 分桶，仅同桶内比对（O(E·k) 加速）
/// 2. 计算所有兼容边对（compatibility ≥ threshold）
/// 3. 按分数降序、最小 edge_index 升序排序
/// 4. Union-Find 合并
/// 5. 提取连通分量，过滤 size ≥ 2
/// 6. 超过 `max_bundle_size` 的 bundle 按目标区域子分
pub fn cluster_edges(
    features: &[EdgeFeatures],
    config: &BundlingConfig,
    stats: &mut EdgeBundlingDebugStats,
) -> Vec<BundleCandidate> {
    let n = features.len();
    if n < 2 {
        return Vec::new();
    }

    // ── 0. 过滤自环和短边（§4.9.5）──
    // 自环边（from == to）和超短边（路径长度 < 3 × fork_distance）不参与 bundling
    let min_path_length = 3.0 * config.fork_distance;
    let eligible: Vec<usize> = (0..n)
        .filter(|&i| {
            let feat = &features[i];
            feat.from_id != feat.to_id && feat.path_length >= min_path_length
        })
        .collect();
    if eligible.len() < 2 {
        return Vec::new();
    }

    // ── 1. 分桶：按 CompatibilityBucket 分组 ──
    // 使用 BTreeMap 保证确定性迭代顺序
    let mut buckets: BTreeMap<CompatibilityBucket, Vec<usize>> = BTreeMap::new();
    for &i in &eligible {
        let bucket = CompatibilityBucket::from_features(&features[i]);
        buckets.entry(bucket).or_default().push(i);
    }

    // ── 2. 计算兼容边对 ──
    let mut pairs: Vec<CompatiblePair> = Vec::new();
    let mut seen_pairs: std::collections::BTreeSet<(usize, usize)> = std::collections::BTreeSet::new();
    for (_, indices) in &buckets {
        if indices.len() < 2 {
            continue;
        }
        if indices.len() <= FULL_PAIRWISE_THRESHOLD {
            collect_compatible_pairs(
                indices,
                features,
                config,
                stats,
                &mut pairs,
                &mut seen_pairs,
            );
            continue;
        }

        // 大桶：按 from_id / to_id 子分组，避免 hub 场景 O(k²) 爆炸。
        // 同一 from 的扇出边、同一 to 的扇入边分别比对；跨节点平行边仍由小子桶覆盖。
        let mut by_from: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
        let mut by_to: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
        for &i in indices {
            let feat = &features[i];
            by_from.entry(feat.from_id.as_str()).or_default().push(i);
            by_to.entry(feat.to_id.as_str()).or_default().push(i);
        }
        for group in by_from.values().chain(by_to.values()) {
            if group.len() >= 2 {
                collect_compatible_pairs(
                    group,
                    features,
                    config,
                    stats,
                    &mut pairs,
                    &mut seen_pairs,
                );
            }
        }
    }

    // ── 3. 按分数降序、最小 edge_index 升序排序（确定性）──
    pairs.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.e1.min(a.e2).cmp(&b.e1.min(b.e2)))
            .then_with(|| a.e1.max(a.e2).cmp(&b.e1.max(b.e2)))
    });

    // ── 4. Union-Find 合并 ──
    let mut uf = UnionFind::new(n);
    for pair in &pairs {
        uf.union(pair.e1, pair.e2);
    }

    // ── 5. 提取连通分量 ──
    // 使用 BTreeMap 按 root 索引排序，保证确定性
    let mut components: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..n {
        let root = uf.find(i);
        components.entry(root).or_default().push(i);
    }

    // ── 6. 过滤 size ≥ 2，子分超大的 bundle ──
    let mut candidates = Vec::new();
    for (_, mut edges) in components {
        if edges.len() < 2 {
            continue;
        }
        // edges 已按 i 升序（因为 0..n 顺序插入）
        edges.sort(); // 确定性排序保证

        if edges.len() <= config.max_bundle_size {
            candidates.push(BundleCandidate { edges });
        } else {
            // 超过 max_bundle_size → 按目标区域子分
            let sub_groups = subdivide_by_target(&edges, features, config.max_bundle_size);
            for group in sub_groups {
                if group.len() >= 2 {
                    candidates.push(BundleCandidate { edges: group });
                }
            }
        }
    }

    // 按 bundle 内最小 edge_index 排序（确定性）
    candidates.sort_by(|a, b| a.edges[0].cmp(&b.edges[0]));
    candidates
}

/// 在 `indices` 内两两评估兼容性，跳过已记录的边对。
fn collect_compatible_pairs(
    indices: &[usize],
    features: &[EdgeFeatures],
    config: &BundlingConfig,
    stats: &mut EdgeBundlingDebugStats,
    pairs: &mut Vec<CompatiblePair>,
    seen_pairs: &mut std::collections::BTreeSet<(usize, usize)>,
) {
    for i in 0..indices.len() {
        for j in (i + 1)..indices.len() {
            let e1 = indices[i];
            let e2 = indices[j];
            let key = if e1 < e2 { (e1, e2) } else { (e2, e1) };
            if !seen_pairs.insert(key) {
                continue;
            }
            stats.compatibility_pairs_evaluated += 1;
            let score = compute_compatibility(&features[e1], &features[e2], config);
            if score >= config.compatibility_threshold {
                stats.compatible_pairs += 1;
                pairs.push(CompatiblePair { e1, e2, score });
            }
        }
    }
}

/// 按目标区域子分超大的 bundle。
///
/// 按 to_rank（无 rank 时按 to_center 的垂直坐标）排序后分组，
/// 每组最多 `max_size` 条边。
fn subdivide_by_target(
    edges: &[usize],
    features: &[EdgeFeatures],
    max_size: usize,
) -> Vec<Vec<usize>> {
    // 按 (to_rank, to_center.y, edge_index) 排序
    let mut sorted: Vec<usize> = edges.to_vec();
    sorted.sort_by(|&a, &b| {
        let rank_a = features[a].to_rank.unwrap_or(usize::MAX);
        let rank_b = features[b].to_rank.unwrap_or(usize::MAX);
        rank_a
            .cmp(&rank_b)
            .then_with(|| {
                features[a]
                    .to_center
                    .y
                    .partial_cmp(&features[b].to_center.y)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.cmp(&b))
    });

    // 分组，每组最多 max_size 条
    sorted
        .chunks(max_size)
        .map(|chunk| {
            let mut group: Vec<usize> = chunk.to_vec();
            group.sort(); // 恢复 edge_index 升序
            group
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        ArrowType, AttributeMap, Identifier, Relation, Span,
    };
    use crate::layout::geometry::Point;
    use crate::layout::NodeLayout;
    use std::collections::HashMap;

    fn make_relation(from: &str, to: &str, arrow: ArrowType) -> Relation {
        Relation {
            from: Identifier::new_unchecked(from),
            to: Identifier::new_unchecked(to),
            arrow,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span: Span::dummy(),
        }
    }

    fn make_node_layout(cx: f64, cy: f64) -> NodeLayout {
        NodeLayout {
            x: cx - 40.0,
            y: cy - 20.0,
            width: 80.0,
            height: 40.0,
            ..Default::default()
        }
    }

    fn make_nodes(ids: &[&str], centers: &[(f64, f64)]) -> HashMap<String, NodeLayout> {
        let mut nodes = HashMap::new();
        for (i, id) in ids.iter().enumerate() {
            nodes.insert(id.to_string(), make_node_layout(centers[i].0, centers[i].1));
        }
        nodes
    }

    fn pt(x: f64, y: f64) -> Point { Point::new(x, y) }

    fn make_features(
        edge_index: usize,
        rel: &Relation,
        nodes: &HashMap<String, NodeLayout>,
        path: &[Point],
    ) -> EdgeFeatures {
        EdgeFeatures::extract(edge_index, rel, nodes, None, path).unwrap()
    }

    fn make_features_with_ranks(
        edge_index: usize,
        rel: &Relation,
        nodes: &HashMap<String, NodeLayout>,
        ranks: &HashMap<String, usize>,
        path: &[Point],
    ) -> EdgeFeatures {
        EdgeFeatures::extract(edge_index, rel, nodes, Some(ranks), path).unwrap()
    }

    #[test]
    fn cluster_two_compatible_edges() {
        let nodes = make_nodes(&["A", "B", "C", "D"], &[(0.0, 0.0), (200.0, 0.0), (0.0, 30.0), (200.0, 30.0)]);
        let rels = vec![
            make_relation("A", "B", ArrowType::Active),
            make_relation("C", "D", ArrowType::Active),
        ];
        let paths = vec![
            vec![pt(40.0, 0.0), pt(240.0, 0.0)],
            vec![pt(40.0, 30.0), pt(240.0, 30.0)],
        ];
        let features: Vec<EdgeFeatures> = rels
            .iter()
            .enumerate()
            .map(|(i, rel)| make_features(i, rel, &nodes, &paths[i]))
            .collect();

        let config = BundlingConfig::default();
        let mut stats = EdgeBundlingDebugStats::default();
        let candidates = cluster_edges(&features, &config, &mut stats);
        assert_eq!(candidates.len(), 1, "应形成 1 个 bundle");
        assert_eq!(candidates[0].edges, vec![0, 1]);
    }

    #[test]
    fn cluster_incompatible_edges_separate() {
        let nodes = make_nodes(&["A", "B", "C", "D"], &[(0.0, 0.0), (200.0, 0.0), (0.0, 30.0), (200.0, 30.0)]);
        let rels = vec![
            make_relation("A", "B", ArrowType::Active),
            make_relation("C", "D", ArrowType::Passive),
        ];
        let paths = vec![
            vec![pt(40.0, 0.0), pt(240.0, 0.0)],
            vec![pt(40.0, 30.0), pt(240.0, 30.0)],
        ];
        let features: Vec<EdgeFeatures> = rels
            .iter()
            .enumerate()
            .map(|(i, rel)| make_features(i, rel, &nodes, &paths[i]))
            .collect();

        let config = BundlingConfig::default();
        let mut stats = EdgeBundlingDebugStats::default();
        let candidates = cluster_edges(&features, &config, &mut stats);
        assert!(candidates.is_empty(), "不兼容边不应形成 bundle");
    }

    #[test]
    fn cluster_three_edges_same_bundle() {
        let nodes = make_nodes(
            &["A", "B", "C", "D", "E", "F"],
            &[(0.0, 0.0), (200.0, 0.0), (0.0, 30.0), (200.0, 30.0), (0.0, 60.0), (200.0, 60.0)],
        );
        let rels = vec![
            make_relation("A", "B", ArrowType::Active),
            make_relation("C", "D", ArrowType::Active),
            make_relation("E", "F", ArrowType::Active),
        ];
        let paths = vec![
            vec![pt(40.0, 0.0), pt(240.0, 0.0)],
            vec![pt(40.0, 30.0), pt(240.0, 30.0)],
            vec![pt(40.0, 60.0), pt(240.0, 60.0)],
        ];
        let features: Vec<EdgeFeatures> = rels
            .iter()
            .enumerate()
            .map(|(i, rel)| make_features(i, rel, &nodes, &paths[i]))
            .collect();

        let config = BundlingConfig::default();
        let mut stats = EdgeBundlingDebugStats::default();
        let candidates = cluster_edges(&features, &config, &mut stats);
        assert_eq!(candidates.len(), 1, "3 条兼容边应形成 1 个 bundle");
        assert_eq!(candidates[0].edges.len(), 3);
    }

    #[test]
    fn cluster_subdivides_oversized_bundle() {
        let ids: Vec<String> = (0..12).map(|i| format!("n{}", i)).collect();
        let id_refs: Vec<&str> = ids.iter().map(|s| s.as_str()).collect();
        let centers: Vec<(f64, f64)> = (0..12)
            .map(|i| {
                let row = i / 2;
                let col = i % 2;
                (col as f64 * 200.0, row as f64 * 30.0)
            })
            .collect();
        let nodes = make_nodes(&id_refs, &centers);

        let rels: Vec<Relation> = (0..6)
            .map(|i| make_relation(&format!("n{}", i * 2), &format!("n{}", i * 2 + 1), ArrowType::Active))
            .collect();
        let paths: Vec<Vec<Point>> = (0..6)
            .map(|i| vec![pt(40.0, i as f64 * 30.0), pt(240.0, i as f64 * 30.0)])
            .collect();
        let features: Vec<EdgeFeatures> = rels
            .iter()
            .enumerate()
            .map(|(i, rel)| make_features(i, rel, &nodes, &paths[i]))
            .collect();

        let config = BundlingConfig {
            max_bundle_size: 4,
            ..Default::default()
        };
        let candidates = cluster_edges(&features, &config, &mut EdgeBundlingDebugStats::default());
        let total: usize = candidates.iter().map(|c| c.edges.len()).sum();
        assert_eq!(total, 6, "所有边应都被聚类");
        assert!(candidates.len() >= 2, "应子分为至少 2 组");
        for c in &candidates {
            assert!(c.edges.len() <= 4, "每组不超过 max_bundle_size");
        }
    }

    #[test]
    fn cluster_deterministic_with_ranks() {
        let nodes = make_nodes(&["A", "B", "C", "D"], &[(0.0, 0.0), (200.0, 0.0), (0.0, 30.0), (200.0, 30.0)]);
        let mut ranks = HashMap::new();
        ranks.insert("A".to_string(), 0);
        ranks.insert("B".to_string(), 1);
        ranks.insert("C".to_string(), 0);
        ranks.insert("D".to_string(), 1);
        let rels = vec![
            make_relation("A", "B", ArrowType::Active),
            make_relation("C", "D", ArrowType::Active),
        ];
        let paths = vec![
            vec![pt(40.0, 0.0), pt(240.0, 0.0)],
            vec![pt(40.0, 30.0), pt(240.0, 30.0)],
        ];
        let features: Vec<EdgeFeatures> = rels
            .iter()
            .enumerate()
            .map(|(i, rel)| make_features_with_ranks(i, rel, &nodes, &ranks, &paths[i]))
            .collect();

        let config = BundlingConfig::default();
        let candidates1 = cluster_edges(&features, &config, &mut EdgeBundlingDebugStats::default());
        let candidates2 = cluster_edges(&features, &config, &mut EdgeBundlingDebugStats::default());
        assert_eq!(candidates1.len(), candidates2.len());
        for (c1, c2) in candidates1.iter().zip(candidates2.iter()) {
            assert_eq!(c1.edges, c2.edges, "聚类结果应确定性一致");
        }
    }

    #[test]
    fn cluster_single_edge_no_bundle() {
        let nodes = make_nodes(&["A", "B"], &[(0.0, 0.0), (200.0, 0.0)]);
        let rels = vec![make_relation("A", "B", ArrowType::Active)];
        let paths = vec![vec![pt(40.0, 0.0), pt(240.0, 0.0)]];
        let features: Vec<EdgeFeatures> = rels
            .iter()
            .enumerate()
            .map(|(i, rel)| make_features(i, rel, &nodes, &paths[i]))
            .collect();

        let config = BundlingConfig::default();
        let mut stats = EdgeBundlingDebugStats::default();
        let candidates = cluster_edges(&features, &config, &mut stats);
        assert!(candidates.is_empty(), "单条边不应形成 bundle");
    }

    #[test]
    fn cluster_empty_features() {
        let features: Vec<EdgeFeatures> = Vec::new();
        let config = BundlingConfig::default();
        let mut stats = EdgeBundlingDebugStats::default();
        let candidates = cluster_edges(&features, &config, &mut stats);
        assert!(candidates.is_empty());
    }
}
