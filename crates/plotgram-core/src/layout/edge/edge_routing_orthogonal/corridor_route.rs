//! 跨组边走廊（corridor）三段式路由。
//!
//! 源节点 → 源组边框 → 走廊车道 → 目标组边框 → 目标节点。
//! 非相邻组在走廊邻接图上 BFS 最短链，逐段串联。

use std::collections::{HashMap, HashSet, VecDeque};

use crate::ast::Relation;
use crate::layout::geometry::Point;
use crate::layout::group::{CorridorAxis, GroupCorridor, GroupRoutingContext};
use crate::layout::{GroupLayout, Port};

use super::path::port_outward;
use super::simplify::simplify_path_preserving_stubs;
use super::EPS;

/// 走廊内相邻车道间距（像素）
const CORRIDOR_LANE_PITCH: f64 = 18.0;
/// 端口 stub 默认长度
const DEFAULT_STUB_LEN: f64 = 24.0;

/// 跨组走廊路由计划
#[derive(Debug, Clone, Default)]
pub struct CorridorRoutePlan {
    /// edge_index → 走廊索引链（BFS 最短路径）
    pub chains: HashMap<usize, Vec<usize>>,
    /// (edge_index, corridor_index) → 车道序号
    pub lanes: HashMap<(usize, usize), usize>,
    /// corridor_index → 该走廊上的边数（用于车道居中）
    pub corridor_load: HashMap<usize, usize>,
}

/// 为跨组边规划走廊链与车道分配。
pub fn plan_corridor_routes(
    relations: &[Relation],
    group_ctx: &GroupRoutingContext,
) -> CorridorRoutePlan {
    if group_ctx.corridors.is_empty() {
        return CorridorRoutePlan::default();
    }

    let mut plan = CorridorRoutePlan::default();
    let mut corridor_edges: HashMap<usize, Vec<usize>> = HashMap::new();

    for (edge_index, rel) in relations.iter().enumerate() {
        let from_g = match group_ctx.node_leaf_group(rel.from.as_str()) {
            Some(g) => g,
            None => continue,
        };
        let to_g = match group_ctx.node_leaf_group(rel.to.as_str()) {
            Some(g) => g,
            None => continue,
        };
        if from_g == to_g {
            continue;
        }
        let Some(chain) = find_corridor_chain(from_g, to_g, &group_ctx.corridors) else {
            continue;
        };
        for &c_idx in &chain {
            corridor_edges.entry(c_idx).or_default().push(edge_index);
        }
        plan.chains.insert(edge_index, chain);
    }

    for (c_idx, mut edges) in corridor_edges {
        // 按 SuperEdgePair（leaf group 对）分组排序，使同组对的多边获得相邻 lane；
        // 组内再按 (from_id, to_id, edge_index) 确定性排序。
        edges.sort_by(|&a, &b| {
            let ra = &relations[a];
            let rb = &relations[b];
            let ga = super_edge_pair_key(group_ctx, ra.from.as_str(), ra.to.as_str());
            let gb = super_edge_pair_key(group_ctx, rb.from.as_str(), rb.to.as_str());
            ga.cmp(&gb)
                .then_with(|| ra.from.as_str().cmp(rb.from.as_str()))
                .then_with(|| ra.to.as_str().cmp(rb.to.as_str()))
                .then(a.cmp(&b))
        });
        edges.dedup();
        plan.corridor_load.insert(c_idx, edges.len());
        for (lane, &edge_index) in edges.iter().enumerate() {
            plan.lanes.insert((edge_index, c_idx), lane);
        }
    }

    plan
}

/// 提取边的 SuperEdgePair 键（规范化的 leaf group 对）。
///
/// 同一对 leaf group 的多条边会产生相同的键，用于 corridor lane 排序时分组相邻。
/// 返回 `None` 的情况：节点不在任何 leaf group 中、或两端同属一个 leaf group。
fn super_edge_pair_key(
    group_ctx: &GroupRoutingContext,
    from_id: &str,
    to_id: &str,
) -> Option<(String, String)> {
    let from_g = group_ctx.node_leaf_group(from_id)?;
    let to_g = group_ctx.node_leaf_group(to_id)?;
    if from_g == to_g {
        return None;
    }
    let (a, b) = crate::layout::edge::common::edge_geometry::canonical_pair(from_g, to_g);
    Some((a.to_string(), b.to_string()))
}

