//! 图结构特征分析模块
//!
//! 从 `Diagram` 中提取拓扑特征，用于：
//! - 按结构特征分类评估结果
//! - 发现算法在特定拓扑下的表现模式
//! - 按规模分桶进行分组评估

use drawify_core::ast::Diagram;
use std::collections::{HashMap, HashSet};

/// 图结构特征描述
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GraphProfile {
    /// 节点数量
    pub node_count: usize,
    /// 边数量
    pub edge_count: usize,
    /// 图密度（实际边数 / 最大可能边数）
    pub density: f64,
    /// 最长路径深度
    pub max_depth: usize,
    /// 最大出度
    pub max_fan_out: usize,
    /// 最大入度
    pub max_fan_in: usize,
    /// 是否存在环
    pub has_cycles: bool,
    /// 平均分支因子
    pub avg_branching: f64,
    /// 规模分桶
    pub size_bucket: SizeBucket,
    /// 拓扑标签
    pub topology_tags: Vec<TopologyTag>,
}

/// 规模分桶
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum SizeBucket {
    /// <= 5 节点
    Tiny,
    /// 6-15 节点
    Small,
    /// 16-40 节点
    Medium,
    /// 41-100 节点
    Large,
    /// > 100 节点
    Huge,
}

impl SizeBucket {
    pub fn from_node_count(count: usize) -> Self {
        match count {
            0..=5 => SizeBucket::Tiny,
            6..=15 => SizeBucket::Small,
            16..=40 => SizeBucket::Medium,
            41..=100 => SizeBucket::Large,
            _ => SizeBucket::Huge,
        }
    }
}

impl std::fmt::Display for SizeBucket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SizeBucket::Tiny => write!(f, "微型(≤5)"),
            SizeBucket::Small => write!(f, "小型(6-15)"),
            SizeBucket::Medium => write!(f, "中型(16-40)"),
            SizeBucket::Large => write!(f, "大型(41-100)"),
            SizeBucket::Huge => write!(f, "巨型(>100)"),
        }
    }
}

/// 拓扑标签
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum TopologyTag {
    /// 长链（深度 > 5，最大出度 <= 2）
    Chain,
    /// 宽扇出（最大出度 >= 4）
    WideFanOut,
    /// 稠密图（密度 > 0.3）
    Dense,
    /// 稀疏图（密度 < 0.1 且节点 > 3）
    Sparse,
    /// 有环
    Cyclic,
    /// 树形（无环，单一根节点）
    Tree,
    /// 有枢纽节点（入度+出度 >= 5）
    Hub,
}

impl std::fmt::Display for TopologyTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TopologyTag::Chain => write!(f, "长链"),
            TopologyTag::WideFanOut => write!(f, "宽扇出"),
            TopologyTag::Dense => write!(f, "稠密"),
            TopologyTag::Sparse => write!(f, "稀疏"),
            TopologyTag::Cyclic => write!(f, "有环"),
            TopologyTag::Tree => write!(f, "树形"),
            TopologyTag::Hub => write!(f, "枢纽"),
        }
    }
}

