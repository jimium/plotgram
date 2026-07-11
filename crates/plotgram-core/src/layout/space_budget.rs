//! 空间契约（Space Contract）：布局预留、路由消费、后处理守约。
//!
//! 同层节点缝、有 label 的水平边、端口 clearance 在布局阶段显式预算，
//! 避免末端「刚好不碰」式补丁。

use crate::ast::Diagram;
use crate::layout::constants::{DEFAULT_LABEL_PADDING, GRID_SNAP_NODE_GAP_ARCH};
use crate::layout::edge::common::label_avoidance::estimate_label_width;
use crate::layout::group::constants::PORT_STUB_CLEARANCE;
use crate::layout::NodeLayout;
use std::collections::{BTreeMap, HashMap};

/// 默认同层节点间距（与 architecture NODE_GAP / grid snap 对齐）。
pub const DEFAULT_NODE_GAP: f64 = GRID_SNAP_NODE_GAP_ARCH;

/// 有 label 的边：标签宽 + 两侧 padding 后的最小缝。
const LABEL_GAP_PAD: f64 = DEFAULT_LABEL_PADDING * 2.0 + 8.0;

/// 全图空间预算：成对最小间距 + 端口 clearance + 走廊升档请求。
#[derive(Debug, Clone, Default)]
pub struct SpaceBudget {
    /// 无特殊边时的默认同层间距。
    pub default_node_gap: f64,
    /// 规范化 pair `(min_id, max_id)` → 最小边距（节点外缘到外缘）。
    pub pair_gaps: BTreeMap<(String, String), f64>,
    /// 端口外向 stub 长度。
    pub port_clearance: f64,
    /// 路由 0 候选时请求抬高走廊/车道预算（S2）。
    pub corridor_boost_requested: bool,
}

impl SpaceBudget {
    pub fn new() -> Self {
        Self {
            default_node_gap: DEFAULT_NODE_GAP,
            pair_gaps: BTreeMap::new(),
            port_clearance: PORT_STUB_CLEARANCE,
            corridor_boost_requested: false,
        }
    }

    /// 从 diagram relations 构建：有 label 的边抬高两端点最小缝。
    pub fn from_diagram(diagram: &Diagram) -> Self {
        let mut budget = Self::new();
        let mut rels: Vec<(&str, &str, Option<&str>)> = diagram
            .relations
            .iter()
            .map(|r| {
                (
                    r.from.as_str(),
                    r.to.as_str(),
                    r.label.as_deref(),
                )
            })
            .collect();
        // 确定性：按端点 id 排序
        rels.sort_by(|a, b| a.0.cmp(b.0).then(a.1.cmp(b.1)).then(a.2.cmp(&b.2)));

        for (from, to, label) in rels {
            if from == to {
                continue;
            }
            let mut gap = budget.default_node_gap;
            if let Some(text) = label {
                if !text.is_empty() {
                    let label_w = estimate_label_width(text) + LABEL_GAP_PAD;
                    gap = gap.max(label_w);
                }
            }
            budget.set_pair_gap(from, to, gap);
        }
        budget
    }

    pub fn set_pair_gap(&mut self, a: &str, b: &str, gap: f64) {
        let key = canonical_pair(a, b);
        let entry = self.pair_gaps.entry(key).or_insert(self.default_node_gap);
        *entry = entry.max(gap);
    }

    /// 两节点外缘之间的最小间距。
    pub fn min_gap(&self, a: &str, b: &str) -> f64 {
        self.pair_gaps
            .get(&canonical_pair(a, b))
            .copied()
            .unwrap_or(self.default_node_gap)
    }

    pub fn request_corridor_boost(&mut self) {
        self.corridor_boost_requested = true;
    }

    pub fn take_corridor_boost(&mut self) -> bool {
        let v = self.corridor_boost_requested;
        self.corridor_boost_requested = false;
        v
    }
}

fn canonical_pair(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

/// 水平方向（同排）强制满足空间契约：Y 有重叠的节点对按 `min_gap` 推开。
///
/// 返回被移动的节点 id（确定性：按 id 排序迭代）。
pub fn enforce_horizontal_gaps(
    nodes: &mut HashMap<String, NodeLayout>,
    budget: &SpaceBudget,
) -> Vec<String> {
    if nodes.len() <= 1 {
        return Vec::new();
    }
    let mut ids: Vec<String> = nodes.keys().cloned().collect();
    ids.sort();
    let mut moved: BTreeMap<String, ()> = BTreeMap::new();

    for _ in 0..24 {
        let mut any = false;
        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                let (ai, bi) = (&ids[i], &ids[j]);
                let (ax, ay, aw, ah) = {
                    let n = &nodes[ai];
                    (n.x, n.y, n.width, n.height)
                };
                let (bx, by, bw, bh) = {
                    let n = &nodes[bi];
                    (n.x, n.y, n.width, n.height)
                };
                // 仅处理「同排」：Y 区间重叠超过一半较短边
                let y_overlap = (ay + ah).min(by + bh) - ay.max(by);
                let min_h = ah.min(bh);
                if y_overlap < min_h * 0.5 {
                    continue;
                }

                let required = budget.min_gap(ai, bi);
                let (left_id, right_id, left_right, right_left) = if ax <= bx {
                    (ai.as_str(), bi.as_str(), ax + aw, bx)
                } else {
                    (bi.as_str(), ai.as_str(), bx + bw, ax)
                };
                let gap = right_left - left_right;
                if gap + 0.5 >= required {
                    continue;
                }
                let deficit = required - gap;
                let half = deficit / 2.0;
                if let Some(nl) = nodes.get_mut(left_id) {
                    nl.x -= half;
                }
                if let Some(nl) = nodes.get_mut(right_id) {
                    nl.x += half;
                }
                moved.insert(left_id.to_string(), ());
                moved.insert(right_id.to_string(), ());
                any = true;
            }
        }
        if !any {
            break;
        }
    }

    moved.into_keys().collect()
}

