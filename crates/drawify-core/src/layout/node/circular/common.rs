//! 圆形布局与弧形边路由的共享逻辑

use crate::types::DiagramType;
use crate::ast::{Diagram, Entity};
use crate::layout::LayoutHints;
use std::collections::{HashMap, HashSet, VecDeque};
use std::f64::consts::PI;

// CircleGroup / APPLICABLE_TYPES / infer_single_circle_from_nodes / resolve_circle_groups
// 已下沉到 edge::common::circular_support，此处 re-export 保持原路径可用。
pub use crate::layout::edge::common::circular_support::{
    APPLICABLE_TYPES, CircleGroup, infer_single_circle_from_nodes, resolve_circle_groups,
};

pub const PADDING: f64 = crate::layout::constants::CIRCULAR_PADDING;
pub const GROUP_PADDING: f64 = crate::layout::constants::DEFAULT_GROUP_PADDING;
pub const DEFAULT_NODE_WIDTH: f64 = 160.0;
pub const DEFAULT_NODE_HEIGHT: f64 = 50.0;
pub const COMPONENT_GAP: f64 = crate::layout::constants::CIRCULAR_COMPONENT_GAP;
pub const MIN_CIRCLE_RADIUS: f64 = 120.0;
pub const MAX_CIRCLE_RADIUS: f64 = 600.0;
pub const CUTPOINT_OFFSET: f64 = 40.0;

#[derive(Debug, Clone)]
pub struct CircularLayoutHints {
    pub circles: Vec<CircleGroup>,
}

impl CircularLayoutHints {
    pub fn into_layout_hints(self) -> LayoutHints {
        LayoutHints {
            circular: Some(self),
            edge_routing_style: crate::layout::EdgeRoutingStyle::Curved,
            ..Default::default()
        }
    }
}

impl From<CircularLayoutHints> for LayoutHints {
    fn from(value: CircularLayoutHints) -> Self {
        value.into_layout_hints()
    }
}

// ─── 图结构 ──────────────────────────────────────────────

pub struct SimpleGraph {
    pub adjacency: Vec<Vec<usize>>,
    pub node_count: usize,
    pub node_ids: Vec<String>,
    pub id_to_idx: HashMap<String, usize>,
}

impl SimpleGraph {
    pub fn from_diagram(diagram: &Diagram) -> Self {
        let node_ids: Vec<String> = diagram
            .entities
            .iter()
            .map(|e| e.id.as_str().to_string())
            .collect();
        let id_to_idx: HashMap<String, usize> = node_ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), i))
            .collect();
        let node_count = node_ids.len();
        let mut adjacency = vec![Vec::new(); node_count];

        for rel in &diagram.relations {
            if let (Some(&fi), Some(&ti)) = (
                id_to_idx.get(rel.from.as_str()),
                id_to_idx.get(rel.to.as_str()),
            ) {
                if fi != ti {
                    if !adjacency[fi].contains(&ti) {
                        adjacency[fi].push(ti);
                    }
                    if !adjacency[ti].contains(&fi) {
                        adjacency[ti].push(fi);
                    }
                }
            }
        }

        Self {
            adjacency,
            node_count,
            node_ids,
            id_to_idx,
        }
    }
}

struct TarjanContext {
    discovery: Vec<usize>,
    low: Vec<usize>,
    stack: Vec<(usize, usize)>,
    components: Vec<Vec<usize>>,
    time: usize,
}

pub fn find_biconnected_components(graph: &SimpleGraph) -> Vec<Vec<usize>> {
    let n = graph.node_count;
    if n == 0 {
        return vec![];
    }

    let mut ctx = TarjanContext {
        discovery: vec![0; n],
        low: vec![0; n],
        stack: Vec::new(),
        components: Vec::new(),
        time: 0,
    };

    for v in 0..n {
        if ctx.discovery[v] == 0 {
            tarjan_dfs(graph, v, v, &mut ctx);
        }
    }

    for v in 0..n {
        let mut found = false;
        for comp in &ctx.components {
            if comp.contains(&v) {
                found = true;
                break;
            }
        }
        if !found {
            ctx.components.push(vec![v]);
        }
    }

    ctx.components
}

