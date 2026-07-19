//! 走廊 demand 模型（B2 容量 v1；可 pre-route，不依赖折线）。

use crate::ast::Diagram;
use crate::layout::group::{
    routing_algo_for_diagram, CorridorAxis, GroupCorridor, GroupRoutingContext,
};
use crate::layout::{GroupLayout, LayoutResult};
use std::collections::{HashMap, HashSet, VecDeque};

use super::types::CorridorDemand;

/// 与 `corridor_route` 车道间距对齐。
pub const CORRIDOR_LANE_PITCH: f64 = 18.0;

/// 廊模型快照（折线无关）。
#[derive(Debug, Clone)]
pub struct CorridorModel {
    pub corridors: Vec<GroupCorridor>,
    pub demands: Vec<CorridorDemand>,
    /// edge_index → 最短廊链（走廊索引）
    pub edge_chains: HashMap<usize, Vec<usize>>,
    pub cross_scope_edges: usize,
    pub with_chain: usize,
    /// 跨 leaf 但无链的边索引
    pub no_chain_edges: Vec<usize>,
}

impl CorridorModel {
    pub fn overloaded_indices(&self) -> HashSet<usize> {
        self.demands
            .iter()
            .filter(|d| d.is_over())
            .map(|d| d.corridor_index)
            .collect()
    }

    /// 边级严重超容：链上存在 load > 2×capacity（与 B2 predicted 扇出一致）。
    pub fn severe_overloaded_indices(&self) -> HashSet<usize> {
        self.demands
            .iter()
            .filter(|d| d.capacity > 0 && d.load > d.capacity.saturating_mul(2))
            .map(|d| d.corridor_index)
            .collect()
    }
}

/// 从完整 LayoutResult 计算（路由前/后均可；不读 edges 折线）。
pub fn compute_corridor_model(diagram: &Diagram, result: &LayoutResult) -> CorridorModel {
    let algo = routing_algo_for_diagram(diagram);
    let group_ctx = GroupRoutingContext::from_layout(diagram, result, algo);
    compute_corridor_model_from_ctx(diagram, &group_ctx)
}

fn compute_corridor_model_from_ctx(
    diagram: &Diagram,
    group_ctx: &GroupRoutingContext,
) -> CorridorModel {
    let corridors = &group_ctx.corridors;
    let mut cross_scope = 0usize;
    let mut with_chain = 0usize;
    let mut corridor_edge_counts: HashMap<usize, usize> = HashMap::new();
    let mut edge_chains: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut no_chain_edges: Vec<usize> = Vec::new();

    for (edge_index, rel) in diagram.relations.iter().enumerate() {
        let from_leaf = group_ctx
            .node_leaf_group(rel.from.as_str())
            .map(str::to_string);
        let to_leaf = group_ctx
            .node_leaf_group(rel.to.as_str())
            .map(str::to_string);
        let (Some(ref fa), Some(ref tb)) = (&from_leaf, &to_leaf) else {
            continue;
        };
        if fa == tb {
            continue;
        }
        cross_scope += 1;
        match find_corridor_chain(fa, tb, corridors) {
            Some(chain) => {
                with_chain += 1;
                for &c_idx in &chain {
                    *corridor_edge_counts.entry(c_idx).or_insert(0) += 1;
                }
                edge_chains.insert(edge_index, chain);
            }
            None => {
                no_chain_edges.push(edge_index);
            }
        }
    }
    no_chain_edges.sort_unstable();

    let mut demands: Vec<CorridorDemand> = Vec::new();
    for (c_idx, c) in corridors.iter().enumerate() {
        let load = corridor_edge_counts.get(&c_idx).copied().unwrap_or(0);
        let span = (c.span_max - c.span_min).abs();
        let gap = corridor_gap(c, &group_ctx.groups);
        let (capacity, degen) = corridor_capacity_v1(gap, span);
        demands.push(CorridorDemand {
            corridor_index: c_idx,
            axis: c.axis,
            group_a: c.group_a.clone(),
            group_b: c.group_b.clone(),
            load,
            capacity: if degen { 0 } else { capacity },
            span,
            gap,
        });
    }
    demands.sort_by(|a, b| {
        b.load
            .cmp(&a.load)
            .then_with(|| a.corridor_index.cmp(&b.corridor_index))
    });

    CorridorModel {
        corridors: corridors.clone(),
        demands,
        edge_chains,
        cross_scope_edges: cross_scope,
        with_chain,
        no_chain_edges,
    }
}

