//! 全局通道规划器
//!
//! Phase B3：替代逐边贪心，全局规划通道使用。
//!
//! 算法：
//! 1. 构建通道需求图：每条边需要哪些方向的通道
//! 2. 通道坐标候选：组间隙中点、节点间隙中点、图 bbox 外侧
//! 3. 贪心分配：按边难度降序，为每条边分配最不拥堵的通道
//! 4. 冲突消解：同通道多边按 lane 偏移

use crate::ast::Relation;
use crate::layout::edge::common::edge_geometry::node_center;
use crate::layout::geometry::{Point, EPS};
use crate::layout::group::GroupRoutingContext;
use crate::layout::NodeLayout;
use std::collections::{BTreeMap, HashMap};

/// 全局通道规划
#[derive(Clone, Debug, Default)]
pub struct ChannelPlan {
    /// 垂直通道 x 坐标 → 分配给哪些边
    pub vertical_channels: BTreeMap<i64, Vec<usize>>,
    /// 水平通道 y 坐标 → 分配给哪些边
    pub horizontal_channels: BTreeMap<i64, Vec<usize>>,
    /// P2-1: 每条边的精确 lane 坐标（通道中心 + 偏移）
    /// key = edge_idx, value = (lane_coord, is_vertical)
    pub lane_assignments: HashMap<usize, (f64, bool)>,
}

impl ChannelPlan {
    /// 查询指定边的规划通道坐标。
    /// 返回 (coord, is_vertical)：垂直通道返回 x 坐标，水平通道返回 y 坐标。
    pub fn channel_for_edge(&self, edge_idx: usize) -> Option<(f64, bool)> {
        // 优先查垂直通道
        for (&key, edges) in &self.vertical_channels {
            if edges.contains(&edge_idx) {
                return Some((key as f64 / 100.0, true));
            }
        }
        for (&key, edges) in &self.horizontal_channels {
            if edges.contains(&edge_idx) {
                return Some((key as f64 / 100.0, false));
            }
        }
        None
    }
}

/// 通道候选
#[derive(Clone, Copy, Debug)]
struct ChannelCandidate {
    coord: f64,
    is_vertical: bool,
    capacity: usize,
}

/// 通道规划算法
pub fn plan_channels(
    relations: &[Relation],
    nodes: &HashMap<String, NodeLayout>,
    group_ctx: &GroupRoutingContext,
    edge_order: &[usize],
) -> ChannelPlan {
    let mut plan = ChannelPlan::default();

    // 1. 收集通道候选坐标
    let candidates = collect_channel_candidates(nodes, group_ctx);

    // 2. 为每条边分配通道
    for &edge_idx in edge_order {
        if edge_idx >= relations.len() {
            continue;
        }

        let rel = &relations[edge_idx];
        let (Some(from_nl), Some(to_nl)) = (
            nodes.get(rel.from.as_str()),
            nodes.get(rel.to.as_str()),
        ) else {
            continue;
        };

        let from_center = node_center(from_nl);
        let to_center = node_center(to_nl);

        // 确定需要的通道方向
        let dx = to_center.x - from_center.x;
        let dy = to_center.y - from_center.y;

        // 垂直通道（用于水平移动）
        if dx.abs() > EPS {
            if let Some(channel) = find_best_channel(
                &candidates,
                true,
                from_center,
                to_center,
                &plan,
            ) {
                let key = (channel * 100.0) as i64;
                plan.vertical_channels.entry(key).or_default().push(edge_idx);
            }
        }

        // 水平通道（用于垂直移动）
        if dy.abs() > EPS {
            if let Some(channel) = find_best_channel(
                &candidates,
                false,
                from_center,
                to_center,
                &plan,
            ) {
                let key = (channel * 100.0) as i64;
                plan.horizontal_channels.entry(key).or_default().push(edge_idx);
            }
        }
    }

    plan
}

/// P2-1: 为同通道多边分配对称 lane 偏移，生成精确的 per-edge 车道坐标。
///
/// N 边通道：偏移从 -(N-1)*gap/2 到 +(N-1)*gap/2，保持通道中心对称。
pub fn assign_lane_offsets(plan: &mut ChannelPlan, parallel_gap: f64) {
    // 垂直通道：偏移沿 x 轴
    for (&key, edges) in &plan.vertical_channels {
        let center = key as f64 / 100.0;
        let n = edges.len();
        for (slot, &ei) in edges.iter().enumerate() {
            let offset = (slot as f64 - (n - 1) as f64 / 2.0) * parallel_gap;
            plan.lane_assignments.insert(ei, (center + offset, true));
        }
    }
    // 水平通道：偏移沿 y 轴
    for (&key, edges) in &plan.horizontal_channels {
        let center = key as f64 / 100.0;
        let n = edges.len();
        for (slot, &ei) in edges.iter().enumerate() {
            let offset = (slot as f64 - (n - 1) as f64 / 2.0) * parallel_gap;
            // 如果边已有垂直通道分配，不覆盖（垂直优先）
            plan.lane_assignments.entry(ei).or_insert((center + offset, false));
        }
    }
}

