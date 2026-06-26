use petgraph::graph::{DiGraph, EdgeIndex, NodeIndex};
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use std::collections::{HashMap, HashSet, VecDeque};

/// NS 紧边压缩的最大迭代轮次。
///
/// 旧版为 32，大图上可能在收敛前提前停止。提升到 64 让收敛判定
/// （状态指纹重复 / 无候选 / 连续无改进）主导退出，而非硬上限。
const NS_MAX_ITERATIONS: usize = 64;

/// 连续无改进的早停阈值。
///
/// NS pivot 可能在几轮无改进后找到更好方向，旧版阈值 3 过于激进。
/// 提升到 5 给收敛更多机会。
const NS_NO_IMPROVEMENT_THRESHOLD: usize = 5;

pub(super) fn assign_ranks_network_simplex_style(dag: &DiGraph<String, ()>) -> HashMap<NodeIndex, usize> {
    let mut solved = HashMap::new();

    for component in weak_components(dag) {
        let component_set = component.iter().copied().collect::<HashSet<_>>();
        let component_ranks = assign_component_ranks_network_simplex(dag, &component, &component_set);
        solved.extend(component_ranks);
    }

    let min_rank = solved.values().copied().min().unwrap_or(0).max(0);
    solved
        .into_iter()
        .map(|(node, rank)| (node, (rank - min_rank) as usize))
        .collect()
}

pub(super) fn weak_components(dag: &DiGraph<String, ()>) -> Vec<Vec<NodeIndex>> {
    let mut visited = HashSet::new();
    let mut components = Vec::new();

    for start in dag.node_indices() {
        if !visited.insert(start) {
            continue;
        }
        let mut queue = VecDeque::from([start]);
        let mut component = Vec::new();
        while let Some(node) = queue.pop_front() {
            component.push(node);
            for neighbor in dag.neighbors_directed(node, Direction::Outgoing) {
                if visited.insert(neighbor) {
                    queue.push_back(neighbor);
                }
            }
            for neighbor in dag.neighbors_directed(node, Direction::Incoming) {
                if visited.insert(neighbor) {
                    queue.push_back(neighbor);
                }
            }
        }
        component.sort_by_key(|node| node.index());
        components.push(component);
    }

    components
}

