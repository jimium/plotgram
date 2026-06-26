//! Group 内布局 hint：DSL `layout:` 属性与自动推断

use super::layout::acyclic::is_effective_edge;
use super::layout::rank::assign_intra_ranks;
use super::layout::types::GraphIndex;
use crate::ast::Group;
use std::collections::{HashMap, HashSet};

/// DSL 可写的 group layout hint
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupLayoutHint {
    /// 按组内拓扑自动推断
    Auto,
    /// 单层水平排列
    Horizontal,
    /// 单列竖排
    Vertical,
    /// hub 在上、子节点在下展开
    FanOut,
    /// 多源 → 单汇：sink 在下，源节点在上层水平展开
    FanIn,
    /// 规则网格排列（适用于无内部边的同质节点组）
    Grid,
}

/// 解析后的组内布局模式
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupLayoutMode {
    Horizontal,
    Vertical,
    FanOut { hub: String },
    /// sink 在下，源节点在上层水平展开
    FanIn { sink: String },
    /// 行列网格排列
    Grid,
    /// 组内 Sugiyama 分层（仅组内边）
    Sugiyama,
}

pub const VALID_GROUP_LAYOUTS: &[&str] = crate::types::attr_constants::group_layout::ALL;

/// 是否为合法的 group layout atom（含简写别名）。
///
/// 规范值见 [`crate::types::attr_constants::group_layout::ALL`]；
/// 此外接受简写别名 `h`/`v`/`fan_out`/`fanout`/`fan_in`/`fanin`（由 [`parse_layout_atom`] 归一化）。
pub fn is_valid_group_layout_atom(raw: &str) -> bool {
    let normalized = raw.trim().to_ascii_lowercase();
    if crate::types::attr_constants::group_layout::ALL.contains(&normalized.as_str()) {
        return true;
    }
    matches!(
        normalized.as_str(),
        "h" | "v" | "fan_out" | "fanout" | "fan_in" | "fanin"
    )
}

/// 从 group 标准属性读取 `layout`
pub fn parse_group_layout_hint(group: &Group) -> GroupLayoutHint {
    group
        .attributes
        .standard
        .get("layout")
        .and_then(|v| v.as_str())
        .map(parse_layout_atom)
        .unwrap_or(GroupLayoutHint::Auto)
}

fn parse_layout_atom(raw: &str) -> GroupLayoutHint {
    match raw.trim().to_ascii_lowercase().as_str() {
        "horizontal" | "h" => GroupLayoutHint::Horizontal,
        "vertical" | "v" => GroupLayoutHint::Vertical,
        "fan-out" | "fan_out" | "fanout" => GroupLayoutHint::FanOut,
        "fan-in" | "fan_in" | "fanin" => GroupLayoutHint::FanIn,
        "grid" => GroupLayoutHint::Grid,
        _ => GroupLayoutHint::Auto,
    }
}

/// 将 hint（含 auto 推断）解析为具体布局模式
pub fn resolve_group_layout_mode(
    hint: GroupLayoutHint,
    members: &[String],
    graph: &GraphIndex,
    reversed: &HashSet<(String, String)>,
) -> GroupLayoutMode {
    match hint {
        GroupLayoutHint::Horizontal => GroupLayoutMode::Horizontal,
        GroupLayoutHint::Vertical => GroupLayoutMode::Vertical,
        GroupLayoutHint::Grid => GroupLayoutMode::Grid,
        GroupLayoutHint::FanOut => {
            if let Some(hub) = pick_fan_out_hub(members, graph, reversed) {
                GroupLayoutMode::FanOut { hub }
            } else {
                GroupLayoutMode::Horizontal
            }
        }
        GroupLayoutHint::FanIn => {
            if let Some(sink) = pick_fan_in_sink(members, graph, reversed) {
                GroupLayoutMode::FanIn { sink }
            } else {
                GroupLayoutMode::Horizontal
            }
        }
        GroupLayoutHint::Auto => detect_auto_mode(members, graph, reversed),
    }
}