/// 检查是否仍有水平方向违反契约的节点对。
pub fn horizontal_gap_violations(
    nodes: &HashMap<String, NodeLayout>,
    budget: &SpaceBudget,
) -> Vec<(String, String, f64, f64)> {
    let mut ids: Vec<String> = nodes.keys().cloned().collect();
    ids.sort();
    let mut out = Vec::new();
    for i in 0..ids.len() {
        for j in (i + 1)..ids.len() {
            let a = &ids[i];
            let b = &ids[j];
            let na = &nodes[a];
            let nb = &nodes[b];
            let y_overlap = (na.y + na.height).min(nb.y + nb.height) - na.y.max(nb.y);
            let min_h = na.height.min(nb.height);
            if y_overlap < min_h * 0.5 {
                continue;
            }
            let required = budget.min_gap(a, b);
            let gap = if na.x <= nb.x {
                nb.x - (na.x + na.width)
            } else {
                na.x - (nb.x + nb.width)
            };
            if gap + 0.5 < required {
                out.push((a.clone(), b.clone(), gap, required));
            }
        }
    }
    out
}

/// 契约失败时的兜底消重叠：margin 取自 SpaceBudget（无则 default_node_gap）。
pub fn resolve_residual_with_budget(
    nodes: &mut HashMap<String, NodeLayout>,
    budget: Option<&SpaceBudget>,
) {
    let margin = budget
        .map(|b| b.default_node_gap)
        .unwrap_or(DEFAULT_NODE_GAP);
    if let Some(b) = budget {
        enforce_horizontal_gaps(nodes, b);
    }
    use crate::layout::node::common::overlap::{
        BruteForceResolver, OverlapConfig, OverlapResolver,
    };
    let config = OverlapConfig {
        margin,
        max_iterations: 30,
        step_factor: 0.5,
    };
    let empty = HashMap::new();
    BruteForceResolver::new(20).resolve(nodes, &empty, &config);
    if let Some(b) = budget {
        enforce_horizontal_gaps(nodes, b);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{ArrowType, AttributeMap, Diagram, Identifier, Relation, Span};

    fn rel(from: &str, to: &str, label: Option<&str>) -> Relation {
        Relation {
            from: Identifier::new_unchecked(from),
            to: Identifier::new_unchecked(to),
            arrow: ArrowType::Active,
            label: label.map(|s| s.to_string()),
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span: Span::dummy(),
        }
    }

    #[test]
    fn labeled_edge_raises_pair_gap() {
        let diagram = Diagram {
            diagram_type: crate::types::DiagramType::Architecture,
            relations: vec![rel("db_master", "db_replica", Some("主从同步"))],
            ..Default::default()
        };
        let budget = SpaceBudget::from_diagram(&diagram);
        let gap = budget.min_gap("db_master", "db_replica");
        assert!(
            gap >= 52.0,
            "labeled pair gap should fit 主从同步, got {gap}"
        );
        assert_eq!(
            budget.min_gap("db_replica", "db_master"),
            gap,
            "pair gap must be symmetric"
        );
    }

    #[test]
    fn enforce_separates_tight_siblings() {
        let diagram = Diagram {
            diagram_type: crate::types::DiagramType::Architecture,
            relations: vec![rel("a", "b", Some("主从同步"))],
            ..Default::default()
        };
        let budget = SpaceBudget::from_diagram(&diagram);
        let mut nodes = HashMap::from([
            (
                "a".to_string(),
                NodeLayout {
                    x: 100.0,
                    y: 200.0,
                    width: 112.0,
                    height: 50.0,
                    ..Default::default()
                },
            ),
            (
                "b".to_string(),
                NodeLayout {
                    x: 200.0,
                    y: 200.0,
                    width: 112.0,
                    height: 50.0,
                    ..Default::default()
                },
            ),
        ]);
        enforce_horizontal_gaps(&mut nodes, &budget);
        let a = &nodes["a"];
        let b = &nodes["b"];
        let gap = b.x - (a.x + a.width);
        assert!(
            gap + 0.5 >= budget.min_gap("a", "b"),
            "gap={gap} required={}",
            budget.min_gap("a", "b")
        );
    }
}