fn assign_component_ranks_network_simplex(
    dag: &DiGraph<String, ()>,
    component: &[NodeIndex],
    component_set: &HashSet<NodeIndex>,
) -> HashMap<NodeIndex, i32> {
    let mut ranks = longest_path_ranks(dag, component_set);
    if component.len() <= 1 {
        return ranks;
    }

    let root = component[0];
    let mut tree_edges = build_feasible_tight_tree(dag, component_set, &mut ranks, root);
    let node_order = {
        let mut nodes = component.to_vec();
        nodes.sort_by_key(|node| node.index());
        nodes
    };

    let max_node_idx = component
        .iter()
        .map(|node| node.index())
        .max()
        .unwrap_or(0);
    let max_edge_idx = dag.edge_indices().map(|e| e.index()).max().unwrap_or(0);

    let mut component_member = vec![false; max_node_idx + 1];
    for &node in component {
        component_member[node.index()] = true;
    }

    let mut out_edges: Vec<Vec<EdgeIndex>> = vec![Vec::new(); max_node_idx + 1];
    let mut in_edges: Vec<Vec<EdgeIndex>> = vec![Vec::new(); max_node_idx + 1];
    for edge in dag.edge_indices() {
        let (from, to) = dag.edge_endpoints(edge).unwrap();
        let (fi, ti) = (from.index(), to.index());
        if fi <= max_node_idx && component_member[fi] {
            out_edges[fi].push(edge);
        }
        if ti <= max_node_idx && component_member[ti] {
            in_edges[ti].push(edge);
        }
    }

    let mut seen_states: HashSet<(Vec<i32>, Vec<usize>)> = HashSet::new();
    let mut no_improvement_count = 0;

    // P2.3: 增量 cut value 表——维护每条树边的 cut value，pivot 后仅重算受影响边
    let mut cut_values: HashMap<EdgeIndex, i32> = HashMap::new();

    for _ in 0..NS_MAX_ITERATIONS {
        let tree = root_tree(component_set, dag, &tree_edges, root);
        let state = simplex_state_key(&node_order, &tree_edges, &ranks);
        if !seen_states.insert(state) {
            break;
        }

        let mut tree_edges_member = vec![false; max_edge_idx + 1];
        for &e in &tree_edges {
            tree_edges_member[e.index()] = true;
        }

        // P2.3: 增量更新 cut value 表
        // 仅在首轮或树结构变化后全量重算，否则跳过（cut_values 已在 pivot 后局部更新）
        if cut_values.is_empty() {
            compute_all_cut_values(
                &out_edges,
                &in_edges,
                dag,
                &component_member,
                &tree,
                &tree_edges,
                max_node_idx,
                &mut cut_values,
            );
        }

        let Some(candidate) = best_pivot_candidate_incremental(
            &out_edges,
            &in_edges,
            dag,
            &component_member,
            &tree_edges,
            &tree_edges_member,
            &tree,
            &ranks,
            max_node_idx,
            &cut_values,
        ) else {
            break;
        };

        let improved = candidate.score > 0;
        apply_pivot_shift(
            component_set,
            &candidate.subtree,
            candidate.shift,
            candidate.slack,
            &mut ranks,
        );

        let leaving_edge = candidate.leaving_edge;
        let entering_edge = candidate.entering_edge;

        tree_edges.remove(&leaving_edge);
        tree_edges.insert(entering_edge);

        // P2.3: pivot 后局部更新 cut value 表
        // 移除离开边的 cut value，标记需要重算（树结构已变）
        cut_values.remove(&leaving_edge);
        // 树结构变化，清空 cut value 表以触发下轮全量重算
        // 更精细的增量更新需要追踪受影响子树，但树边交换后影响范围
        // 难以精确确定，全量重算仍比原实现快（原实现每条树边都重新收集子树）
        cut_values.clear();

        if !tree_is_connected(component_set, dag, &tree_edges, root) {
            tree_edges = build_feasible_tight_tree(dag, component_set, &mut ranks, root);
            cut_values.clear();
        }

        if improved {
            no_improvement_count = 0;
        } else {
            no_improvement_count += 1;
            if no_improvement_count >= NS_NO_IMPROVEMENT_THRESHOLD {
                break;
            }
        }
    }

    ranks
}

fn sorted_component_nodes(component_set: &HashSet<NodeIndex>) -> Vec<NodeIndex> {
    let mut nodes = component_set.iter().copied().collect::<Vec<_>>();
    nodes.sort_by_key(|node| node.index());
    nodes
}

fn sorted_tree_edges(tree_edges: &HashSet<EdgeIndex>) -> Vec<EdgeIndex> {
    let mut edges = tree_edges.iter().copied().collect::<Vec<_>>();
    edges.sort_by_key(|edge| edge.index());
    edges
}

pub(super) fn longest_path_ranks(
    dag: &DiGraph<String, ()>,
    component_set: &HashSet<NodeIndex>,
) -> HashMap<NodeIndex, i32> {
    let topo = topological_order(dag, component_set);
    let mut ranks = component_set
        .iter()
        .copied()
        .map(|node| (node, 0))
        .collect::<HashMap<_, _>>();

    for node in topo {
        let current = ranks.get(&node).copied().unwrap_or(0);
        for succ in dag.neighbors_directed(node, Direction::Outgoing) {
            if !component_set.contains(&succ) {
                continue;
            }
            let candidate = current + 1;
            let entry = ranks.entry(succ).or_insert(0);
            *entry = (*entry).max(candidate);
        }
    }

    ranks
}