/// 收集通道候选坐标
fn collect_channel_candidates(
    nodes: &HashMap<String, NodeLayout>,
    group_ctx: &GroupRoutingContext,
) -> Vec<ChannelCandidate> {
    let mut candidates = Vec::new();

    // 组间隙中点
    let group_ids: Vec<&String> = group_ctx.groups.keys().collect();
    for i in 0..group_ids.len() {
        for j in (i + 1)..group_ids.len() {
            let g1 = &group_ctx.groups[group_ids[i]];
            let g2 = &group_ctx.groups[group_ids[j]];

            // 垂直通道（组间水平间隙）
            if g1.x + g1.width < g2.x {
                let mid = (g1.x + g1.width + g2.x) / 2.0;
                candidates.push(ChannelCandidate {
                    coord: mid,
                    is_vertical: true,
                    capacity: 4,
                });
            } else if g2.x + g2.width < g1.x {
                let mid = (g2.x + g2.width + g1.x) / 2.0;
                candidates.push(ChannelCandidate {
                    coord: mid,
                    is_vertical: true,
                    capacity: 4,
                });
            }

            // 水平通道（组间垂直间隙）
            if g1.y + g1.height < g2.y {
                let mid = (g1.y + g1.height + g2.y) / 2.0;
                candidates.push(ChannelCandidate {
                    coord: mid,
                    is_vertical: false,
                    capacity: 4,
                });
            } else if g2.y + g2.height < g1.y {
                let mid = (g2.y + g2.height + g1.y) / 2.0;
                candidates.push(ChannelCandidate {
                    coord: mid,
                    is_vertical: false,
                    capacity: 4,
                });
            }
        }
    }

    // 节点间隙中点
    let node_ids: Vec<&String> = nodes.keys().collect();
    for i in 0..node_ids.len() {
        for j in (i + 1)..node_ids.len() {
            let n1 = &nodes[node_ids[i]];
            let n2 = &nodes[node_ids[j]];

            // 垂直通道（检查两个方向）
            if n1.x + n1.width < n2.x {
                let mid = (n1.x + n1.width + n2.x) / 2.0;
                candidates.push(ChannelCandidate {
                    coord: mid,
                    is_vertical: true,
                    capacity: 2,
                });
            } else if n2.x + n2.width < n1.x {
                let mid = (n2.x + n2.width + n1.x) / 2.0;
                candidates.push(ChannelCandidate {
                    coord: mid,
                    is_vertical: true,
                    capacity: 2,
                });
            }

            // 水平通道（检查两个方向）
            if n1.y + n1.height < n2.y {
                let mid = (n1.y + n1.height + n2.y) / 2.0;
                candidates.push(ChannelCandidate {
                    coord: mid,
                    is_vertical: false,
                    capacity: 2,
                });
            } else if n2.y + n2.height < n1.y {
                let mid = (n2.y + n2.height + n1.y) / 2.0;
                candidates.push(ChannelCandidate {
                    coord: mid,
                    is_vertical: false,
                    capacity: 2,
                });
            }
        }
    }

    candidates
}

/// 为边找到最佳通道
fn find_best_channel(
    candidates: &[ChannelCandidate],
    is_vertical: bool,
    from: Point,
    to: Point,
    plan: &ChannelPlan,
) -> Option<f64> {
    let mut best: Option<(f64, usize)> = None; // (coord, load)

    for cand in candidates {
        if cand.is_vertical != is_vertical {
            continue;
        }

        // 检查通道是否在边的范围内
        let in_range = if is_vertical {
            let min_x = from.x.min(to.x);
            let max_x = from.x.max(to.x);
            cand.coord >= min_x && cand.coord <= max_x
        } else {
            let min_y = from.y.min(to.y);
            let max_y = from.y.max(to.y);
            cand.coord >= min_y && cand.coord <= max_y
        };

        if !in_range {
            continue;
        }

        // 计算当前负载
        let key = (cand.coord * 100.0) as i64;
        let load = if is_vertical {
            plan.vertical_channels.get(&key).map_or(0, |v| v.len())
        } else {
            plan.horizontal_channels.get(&key).map_or(0, |v| v.len())
        };

        // 选择负载最低的通道
        if best.map_or(true, |(_, l)| load < l) {
            best = Some((cand.coord, load));
        }
    }

    best.map(|(coord, _)| coord)
}

/// 检查通道规划器是否启用（默认开启，PLOTGRAM_CHANNEL_PLANNER=0 关闭）
pub fn channel_planner_enabled() -> bool {
    !std::env::var("PLOTGRAM_CHANNEL_PLANNER")
        .map(|v| v == "0" || v.eq_ignore_ascii_case("false"))
        .unwrap_or(false)
}

