//! 贪心反馈边集（Greedy Feedback Arc Set）
//!
//! 统一的去环算法，供 `sugiyama_v2` 和 `architecture_v2` 共享。
//!
//! 算法步骤：
//! 1. 迭代剥离 sink（出度 0）到右端、source（入度 0）到左端
//! 2. 剩余环中按 `out_deg - in_deg` 启发式选节点剥到左端
//! 3. 拼接 left + right 得到拓扑序
//! 4. 所有 `from_pos > to_pos` 的边标记为需反转
//!
//! 返回需要**反向**的边集合，调用方据此得到 DAG。

use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::Hash;

/// 贪心 FAS 去环，返回需要反转的边集合。
///
/// # 泛型参数
/// - `N`: 节点 ID 类型，需可克隆、可排序、可哈希
///
/// # 参数
/// - `nodes`: 所有节点 ID 列表（决定迭代顺序）
/// - `out_neighbors`: 前向邻接表 `from -> [to, ...]`
/// - `in_neighbors`: 后向邻接表 `to -> [from, ...]`
///
/// # 返回
/// 需要反转的边集合 `HashSet<(N, N)>`，每条边为 `(from, to)`
pub fn greedy_fas<N>(
    nodes: &[N],
    out_neighbors: &HashMap<N, Vec<N>>,
    in_neighbors: &HashMap<N, Vec<N>>,
) -> HashSet<(N, N)>
where
    N: Clone + Ord + Hash,
{
    let mut remaining: HashSet<N> = nodes.iter().cloned().collect();
    let mut out_deg: HashMap<N, usize> = HashMap::new();
    let mut in_deg: HashMap<N, usize> = HashMap::new();

    for node in nodes {
        let outs = out_neighbors.get(node).map(|v| v.len()).unwrap_or(0);
        let ins = in_neighbors.get(node).map(|v| v.len()).unwrap_or(0);
        out_deg.insert(node.clone(), outs);
        in_deg.insert(node.clone(), ins);
    }

    let mut left: VecDeque<N> = VecDeque::new();
    let mut right: VecDeque<N> = VecDeque::new();

    while !remaining.is_empty() {
        // 1) 剥离 sinks（出度 0）→ 右端
        let mut sinks: Vec<N> = remaining
            .iter()
            .filter(|n| out_deg.get(*n).copied().unwrap_or(0) == 0)
            .cloned()
            .collect();
        sinks.sort();
        if !sinks.is_empty() {
            for sink in sinks {
                remove_node(&sink, &mut remaining, &mut out_deg, &mut in_deg, out_neighbors, in_neighbors);
                right.push_front(sink);
            }
            continue;
        }

        // 2) 剥离 sources（入度 0）→ 左端
        let mut sources: Vec<N> = remaining
            .iter()
            .filter(|n| in_deg.get(*n).copied().unwrap_or(0) == 0)
            .cloned()
            .collect();
        sources.sort();
        if !sources.is_empty() {
            for source in sources {
                remove_node(&source, &mut remaining, &mut out_deg, &mut in_deg, out_neighbors, in_neighbors);
                left.push_back(source);
            }
            continue;
        }

        // 3) 剩余必含环；按 (out_deg - in_deg) 最大启发式选节点
        let selected = remaining
            .iter()
            .max_by(|a, b| {
                let a_score = out_deg.get(*a).copied().unwrap_or(0) as isize
                    - in_deg.get(*a).copied().unwrap_or(0) as isize;
                let b_score = out_deg.get(*b).copied().unwrap_or(0) as isize
                    - in_deg.get(*b).copied().unwrap_or(0) as isize;
                a_score.cmp(&b_score).then_with(|| b.cmp(a))
            })
            .cloned()
            .unwrap();
        remove_node(&selected, &mut remaining, &mut out_deg, &mut in_deg, out_neighbors, in_neighbors);
        left.push_back(selected);
    }

    // 拼接拓扑序
    let order: Vec<N> = left.into_iter().chain(right).collect();
    let position: HashMap<N, usize> = order
        .iter()
        .enumerate()
        .map(|(i, n)| (n.clone(), i))
        .collect();

    // 标记所有后向边（from 在拓扑序中晚于 to），以及自环（from == to）
    let mut reversed = HashSet::new();
    for node in nodes {
        if let Some(successors) = out_neighbors.get(node) {
            for succ in successors {
                // 自环始终需反转（长度为 1 的环）
                if node == succ {
                    reversed.insert((node.clone(), succ.clone()));
                    continue;
                }
                let pf = position.get(node).copied().unwrap_or(0);
                let pt = position.get(succ).copied().unwrap_or(0);
                if pf > pt {
                    reversed.insert((node.clone(), succ.clone()));
                }
            }
        }
    }
    reversed
}