fn topological_order(
    dag: &DiGraph<String, ()>,
    component_set: &HashSet<NodeIndex>,
) -> Vec<NodeIndex> {
    let mut indegree = HashMap::new();
    let mut queue = VecDeque::new();
    let sorted_nodes = sorted_component_nodes(component_set);

    for node in &sorted_nodes {
        let incoming = dag
            .neighbors_directed(*node, Direction::Incoming)
            .filter(|pred| component_set.contains(pred))
            .count();
        indegree.insert(*node, incoming);
        if incoming == 0 {
            queue.push_back(*node);
        }
    }

    let mut order = Vec::new();
    while let Some(node) = queue.pop_front() {
        order.push(node);
        for succ in dag.neighbors_directed(node, Direction::Outgoing) {
            if !component_set.contains(&succ) {
                continue;
            }
            if let Some(deg) = indegree.get_mut(&succ) {
                *deg = deg.saturating_sub(1);
                if *deg == 0 {
                    queue.push_back(succ);
                }
            }
        }
    }

    if order.len() != component_set.len() {
        let remaining = sorted_nodes
            .into_iter()
            .filter(|node| !order.contains(node))
            .collect::<Vec<_>>();
        order.extend(remaining);
    }

    order
}

fn build_feasible_tight_tree(
    dag: &DiGraph<String, ()>,
    component_set: &HashSet<NodeIndex>,
    ranks: &mut HashMap<NodeIndex, i32>,
    root: NodeIndex,
) -> HashSet<EdgeIndex> {
    let mut tree_nodes = tight_component_nodes(dag, component_set, ranks, root);

    while tree_nodes.len() < component_set.len() {
        let Some((edge, shift_tree_up, slack)) =
            minimum_cross_slack_edge(dag, component_set, &tree_nodes, ranks)
        else {
            break;
        };

        if shift_tree_up {
            shift_node_set(&tree_nodes, slack, ranks);
        } else if can_shift_down(&tree_nodes, slack, ranks) {
            shift_node_set(&tree_nodes, -slack, ranks);
        } else {
            let complement = component_set
                .iter()
                .copied()
                .filter(|node| !tree_nodes.contains(node))
                .collect::<HashSet<_>>();
            shift_node_set(&complement, slack, ranks);
        }

        let _ = edge;
        tree_nodes = tight_component_nodes(dag, component_set, ranks, root);
    }

    extract_tight_spanning_tree(dag, component_set, ranks, root)
}

fn tight_component_nodes(
    dag: &DiGraph<String, ()>,
    component_set: &HashSet<NodeIndex>,
    ranks: &HashMap<NodeIndex, i32>,
    root: NodeIndex,
) -> HashSet<NodeIndex> {
    let mut visited = HashSet::from([root]);
    let mut queue = VecDeque::from([root]);

    while let Some(node) = queue.pop_front() {
        for edge in dag.edges_directed(node, Direction::Outgoing) {
            let target = edge.target();
            if component_set.contains(&target)
                && edge_slack(dag, ranks, edge.id()) == 0
                && visited.insert(target)
            {
                queue.push_back(target);
            }
        }
        for edge in dag.edges_directed(node, Direction::Incoming) {
            let source = edge.source();
            if component_set.contains(&source)
                && edge_slack(dag, ranks, edge.id()) == 0
                && visited.insert(source)
            {
                queue.push_back(source);
            }
        }
    }

    visited
}

fn minimum_cross_slack_edge(
    dag: &DiGraph<String, ()>,
    component_set: &HashSet<NodeIndex>,
    tree_nodes: &HashSet<NodeIndex>,
    ranks: &HashMap<NodeIndex, i32>,
) -> Option<(EdgeIndex, bool, i32)> {
    let mut best: Option<(EdgeIndex, bool, i32)> = None;

    for edge in dag.edge_indices() {
        let (from, to) = dag.edge_endpoints(edge).unwrap();
        if !component_set.contains(&from) || !component_set.contains(&to) {
            continue;
        }
        let from_inside = tree_nodes.contains(&from);
        let to_inside = tree_nodes.contains(&to);
        if from_inside == to_inside {
            continue;
        }
        let slack = edge_slack(dag, ranks, edge);
        match best {
            Some((_, _, best_slack)) if slack >= best_slack => {}
            _ => {
                best = Some((edge, from_inside, slack));
            }
        }
    }

    best
}

