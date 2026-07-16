//! S3：语义 FanIn/FanOut 合流写者（architecture）。
//!
//! 在 C 阶段 lane 之后、Annotation 冻结之前：对 `SameTargetFanIn` /
//! `SameSourceFanOut` 组生成共享 approach trunk + 短分叉 stub，并声明
//! `MergeInterval`；失败则 `Degraded(MergeInfeasible)`，不改路径。

use crate::ast::Relation;
use crate::layout::edge::edge_merge_policy::merge_groups_for_edge;
use crate::layout::edge::edge_merge_policy::{EdgeMergeContext, MergeGroup};
use crate::layout::edge::route_annotation::MergeInterval;
use crate::layout::edge::segment_pair::MIN_SHARED_TRUNK_LEN;
use crate::layout::geometry::Point;
use crate::layout::refine::segment_intersects_node;
use crate::layout::{EdgeLayout, NodeLayout, Port};
use crate::types::DiagramType;
use std::collections::{BTreeMap, HashMap, HashSet};

use super::path::port_outward;
use super::simplify::simplify_path;
use super::{EPS, PORT_CLEARANCE};

const DEGRADED_MERGE: &str = "MergeInfeasible";
/// 单组最多边数；过大 FanIn 易穿模，留给 S3.2b / lane
const MAX_FANIN_GROUP: usize = 5;

#[derive(Debug, Clone, Default)]
pub struct SemanticTrunkMergeStats {
    pub groups_considered: usize,
    pub groups_merged: usize,
    pub edges_rewritten: usize,
    pub degraded_groups: usize,
}