fn tarjan_dfs(graph: &SimpleGraph, u: usize, parent: usize, ctx: &mut TarjanContext) {
    ctx.time += 1;
    ctx.discovery[u] = ctx.time;
    ctx.low[u] = ctx.time;
    let mut children = 0;

    for &v in &graph.adjacency[u] {
        if v == parent {
            continue;
        }

        if ctx.discovery[v] == 0 {
            children += 1;
            ctx.stack.push((u, v));
            tarjan_dfs(graph, v, u, ctx);
            ctx.low[u] = ctx.low[u].min(ctx.low[v]);

            if (parent == u && children > 1) || (parent != u && ctx.low[v] >= ctx.discovery[u]) {
                let mut component = HashSet::new();
                loop {
                    let Some((x, y)) = ctx.stack.pop() else { break };
                    component.insert(x);
                    component.insert(y);
                    if (x == u && y == v) || (x == v && y == u) {
                        break;
                    }
                }
                if !component.is_empty() {
                    // 排序保证 BCC 内节点顺序确定（HashSet 迭代顺序随机）
                    let mut comp_vec: Vec<usize> = component.into_iter().collect();
                    comp_vec.sort();
                    ctx.components.push(comp_vec);
                }
            }
        } else if ctx.discovery[v] < ctx.discovery[u] {
            ctx.stack.push((u, v));
            ctx.low[u] = ctx.low[u].min(ctx.discovery[v]);
        }
    }
}

pub fn find_articulation_points(graph: &SimpleGraph) -> HashSet<usize> {
    let n = graph.node_count;
    if n == 0 {
        return HashSet::new();
    }

    let mut discovery = vec![0usize; n];
    let mut low = vec![0usize; n];
    let mut articulation = HashSet::new();
    let mut time = 0;

    fn dfs(
        graph: &SimpleGraph,
        u: usize,
        parent: usize,
        discovery: &mut [usize],
        low: &mut [usize],
        articulation: &mut HashSet<usize>,
        time: &mut usize,
    ) {
        *time += 1;
        discovery[u] = *time;
        low[u] = *time;
        let mut children = 0;

        for &v in &graph.adjacency[u] {
            if v == parent {
                continue;
            }
            if discovery[v] == 0 {
                children += 1;
                dfs(graph, v, u, discovery, low, articulation, time);
                low[u] = low[u].min(low[v]);
                if (parent == u && children > 1)
                    || (parent != u && low[v] >= discovery[u])
                {
                    articulation.insert(u);
                }
            } else {
                low[u] = low[u].min(discovery[v]);
            }
        }
    }

    for v in 0..n {
        if discovery[v] == 0 {
            dfs(graph, v, usize::MAX, &mut discovery, &mut low, &mut articulation, &mut time);
        }
    }

    articulation
}

/// 是否使用多圆模式：
/// - 多个含 ≥2 节点的双连通分量，或
/// - 多个连通分量且至少两个分量各有 ≥2 个节点
pub fn should_use_multi_ring(graph: &SimpleGraph, bccs: &[Vec<usize>]) -> bool {
    if bccs.iter().filter(|bcc| bcc.len() >= 2).count() > 1 {
        return true;
    }
    connected_component_sizes(graph)
        .iter()
        .filter(|&&size| size >= 2)
        .count()
        > 1
}

pub fn connected_component_sizes(graph: &SimpleGraph) -> Vec<usize> {
    let n = graph.node_count;
    if n == 0 {
        return vec![];
    }
    let mut visited = vec![false; n];
    let mut sizes = Vec::new();
    for start in 0..n {
        if visited[start] {
            continue;
        }
        let mut size = 0usize;
        let mut stack = vec![start];
        visited[start] = true;
        while let Some(u) = stack.pop() {
            size += 1;
            for &v in &graph.adjacency[u] {
                if !visited[v] {
                    visited[v] = true;
                    stack.push(v);
                }
            }
        }
        sizes.push(size);
    }
    sizes
}