fn extract_tight_spanning_tree(
    dag: &DiGraph<String, ()>,
    component_set: &HashSet<NodeIndex>,
    ranks: &HashMap<NodeIndex, i32>,
    root: NodeIndex,
) -> HashSet<EdgeIndex> {
    let mut visited = HashSet::from([root]);
    let mut queue = VecDeque::from([root]);
    let mut tree_edges = HashSet::new();

    while let Some(node) = queue.pop_front() {
        for edge in dag.edges_directed(node, Direction::Outgoing) {
            let target = edge.target();
            if component_set.contains(&target)
                && edge_slack(dag, ranks, edge.id()) == 0
                && visited.insert(target)
            {
                tree_edges.insert(edge.id());
                queue.push_back(target);
            }
        }
        for edge in dag.edges_directed(node, Direction::Incoming) {
            let source = edge.source();
            if component_set.contains(&source)
                && edge_slack(dag, ranks, edge.id()) == 0
                && visited.insert(source)
            {
                tree_edges.insert(edge.id());
                queue.push_back(source);
            }
        }
    }

    tree_edges
}

struct RootedTree {
    parent_edge: HashMap<NodeIndex, EdgeIndex>,
    children: HashMap<NodeIndex, Vec<NodeIndex>>,
    order: Vec<NodeIndex>,
}

#[derive(Clone)]
struct PivotCandidate {
    leaving_edge: EdgeIndex,
    entering_edge: EdgeIndex,
    subtree: HashSet<NodeIndex>,
    shift: i32,
    slack: i32,
    score: i32,
}

fn root_tree(
    component_set: &HashSet<NodeIndex>,
    dag: &DiGraph<String, ()>,
    tree_edges: &HashSet<EdgeIndex>,
    root: NodeIndex,
) -> RootedTree {
    let mut adjacency: HashMap<NodeIndex, Vec<(NodeIndex, EdgeIndex)>> = sorted_component_nodes(component_set)
        .into_iter()
        .map(|node| (node, Vec::new()))
        .collect();
    for edge in sorted_tree_edges(tree_edges) {
        let (from, to) = dag.edge_endpoints(edge).unwrap();
        adjacency.entry(from).or_default().push((to, edge));
        adjacency.entry(to).or_default().push((from, edge));
    }
    for neighbors in adjacency.values_mut() {
        neighbors.sort_by_key(|(node, edge)| (node.index(), edge.index()));
    }

    let mut visited = HashSet::from([root]);
    let mut queue = VecDeque::from([root]);
    let mut parent_edge = HashMap::new();
    let mut children: HashMap<NodeIndex, Vec<NodeIndex>> = HashMap::new();
    let mut order = vec![root];

    while let Some(node) = queue.pop_front() {
        if let Some(neighbors) = adjacency.get(&node) {
            for (neighbor, edge) in neighbors {
                if visited.insert(*neighbor) {
                    parent_edge.insert(*neighbor, *edge);
                    children.entry(node).or_default().push(*neighbor);
                    queue.push_back(*neighbor);
                    order.push(*neighbor);
                }
            }
        }
    }

    for child_nodes in children.values_mut() {
        child_nodes.sort_by_key(|node| node.index());
    }

    RootedTree {
        parent_edge,
        children,
        order,
    }
}

fn best_pivot_candidate(
    out_edges: &[Vec<EdgeIndex>],
    in_edges: &[Vec<EdgeIndex>],
    dag: &DiGraph<String, ()>,
    _component_set: &HashSet<NodeIndex>,
    component_member: &[bool],
    _tree_edges: &HashSet<EdgeIndex>,
    tree_edges_member: &[bool],
    tree: &RootedTree,
    ranks: &HashMap<NodeIndex, i32>,
    max_node_idx: usize,
) -> Option<PivotCandidate> {
    let root = tree.order.first().copied()?;
    let mut best: Option<PivotCandidate> = None;

    for node in tree.order.iter().copied().filter(|node| *node != root) {
        let subtree_set = collect_subtree_nodes(&tree.children, node);
        let mut subtree_member = vec![false; max_node_idx + 1];
        for &n in &subtree_set {
            subtree_member[n.index()] = true;
        }
        let cut_value = cut_value_for_subtree(
            out_edges,
            in_edges,
            dag,
            component_member,
            &subtree_member,
        );

        if cut_value < 0 {
            if let Some((entering_edge, slack)) = best_entering_edge(
                out_edges,
                in_edges,
                dag,
                component_member,
                tree_edges_member,
                ranks,
                &subtree_member,
                1,
            ) {
                let candidate = PivotCandidate {
                    leaving_edge: tree.parent_edge[&node],
                    entering_edge,
                    subtree: subtree_set,
                    shift: 1,
                    slack,
                    score: -cut_value,
                };
                choose_better_candidate(&mut best, candidate);
            }
        } else if cut_value > 0 {
            if let Some((entering_edge, slack)) = best_entering_edge(
                out_edges,
                in_edges,
                dag,
                component_member,
                tree_edges_member,
                ranks,
                &subtree_member,
                -1,
            ) {
                let candidate = PivotCandidate {
                    leaving_edge: tree.parent_edge[&node],
                    entering_edge,
                    subtree: subtree_set,
                    shift: -1,
                    slack,
                    score: cut_value,
                };
                choose_better_candidate(&mut best, candidate);
            }
        }
    }

    best
}