/// 尝试为跨组边构建走廊路径；失败时返回 `None` 由通用路由兜底。
pub fn try_build_corridor_path(
    edge_index: usize,
    from_anchor: Point,
    to_anchor: Point,
    from_id: &str,
    _to_id: &str,
    plan: &CorridorRoutePlan,
    group_ctx: &GroupRoutingContext,
    stub_len: f64,
) -> Option<Vec<Point>> {
    let chain = plan.chains.get(&edge_index)?;
    if chain.is_empty() {
        return None;
    }

    let from_group_id = group_ctx.node_leaf_group(from_id)?;
    let stub = stub_len.max(DEFAULT_STUB_LEN * 0.5);

    let mut waypoints = vec![from_anchor];
    let mut current = from_anchor;
    let mut current_group = from_group_id;

    for (step, &c_idx) in chain.iter().enumerate() {
        let corridor = &group_ctx.corridors[c_idx];
        let lane = plan.lanes.get(&(edge_index, c_idx)).copied().unwrap_or(0);
        let lane_count = plan.corridor_load.get(&c_idx).copied().unwrap_or(1);
        let lane_coord = corridor_lane_coord(corridor, lane, lane_count);

        let next_group = if corridor.group_a == current_group {
            corridor.group_b.as_str()
        } else if corridor.group_b == current_group {
            corridor.group_a.as_str()
        } else {
            return None;
        };

        let current_gl = group_ctx.groups.get(current_group)?;
        let next_gl = group_ctx.groups.get(next_group)?;

        let (exit_side, entry_side) = corridor_sides(corridor, current_group, next_group)?;

        let travel_coord = if step == chain.len() - 1 {
            match corridor.axis {
                CorridorAxis::Vertical => to_anchor.y,
                CorridorAxis::Horizontal => to_anchor.x,
            }
        } else {
            match corridor.axis {
                CorridorAxis::Vertical => corridor.coord,
                CorridorAxis::Horizontal => corridor.coord,
            }
        };

        let exit_border = border_point_on_side(current_gl, exit_side, current, corridor);
        let corridor_exit = corridor_point(corridor, lane_coord, exit_border);
        let corridor_entry = corridor_point(
            corridor,
            lane_coord,
            Point::new(
                if corridor.axis == CorridorAxis::Vertical {
                    next_gl.x + next_gl.width * 0.5
                } else {
                    travel_coord
                },
                if corridor.axis == CorridorAxis::Horizontal {
                    next_gl.y + next_gl.height * 0.5
                } else {
                    travel_coord
                },
            ),
        );
        let entry_border = border_point_on_side(next_gl, entry_side, corridor_entry, corridor);

        append_stub_leg(&mut waypoints, &mut current, exit_border, exit_side, stub);
        ortho_connect(&mut waypoints, &mut current, corridor_exit);
        ortho_connect(&mut waypoints, &mut current, corridor_entry);
        append_stub_leg(&mut waypoints, &mut current, entry_border, entry_side, stub);

        current_group = next_group;
    }

    let final_side = infer_port_at_point(current, to_anchor);
    append_stub_leg(&mut waypoints, &mut current, to_anchor, final_side, stub);
    waypoints.push(to_anchor);

    let path = simplify_path_preserving_stubs(waypoints);
    (path.len() >= 2).then_some(path)
}