impl GraphProfile {
    /// 从 Diagram 分析图结构特征
    pub fn analyze(diagram: &Diagram) -> Self {
        let node_count = diagram.entities.len();
        let edge_count = diagram.relations.len();

        // 构建邻接表
        let mut adj_out: HashMap<&str, Vec<&str>> = HashMap::new();
        let mut adj_in: HashMap<&str, Vec<&str>> = HashMap::new();
        let mut node_set: HashSet<&str> = HashSet::new();

        for entity in &diagram.entities {
            let id = entity.id.as_str();
            node_set.insert(id);
            adj_out.entry(id).or_default();
            adj_in.entry(id).or_default();
        }

        for rel in &diagram.relations {
            let from = rel.from.as_str();
            let to = rel.to.as_str();
            adj_out.entry(from).or_default().push(to);
            adj_in.entry(to).or_default().push(from);
        }

        // 计算密度
        let max_edges = if node_count > 1 {
            node_count * (node_count - 1)
        } else {
            1
        };
        let density = edge_count as f64 / max_edges as f64;

        // 计算最大入度/出度
        let max_fan_out = adj_out.values().map(|v| v.len()).max().unwrap_or(0);
        let max_fan_in = adj_in.values().map(|v| v.len()).max().unwrap_or(0);

        // 平均分支因子
        let avg_branching = if node_count > 0 {
            edge_count as f64 / node_count as f64
        } else {
            0.0
        };

        // 检测环
        let has_cycles = detect_cycles(&node_set, &adj_out);

        // 计算最长路径深度
        let max_depth = if has_cycles {
            compute_max_depth_approx(&node_set, &adj_out)
        } else {
            compute_max_depth_dag(&node_set, &adj_out)
        };

        // 规模分桶
        let size_bucket = SizeBucket::from_node_count(node_count);

        // 拓扑标签
        let mut topology_tags = Vec::new();
        if max_depth > 5 && max_fan_out <= 2 {
            topology_tags.push(TopologyTag::Chain);
        }
        if max_fan_out >= 4 {
            topology_tags.push(TopologyTag::WideFanOut);
        }
        if density > 0.3 {
            topology_tags.push(TopologyTag::Dense);
        } else if density < 0.1 && node_count > 3 {
            topology_tags.push(TopologyTag::Sparse);
        }
        if has_cycles {
            topology_tags.push(TopologyTag::Cyclic);
        }
        if !has_cycles {
            let roots_count = node_set
                .iter()
                .filter(|n| adj_in.get(*n).map_or(true, |v| v.is_empty()))
                .count();
            if roots_count == 1 {
                topology_tags.push(TopologyTag::Tree);
            }
        }
        // 枢纽节点
        let has_hub = node_set.iter().any(|id| {
            let total = adj_out.get(id).map_or(0, |v| v.len()) + adj_in.get(id).map_or(0, |v| v.len());
            total >= 5
        });
        if has_hub {
            topology_tags.push(TopologyTag::Hub);
        }

        Self {
            node_count,
            edge_count,
            density,
            max_depth,
            max_fan_out,
            max_fan_in,
            has_cycles,
            avg_branching,
            size_bucket,
            topology_tags,
        }
    }

    /// 拓扑标签的简短描述
    pub fn topology_summary(&self) -> String {
        if self.topology_tags.is_empty() {
            "通用".to_string()
        } else {
            self.topology_tags
                .iter()
                .map(|t| t.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        }
    }
}

/// DFS 检测环
fn detect_cycles<'a>(nodes: &HashSet<&'a str>, adj: &HashMap<&'a str, Vec<&'a str>>) -> bool {
    let mut visited: HashSet<&str> = HashSet::new();
    let mut in_stack: HashSet<&str> = HashSet::new();

    for &node in nodes {
        if !visited.contains(node) && dfs_cycle(node, adj, &mut visited, &mut in_stack) {
            return true;
        }
    }
    false
}

fn dfs_cycle<'a>(
    node: &'a str,
    adj: &HashMap<&'a str, Vec<&'a str>>,
    visited: &mut HashSet<&'a str>,
    in_stack: &mut HashSet<&'a str>,
) -> bool {
    visited.insert(node);
    in_stack.insert(node);

    if let Some(neighbors) = adj.get(node) {
        for &next in neighbors {
            if !visited.contains(next) {
                if dfs_cycle(next, adj, visited, in_stack) {
                    return true;
                }
            } else if in_stack.contains(next) {
                return true;
            }
        }
    }

    in_stack.remove(node);
    false
}

/// DAG 最长路径（拓扑排序 + DP）
fn compute_max_depth_dag<'a>(
    nodes: &HashSet<&'a str>,
    adj: &HashMap<&'a str, Vec<&'a str>>,
) -> usize {
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    for &node in nodes {
        in_degree.entry(node).or_insert(0);
    }
    for neighbors in adj.values() {
        for &neighbor in neighbors {
            *in_degree.entry(neighbor).or_insert(0) += 1;
        }
    }

    let mut queue: Vec<&str> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(&node, _)| node)
        .collect();

    let mut depth: HashMap<&str, usize> = HashMap::new();
    for &node in &queue {
        depth.insert(node, 0);
    }

    let mut max_depth = 0;
    let mut idx = 0;
    while idx < queue.len() {
        let node = queue[idx];
        idx += 1;

        if let Some(neighbors) = adj.get(node) {
            for &next in neighbors {
                let new_depth = depth.get(node).copied().unwrap_or(0) + 1;
                let current = depth.entry(next).or_insert(0);
                *current = (*current).max(new_depth);
                max_depth = max_depth.max(*current);

                let deg = in_degree.get_mut(next).unwrap();
                *deg -= 1;
                if *deg == 0 {
                    queue.push(next);
                }
            }
        }
    }

    max_depth
}

