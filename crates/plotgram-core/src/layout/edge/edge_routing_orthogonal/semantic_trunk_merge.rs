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

/// C 阶段：仅无分组 architecture 执行 FanIn 合流（有组大图易抬 tight / 穿模）。
pub fn apply_semantic_trunk_merge(
    edges: &mut [EdgeLayout],
    relations: &[Relation],
    from_side: &[Port],
    to_side: &[Port],
    nodes: &HashMap<String, NodeLayout>,
    diagram_type: DiagramType,
    has_groups: bool,
) -> SemanticTrunkMergeResult {
    let mut out = SemanticTrunkMergeResult::default();
    if !matches!(diagram_type, DiagramType::Architecture) || has_groups {
        return out;
    }
    if relations.len() != edges.len() {
        return out;
    }

    let groups = collect_semantic_merge_groups(relations, from_side, to_side);
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
            &members,
            edges,
            relations,
            from_side,
            to_side,
            nodes,
            &key,
            &mut out,
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
    from_side: &[Port],
    to_side: &[Port],
) -> BTreeMap<SemanticMergeKey, Vec<usize>> {
    let mut fanin: BTreeMap<SemanticMergeKey, Vec<usize>> = BTreeMap::new();

    for (ei, rel) in relations.iter().enumerate() {
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

    let mut ends: Vec<(usize, Point, Point, Port)> = Vec::new();
    for &ei in members {
        let pts = edges[ei].path_points();
        if pts.len() < 2 {
            return false;
        }
        let start = pts[0];
        let end = *pts.last().unwrap();
        ends.push((ei, start, end, from_side[ei]));
    }

    let mut end_xs: Vec<f64> = ends.iter().map(|(_, _, e, _)| e.x).collect();
    end_xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let trunk_x = end_xs[end_xs.len() / 2];

    let (tox, toy) = port_outward(to_port);
    // 所有边应落在同一目标节点顶/底 → to_stub.y 一致
    let to_stub_ys: Vec<f64> = ends
        .iter()
        .map(|(_, _, e, _)| e.y + toy * PORT_CLEARANCE)
        .collect();
    let fork_y = if toy < 0.0 {
        to_stub_ys.iter().cloned().fold(f64::INFINITY, f64::min)
    } else {
        to_stub_ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
    };

    let from_stub_ys: Vec<f64> = ends
        .iter()
        .map(|(_, s, _, fp)| {
            let (ox, oy) = port_outward(*fp);
            s.y + oy * PORT_CLEARANCE
        })
        .collect();
    // 共享干线起点：在源 stub 与 fork 之间留够 MIN_SHARED
    let join_y = if toy < 0.0 {
        // Top 汇入：y 向下增大；from_stub.y < fork_y < end.y
        let max_from = from_stub_ys
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max);
        let desired = fork_y - MIN_SHARED_TRUNK_LEN.max(32.0);
        desired.max(max_from + 4.0)
    } else {
        // Bottom 汇入：从下方上来
        let min_from = from_stub_ys.iter().cloned().fold(f64::INFINITY, f64::min);
        let desired = fork_y + MIN_SHARED_TRUNK_LEN.max(32.0);
        desired.min(min_from - 4.0)
    };

    if (fork_y - join_y).abs() + EPS < MIN_SHARED_TRUNK_LEN {
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
    for &(ei, start, end, fp) in &ends {
        let (fox, foy) = port_outward(fp);
        let from_stub = Point::new(start.x + fox * PORT_CLEARANCE, start.y + foy * PORT_CLEARANCE);
        let to_stub = Point::new(end.x + tox * PORT_CLEARANCE, end.y + toy * PORT_CLEARANCE);

        let mut pts = vec![start, from_stub];
        // 走到共享干线入口
        if (from_stub.x - trunk_x).abs() > EPS {
            pts.push(Point::new(trunk_x, from_stub.y));
        }
        if (from_stub.y - join_y).abs() > EPS || (pts.last().unwrap().y - join_y).abs() > EPS {
            pts.push(Point::new(trunk_x, join_y));
        }
        pts.push(Point::new(trunk_x, fork_y));
        if (end.x - trunk_x).abs() > EPS {
            pts.push(Point::new(end.x, fork_y));
        }
        if (to_stub.x - end.x).abs() > EPS || (to_stub.y - fork_y).abs() > EPS {
            pts.push(to_stub);
        }
        pts.push(end);
        let pts = simplify_path(pts, true);
        if pts.len() < 4 {
            return false;
        }
        if path_hits_nodes(&pts, nodes, &relations[ei]) {
            return false;
        }
        new_paths.push((ei, pts));
    }

    for (ei, pts) in new_paths {
        edges[ei].set_polyline_points(pts);
        out.merge_intervals.insert(ei, vec![interval.clone()]);
        out.stats.edges_rewritten += 1;
    }
    true
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
                vec![
                    Point::new(100.0, 100.0),
                    Point::new(100.0, 200.0),
                ],
                Port::Bottom,
                Port::Top,
            ),
            edge(
                vec![
                    Point::new(200.0, 100.0),
                    Point::new(200.0, 200.0),
                ],
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