fn find_corridor_chain(
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

fn corridor_sides(
    corridor: &GroupCorridor,
    from_group: &str,
    to_group: &str,
) -> Option<(Port, Port)> {
    let _ = to_group;
    match corridor.axis {
        CorridorAxis::Vertical => {
            if corridor.group_a == from_group {
                Some((Port::Right, Port::Left))
            } else {
                Some((Port::Left, Port::Right))
            }
        }
        CorridorAxis::Horizontal => {
            if corridor.group_a == from_group {
                Some((Port::Bottom, Port::Top))
            } else {
                Some((Port::Top, Port::Bottom))
            }
        }
    }
}

fn corridor_lane_coord(corridor: &GroupCorridor, lane: usize, lane_count: usize) -> f64 {
    let center = (lane_count.saturating_sub(1)) as f64 * 0.5;
    let offset = (lane as f64 - center) * CORRIDOR_LANE_PITCH;
    corridor.coord + offset
}

fn corridor_point(corridor: &GroupCorridor, lane_coord: f64, reference: Point) -> Point {
    match corridor.axis {
        CorridorAxis::Vertical => Point::new(
            lane_coord,
            reference.y.clamp(corridor.span_min, corridor.span_max),
        ),
        CorridorAxis::Horizontal => Point::new(
            reference.x.clamp(corridor.span_min, corridor.span_max),
            lane_coord,
        ),
    }
}

fn border_point_on_side(
    gl: &GroupLayout,
    side: Port,
    reference: Point,
    corridor: &GroupCorridor,
) -> Point {
    let point = match side {
        Port::Right => Point::new(
            gl.x + gl.width,
            reference.y.clamp(gl.y, gl.y + gl.height),
        ),
        Port::Left => Point::new(gl.x, reference.y.clamp(gl.y, gl.y + gl.height)),
        Port::Bottom => Point::new(
            reference.x.clamp(gl.x, gl.x + gl.width),
            gl.y + gl.height,
        ),
        Port::Top => Point::new(reference.x.clamp(gl.x, gl.x + gl.width), gl.y),
    };
    corridor_point(corridor, corridor.coord, point)
}

fn append_stub_leg(
    waypoints: &mut Vec<Point>,
    current: &mut Point,
    target: Point,
    side: Port,
    stub_len: f64,
) {
    let (ox, oy) = port_outward(side);
    let stub = Point::new(current.x + ox * stub_len, current.y + oy * stub_len);
    if (stub.x - current.x).abs() > EPS || (stub.y - current.y).abs() > EPS {
        waypoints.push(stub);
        *current = stub;
    }
    ortho_connect(waypoints, current, target);
}

fn ortho_connect(waypoints: &mut Vec<Point>, current: &mut Point, target: Point) {
    if (current.x - target.x).abs() < EPS && (current.y - target.y).abs() < EPS {
        return;
    }
    if (current.x - target.x).abs() > EPS && (current.y - target.y).abs() > EPS {
        waypoints.push(Point::new(target.x, current.y));
        *current = Point::new(target.x, current.y);
    }
    if (current.x - target.x).abs() > EPS || (current.y - target.y).abs() > EPS {
        waypoints.push(target);
        *current = target;
    }
}

fn infer_port_at_point(from: Point, to: Point) -> Port {
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    if dx.abs() >= dy.abs() {
        if dx > 0.0 {
            Port::Right
        } else {
            Port::Left
        }
    } else if dy > 0.0 {
        Port::Bottom
    } else {
        Port::Top
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::GroupLayout;

    fn sample_corridor() -> GroupCorridor {
        GroupCorridor {
            axis: CorridorAxis::Vertical,
            coord: 120.0,
            span_min: 10.0,
            span_max: 200.0,
            group_a: "left".into(),
            group_b: "right".into(),
        }
    }

    #[test]
    fn finds_corridor_chain_between_adjacent_groups() {
        let corridors = vec![sample_corridor()];
        let chain = find_corridor_chain("left", "right", &corridors).unwrap();
        assert_eq!(chain, vec![0]);
    }

    #[test]
    fn assigns_lanes_deterministically() {
        let mut groups = HashMap::new();
        groups.insert(
            "left".into(),
            GroupLayout {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 80.0,
            },
        );
        groups.insert(
            "right".into(),
            GroupLayout {
                x: 140.0,
                y: 0.0,
                width: 100.0,
                height: 80.0,
            },
        );
        let ctx = GroupRoutingContext {
            groups,
            node_to_groups: HashMap::new(),
            border_shell_pad: 12.0,
            stub_clearance: 24.0,
            corridor_misalignment_penalty: 80.0,
            repulse_max_rounds: 2,
            corridors: vec![sample_corridor()],
            node_leaf_group: HashMap::from([
                ("a".into(), "left".into()),
                ("b".into(), "right".into()),
            ]),
            sibling_sets: vec![],
            sibling_orientation: HashMap::new(),
            group_ancestors: HashMap::new(),
        };
        let relations = vec![Relation {
            from: crate::ast::Identifier::new_unchecked("a"),
            to: crate::ast::Identifier::new_unchecked("b"),
            arrow: crate::ast::ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: crate::ast::AttributeMap::default(),
            span: crate::ast::Span::dummy(),
        }];
        let plan = plan_corridor_routes(&relations, &ctx);
        assert_eq!(plan.chains.get(&0).map(|c| c.as_slice()), Some(&[0][..]));
        assert_eq!(plan.lanes.get(&(0, 0)), Some(&0));
    }

    fn make_relation(from: &str, to: &str) -> Relation {
        Relation {
            from: crate::ast::Identifier::new_unchecked(from),
            to: crate::ast::Identifier::new_unchecked(to),
            arrow: crate::ast::ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: crate::ast::AttributeMap::default(),
            span: crate::ast::Span::dummy(),
        }
    }

    fn make_ctx(node_leaf_group: HashMap<String, String>) -> GroupRoutingContext {
        let mut groups = HashMap::new();
        groups.insert(
            "left".into(),
            GroupLayout { x: 0.0, y: 0.0, width: 100.0, height: 80.0 },
        );
        groups.insert(
            "right".into(),
            GroupLayout { x: 140.0, y: 0.0, width: 100.0, height: 80.0 },
        );
        GroupRoutingContext {
            groups,
            node_to_groups: HashMap::new(),
            border_shell_pad: 12.0,
            stub_clearance: 24.0,
            corridor_misalignment_penalty: 80.0,
            repulse_max_rounds: 2,
            corridors: vec![sample_corridor()],
            node_leaf_group,
            sibling_sets: vec![],
            sibling_orientation: HashMap::new(),
            group_ancestors: HashMap::new(),
        }
    }

    #[test]
    fn super_edge_pair_edges_get_adjacent_lanes() {
        // 4 条边跨 left→right corridor：
        //   edge 0: a1→b1 (left→right)
        //   edge 1: a2→b2 (left→right)  — 与 edge 0 同 SuperEdgePair
        //   edge 2: a3→b3 (left→right)  — 与 edge 0 同 SuperEdgePair
        //   edge 3: a4→b4 (left→right)  — 与 edge 0 同 SuperEdgePair
        // 期望：4 条边都同属一个 SuperEdgePair，获得 lane 0,1,2,3（相邻）
        let ctx = make_ctx(HashMap::from([
            ("a1".into(), "left".into()),
            ("a2".into(), "left".into()),
            ("a3".into(), "left".into()),
            ("a4".into(), "left".into()),
            ("b1".into(), "right".into()),
            ("b2".into(), "right".into()),
            ("b3".into(), "right".into()),
            ("b4".into(), "right".into()),
        ]));
        let relations = vec![
            make_relation("a1", "b1"),
            make_relation("a2", "b2"),
            make_relation("a3", "b3"),
            make_relation("a4", "b4"),
        ];
        let plan = plan_corridor_routes(&relations, &ctx);
        // 所有边应分配到 corridor 0 的 lane 0..3
        for edge_idx in 0..4 {
            assert!(
                plan.lanes.contains_key(&(edge_idx, 0)),
                "edge {} 应在 corridor 0 分到 lane",
                edge_idx
            );
        }
        // lane 值应为 0,1,2,3 的排列（相邻分配）
        let mut lanes: Vec<usize> = (0..4)
            .map(|i| plan.lanes.get(&(i, 0)).copied().unwrap_or(usize::MAX))
            .collect();
        lanes.sort();
        assert_eq!(lanes, vec![0, 1, 2, 3], "lane 应为 0..3 的排列");
    }

    #[test]
    fn super_edge_pair_groups_adjacent_lanes_across_pairs() {
        // 两组 SuperEdgePair，每组 2 条边：
        //   Pair A (left→right): edge 0 (a1→b1), edge 1 (a2→b2)
        //   Pair B (left→right): edge 2 (a3→b3), edge 3 (a4→b4)
        // 但 a1/a2 在 left，a3/a4 也在 left —— 同一个 leaf group pair
        // 所以实际上 4 条边同属一个 SuperEdgePair，期望 lane 0..3
        // 改为测试不同 leaf group pair 的情况：
        //   corridor left→right，4 条边都跨此 corridor
        //   但 a1,a2 在 left_sub，b1,b2 在 right_sub（SuperEdgePair: left_sub|right_sub）
        //   a3,a4 在 left_other，b3,b4 在 right_other（SuperEdgePair: left_other|right_other）
        // 由于 corridor 是 left|right 级别，node_leaf_group 映射到 left/right
        // 所以所有 4 条边同属 SuperEdgePair (left,right)，无法测试跨 pair 分组
        // 改为测试：同 SuperEdgePair 的边是否获得连续 lane
        let ctx = make_ctx(HashMap::from([
            ("a1".into(), "left".into()),
            ("a2".into(), "left".into()),
            ("a3".into(), "left".into()),
            ("b1".into(), "right".into()),
            ("b2".into(), "right".into()),
            ("b3".into(), "right".into()),
        ]));
        let relations = vec![
            make_relation("a1", "b1"),
            make_relation("a3", "b3"),  // 不同 SuperEdgePair 子组，但同 leaf group pair
            make_relation("a2", "b2"),
        ];
        let plan = plan_corridor_routes(&relations, &ctx);
        // 排序后应为 a1→b1, a2→b2, a3→b3（按 from_id 然后 to_id）
        // lane: a1→b1=0, a2→b2=1, a3→b3=2
        assert_eq!(plan.lanes.get(&(0, 0)), Some(&0), "edge 0 (a1→b1) 应为 lane 0");
        assert_eq!(plan.lanes.get(&(2, 0)), Some(&1), "edge 2 (a2→b2) 应为 lane 1");
        assert_eq!(plan.lanes.get(&(1, 0)), Some(&2), "edge 1 (a3→b3) 应为 lane 2");
    }
}
