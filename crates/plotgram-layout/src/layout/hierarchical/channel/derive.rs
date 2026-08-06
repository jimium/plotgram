//! Group-aware Substrate derive (cut_line + side gates).
//!
//! Ported shape from Atlas `channel/derive.rs` — group tree from
//! `RealGraph.group_path` + PlanGraph rank/order slots. No PortSlot attach
//! (D1.1 host-track resolution via rank/order + EdgePorts remains).

use std::collections::{BTreeMap, BTreeSet};

use plotgram_engine_api::LayoutError;

use super::substrate::{
    derive_root_substrate, BlueprintIndex, GateCapacity, GateSide, GroupId, SegmentRef, Substrate,
    TrackOrient,
};
use crate::layout::hierarchical::model::{ElemKey, PlanGraph, RealGraph};

#[derive(Debug, Clone, Copy)]
struct NodeSpec {
    rank: usize,
    order: usize,
}

#[derive(Debug, Clone)]
struct GroupSpec {
    members: Vec<String>,
    parent: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeriveError {
    EmptyGroup { group: String },
    OverlappingGroups { a: String, b: String },
    ForeignNodeInGroupRect { group: String, node: String },
    Substrate(String),
}

impl From<super::substrate::SubstrateError> for DeriveError {
    fn from(e: super::substrate::SubstrateError) -> Self {
        DeriveError::Substrate(e.to_string())
    }
}

impl DeriveError {
    pub fn to_layout_error(&self) -> LayoutError {
        LayoutError::message(format!("channel derive: {self:?}"))
    }
}

/// Build substrate: group-cut when groups form a nested tree; else root-scope.
///
/// Returns `(substrate, index, used_gates)` — `used_gates` is false when we
/// fell back to root-scope (overlapping / impure group rects).
pub fn derive_substrate(
    plan: &PlanGraph,
    graph: &RealGraph,
) -> Result<(Substrate, BlueprintIndex, bool), LayoutError> {
    let (nodes, groups) = collect_blueprint(plan, graph);
    if groups.is_empty() {
        let (s, idx) = derive_root_substrate(plan);
        return Ok((s, idx, false));
    }
    match derive_group_substrate(plan, &nodes, &groups) {
        Ok((s, idx)) => Ok((s, idx, true)),
        Err(DeriveError::OverlappingGroups { .. })
        | Err(DeriveError::ForeignNodeInGroupRect { .. })
        | Err(DeriveError::EmptyGroup { .. }) => {
            // Weak-group layouts may not yield nested rectangles yet (no
            // group-frame). Fall back to D1.1 root-scope rather than hard-fail
            // the whole diagram; Gate activates when rects nest cleanly.
            let (s, idx) = derive_root_substrate(plan);
            Ok((s, idx, false))
        }
        Err(e) => Err(e.to_layout_error()),
    }
}

fn collect_blueprint(
    plan: &PlanGraph,
    graph: &RealGraph,
) -> (BTreeMap<String, NodeSpec>, BTreeMap<String, GroupSpec>) {
    let mut nodes = BTreeMap::new();
    for (i, id) in graph.ids.iter().enumerate() {
        let Some(&elem) = plan.index_of.get(&ElemKey::Real(id.clone())) else {
            continue;
        };
        let rank = plan.elems[elem].rank as usize;
        let order = plan.layers[rank]
            .iter()
            .position(|&e| e == elem)
            .unwrap_or(0);
        nodes.insert(id.clone(), NodeSpec { rank, order });
    }

    let mut groups: BTreeMap<String, GroupSpec> = BTreeMap::new();
    for (i, path) in graph.group_path.iter().enumerate() {
        let id = &graph.ids[i];
        if !nodes.contains_key(id) {
            continue;
        }
        for (depth, gname) in path.iter().enumerate() {
            let parent = if depth == 0 {
                None
            } else {
                Some(path[depth - 1].clone())
            };
            let entry = groups.entry(gname.clone()).or_insert_with(|| GroupSpec {
                members: Vec::new(),
                parent: parent.clone(),
            });
            // Prefer deepest listing for parent (first write wins if consistent).
            if entry.parent.is_none() {
                entry.parent = parent;
            }
            if depth + 1 == path.len() {
                // Direct member = leaf of path.
                if !entry.members.iter().any(|m| m == id) {
                    entry.members.push(id.clone());
                }
            }
        }
    }
    for spec in groups.values_mut() {
        spec.members.sort();
    }
    (nodes, groups)
}

fn derive_group_substrate(
    plan: &PlanGraph,
    nodes: &BTreeMap<String, NodeSpec>,
    groups: &BTreeMap<String, GroupSpec>,
) -> Result<(Substrate, BlueprintIndex), DeriveError> {
    let mut s = Substrate::new();

    let children: BTreeMap<String, Vec<String>> = {
        let mut m: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (gname, spec) in groups {
            if let Some(p) = &spec.parent {
                m.entry(p.clone()).or_default().push(gname.clone());
            }
        }
        m
    };

    let mut group_rect: BTreeMap<String, (usize, usize, usize, usize)> = BTreeMap::new();
    let mut group_desc: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for gname in groups.keys() {
        let descendants = collect_descendants(gname, groups, &children);
        if descendants.is_empty() {
            return Err(DeriveError::EmptyGroup {
                group: gname.clone(),
            });
        }
        let (mut r0, mut r1) = (usize::MAX, 0usize);
        let (mut o0, mut o1) = (usize::MAX, 0usize);
        for m in &descendants {
            let slot = nodes.get(m).ok_or_else(|| DeriveError::EmptyGroup {
                group: gname.clone(),
            })?;
            r0 = r0.min(slot.rank);
            r1 = r1.max(slot.rank);
            o0 = o0.min(slot.order);
            o1 = o1.max(slot.order);
        }
        group_rect.insert(gname.clone(), (r0, r1, o0, o1));
        group_desc.insert(gname.clone(), descendants.into_iter().collect());
    }

    let rect_names: Vec<&String> = group_rect.keys().collect();
    for i in 0..rect_names.len() {
        for j in i + 1..rect_names.len() {
            let (a, b) = (rect_names[i], rect_names[j]);
            if is_ancestor(a, b, groups) || is_ancestor(b, a, groups) {
                continue;
            }
            let ra = group_rect[a];
            let rb = group_rect[b];
            if ra.0 <= rb.1 && rb.0 <= ra.1 && ra.2 <= rb.3 && rb.2 <= ra.3 {
                return Err(DeriveError::OverlappingGroups {
                    a: a.clone(),
                    b: b.clone(),
                });
            }
        }
    }

    for (gname, &(r0, r1, o0, o1)) in &group_rect {
        let desc = &group_desc[gname];
        for (nname, slot) in nodes {
            if r0 <= slot.rank
                && slot.rank <= r1
                && o0 <= slot.order
                && slot.order <= o1
                && !desc.contains(nname)
            {
                return Err(DeriveError::ForeignNodeInGroupRect {
                    group: gname.clone(),
                    node: nname.clone(),
                });
            }
        }
    }

    let mut group_names: Vec<String> = groups.keys().cloned().collect();
    group_names.sort_by_key(|g| (depth_of(g, groups), g.clone()));
    let mut group_id: BTreeMap<String, GroupId> = BTreeMap::new();
    for gname in &group_names {
        let spec = &groups[gname];
        let parent = spec.parent.as_ref().map(|p| group_id[p]);
        let (r0, r1, o0, o1) = group_rect[gname];
        let gid = s.alloc_group_id();
        s.add_group(gid, parent, (r0, r1), (o0, o1))?;
        group_id.insert(gname.clone(), gid);
    }

    let rank_count = plan.layers.len();
    let order_count = plan
        .layers
        .iter()
        .map(|l| l.len())
        .max()
        .unwrap_or(0);

    let mut cross_lines: BTreeMap<usize, Vec<SegmentRef>> = BTreeMap::new();
    for k in 0..=rank_count {
        let mut cutters: Vec<(usize, GroupId, (usize, usize))> = Vec::new();
        for gname in &group_names {
            let (r0, r1, o0, o1) = group_rect[gname];
            if r0 < k && k <= r1 {
                cutters.push((
                    depth_of(gname, groups),
                    group_id[gname],
                    (2 * o0 + 1, 2 * o1 + 1),
                ));
            }
        }
        let segs = cut_line(&mut s, TrackOrient::Cross, k, 2 * order_count, &cutters);
        cross_lines.insert(k, segs);
    }

    let mut main_lines: BTreeMap<usize, Vec<SegmentRef>> = BTreeMap::new();
    for og in 0..=order_count {
        let mut cutters: Vec<(usize, GroupId, (usize, usize))> = Vec::new();
        for gname in &group_names {
            let (r0, r1, o0, o1) = group_rect[gname];
            if o0 < og && og <= o1 {
                cutters.push((
                    depth_of(gname, groups),
                    group_id[gname],
                    (2 * r0 + 1, 2 * r1 + 1),
                ));
            }
        }
        let segs = cut_line(&mut s, TrackOrient::Main, og, 2 * rank_count, &cutters);
        main_lines.insert(og, segs);
    }

    for (k, csegs) in &cross_lines {
        for cs in csegs {
            let og_lo = (cs.ext.0 + 1) / 2;
            let og_hi = cs.ext.1 / 2;
            for og in og_lo..=og_hi {
                if let Some(msegs) = main_lines.get(&og) {
                    for ms in msegs {
                        if ms.covers(2 * k) && ms.scope == cs.scope {
                            s.link(cs.id, ms.id)?;
                        }
                    }
                }
            }
        }
    }

    let capacity = GateCapacity::Unbounded;
    for gname in &group_names {
        let gid = group_id[gname];
        let (r0, r1, o0, o1) = group_rect[gname];
        add_side_gate(
            &mut s,
            gid,
            GateSide::MainLow,
            r0,
            &main_lines,
            o0 + 1,
            o1,
            capacity,
        )?;
        add_side_gate(
            &mut s,
            gid,
            GateSide::MainHigh,
            r1 + 1,
            &main_lines,
            o0 + 1,
            o1,
            capacity,
        )?;
        add_side_gate(
            &mut s,
            gid,
            GateSide::CrossLow,
            o0,
            &cross_lines,
            r0 + 1,
            r1,
            capacity,
        )?;
        add_side_gate(
            &mut s,
            gid,
            GateSide::CrossHigh,
            o1 + 1,
            &cross_lines,
            r0 + 1,
            r1,
            capacity,
        )?;
    }

    let mut node_region: BTreeMap<String, Option<String>> = BTreeMap::new();
    for name in nodes.keys() {
        node_region.insert(name.clone(), None);
    }
    for gname in &group_names {
        for m in &groups[gname].members {
            node_region.insert(m.clone(), Some(gname.clone()));
        }
    }

    let index = BlueprintIndex {
        cross_lines,
        main_lines,
        rank_count,
        order_count,
        group_ids: group_id,
        node_region,
    };
    Ok((s, index))
}

fn cut_line(
    s: &mut Substrate,
    orient: TrackOrient,
    line: usize,
    full_hi: usize,
    cutters: &[(usize, GroupId, (usize, usize))],
) -> Vec<SegmentRef> {
    let mut cuts: BTreeSet<usize> = BTreeSet::new();
    for &(_, _, interior) in cutters {
        cuts.insert(interior.0);
        cuts.insert(interior.1 + 1);
    }
    let mut bounds: Vec<usize> = Vec::with_capacity(cuts.len() + 2);
    bounds.push(0);
    bounds.extend(cuts.iter().copied().filter(|&c| c > 0 && c <= full_hi));
    bounds.push(full_hi + 1);

    let mut by_depth: Vec<&(usize, GroupId, (usize, usize))> = cutters.iter().collect();
    by_depth.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));

    let mut segs = Vec::new();
    for w in bounds.windows(2) {
        let (a, b) = (w[0], w[1] - 1);
        if a > b {
            continue;
        }
        let scope = by_depth
            .iter()
            .find(|(_, _, interior)| interior.0 <= a && b <= interior.1)
            .map(|(_, gid, _)| *gid);
        let lo_even = a + (a & 1);
        let hi_even = b - (b & 1);
        let gaps = if lo_even > hi_even {
            0
        } else {
            (hi_even - lo_even) / 2 + 1
        };
        let id = s.alloc_track_id();
        s.add_track(id, orient, scope, gaps.max(1) as f64, line, (a, b))
            .expect("cut_line: add segment");
        segs.push(SegmentRef {
            id,
            ext: (a, b),
            scope,
        });
    }
    segs
}