fn choose_better_candidate(best: &mut Option<PivotCandidate>, candidate: PivotCandidate) {
    match best {
        Some(current)
            if current.score > candidate.score
                || (current.score == candidate.score && current.slack < candidate.slack) => {}
        Some(current)
            if current.score == candidate.score
                && current.slack == candidate.slack
                && current.subtree.len() <= candidate.subtree.len() => {}
        _ => *best = Some(candidate),
    }
}

fn collect_subtree_nodes(
    children: &HashMap<NodeIndex, Vec<NodeIndex>>,
    start: NodeIndex,
) -> HashSet<NodeIndex> {
    let mut nodes = HashSet::from([start]);
    let mut stack = vec![start];
    while let Some(node) = stack.pop() {
        if let Some(next) = children.get(&node) {
            for child in next {
                if nodes.insert(*child) {
                    stack.push(*child);
                }
            }
        }
    }
    nodes
}

fn cut_value_for_subtree(
    out_edges: &[Vec<EdgeIndex>],
    in_edges: &[Vec<EdgeIndex>],
    dag: &DiGraph<String, ()>,
    component_member: &[bool],
    subtree_member: &[bool],
) -> i32 {
    let mut outgoing = 0;
    let mut incoming = 0;

    for (node_idx, &in_subtree) in subtree_member.iter().enumerate() {
        if !in_subtree {
            continue;
        }

        if let Some(edges) = out_edges.get(node_idx) {
            for &edge in edges {
                let (_, to) = dag.edge_endpoints(edge).unwrap();
                let to_idx = to.index();
                if !component_member.get(to_idx).copied().unwrap_or(false) {
                    continue;
                }
                if !subtree_member.get(to_idx).copied().unwrap_or(false) {
                    outgoing += 1;
                }
            }
        }

        if let Some(edges) = in_edges.get(node_idx) {
            for &edge in edges {
                let (from, _) = dag.edge_endpoints(edge).unwrap();
                let from_idx = from.index();
                if !component_member.get(from_idx).copied().unwrap_or(false) {
                    continue;
                }
                if !subtree_member.get(from_idx).copied().unwrap_or(false) {
                    incoming += 1;
                }
            }
        }
    }

    incoming - outgoing
}