fn detect_auto_mode(
    members: &[String],
    graph: &GraphIndex,
    reversed: &HashSet<(String, String)>,
) -> GroupLayoutMode {
    if members.len() <= 1 {
        return GroupLayoutMode::Horizontal;
    }

    let member_set: HashSet<&str> = members.iter().map(|m| m.as_str()).collect();
    let mut max_fanout = 0usize;
    let mut max_fanin = 0usize;
    let mut internal_edges = 0usize;

    for node in members {
        let fanout = internal_successors(node, graph, reversed, &member_set).len();
        let fanin = internal_predecessors(node, graph, reversed, &member_set).len();
        internal_edges += fanout;
        max_fanout = max_fanout.max(fanout);
        max_fanin = max_fanin.max(fanin);
    }

    // fan-out 优先：hub 出度 >= 2
    if max_fanout >= 2 {
        if let Some(hub) = pick_fan_out_hub(members, graph, reversed) {
            return GroupLayoutMode::FanOut { hub };
        }
    }

    // fan-in：单汇入度 >= 2，且无 fan-out 分叉
    if max_fanin >= 2 && max_fanout < 2 {
        if let Some(sink) = pick_fan_in_sink(members, graph, reversed) {
            return GroupLayoutMode::FanIn { sink };
        }
    }

    if internal_edges == 0 {
        // 无内部边：节点数较多时用 Grid，否则水平排列
        if members.len() >= 4 {
            return GroupLayoutMode::Grid;
        }
        return GroupLayoutMode::Horizontal;
    }

    if is_simple_chain(members, graph, reversed, &member_set) {
        return GroupLayoutMode::Vertical;
    }

    GroupLayoutMode::Sugiyama
}

fn pick_fan_out_hub(
    members: &[String],
    graph: &GraphIndex,
    reversed: &HashSet<(String, String)>,
) -> Option<String> {
    let member_set: HashSet<&str> = members.iter().map(|m| m.as_str()).collect();
    let mut best: Option<(usize, &String)> = None;

    for node in members {
        let fanout = internal_successors(node, graph, reversed, &member_set).len();
        if fanout < 2 {
            continue;
        }
        match best {
            None => best = Some((fanout, node)),
            Some((best_fanout, best_id)) => {
                if fanout > best_fanout || (fanout == best_fanout && node < best_id) {
                    best = Some((fanout, node));
                }
            }
        }
    }

    best.map(|(_, id)| id.clone())
}