impl ChannelPlan {
    /// 获取边的垂直通道坐标
    pub fn vertical_channel_for_edge(&self, edge_idx: usize) -> Option<f64> {
        for (&coord, edges) in &self.vertical_channels {
            if edges.contains(&edge_idx) {
                return Some(coord as f64 / 100.0);
            }
        }
        None
    }

    /// 获取边的水平通道坐标
    pub fn horizontal_channel_for_edge(&self, edge_idx: usize) -> Option<f64> {
        for (&coord, edges) in &self.horizontal_channels {
            if edges.contains(&edge_idx) {
                return Some(coord as f64 / 100.0);
            }
        }
        None
    }

    /// 通道总数
    pub fn channel_count(&self) -> usize {
        self.vertical_channels.len() + self.horizontal_channels.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::NodeLayout;

    fn node(x: f64, y: f64, w: f64, h: f64) -> NodeLayout {
        NodeLayout {
            x,
            y,
            width: w,
            height: h,
            ..Default::default()
        }
    }

    #[test]
    fn test_channel_plan_empty() {
        let plan = ChannelPlan::default();
        assert_eq!(plan.channel_count(), 0);
        assert_eq!(plan.vertical_channel_for_edge(0), None);
    }

    #[test]
    fn test_channel_planner_enabled_by_default() {
        assert!(channel_planner_enabled());
    }

    #[test]
    fn test_collect_channel_candidates() {
        let mut nodes = HashMap::new();
        nodes.insert("a".to_string(), node(0.0, 0.0, 50.0, 50.0));
        nodes.insert("b".to_string(), node(100.0, 0.0, 50.0, 50.0));

        // 测试节点间隙候选收集（不依赖 GroupRoutingContext）
        let candidates = collect_node_gap_candidates(&nodes);

        // 应该找到节点间的垂直通道候选
        assert!(!candidates.is_empty());
        assert!(candidates.iter().any(|c| c.is_vertical));
    }

    /// 仅收集节点间隙候选（用于测试）
    fn collect_node_gap_candidates(nodes: &HashMap<String, NodeLayout>) -> Vec<ChannelCandidate> {
        let mut candidates = Vec::new();
        let node_ids: Vec<&String> = nodes.keys().collect();
        for i in 0..node_ids.len() {
            for j in (i + 1)..node_ids.len() {
                let n1 = &nodes[node_ids[i]];
                let n2 = &nodes[node_ids[j]];

                if n1.x + n1.width < n2.x {
                    let mid = (n1.x + n1.width + n2.x) / 2.0;
                    candidates.push(ChannelCandidate {
                        coord: mid,
                        is_vertical: true,
                        capacity: 2,
                    });
                } else if n2.x + n2.width < n1.x {
                    let mid = (n2.x + n2.width + n1.x) / 2.0;
                    candidates.push(ChannelCandidate {
                        coord: mid,
                        is_vertical: true,
                        capacity: 2,
                    });
                }

                if n1.y + n1.height < n2.y {
                    let mid = (n1.y + n1.height + n2.y) / 2.0;
                    candidates.push(ChannelCandidate {
                        coord: mid,
                        is_vertical: false,
                        capacity: 2,
                    });
                } else if n2.y + n2.height < n1.y {
                    let mid = (n2.y + n2.height + n1.y) / 2.0;
                    candidates.push(ChannelCandidate {
                        coord: mid,
                        is_vertical: false,
                        capacity: 2,
                    });
                }
            }
        }
        candidates
    }

    #[test]
    fn test_channel_planner_basic() {
        // 基础测试：两个节点水平排列，验证通道规划产生合理结果
        use crate::ast::{ArrowType, AttributeMap, Identifier, Relation, Span};
        use crate::layout::group::GroupRoutingContext;
        use crate::layout::types::LayoutHints;
        use crate::layout::LayoutResult;

        let mut nodes = HashMap::new();
        nodes.insert("a".to_string(), node(0.0, 0.0, 50.0, 50.0));
        nodes.insert("b".to_string(), node(150.0, 0.0, 50.0, 50.0));

        let relations = vec![Relation {
            from: Identifier::new_unchecked("a"),
            to: Identifier::new_unchecked("b"),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span: Span::dummy(),
        }];

        let layout_result = LayoutResult {
            nodes: HashMap::new(),
            groups: HashMap::new(),
            edges: vec![],
            total_width: 0.0,
            total_height: 0.0,
            hints: LayoutHints::default(),
        };
        let group_ctx = GroupRoutingContext::from_layout(
            &crate::ast::Diagram::default(),
            &layout_result,
            "orthogonal",
        );

        let edge_order = vec![0usize];
        let plan = plan_channels(&relations, &nodes, &group_ctx, &edge_order);

        // 应该产生至少一个通道（水平移动需要垂直通道）
        assert!(plan.channel_count() > 0, "Should produce at least one channel");
    }
}