/// 规划多圆模式下每个圆包含的节点列表
pub fn plan_multi_ring_groups(graph: &SimpleGraph, bccs: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let sig_comps: Vec<Vec<usize>> = connected_component_node_lists(graph)
        .into_iter()
        .filter(|c| c.len() >= 2)
        .collect();
    if sig_comps.len() > 1 {
        return sig_comps;
    }

    bccs.iter().filter(|bcc| bcc.len() >= 2).cloned().collect()
}

pub fn connected_component_node_lists(graph: &SimpleGraph) -> Vec<Vec<usize>> {
    let n = graph.node_count;
    if n == 0 {
        return vec![];
    }
    let mut visited = vec![false; n];
    let mut components = Vec::new();
    for start in 0..n {
        if visited[start] {
            continue;
        }
        let mut comp = Vec::new();
        let mut stack = vec![start];
        visited[start] = true;
        while let Some(u) = stack.pop() {
            comp.push(u);
            for &v in &graph.adjacency[u] {
                if !visited[v] {
                    visited[v] = true;
                    stack.push(v);
                }
            }
        }
        components.push(comp);
    }
    components
}

// ─── 节点尺寸与排序 ──────────────────────────────────────

pub fn entity_type_name(_diagram: &Diagram, entity: &Entity) -> String {
    entity
        .attributes
        .standard
        .get("type")
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| "state".to_string())
}

pub fn node_size_for(diagram: &Diagram, entity: &Entity) -> (f64, f64) {
    if diagram.diagram_type != DiagramType::State {
        return crate::layout::styled_node_size(entity, DEFAULT_NODE_WIDTH, DEFAULT_NODE_HEIGHT);
    }

    let (w, h) = match entity_type_name(diagram, entity).as_str() {
        "initial" => (28.0, 28.0),
        "final" => (36.0, 36.0),
        "choice" => {
            let chars = entity.label.chars().count() as f64;
            let side = (chars * 13.0 + 48.0).clamp(72.0, 120.0);
            (side, side * 0.72)
        }
        _ => {
            let chars = entity.label.chars().count() as f64;
            let w = (chars * 14.0 + 36.0).clamp(80.0, 200.0);
            (w, 44.0)
        }
    };

    crate::layout::styled_node_size(entity, w, h)
}

pub fn order_entities_on_circle(diagram: &Diagram) -> Vec<usize> {
    let n = diagram.entities.len();
    if n <= 1 {
        return (0..n).collect();
    }

    let id_to_idx: HashMap<&str, usize> = diagram
        .entities
        .iter()
        .enumerate()
        .map(|(i, e)| (e.id.as_str(), i))
        .collect();

    let mut adj: Vec<Vec<usize>> = vec![vec![]; n];
    for rel in &diagram.relations {
        if let (Some(&from), Some(&to)) = (
            id_to_idx.get(rel.from.as_str()),
            id_to_idx.get(rel.to.as_str()),
        ) {
            if from != to && !adj[from].contains(&to) {
                adj[from].push(to);
            }
        }
    }

    let start = diagram
        .entities
        .iter()
        .position(|e| entity_type_name(diagram, e) == "initial")
        .or_else(|| {
            let mut indegree = vec![0usize; n];
            for rel in &diagram.relations {
                if let (Some(&from), Some(&to)) = (
                    id_to_idx.get(rel.from.as_str()),
                    id_to_idx.get(rel.to.as_str()),
                ) {
                    if from != to {
                        indegree[to] += 1;
                    }
                }
            }
            indegree.iter().position(|&d| d == 0)
        })
        .unwrap_or(0);

    let mut order = Vec::with_capacity(n);
    let mut visited = vec![false; n];
    let mut queue = VecDeque::new();
    queue.push_back(start);
    visited[start] = true;

    while let Some(u) = queue.pop_front() {
        order.push(u);
        let mut neighbors = adj[u].clone();
        neighbors.sort_unstable();
        for v in neighbors {
            if !visited[v] {
                visited[v] = true;
                queue.push_back(v);
            }
        }
    }

    for i in 0..n {
        if !visited[i] {
            order.push(i);
        }
    }

    optimize_rotation(&order, diagram, &id_to_idx)
}