/// 合流写结果：按边写入的 merge 区间与 degraded 原因。
#[derive(Debug, Clone, Default)]
pub struct SemanticTrunkMergeResult {
    pub stats: SemanticTrunkMergeStats,
    pub merge_intervals: HashMap<usize, Vec<MergeInterval>>,
    pub degraded: HashMap<usize, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum SemanticMergeKey {
    FanIn { to_id: String, to_port: u8 },
}

fn port_code(p: Port) -> u8 {
    match p {
        Port::Top => 0,
        Port::Bottom => 1,
        Port::Left => 2,
        Port::Right => 3,
    }
}

fn is_vertical_port(p: Port) -> bool {
    matches!(p, Port::Top | Port::Bottom)
}

/// C 阶段：architecture FanIn 合流。
///
/// - 无组：全开（S3）
/// - 有组（S3.2b）：同样尝试；`try_merge_fan_in` 穿模则 degraded，不改路径
pub fn apply_semantic_trunk_merge(
    edges: &mut [EdgeLayout],
    relations: &[Relation],
    from_side: &[Port],
    to_side: &[Port],
    nodes: &HashMap<String, NodeLayout>,
    diagram_type: DiagramType,
    _has_groups: bool,
) -> SemanticTrunkMergeResult {
    apply_semantic_trunk_merge_filtered(
        edges,
        relations,
        from_side,
        to_side,
        nodes,
        diagram_type,
        None,
    )
}

/// S4 之后对监控边做目标局部合流；只改 `allowed_edges`，不把监控干混入业务干。
pub fn apply_monitor_local_trunk_merge(
    edges: &mut [EdgeLayout],
    relations: &[Relation],
    from_side: &[Port],
    to_side: &[Port],
    nodes: &HashMap<String, NodeLayout>,
    diagram_type: DiagramType,
    allowed_edges: &HashSet<usize>,
) -> SemanticTrunkMergeResult {
    apply_semantic_trunk_merge_filtered(
        edges,
        relations,
        from_side,
        to_side,
        nodes,
        diagram_type,
        Some(allowed_edges),
    )
}

fn apply_semantic_trunk_merge_filtered(
    edges: &mut [EdgeLayout],
    relations: &[Relation],
    from_side: &[Port],
    to_side: &[Port],
    nodes: &HashMap<String, NodeLayout>,
    diagram_type: DiagramType,
    allowed_edges: Option<&HashSet<usize>>,
) -> SemanticTrunkMergeResult {
    let mut out = SemanticTrunkMergeResult::default();
    if !matches!(diagram_type, DiagramType::Architecture) {
        return out;
    }
    if relations.len() != edges.len() {
        return out;
    }

    let groups = collect_semantic_merge_groups(relations, to_side, allowed_edges);
    out.stats.groups_considered = groups.len();

    let mut claimed: HashSet<usize> = HashSet::new();
    let mut keys: Vec<SemanticMergeKey> = groups.keys().cloned().collect();
    keys.sort();

    for key in keys {
        let Some(mut members) = groups.get(&key).cloned() else {
            continue;
        };
        members.retain(|ei| !claimed.contains(ei));
        if members.len() < 2 {
            continue;
        }
        members.sort_unstable();

        let ok = try_merge_fan_in(
            &members, edges, relations, from_side, to_side, nodes, &key, &mut out,
        );
        if ok {
            out.stats.groups_merged += 1;
            for &ei in &members {
                claimed.insert(ei);
            }
        } else {
            out.stats.degraded_groups += 1;
            for &ei in &members {
                out.degraded
                    .entry(ei)
                    .or_insert_with(|| DEGRADED_MERGE.to_string());
            }
        }
    }
    out
}

fn collect_semantic_merge_groups(
    relations: &[Relation],
    to_side: &[Port],
    allowed_edges: Option<&HashSet<usize>>,
) -> BTreeMap<SemanticMergeKey, Vec<usize>> {
    let mut fanin: BTreeMap<SemanticMergeKey, Vec<usize>> = BTreeMap::new();

    for (ei, rel) in relations.iter().enumerate() {
        if allowed_edges.is_some_and(|allowed| !allowed.contains(&ei)) {
            continue;
        }
        let ts = to_side.get(ei).copied().unwrap_or(Port::Top);
        let ctx = EdgeMergeContext {
            from_id: rel.from.as_str(),
            to_id: rel.to.as_str(),
            edge_index: ei,
            from_leaf_group: None,
            to_leaf_group: None,
        };
        for g in merge_groups_for_edge(&ctx) {
            // S3 首版只兑现 FanIn（T2 Postgres）；FanOut 易与 gateway 多路交叉抬高 lint
            if let MergeGroup::SameTargetFanIn { to_id } = g {
                if is_vertical_port(ts) {
                    fanin
                        .entry(SemanticMergeKey::FanIn {
                            to_id,
                            to_port: port_code(ts),
                        })
                        .or_default()
                        .push(ei);
                }
            }
        }
    }

    let mut out = BTreeMap::new();
    for (k, mut v) in fanin {
        v.sort_unstable();
        v.dedup();
        if (2..=MAX_FANIN_GROUP).contains(&v.len()) {
            out.insert(k, v);
        }
    }
    out
}

fn group_key_str(key: &SemanticMergeKey) -> String {
    match key {
        SemanticMergeKey::FanIn { to_id, to_port } => format!("fanin:{to_id}:{to_port}"),
    }
}

fn try_merge_fan_in(
    members: &[usize],
    edges: &mut [EdgeLayout],
    relations: &[Relation],
    from_side: &[Port],
    to_side: &[Port],
    nodes: &HashMap<String, NodeLayout>,
    key: &SemanticMergeKey,
    out: &mut SemanticTrunkMergeResult,
) -> bool {
    let to_port = to_side[members[0]];
    if !members.iter().all(|&ei| to_side[ei] == to_port) {
        return false;
    }

    let mut ends: Vec<(usize, Vec<Point>, Point)> = Vec::new();
    for &ei in members {
        let pts: Vec<Point> = edges[ei].path_points().into_owned();
        if pts.len() < 2 {
            return false;
        }
        let end = *pts.last().unwrap();
        ends.push((ei, pts, end));
    }

    let mut end_xs: Vec<f64> = ends.iter().map(|(_, _, e)| e.x).collect();
    end_xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let trunk_x = end_xs[end_xs.len() / 2];

    let (_, toy) = port_outward(to_port);
    let end_y = ends[0].2.y;
    if !ends.iter().all(|(_, _, end)| (end.y - end_y).abs() <= 1.0) {
        return false;
    }
    let fork_y = end_y + toy * PORT_CLEARANCE;
    // 同 rank、同出侧的源节点优先在源 stub 后立即合流，保证 pendant fan-in 对称；
    // 其它情况（监控外环等）只改目标局部 suffix。
    let aligned_source_join = {
        let fp = from_side[members[0]];
        if !is_vertical_port(fp) || !members.iter().all(|&ei| from_side[ei] == fp) {
            None
        } else {
            let (_, foy) = port_outward(fp);
            let ys: Vec<f64> = ends
                .iter()
                .map(|(_, path, _)| path[0].y + foy * PORT_CLEARANCE)
                .collect();
            let y0 = ys[0];
            ys.iter().all(|y| (y - y0).abs() <= 1.0).then_some(y0)
        }
    };
    let join_y =
        aligned_source_join.unwrap_or_else(|| fork_y + toy * MIN_SHARED_TRUNK_LEN.max(32.0));
    if (join_y - fork_y).abs() + EPS < MIN_SHARED_TRUNK_LEN {
        return false;
    }

    let gkey = group_key_str(key);
    let interval = MergeInterval {
        horizontal: false,
        coord: trunk_x,
        t0: join_y.min(fork_y),
        t1: join_y.max(fork_y),
        group_key: Some(gkey),
    };

    let mut new_paths: Vec<(usize, Vec<Point>)> = Vec::new();
    for (ei, old_path, end) in &ends {
        let mut pts = if aligned_source_join.is_some() {
            let start = old_path[0];
            let (fox, foy) = port_outward(from_side[*ei]);
            vec![
                start,
                Point::new(
                    start.x + fox * PORT_CLEARANCE,
                    start.y + foy * PORT_CLEARANCE,
                ),
            ]
        } else {
            // 只重写目标附近 suffix：源端 stub、走廊选择与绕障前缀保持原路由写者的结果。
            let Some(prefix) = prefix_through_horizontal_cut(old_path, join_y) else {
                return false;
            };
            prefix
        };
        let branch = *pts.last().unwrap();
        let join = Point::new(trunk_x, join_y);
        if (branch.x - join.x).abs() > EPS || (branch.y - join.y).abs() > EPS {
            pts.push(join);
        }
        let fork = Point::new(trunk_x, fork_y);
        if (join.y - fork.y).abs() > EPS {
            pts.push(fork);
        }
        // 合流后目标侧共锚（同一 trunk_x）。
        let shared_end = Point::new(trunk_x, end.y);
        pts.push(shared_end);
        let pts = simplify_path(pts, true);
        if pts.len() < 4 {
            return false;
        }
        if path_hits_nodes(&pts, nodes, &relations[*ei]) {
            return false;
        }
        new_paths.push((*ei, pts));
    }

    for (ei, pts) in new_paths {
        edges[ei].set_polyline_points(pts);
        out.merge_intervals.insert(ei, vec![interval.clone()]);
        out.stats.edges_rewritten += 1;
    }
    true
}

/// 保留路径到最后一次穿过 `cut_y` 的位置（面向目标的最近交点）。
///
/// FanIn 合流只改该交点之后的局部 suffix，避免重新发明源端 stub 与跨组走廊。
fn prefix_through_horizontal_cut(points: &[Point], cut_y: f64) -> Option<Vec<Point>> {
    for si in (0..points.len().saturating_sub(1)).rev() {
        let a = points[si];
        let b = points[si + 1];
        if (a.x - b.x).abs() > EPS {
            if (a.y - cut_y).abs() <= EPS && (b.y - cut_y).abs() <= EPS {
                return Some(points[..=si + 1].to_vec());
            }
            continue;
        }
        if cut_y + EPS < a.y.min(b.y) || cut_y - EPS > a.y.max(b.y) {
            continue;
        }
        let hit = Point::new(a.x, cut_y);
        let mut prefix = points[..=si].to_vec();
        if prefix
            .last()
            .is_none_or(|last| (last.x - hit.x).abs() > EPS || (last.y - hit.y).abs() > EPS)
        {
            prefix.push(hit);
        }
        return Some(prefix);
    }
    None
}

fn path_hits_nodes(points: &[Point], nodes: &HashMap<String, NodeLayout>, rel: &Relation) -> bool {
    let from = rel.from.as_str();
    let to = rel.to.as_str();
    for w in points.windows(2) {
        let a = w[0];
        let b = w[1];
        if (a.x - b.x).abs() + (a.y - b.y).abs() < 1.0 {
            continue;
        }
        for (id, nl) in nodes {
            if id.as_str() == from || id.as_str() == to {
                continue;
            }
            if segment_intersects_node(a, b, nl) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{ArrowType, AttributeMap, Identifier, Relation, Span};
    use crate::layout::types::PathGeometry;

    fn rel(from: &str, to: &str) -> Relation {
        Relation {
            from: Identifier::new_unchecked(from),
            to: Identifier::new_unchecked(to),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span: Span::dummy(),
        }
    }

    fn edge(pts: Vec<Point>, fp: Port, tp: Port) -> EdgeLayout {
        let mut e = EdgeLayout {
            geometry: PathGeometry::Polyline { points: Vec::new() },
            labels: Vec::new(),
            from_port: fp,
            to_port: tp,
        };
        e.set_polyline_points(pts);
        e
    }

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
    fn fanin_three_edges_share_vertical_trunk() {
        // 三源 → postgres 顶：梳状初值
        let relations = vec![rel("a", "pg"), rel("b", "pg"), rel("c", "pg")];
        let mut edges = vec![
            edge(
                vec![
                    Point::new(100.0, 100.0),
                    Point::new(100.0, 116.0),
                    Point::new(190.0, 116.0),
                    Point::new(190.0, 200.0),
                ],
                Port::Bottom,
                Port::Top,
            ),
            edge(
                vec![
                    Point::new(200.0, 100.0),
                    Point::new(200.0, 116.0),
                    Point::new(210.0, 116.0),
                    Point::new(210.0, 200.0),
                ],
                Port::Bottom,
                Port::Top,
            ),
            edge(
                vec![
                    Point::new(300.0, 100.0),
                    Point::new(300.0, 116.0),
                    Point::new(230.0, 116.0),
                    Point::new(230.0, 200.0),
                ],
                Port::Bottom,
                Port::Top,
            ),
        ];
        let from_side = vec![Port::Bottom; 3];
        let to_side = vec![Port::Top; 3];
        let mut nodes = HashMap::new();
        nodes.insert("a".into(), node(70.0, 50.0, 60.0, 50.0));
        nodes.insert("b".into(), node(170.0, 50.0, 60.0, 50.0));
        nodes.insert("c".into(), node(270.0, 50.0, 60.0, 50.0));
        nodes.insert("pg".into(), node(160.0, 200.0, 100.0, 50.0));

        let result = apply_semantic_trunk_merge(
            &mut edges,
            &relations,
            &from_side,
            &to_side,
            &nodes,
            DiagramType::Architecture,
            false,
        );
        assert!(result.stats.groups_merged >= 1, "expected merge");
        assert_eq!(result.merge_intervals.len(), 3);
        // 三边应声明同一竖直 merge coord
        let coords: Vec<f64> = result
            .merge_intervals
            .values()
            .map(|v| v[0].coord)
            .collect();
        assert!(
            coords.windows(2).all(|w| (w[0] - w[1]).abs() < 1e-6),
            "shared trunk x"
        );
        let trunk_x = coords[0];
        for e in &edges {
            let pts = e.path_points();
            let on_trunk = pts.windows(2).any(|w| {
                (w[0].x - trunk_x).abs() < 1.0
                    && (w[1].x - trunk_x).abs() < 1.0
                    && (w[0].y - w[1].y).abs() >= MIN_SHARED_TRUNK_LEN - 1.0
            });
            assert!(on_trunk, "edge should contain shared vertical trunk");
        }
    }

    #[test]
    fn flowchart_skips_semantic_merge() {
        let relations = vec![rel("a", "pg"), rel("b", "pg")];
        let mut edges = vec![
            edge(
                vec![Point::new(100.0, 100.0), Point::new(100.0, 200.0)],
                Port::Bottom,
                Port::Top,
            ),
            edge(
                vec![Point::new(200.0, 100.0), Point::new(200.0, 200.0)],
                Port::Bottom,
                Port::Top,
            ),
        ];
        let result = apply_semantic_trunk_merge(
            &mut edges,
            &relations,
            &[Port::Bottom, Port::Bottom],
            &[Port::Top, Port::Top],
            &HashMap::new(),
            DiagramType::Flowchart,
            false,
        );
        assert_eq!(result.stats.groups_merged, 0);
        assert!(result.merge_intervals.is_empty());
    }
}

#[test]
fn s32b_microservices_db_fanin_merges() {
    let source = include_str!("../../../../../../showcase/architecture/n.microservices.pgm");
    let output =
        crate::pipeline::parse_prepare_validate(source, &crate::prepare::StyleRequest::default());
    let prepared = output.diagram.expect("valid");
    assert!(!prepared.inner().groups.is_empty());
    let layout = crate::layout::compute_layout_with_plan(prepared.inner(), prepared.layout_plan())
        .expect("layout");
    let relations = &prepared.inner().relations;
    let db_edges: Vec<usize> = relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.to.as_str() == "db")
        .map(|(i, _)| i)
        .collect();
    assert!(db_edges.len() >= 2, "need fanin to db");
    // 近目标竖直干线应共享（共锚：终点 x 一致）
    let mut end_xs = Vec::new();
    for &ei in &db_edges {
        let pts: Vec<Point> = layout.edges[ei].path_points().into_owned();
        let end = *pts.last().unwrap();
        end_xs.push(end.x);
    }
    assert!(end_xs.len() >= 2);
    let min_x = end_xs.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_x = end_xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    eprintln!("db end xs={end_xs:?}");
    assert!(
        (max_x - min_x).abs() < 2.0,
        "S3.2b: db FanIn should share dock, xs span={}",
        max_x - min_x
    );
}