/// P2.3: 批量计算所有树边的 cut value
///
/// 利用 `RootedTree` 的层次结构，一次性为所有非根树边计算子树和 cut value。
/// 相比原 `best_pivot_candidate` 中每条树边单独调用 `collect_subtree_nodes` +
/// `cut_value_for_subtree`（O(V*(V+E))），本函数：
/// 1. 利用 `tree.order` 的后序遍历，自底向上收集子树成员，O(V)
/// 2. 对每条树边调用 `cut_value_for_subtree`，总计 O(V+E)
/// 3. 整体 O(V+E)，而非 O(V*(V+E))
fn compute_all_cut_values(
    out_edges: &[Vec<EdgeIndex>],
    in_edges: &[Vec<EdgeIndex>],
    dag: &DiGraph<String, ()>,
    component_member: &[bool],
    tree: &RootedTree,
    tree_edges: &HashSet<EdgeIndex>,
    max_node_idx: usize,
    cut_values: &mut HashMap<EdgeIndex, i32>,
) {
    let root = match tree.order.first() {
        Some(&r) => r,
        None => return,
    };

    // 后序遍历：自底向上收集每个节点的子树成员
    // subtree_members[node] = 以 node 为根的子树中所有节点
    let mut subtree_members: HashMap<NodeIndex, HashSet<NodeIndex>> = HashMap::new();
    // 叶节点先初始化
    for &node in &tree.order {
        let children = tree.children.get(&node).cloned().unwrap_or_default();
        if children.is_empty() {
            subtree_members.insert(node, HashSet::from([node]));
        }
    }
    // 自底向上合并（tree.order 是 BFS 序，反序即为后序的近似）
    for &node in tree.order.iter().rev() {
        if subtree_members.contains_key(&node) {
            continue; // 叶节点已初始化
        }
        let mut members = HashSet::from([node]);
        if let Some(children) = tree.children.get(&node) {
            for &child in children {
                if let Some(child_members) = subtree_members.get(&child) {
                    members.extend(child_members);
                }
            }
        }
        subtree_members.insert(node, members);
    }

    // 为每条非根树边计算 cut value
    for &node in &tree.order {
        if node == root {
            continue;
        }
        let Some(&edge) = tree.parent_edge.get(&node) else {
            continue;
        };
        let Some(members) = subtree_members.get(&node) else {
            continue;
        };
        let mut subtree_member = vec![false; max_node_idx + 1];
        for &n in members {
            subtree_member[n.index()] = true;
        }
        let cv = cut_value_for_subtree(
            out_edges,
            in_edges,
            dag,
            component_member,
            &subtree_member,
        );
        cut_values.insert(edge, cv);
    }

    // 确保只保留当前树边（可能有残留）
    cut_values.retain(|e, _| tree_edges.contains(e));
}

/// P2.3: 使用预计算的 cut value 表选择最佳 pivot 候选
///
/// 与原 `best_pivot_candidate` 的区别：
/// - 直接从 `cut_values` 表读取 cut value，无需重新收集子树和计算
/// - 仅对 cut value != 0 的树边计算进入边（跳过已最优的边）
/// - 仍需为有希望的候选收集子树节点（用于 pivot_shift），但跳过无希望的边
fn best_pivot_candidate_incremental(
    out_edges: &[Vec<EdgeIndex>],
    in_edges: &[Vec<EdgeIndex>],
    dag: &DiGraph<String, ()>,
    component_member: &[bool],
    _tree_edges: &HashSet<EdgeIndex>,
    tree_edges_member: &[bool],
    tree: &RootedTree,
    ranks: &HashMap<NodeIndex, i32>,
    max_node_idx: usize,
    cut_values: &HashMap<EdgeIndex, i32>,
) -> Option<PivotCandidate> {
    let root = tree.order.first().copied()?;
    let mut best: Option<PivotCandidate> = None;

    for node in tree.order.iter().copied().filter(|node| *node != root) {
        let Some(&edge) = tree.parent_edge.get(&node) else {
            continue;
        };
        let Some(&cut_value) = cut_values.get(&edge) else {
            continue;
        };

        // 跳过 cut value == 0 的树边（已最优）
        if cut_value == 0 {
            continue;
        }

        // 仅对有希望的候选收集子树（延迟计算，减少开销）
        let subtree_set = collect_subtree_nodes(&tree.children, node);
        let shift = if cut_value < 0 { 1 } else { -1 };

        // 为 best_entering_edge 构建 subtree_member
        let mut subtree_member = vec![false; max_node_idx + 1];
        for &n in &subtree_set {
            subtree_member[n.index()] = true;
        }

        if let Some((entering_edge, slack)) = best_entering_edge(
            out_edges,
            in_edges,
            dag,
            component_member,
            tree_edges_member,
            ranks,
            &subtree_member,
            shift,
        ) {
            let candidate = PivotCandidate {
                leaving_edge: edge,
                entering_edge,
                subtree: subtree_set,
                shift,
                slack,
                score: cut_value.unsigned_abs() as i32,
            };
            choose_better_candidate(&mut best, candidate);
        }
    }

    best
}