#[allow(clippy::too_many_arguments)]
fn add_side_gate(
    s: &mut Substrate,
    gid: GroupId,
    side: GateSide,
    boundary: usize,
    lines: &BTreeMap<usize, Vec<SegmentRef>>,
    lo: usize,
    hi: usize,
    capacity: GateCapacity,
) -> Result<(), DeriveError> {
    if lo > hi {
        return Ok(());
    }
    let mut crossings: Vec<(super::substrate::TrackId, super::substrate::TrackId)> = Vec::new();
    for line in lo..=hi {
        let Some(segs) = lines.get(&line) else {
            continue;
        };
        let low_side = matches!(side, GateSide::MainLow | GateSide::CrossLow);
        let inner = segs.iter().find(|sg| {
            sg.scope == Some(gid)
                && if low_side {
                    sg.ext.0 == 2 * boundary + 1
                } else {
                    boundary > 0 && sg.ext.1 == 2 * boundary - 1
                }
        });
        let outer = segs.iter().find(|sg| {
            s.is_ancestor_scope(sg.scope, gid)
                && if low_side {
                    sg.ext.1 == 2 * boundary
                } else {
                    sg.ext.0 == 2 * boundary
                }
        });
        if let (Some(i), Some(o)) = (inner, outer) {
            crossings.push((i.id, o.id));
        }
    }
    if crossings.is_empty() {
        return Ok(());
    }
    let id = s.alloc_gate_id();
    s.add_gate(id, gid, side, boundary, crossings, capacity)?;
    Ok(())
}