/// 从 remaining 中移除节点，并更新邻居的度数。
fn remove_node<N>(
    node: &N,
    remaining: &mut HashSet<N>,
    out_deg: &mut HashMap<N, usize>,
    in_deg: &mut HashMap<N, usize>,
    out_neighbors: &HashMap<N, Vec<N>>,
    in_neighbors: &HashMap<N, Vec<N>>,
) where
    N: Clone + Ord + Hash,
{
    if !remaining.remove(node) {
        return;
    }
    if let Some(neighbors) = out_neighbors.get(node) {
        for n in neighbors {
            if remaining.contains(n) {
                if let Some(d) = in_deg.get_mut(n) {
                    *d = d.saturating_sub(1);
                }
            }
        }
    }
    if let Some(neighbors) = in_neighbors.get(node) {
        for n in neighbors {
            if remaining.contains(n) {
                if let Some(d) = out_deg.get_mut(n) {
                    *d = d.saturating_sub(1);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_adjacency(
        edges: &[(&str, &str)],
    ) -> (Vec<String>, HashMap<String, Vec<String>>, HashMap<String, Vec<String>>) {
        let mut nodes = Vec::new();
        let mut out_n: HashMap<String, Vec<String>> = HashMap::new();
        let mut in_n: HashMap<String, Vec<String>> = HashMap::new();
        let mut seen = HashSet::new();
        for (a, b) in edges {
            if seen.insert(a.to_string()) {
                nodes.push(a.to_string());
            }
            if seen.insert(b.to_string()) {
                nodes.push(b.to_string());
            }
            out_n.entry(a.to_string()).or_default().push(b.to_string());
            in_n.entry(b.to_string()).or_default().push(a.to_string());
        }
        for n in &nodes {
            out_n.entry(n.clone()).or_default();
            in_n.entry(n.clone()).or_default();
        }
        nodes.sort();
        (nodes, out_n, in_n)
    }

    #[test]
    fn test_dag_no_reversal() {
        // A -> B -> C，无环，无需反转
        let (nodes, out_n, in_n) = build_adjacency(&[("a", "b"), ("b", "c")]);
        let reversed = greedy_fas(&nodes, &out_n, &in_n);
        assert!(reversed.is_empty(), "DAG should have no reversed edges");
    }

    #[test]
    fn test_simple_cycle() {
        // A -> B -> A，两节点环，需反转一条边
        let (nodes, out_n, in_n) = build_adjacency(&[("a", "b"), ("b", "a")]);
        let reversed = greedy_fas(&nodes, &out_n, &in_n);
        assert_eq!(reversed.len(), 1, "simple cycle should reverse exactly 1 edge");
    }

    #[test]
    fn test_self_loop() {
        // A -> A，自环
        let (nodes, out_n, in_n) = build_adjacency(&[("a", "a")]);
        let reversed = greedy_fas(&nodes, &out_n, &in_n);
        assert_eq!(reversed.len(), 1, "self-loop should be reversed");
    }

    #[test]
    fn test_complex_cycle() {
        // A -> B -> C -> A + A -> C
        let (nodes, out_n, in_n) = build_adjacency(&[("a", "b"), ("b", "c"), ("c", "a"), ("a", "c")]);
        let reversed = greedy_fas(&nodes, &out_n, &in_n);
        // 反转后应为 DAG
        assert!(!reversed.is_empty(), "cycle should require at least 1 reversal");
    }
}