/// 组间法向间隙：车道沿此方向以 `CORRIDOR_LANE_PITCH` 排布。
pub fn corridor_gap(c: &GroupCorridor, groups: &HashMap<String, GroupLayout>) -> f64 {
    let Some(ga) = groups.get(&c.group_a) else {
        return 0.0;
    };
    let Some(gb) = groups.get(&c.group_b) else {
        return 0.0;
    };
    match c.axis {
        CorridorAxis::Horizontal => {
            let a_bottom = ga.y + ga.height;
            let b_bottom = gb.y + gb.height;
            if a_bottom <= gb.y {
                gb.y - a_bottom
            } else if b_bottom <= ga.y {
                ga.y - b_bottom
            } else {
                0.0
            }
        }
        CorridorAxis::Vertical => {
            let a_right = ga.x + ga.width;
            let b_right = gb.x + gb.width;
            if a_right <= gb.x {
                gb.x - a_right
            } else if b_right <= ga.x {
                ga.x - b_right
            } else {
                0.0
            }
        }
    }
}

/// v1 容量：法向 gap 定车道数；gap 或 span 小于一车道 → DEGEN。
pub fn corridor_capacity_v1(gap: f64, span: f64) -> (usize, bool) {
    if gap + 0.1 < CORRIDOR_LANE_PITCH || span + 0.1 < CORRIDOR_LANE_PITCH {
        return (0, true);
    }
    let cap = ((gap / CORRIDOR_LANE_PITCH).floor() as usize).max(1);
    (cap, false)
}

pub fn find_corridor_chain(
    from_group: &str,
    to_group: &str,
    corridors: &[GroupCorridor],
) -> Option<Vec<usize>> {
    if from_group == to_group {
        return None;
    }
    let mut adj: HashMap<&str, Vec<(usize, &str)>> = HashMap::new();
    for (idx, c) in corridors.iter().enumerate() {
        adj.entry(c.group_a.as_str())
            .or_default()
            .push((idx, c.group_b.as_str()));
        adj.entry(c.group_b.as_str())
            .or_default()
            .push((idx, c.group_a.as_str()));
    }
    let mut visited: HashSet<&str> = HashSet::from([from_group]);
    let mut queue: VecDeque<(&str, Vec<usize>)> = VecDeque::from([(from_group, Vec::new())]);
    while let Some((current, chain)) = queue.pop_front() {
        if current == to_group {
            return Some(chain);
        }
        let mut neighbors: Vec<(usize, &str)> = adj.get(current).cloned().unwrap_or_default();
        neighbors.sort_by_key(|(idx, neighbor)| (*idx, *neighbor));
        for (c_idx, neighbor) in neighbors {
            if visited.insert(neighbor) {
                let mut next = chain.clone();
                next.push(c_idx);
                queue.push_back((neighbor, next));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capacity_uses_gap_not_span() {
        let (cap, degen) = corridor_capacity_v1(80.0, 33.0);
        assert!(!degen);
        assert_eq!(cap, 4);
        let (cap_thin, degen_thin) = corridor_capacity_v1(10.0, 100.0);
        assert!(degen_thin);
        assert_eq!(cap_thin, 0);
        let (cap_short_span, degen_ss) = corridor_capacity_v1(80.0, 10.0);
        assert!(degen_ss);
        assert_eq!(cap_short_span, 0);
    }
}