fn collect_descendants(
    gname: &str,
    groups: &BTreeMap<String, GroupSpec>,
    children: &BTreeMap<String, Vec<String>>,
) -> Vec<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    fn walk(
        g: &str,
        groups: &BTreeMap<String, GroupSpec>,
        children: &BTreeMap<String, Vec<String>>,
        out: &mut BTreeSet<String>,
    ) {
        if let Some(spec) = groups.get(g) {
            for m in &spec.members {
                out.insert(m.clone());
            }
        }
        if let Some(kids) = children.get(g) {
            for c in kids {
                walk(c, groups, children, out);
            }
        }
    }
    walk(gname, groups, children, &mut out);
    out.into_iter().collect()
}

fn depth_of(gname: &str, groups: &BTreeMap<String, GroupSpec>) -> usize {
    let mut d = 0;
    let mut cur = groups.get(gname).and_then(|s| s.parent.as_ref());
    while let Some(p) = cur {
        d += 1;
        cur = groups.get(p).and_then(|s| s.parent.as_ref());
    }
    d
}

fn is_ancestor(a: &str, b: &str, groups: &BTreeMap<String, GroupSpec>) -> bool {
    let mut cur = groups.get(b).and_then(|s| s.parent.as_ref());
    while let Some(p) = cur {
        if p == a {
            return true;
        }
        cur = groups.get(p).and_then(|s| s.parent.as_ref());
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::hierarchical::model::{Elem, ElemKey, RealEdge, RealGraph};

    /// Sibling groups side-by-side → clean rects → Gate IR active.
    #[test]
    fn sibling_groups_derive_gates() {
        // Layer 0: a (g1) | b (g2); Layer 1: c (g1) | d (g2)
        let elems = vec![
            Elem {
                key: ElemKey::Real("a".into()),
                group_path: vec!["g1".into()],
                rank: 0,
            },
            Elem {
                key: ElemKey::Real("b".into()),
                group_path: vec!["g2".into()],
                rank: 0,
            },
            Elem {
                key: ElemKey::Real("c".into()),
                group_path: vec!["g1".into()],
                rank: 1,
            },
            Elem {
                key: ElemKey::Real("d".into()),
                group_path: vec!["g2".into()],
                rank: 1,
            },
        ];
        let index_of = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let plan = PlanGraph {
            elems,
            index_of,
            decl_index: (0..4).collect(),
            segments: vec![],
            layers: vec![vec![0, 1], vec![2, 3]],
        };
        let mut index_of_ids = BTreeMap::new();
        index_of_ids.insert("a".into(), 0);
        index_of_ids.insert("b".into(), 1);
        index_of_ids.insert("c".into(), 2);
        index_of_ids.insert("d".into(), 3);
        let graph = RealGraph {
            ids: vec!["a".into(), "b".into(), "c".into(), "d".into()],
            index_of: index_of_ids,
            group_path: vec![
                vec!["g1".into()],
                vec!["g2".into()],
                vec!["g1".into()],
                vec!["g2".into()],
            ],
            shapes: vec![plotgram_model::NodeShape::DEFAULT; 4],
            edges: vec![RealEdge {
                edge_id: "cross".into(),
                original_source: 0,
                original_target: 3,
                working_source: 0,
                working_target: 3,
                reversed: false,
                from_port: None,
                to_port: None,
                critical: false,
            }],
            self_loops: vec![],
        };
        let (sub, idx, used) = derive_substrate(&plan, &graph).unwrap();
        assert!(used, "clean sibling rects must activate Gate IR");
        assert!(!sub.gates().collect::<Vec<_>>().is_empty());
        assert!(idx.group_ids.contains_key("g1"));
        assert!(idx.group_ids.contains_key("g2"));
        // Cross-group path must go through at least one gate.
        use super::super::graph::{ChannelGraph, Occupancy};
        use super::super::search::{route_edge, RouteHints, ScopeMask};
        use super::super::substrate::PortSide;
        let g = ChannelGraph::from_substrate(&sub);
        let start = idx.resolve_host_track(0, 0, PortSide::MainHigh).unwrap();
        let goal = idx.resolve_host_track(1, 1, PortSide::MainLow).unwrap();
        let mask = ScopeMask::for_scopes(&sub, idx.node_scope("a"), idx.node_scope("d"));
        let out = route_edge(
            &g,
            start,
            goal,
            &Occupancy::new(),
            true,
            &mask,
            RouteHints::default(),
        );
        assert!(out.feasible);
        assert!(
            !out.path.gates.is_empty(),
            "cross-group edge must traverse a gate"
        );
    }
}