/// 有环图的最长路径近似（限制 DFS 深度避免无限循环）
fn compute_max_depth_approx<'a>(
    nodes: &HashSet<&'a str>,
    adj: &HashMap<&'a str, Vec<&'a str>>,
) -> usize {
    let mut max_depth = 0;

    for &node in nodes {
        let mut visited: HashSet<&str> = HashSet::new();
        let depth = dfs_depth(node, adj, &mut visited, 0, 100);
        max_depth = max_depth.max(depth);
    }

    max_depth
}

fn dfs_depth<'a>(
    node: &'a str,
    adj: &HashMap<&'a str, Vec<&'a str>>,
    visited: &mut HashSet<&'a str>,
    current_depth: usize,
    max_limit: usize,
) -> usize {
    if current_depth >= max_limit || visited.contains(node) {
        return current_depth;
    }

    visited.insert(node);
    let mut max_child_depth = current_depth;

    if let Some(neighbors) = adj.get(node) {
        for &next in neighbors {
            let d = dfs_depth(next, adj, visited, current_depth + 1, max_limit);
            max_child_depth = max_child_depth.max(d);
        }
    }

    visited.remove(node);
    max_child_depth
}

#[cfg(test)]
mod tests {
    use super::*;
    use drawify_core::ast::*;
    use drawify_core::types::DiagramType;

    fn make_diagram(node_count: usize, edges: &[(&str, &str)]) -> Diagram {
        let entities: Vec<Entity> = (0..node_count)
            .map(|i| Entity {
                id: Identifier::new_unchecked(&format!("n{}", i)),
                label: format!("N{}", i),
                attributes: AttributeMap::default(),
                group_id: None,
                span: Span::dummy(),
            })
            .collect();

        let relations: Vec<Relation> = edges
            .iter()
            .map(|(from, to)| Relation {
                from: Identifier::new_unchecked(from),
                to: Identifier::new_unchecked(to),
                arrow: ArrowType::Active,
                label: None,
                head_label: None,
                tail_label: None,
                attributes: AttributeMap::default(),
                span: Span::dummy(),
            })
            .collect();

        Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: vec![],
            entities,
            relations,
            groups: vec![],
            style_decls: vec![],
            doc_comment: None,
            source_info: SourceInfo {
                file: None,
                line_count: 1,
            },
        }
    }

    #[test]
    fn test_simple_chain() {
        let diag = make_diagram(4, &[("n0", "n1"), ("n1", "n2"), ("n2", "n3")]);
        let profile = GraphProfile::analyze(&diag);
        assert_eq!(profile.node_count, 4);
        assert_eq!(profile.edge_count, 3);
        assert!(!profile.has_cycles);
        assert_eq!(profile.max_depth, 3);
        // Chain tag requires depth > 5, so not tagged for short chains
        assert!(!profile.topology_tags.contains(&TopologyTag::Chain));
        // But it is a tree (single root)
        assert!(profile.topology_tags.contains(&TopologyTag::Tree));
    }

    #[test]
    fn test_cycle_detection() {
        let diag = make_diagram(3, &[("n0", "n1"), ("n1", "n2"), ("n2", "n0")]);
        let profile = GraphProfile::analyze(&diag);
        assert!(profile.has_cycles);
        assert!(profile.topology_tags.contains(&TopologyTag::Cyclic));
    }

    #[test]
    fn test_size_bucket() {
        let diag = make_diagram(3, &[]);
        let profile = GraphProfile::analyze(&diag);
        assert_eq!(profile.size_bucket, SizeBucket::Tiny);

        let diag = make_diagram(20, &[]);
        let profile = GraphProfile::analyze(&diag);
        assert_eq!(profile.size_bucket, SizeBucket::Medium);
    }

    #[test]
    fn test_wide_fan_out() {
        let diag = make_diagram(
            5,
            &[("n0", "n1"), ("n0", "n2"), ("n0", "n3"), ("n0", "n4")],
        );
        let profile = GraphProfile::analyze(&diag);
        assert_eq!(profile.max_fan_out, 4);
        assert!(profile.topology_tags.contains(&TopologyTag::WideFanOut));
    }

    #[test]
    fn test_hub_node() {
        let diag = make_diagram(
            6,
            &[
                ("n0", "n1"),
                ("n2", "n1"),
                ("n3", "n1"),
                ("n1", "n4"),
                ("n1", "n5"),
            ],
        );
        let profile = GraphProfile::analyze(&diag);
        assert!(profile.topology_tags.contains(&TopologyTag::Hub));
    }
}