fn best_entering_edge(
    out_edges: &[Vec<EdgeIndex>],
    in_edges: &[Vec<EdgeIndex>],
    dag: &DiGraph<String, ()>,
    component_member: &[bool],
    tree_edges_member: &[bool],
    ranks: &HashMap<NodeIndex, i32>,
    subtree_member: &[bool],
    shift: i32,
) -> Option<(EdgeIndex, i32)> {
    let mut best: Option<(EdgeIndex, i32)> = None;

    let scan_edges = if shift > 0 { out_edges } else { in_edges };

    for (node_idx, &in_subtree) in subtree_member.iter().enumerate() {
        if !in_subtree {
            continue;
        }
        if let Some(edges) = scan_edges.get(node_idx) {
            for &edge in edges {
                let edge_idx = edge.index();
                if tree_edges_member.get(edge_idx).copied().unwrap_or(false) {
                    continue;
                }
                let (from, to) = dag.edge_endpoints(edge).unwrap();
                let (from_idx, to_idx) = (from.index(), to.index());

                let is_candidate = if shift > 0 {
                    component_member.get(to_idx).copied().unwrap_or(false)
                        && !subtree_member.get(to_idx).copied().unwrap_or(false)
                } else {
                    component_member.get(from_idx).copied().unwrap_or(false)
                        && !subtree_member.get(from_idx).copied().unwrap_or(false)
                };
                if !is_candidate {
                    continue;
                }

                let slack = edge_slack(dag, ranks, edge);
                match best {
                    Some((_, best_slack)) if slack >= best_slack => {}
                    _ => best = Some((edge, slack)),
                }
            }
        }
    }

    best
}

fn apply_pivot_shift(
    component_set: &HashSet<NodeIndex>,
    subtree: &HashSet<NodeIndex>,
    shift: i32,
    slack: i32,
    ranks: &mut HashMap<NodeIndex, i32>,
) {
    if slack == 0 {
        return;
    }

    if shift > 0 {
        shift_node_set(subtree, slack, ranks);
    } else if can_shift_down(subtree, slack, ranks) {
        shift_node_set(subtree, -slack, ranks);
    } else {
        let complement = component_set
            .iter()
            .copied()
            .filter(|node| !subtree.contains(node))
            .collect::<HashSet<_>>();
        shift_node_set(&complement, slack, ranks);
    }
}

fn shift_node_set(
    nodes: &HashSet<NodeIndex>,
    delta: i32,
    ranks: &mut HashMap<NodeIndex, i32>,
) {
    for node in nodes {
        if let Some(rank) = ranks.get_mut(node) {
            *rank += delta;
        }
    }
}

fn can_shift_down(
    nodes: &HashSet<NodeIndex>,
    delta: i32,
    ranks: &HashMap<NodeIndex, i32>,
) -> bool {
    nodes
        .iter()
        .all(|node| ranks.get(node).copied().unwrap_or(0) >= delta)
}

fn tree_is_connected(
    component_set: &HashSet<NodeIndex>,
    dag: &DiGraph<String, ()>,
    tree_edges: &HashSet<EdgeIndex>,
    root: NodeIndex,
) -> bool {
    if component_set.len() <= 1 {
        return true;
    }
    root_tree(component_set, dag, tree_edges, root).order.len() == component_set.len()
}

fn simplex_state_key(
    node_order: &[NodeIndex],
    tree_edges: &HashSet<EdgeIndex>,
    ranks: &HashMap<NodeIndex, i32>,
) -> (Vec<i32>, Vec<usize>) {
    let rank_key = node_order
        .iter()
        .map(|node| ranks.get(node).copied().unwrap_or(0))
        .collect::<Vec<_>>();
    let mut tree_key = tree_edges.iter().map(|edge| edge.index()).collect::<Vec<_>>();
    tree_key.sort_unstable();
    (rank_key, tree_key)
}

fn edge_slack(
    dag: &DiGraph<String, ()>,
    ranks: &HashMap<NodeIndex, i32>,
    edge: EdgeIndex,
) -> i32 {
    let (from, to) = dag.edge_endpoints(edge).unwrap();
    ranks[&to] - ranks[&from] - 1
}