fn internal_successors(
    node: &str,
    graph: &GraphIndex,
    reversed: &HashSet<(String, String)>,
    member_set: &HashSet<&str>,
) -> Vec<String> {
    graph
        .out_edges
        .get(node)
        .map(|succs| {
            succs
                .iter()
                .filter(|s| member_set.contains(s.as_str()))
                .filter(|s| is_effective_edge(node, s, reversed))
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

fn internal_predecessors(
    node: &str,
    graph: &GraphIndex,
    reversed: &HashSet<(String, String)>,
    member_set: &HashSet<&str>,
) -> Vec<String> {
    graph
        .in_edges
        .get(node)
        .map(|preds| {
            preds
                .iter()
                .filter(|p| member_set.contains(p.as_str()))
                .filter(|p| is_effective_edge(p, node, reversed))
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/// 在 fan-in 模式下挑选汇聚点（sink）：内部入度最大、且内部出度为 0 的节点优先。
/// 若所有节点都有出度（存在环），退而选入度最大的节点。
fn pick_fan_in_sink(
    members: &[String],
    graph: &GraphIndex,
    reversed: &HashSet<(String, String)>,
) -> Option<String> {
    let member_set: HashSet<&str> = members.iter().map(|m| m.as_str()).collect();
    let mut best: Option<(usize, bool, &String)> = None;

    for node in members {
        let fanin = internal_predecessors(node, graph, reversed, &member_set).len();
        if fanin < 2 {
            continue;
        }
        let fanout = internal_successors(node, graph, reversed, &member_set).len();
        let is_pure_sink = fanout == 0;
        match best {
            None => best = Some((fanin, is_pure_sink, node)),
            Some((best_fanin, best_pure, best_id)) => {
                // 优先 pure sink，其次入度大，最后按 ID 排序保证确定性
                let better = is_pure_sink && !best_pure
                    || (is_pure_sink == best_pure
                        && (fanin > best_fanin || (fanin == best_fanin && node < best_id)));
                if better {
                    best = Some((fanin, is_pure_sink, node));
                }
            }
        }
    }

    best.map(|(_, _, id)| id.clone())
}

fn is_simple_chain(
    members: &[String],
    graph: &GraphIndex,
    reversed: &HashSet<(String, String)>,
    member_set: &HashSet<&str>,
) -> bool {
    if members.len() <= 1 {
        return true;
    }

    let mut out_deg = 0usize;
    let mut in_deg_gt1 = 0usize;

    for node in members {
        let outs = internal_successors(node, graph, reversed, member_set).len();
        let ins = graph
            .in_edges
            .get(node)
            .map(|preds| {
                preds
                    .iter()
                    .filter(|p| member_set.contains(p.as_str()))
                    .filter(|p| is_effective_edge(p, node, reversed))
                    .count()
            })
            .unwrap_or(0);

        if outs > 1 {
            return false;
        }
        if ins > 1 {
            in_deg_gt1 += 1;
        }
        out_deg += outs;
    }

    // 链式：无分叉点（入度>1），且总边数约为 n-1。
    // 注意：fan-in 模式（多源 → 单汇）的 in_deg_gt1 ≥ 1，不应视为链式，
    // 否则会被误判为 Vertical 产生过高的单列布局。
    in_deg_gt1 == 0 && out_deg >= members.len().saturating_sub(1)
}

/// 按布局模式分配组内 rank
pub fn assign_ranks_for_mode(
    mode: &GroupLayoutMode,
    members: &[String],
    graph: &GraphIndex,
    reversed: &HashSet<(String, String)>,
) -> HashMap<String, usize> {
    match mode {
        GroupLayoutMode::Horizontal => members.iter().map(|m| (m.clone(), 0)).collect(),
        GroupLayoutMode::Vertical => assign_vertical_ranks(members, graph, reversed),
        GroupLayoutMode::FanOut { hub } => assign_fan_out_ranks(hub, members, graph, reversed),
        GroupLayoutMode::FanIn { sink } => assign_fan_in_ranks(sink, members, graph, reversed),
        GroupLayoutMode::Grid => assign_grid_ranks(members),
        GroupLayoutMode::Sugiyama => assign_intra_ranks(members, graph, reversed),
    }
}

fn assign_vertical_ranks(
    members: &[String],
    graph: &GraphIndex,
    reversed: &HashSet<(String, String)>,
) -> HashMap<String, usize> {
    let member_set: HashSet<&str> = members.iter().map(|m| m.as_str()).collect();
    let mut in_degree: HashMap<String, usize> = members.iter().map(|m| (m.clone(), 0)).collect();

    for node in members {
        for succ in internal_successors(node, graph, reversed, &member_set) {
            *in_degree.entry(succ).or_insert(0) += 1;
        }
    }

    let mut ranks: HashMap<String, usize> = HashMap::new();
    let mut queue: Vec<String> = in_degree
        .iter()
        .filter(|(_, deg)| **deg == 0)
        .map(|(id, _)| id.clone())
        .collect();
    queue.sort();

    let mut rank_cursor = 0usize;
    while let Some(node) = queue.pop() {
        if ranks.contains_key(&node) {
            continue;
        }
        ranks.insert(node.clone(), rank_cursor);
        rank_cursor += 1;

        for succ in internal_successors(&node, graph, reversed, &member_set) {
            let sid = succ;
            if let Some(deg) = in_degree.get_mut(&sid) {
                *deg = deg.saturating_sub(1);
                if *deg == 0 {
                    queue.push(sid);
                }
            }
        }
    }

    // 环或孤立：按 ID 顺序补齐
    for node in members {
        if !ranks.contains_key(node) {
            ranks.insert(node.clone(), rank_cursor);
            rank_cursor += 1;
        }
    }

    ranks
}

fn assign_fan_out_ranks(
    hub: &str,
    members: &[String],
    graph: &GraphIndex,
    reversed: &HashSet<(String, String)>,
) -> HashMap<String, usize> {
    let member_set: HashSet<&str> = members.iter().map(|m| m.as_str()).collect();
    let mut ranks: HashMap<String, usize> = HashMap::new();
    ranks.insert(hub.to_string(), 0);

    for succ in internal_successors(hub, graph, reversed, &member_set) {
        ranks.insert(succ, 1);
    }

    // 其余节点：沿用组内最长路径，但不低于 1
    let fallback = assign_intra_ranks(members, graph, reversed);
    for node in members {
        ranks.entry(node.clone()).or_insert_with(|| {
            fallback.get(node).copied().unwrap_or(1).max(1)
        });
    }

    ranks
}

/// fan-in rank 分配：所有直接前驱（源）在 rank 0（视觉最上层），sink 在 rank 1。
/// 其余节点按组内最长路径回填，但源/sink 已固定。
fn assign_fan_in_ranks(
    sink: &str,
    members: &[String],
    graph: &GraphIndex,
    reversed: &HashSet<(String, String)>,
) -> HashMap<String, usize> {
    let member_set: HashSet<&str> = members.iter().map(|m| m.as_str()).collect();
    let mut ranks: HashMap<String, usize> = HashMap::new();
    // sink 在最下层
    ranks.insert(sink.to_string(), 1);

    let preds = internal_predecessors(sink, graph, reversed, &member_set);
    for pred in &preds {
        ranks.insert(pred.clone(), 0);
    }

    // 其余节点：沿用组内最长路径；未分配的孤立节点放 rank 0（与源同层）
    let fallback = assign_intra_ranks(members, graph, reversed);
    for node in members {
        if ranks.contains_key(node) {
            continue;
        }
        let r = fallback.get(node).copied().unwrap_or(0);
        // 不能与 sink 同层或更低（rank >= 1），否则破坏 fan-in 结构
        let r = r.max(1);
        ranks.insert(node.clone(), r);
    }

    ranks
}

/// grid rank 分配：将节点按 ID 排序后填入列数 = ceil(sqrt(n)) 的网格。
/// 同一行的节点共享 rank（视觉上水平排列），行号即 rank。
fn assign_grid_ranks(members: &[String]) -> HashMap<String, usize> {
    let n = members.len();
    if n == 0 {
        return HashMap::new();
    }
    let cols = (n as f64).sqrt().ceil() as usize;
    let cols = cols.max(1);
    let mut sorted: Vec<&String> = members.iter().collect();
    sorted.sort();
    sorted
        .into_iter()
        .enumerate()
        .map(|(i, id)| (id.clone(), i / cols))
        .collect()
}

/// 将组内节点收拢为单列（用于 vertical）
pub fn align_nodes_in_column(nodes: &mut HashMap<String, crate::layout::NodeLayout>) {
    if nodes.len() <= 1 {
        return;
    }

    // 按 id 排序保证求和顺序确定（HashMap 迭代顺序随机，f64 加法非结合）
    let mut ids: Vec<String> = nodes.keys().cloned().collect();
    ids.sort();
    let column_center = {
        let sum: f64 = ids
            .iter()
            .filter_map(|id| nodes.get(id))
            .map(|n| n.x + n.width / 2.0)
            .sum();
        sum / nodes.len() as f64
    };

    for nl in nodes.values_mut() {
        nl.x = column_center - nl.width / 2.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::DiagramType;
    use crate::ast::{ArrowType, AttributeMap, AttributeValue, Diagram, Entity, Identifier, Relation, SourceInfo, Span, TextValue};
    use crate::layout::node::architecture_v2::layout::types::GraphIndex;

    fn member_graph(edges: &[(&str, &str)]) -> (GraphIndex, Vec<String>) {
        let mut entities = Vec::new();
        let mut ids = HashSet::new();
        for (from, to) in edges {
            ids.insert(from.to_string());
            ids.insert(to.to_string());
        }
        for id in &ids {
            entities.push(Entity {
                id: Identifier::new_unchecked(id),
                label: id.to_string(),
                attributes: AttributeMap::default(),
                group_id: None,
                span: Span::dummy(),
            });
        }
        let relations = edges
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
        let diagram = Diagram {
            diagram_type: DiagramType::Architecture,
            attributes: vec![],
            entities,
            relations,
            groups: vec![],
            style_decls: vec![],
            source_info: SourceInfo { file: None, line_count: 1 },
            ..Default::default()
        };
        let graph = GraphIndex::build(&diagram);
        let members: Vec<String> = ids.into_iter().collect();
        (graph, members)
    }

    #[test]
    fn parse_group_layout_values() {
        let mut attrs = AttributeMap::default();
        attrs
            .standard
            .insert("layout".to_string(), AttributeValue::String(TextValue::unquoted("fan-out".to_string())));
        let group = Group {
            id: Identifier::new_unchecked("g"),
            label: "G".to_string(),
            attributes: attrs,
            parent_id: None,
            depth: 1,
            entity_ids: vec![],
            child_group_ids: vec![],
            span: Span::dummy(),
        };
        assert_eq!(parse_group_layout_hint(&group), GroupLayoutHint::FanOut);
    }

    #[test]
    fn auto_detects_fan_out() {
        let (graph, members) = member_graph(&[("kafka", "spark"), ("kafka", "flink")]);
        let reversed = HashSet::new();
        let mode = detect_auto_mode(&members, &graph, &reversed);
        assert!(matches!(mode, GroupLayoutMode::FanOut { .. }));
    }

    #[test]
    fn auto_detects_horizontal_without_edges() {
        let (graph, _) = member_graph(&[]);
        let members = vec!["a".to_string(), "b".to_string()];
        let reversed = HashSet::new();
        let mode = detect_auto_mode(&members, &graph, &reversed);
        assert_eq!(mode, GroupLayoutMode::Horizontal);
    }

    #[test]
    fn auto_detects_grid_for_four_or_more_isolated_nodes() {
        let (graph, _) = member_graph(&[]);
        let members = vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
        ];
        let reversed = HashSet::new();
        let mode = detect_auto_mode(&members, &graph, &reversed);
        assert_eq!(mode, GroupLayoutMode::Grid);
    }

    #[test]
    fn auto_detects_fan_in_for_multi_source_single_sink() {
        // metrics/logs/traces → grafana：典型 fan-in
        let (graph, members) = member_graph(&[
            ("metrics", "grafana"),
            ("logs", "grafana"),
            ("traces", "grafana"),
        ]);
        let reversed = HashSet::new();
        let mode = detect_auto_mode(&members, &graph, &reversed);
        match mode {
            GroupLayoutMode::FanIn { sink } => assert_eq!(sink, "grafana"),
            other => panic!("expected FanIn, got {:?}", other),
        }
    }

    #[test]
    fn fan_in_ranks_place_sources_above_sink() {
        let (graph, members) = member_graph(&[
            ("metrics", "grafana"),
            ("logs", "grafana"),
            ("traces", "grafana"),
        ]);
        let reversed = HashSet::new();
        let ranks = assign_ranks_for_mode(
            &GroupLayoutMode::FanIn {
                sink: "grafana".to_string(),
            },
            &members,
            &graph,
            &reversed,
        );
        // sink 在下层（rank 1）
        assert_eq!(ranks["grafana"], 1);
        // 源节点在上层（rank 0）
        assert_eq!(ranks["metrics"], 0);
        assert_eq!(ranks["logs"], 0);
        assert_eq!(ranks["traces"], 0);
    }

    #[test]
    fn grid_racks_distribute_into_rows() {
        let (graph, _) = member_graph(&[]);
        let members = vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
            "e".to_string(),
            "f".to_string(),
        ];
        let ranks = assign_ranks_for_mode(
            &GroupLayoutMode::Grid,
            &members,
            &graph,
            &HashSet::new(),
        );
        // 6 节点 → cols = ceil(sqrt(6)) = 3 → 2 行
        // 排序后 a,b,c 在 rank 0；d,e,f 在 rank 1
        assert_eq!(ranks["a"], 0);
        assert_eq!(ranks["b"], 0);
        assert_eq!(ranks["c"], 0);
        assert_eq!(ranks["d"], 1);
        assert_eq!(ranks["e"], 1);
        assert_eq!(ranks["f"], 1);
    }

    #[test]
    fn parse_fan_in_and_grid_atoms() {
        assert_eq!(parse_layout_atom("fan-in"), GroupLayoutHint::FanIn);
        assert_eq!(parse_layout_atom("fanin"), GroupLayoutHint::FanIn);
        assert_eq!(parse_layout_atom("grid"), GroupLayoutHint::Grid);
        assert!(is_valid_group_layout_atom("fan-in"));
        assert!(is_valid_group_layout_atom("grid"));
    }

    #[test]
    fn fan_out_ranks_place_hub_on_top() {
        let (graph, members) = member_graph(&[("kafka", "spark"), ("kafka", "flink")]);
        let reversed = HashSet::new();
        let ranks = assign_ranks_for_mode(
            &GroupLayoutMode::FanOut {
                hub: "kafka".to_string(),
            },
            &members,
            &graph,
            &reversed,
        );
        assert_eq!(ranks["kafka"], 0);
        assert_eq!(ranks["spark"], 1);
        assert_eq!(ranks["flink"], 1);
    }
}
