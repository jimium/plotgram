//! 词典序 A*：在 [`ResourceGraph`] 上搜索路径，距离标签为 [`LexCost`]。
//!
//! H1/H2 由构图保证（障碍不入图）；本搜索只优化弯折 / 长度 / 交叉等软目标。

use super::graph::{ResourceEdge, ResourceGraph, ResourceVertexId};
use super::model::{LexCost, OrderedF64, SolverStatus};
use crate::layout::geometry::Point;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

/// 搜索结果。
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub points: Vec<Point>,
    pub cost: LexCost,
    pub status: SolverStatus,
}

#[derive(Clone)]
struct State {
    cost: LexCost,
    vertex: ResourceVertexId,
    /// 进入该顶点的边是否水平（None = 起点）
    incoming_horizontal: Option<bool>,
}

impl PartialEq for State {
    fn eq(&self, other: &Self) -> bool {
        self.cost == other.cost && self.vertex == other.vertex
    }
}
impl Eq for State {}
impl PartialOrd for State {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for State {
    fn cmp(&self, other: &Self) -> Ordering {
        // BinaryHeap 是最大堆；反转使最小 LexCost 优先
        other
            .cost
            .cmp(&self.cost)
            .then_with(|| self.vertex.cmp(&other.vertex))
    }
}

/// 词典序 A*（此处启发为 0，退化为词典序 Dijkstra，确定性全展开）。
///
/// `prefer_periphery`: 对外围 GroupGate / 外扩 ChannelJunction 给 Q5 奖励（负对齐代价）。
/// `occupied`: 已占用轴向段，用于 Q2 交叉/重叠近似计数。
pub fn lex_astar(
    graph: &ResourceGraph,
    start: ResourceVertexId,
    goal: ResourceVertexId,
    prefer_periphery: bool,
    occupied: &[(f64, f64, f64, f64)],
) -> SearchResult {
    if start == goal {
        let p = graph
            .vertex(start)
            .map(|v| v.position)
            .unwrap_or(Point::new(0.0, 0.0));
        return SearchResult {
            points: vec![p, p],
            cost: LexCost::default(),
            status: SolverStatus::Converged,
        };
    }

    let mut open = BinaryHeap::new();
    open.push(State {
        cost: LexCost::default(),
        vertex: start,
        incoming_horizontal: None,
    });

    // best[vertex] = (cost, parent, via_horizontal)
    let mut best: HashMap<ResourceVertexId, (LexCost, Option<ResourceVertexId>, Option<bool>)> =
        HashMap::new();
    best.insert(start, (LexCost::default(), None, None));

    while let Some(State {
        cost,
        vertex,
        incoming_horizontal,
    }) = open.pop()
    {
        if let Some((bc, _, _)) = best.get(&vertex) {
            if &cost > bc {
                continue;
            }
        }
        if vertex == goal {
            let points = reconstruct(graph, &best, start, goal);
            return SearchResult {
                points,
                cost,
                status: SolverStatus::Converged,
            };
        }

        for edge in graph.neighbors(vertex) {
            let next_cost = step_cost(
                graph,
                &cost,
                incoming_horizontal,
                edge,
                prefer_periphery,
                occupied,
            );
            let better = match best.get(&edge.to) {
                None => true,
                Some((bc, _, _)) => next_cost < *bc,
            };
            if better {
                best.insert(
                    edge.to,
                    (next_cost, Some(vertex), Some(edge.is_horizontal)),
                );
                open.push(State {
                    cost: next_cost,
                    vertex: edge.to,
                    incoming_horizontal: Some(edge.is_horizontal),
                });
            }
        }
    }

    // 无解：直线兜底（显式 Degraded）
    let sp = graph
        .vertex(start)
        .map(|v| v.position)
        .unwrap_or(Point::new(0.0, 0.0));
    let ep = graph
        .vertex(goal)
        .map(|v| v.position)
        .unwrap_or(Point::new(0.0, 0.0));
    let degraded = orthogonal_fallback(sp, ep);
    SearchResult {
        points: degraded,
        cost: LexCost {
            q1_hard_residual: OrderedF64(1.0),
            ..LexCost::default()
        },
        status: SolverStatus::Degraded,
    }
}

fn step_cost(
    graph: &ResourceGraph,
    prev: &LexCost,
    incoming_horizontal: Option<bool>,
    edge: &ResourceEdge,
    prefer_periphery: bool,
    occupied: &[(f64, f64, f64, f64)],
) -> LexCost {
    let mut c = *prev;
    let bend = match incoming_horizontal {
        None => 0,
        Some(h) if h != edge.is_horizontal => 1,
        _ => 0,
    };
    c.q3_bends = c.q3_bends.saturating_add(bend);
    c.q4_length = OrderedF64(c.q4_length.0 + edge.length);

    // Q2：与已占用段轴对齐重叠近似；同时计命中数供 H4
    let mut overlap_hits = 0u32;
    if let (Some(from_v), Some(to_v)) = (graph.vertex(edge.from), graph.vertex(edge.to)) {
        let overlap = axis_overlap_len(
            from_v.position,
            to_v.position,
            edge.is_horizontal,
            occupied,
            &mut overlap_hits,
        );
        if overlap > 0.0 {
            c.q2_crossings = c.q2_crossings.saturating_add(1);
        }
    }

    // H4：ResourceEdge.capacity —— 与已占用段并发数超过容量 → q1 硬残差
    if overlap_hits >= edge.capacity {
        c.q1_hard_residual =
            OrderedF64(c.q1_hard_residual.0 + f64::from(overlap_hits - edge.capacity + 1));
    }

    // Q5：外围偏好 —— GroupGate 给负对齐代价
    if prefer_periphery {
        if let Some(v) = graph.vertex(edge.to) {
            if matches!(
                v.kind,
                super::graph::ResourceVertexKind::GroupGate
            ) {
                c.q5_alignment = OrderedF64(c.q5_alignment.0 - 10.0);
            }
        }
    }

    c
}

fn axis_overlap_len(
    a: Point,
    b: Point,
    horizontal: bool,
    occupied: &[(f64, f64, f64, f64)],
    hit_count: &mut u32,
) -> f64 {
    const PROX: f64 = 4.0;
    let mut total = 0.0;
    for &(x1, y1, x2, y2) in occupied {
        let oh = (y1 - y2).abs() < 0.5;
        if oh != horizontal {
            continue;
        }
        if horizontal {
            if (a.y - y1).abs() > PROX {
                continue;
            }
            let t0 = a.x.min(b.x);
            let t1 = a.x.max(b.x);
            let u0 = x1.min(x2);
            let u1 = x1.max(x2);
            let o = t1.min(u1) - t0.max(u0);
            if o > 0.0 {
                total += o;
                *hit_count = hit_count.saturating_add(1);
            }
        } else {
            if (a.x - x1).abs() > PROX {
                continue;
            }
            let t0 = a.y.min(b.y);
            let t1 = a.y.max(b.y);
            let u0 = y1.min(y2);
            let u1 = y1.max(y2);
            let o = t1.min(u1) - t0.max(u0);
            if o > 0.0 {
                total += o;
                *hit_count = hit_count.saturating_add(1);
            }
        }
    }
    total
}

fn reconstruct(
    graph: &ResourceGraph,
    best: &HashMap<ResourceVertexId, (LexCost, Option<ResourceVertexId>, Option<bool>)>,
    start: ResourceVertexId,
    goal: ResourceVertexId,
) -> Vec<Point> {
    let mut chain = Vec::new();
    let mut cur = goal;
    chain.push(cur);
    while cur != start {
        let Some((_, Some(parent), _)) = best.get(&cur) else {
            break;
        };
        cur = *parent;
        chain.push(cur);
    }
    chain.reverse();
    let mut pts: Vec<Point> = chain
        .iter()
        .filter_map(|id| graph.vertex(*id).map(|v| v.position))
        .collect();
    // 压缩共线点
    simplify_orthogonal(&mut pts);
    pts
}

fn simplify_orthogonal(pts: &mut Vec<Point>) {
    if pts.len() < 3 {
        return;
    }
    let mut out = vec![pts[0]];
    for i in 1..pts.len() - 1 {
        let a = out[out.len() - 1];
        let b = pts[i];
        let c = pts[i + 1];
        let colinear = ((a.x - b.x).abs() < 1e-6 && (b.x - c.x).abs() < 1e-6)
            || ((a.y - b.y).abs() < 1e-6 && (b.y - c.y).abs() < 1e-6);
        if !colinear {
            out.push(b);
        }
    }
    out.push(*pts.last().unwrap());
    *pts = out;
}

fn orthogonal_fallback(start: Point, end: Point) -> Vec<Point> {
    if (start.x - end.x).abs() < 1e-6 || (start.y - end.y).abs() < 1e-6 {
        return vec![start, end];
    }
    // L 形：先水平后垂直
    vec![start, Point::new(end.x, start.y), end]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::geometry::Rect;
    use crate::layout::types::Port;
    use crate::layout::kernel::route::graph::ResourceGraph;

    #[test]
    fn lex_astar_finds_path_around_obstacle() {
        let from = Point::new(10.0, 50.0);
        let to = Point::new(150.0, 50.0);
        // 中间障碍挡住直线
        let obstacles = [Rect::new(60.0, 20.0, 40.0, 60.0)];
        let (g, s, e) = ResourceGraph::build_for_query(
            from,
            to,
            Port::Right,
            Port::Left,
            "a",
            "b",
            &obstacles,
            &[],
            None,
        );
        let result = lex_astar(&g, s, e, false, &[]);
        assert_eq!(result.status, SolverStatus::Converged);
        assert!(result.points.len() >= 2);
        assert_eq!(result.points[0], from);
        let last = *result.points.last().unwrap();
        assert!((last.x - to.x).abs() < 1.0 && (last.y - to.y).abs() < 1.0);
        // 不应穿过障碍内部
        for w in result.points.windows(2) {
            assert!(
                obstacles[0].segment_interior_overlap_length(w[0], w[1], 0.1) <= 0.0,
                "segment {:?}→{:?} crosses obstacle",
                w[0],
                w[1]
            );
        }
    }

    #[test]
    fn lex_astar_deterministic() {
        let from = Point::new(0.0, 0.0);
        let to = Point::new(80.0, 60.0);
        let (g, s, e) = ResourceGraph::build_for_query(
            from,
            to,
            Port::Bottom,
            Port::Top,
            "a",
            "b",
            &[],
            &[],
            None,
        );
        let r1 = lex_astar(&g, s, e, false, &[]);
        let r2 = lex_astar(&g, s, e, false, &[]);
        assert_eq!(r1.points, r2.points);
        assert_eq!(r1.cost, r2.cost);
    }
}