fn optimize_rotation(
    order: &[usize],
    diagram: &Diagram,
    id_to_idx: &HashMap<&str, usize>,
) -> Vec<usize> {
    let n = order.len();
    if n <= 2 {
        return order.to_vec();
    }

    let mut best = order.to_vec();
    let mut best_score = rotation_score(&best, diagram, id_to_idx);

    for rot in 1..n {
        let rotated: Vec<usize> = order.iter().cycle().skip(rot).take(n).copied().collect();
        let score = rotation_score(&rotated, diagram, id_to_idx);
        if score < best_score {
            best_score = score;
            best = rotated;
        }
    }

    best
}

fn rotation_score(order: &[usize], diagram: &Diagram, id_to_idx: &HashMap<&str, usize>) -> f64 {
    let n = order.len();
    let pos: HashMap<usize, usize> = order.iter().enumerate().map(|(i, &e)| (e, i)).collect();
    let mut total = 0.0;

    for rel in &diagram.relations {
        if let (Some(&from), Some(&to)) = (
            id_to_idx.get(rel.from.as_str()),
            id_to_idx.get(rel.to.as_str()),
        ) {
            if from == to {
                continue;
            }
            let p1 = pos[&from];
            let p2 = pos[&to];
            let forward = (p2 + n - p1) % n;
            let backward = n - forward;
            total += forward.min(backward) as f64;
        }
    }

    total
}

pub fn calculate_circle_radius(sizes: &[(f64, f64)]) -> f64 {
    let node_count = sizes.len();
    if node_count == 0 {
        return 0.0;
    }
    if node_count == 1 {
        return 80.0;
    }

    let circumference: f64 = sizes
        .iter()
        .map(|(w, h)| w.max(*h) * 1.45)
        .sum();

    (circumference / (2.0 * PI)).max(110.0)
}

pub fn circle_radius_for_bcc(
    bcc: &[usize],
    sizes: &HashMap<String, (f64, f64)>,
    graph: &SimpleGraph,
) -> f64 {
    let n = bcc.len();
    if n <= 1 {
        return MIN_CIRCLE_RADIUS * 0.5;
    }

    let circumference: f64 = bcc
        .iter()
        .map(|&idx| {
            let node_id = &graph.node_ids[idx];
            let (w, h) = sizes
                .get(node_id)
                .copied()
                .unwrap_or((DEFAULT_NODE_WIDTH, DEFAULT_NODE_HEIGHT));
            w.max(h) * 1.5
        })
        .sum();

    (circumference / (2.0 * PI)).max(MIN_CIRCLE_RADIUS)
}

pub fn reorder_bcc_by_bfs(bcc: &[usize], graph: &SimpleGraph) -> Vec<usize> {
    if bcc.len() <= 2 {
        return bcc.to_vec();
    }

    let bcc_set: HashSet<usize> = bcc.iter().copied().collect();
    let mut visited = HashSet::new();
    let mut order = Vec::with_capacity(bcc.len());
    let mut queue = VecDeque::new();

    let start = bcc[0];
    queue.push_back(start);
    visited.insert(start);

    while let Some(node) = queue.pop_front() {
        order.push(node);
        if let Some(nbrs) = graph.adjacency.get(node) {
            for &nbr in nbrs {
                if bcc_set.contains(&nbr) && visited.insert(nbr) {
                    queue.push_back(nbr);
                }
            }
        }
    }

    for &node in bcc {
        if !visited.contains(&node) {
            order.push(node);
        }
    }

    order
}

// infer_single_circle_from_nodes / resolve_circle_groups 已下沉到
// edge::common::circular_support，见文件顶部 re-export。
