//! Candidate scoring plus hard geometry and spacing checks.

use super::*;
use crate::layout::geometry::{Point, EPS};
use crate::layout::group::GroupRoutingContext;
use crate::layout::{EdgeLayout, GroupLayout, NodeLayout};
use std::collections::HashMap;

pub const NODE_OBSTACLE_PAD: f64 = crate::layout::constants::DEFAULT_NODE_MARGIN;
pub const GROUP_OBSTACLE_PAD: f64 = crate::layout::group::GROUP_BORDER_SHELL_PAD;
const BBOX_EXPAND: f64 = 10.0;
const CROSSING_PENALTY: f64 = 70.0;

pub trait CandidateScorer {
    fn score(&self, path: &[Point], ctx: &OrthoRoutingContext, pair: &EndpointPair) -> f64;
}

/// Legacy template scorer. LexAStar owns its own lexicographic cost model.
pub struct DefaultScorer;
impl CandidateScorer for DefaultScorer {
    fn score(&self, path: &[Point], ctx: &OrthoRoutingContext, _pair: &EndpointPair) -> f64 {
        let w = ctx.profile.scoring;
        let bends = path.len().saturating_sub(2) as f64;
        let mut score = path_length(path) * w.path_length + bends * BEND_PENALTY * w.bend;
        score += edge_overlap_penalty(path, ctx.grid);
        if !ctx.first_pass { score += crossing_penalty(path, ctx.grid) * w.crossing; }
        score
    }
}

pub fn path_length(path: &[Point]) -> f64 {
    path.windows(2).map(|w| {
        let dx = w[1].x - w[0].x;
        let dy = w[1].y - w[0].y;
        (dx * dx + dy * dy).sqrt()
    }).sum()
}

/// Compatibility diagnostic for callers that still request an obstacle score.
/// Routing correctness is enforced by the hard checks below, not this value.
pub fn obstacle_penalty(path: &[Point], from_id: &str, to_id: &str, nodes: &HashMap<String, NodeLayout>, _groups: &GroupRoutingContext, obstacles: &PreparedObstacles) -> f64 {
    path.windows(2).enumerate().map(|(si, w)| obstacles.sorted_node_ids.iter().filter(|id| {
        let endpoint_stub = (id.as_str() == from_id && si == 0)
            || (id.as_str() == to_id && si + 1 == path.len() - 1);
        !endpoint_stub && nodes.get(*id).is_some_and(|n| segment_intersects_node(w[0], w[1], n, NODE_OBSTACLE_PAD))
    }).count() as f64).sum()
}

pub fn path_is_clean(path: &[Point], from_id: &str, to_id: &str, nodes: &HashMap<String, NodeLayout>, _groups: &GroupRoutingContext, sorted_node_ids: &[String]) -> bool {
    let last = path.len().saturating_sub(2);
    path.windows(2).enumerate().all(|(si, w)| sorted_node_ids.iter().all(|id| {
        if (id == from_id && si == 0) || (id == to_id && si == last) { return true; }
        let Some(node) = nodes.get(id) else { return true; };
        let pad = if id == from_id || id == to_id { 0.0 } else { NODE_OBSTACLE_PAD };
        !segment_intersects_node(w[0], w[1], node, pad)
    }))
}

pub fn path_avoids_group_interiors(path: &[Point], from_id: &str, to_id: &str, groups: &GroupRoutingContext, sorted_group_ids: &[String]) -> bool {
    let endpoints = groups.endpoint_group_set(from_id, to_id);
    path.windows(2).all(|w| sorted_group_ids.iter().all(|id| {
        endpoints.contains(id.as_str()) || groups.groups.get(id).is_none_or(|g| !segment_crosses_rect_interior(w[0], w[1], g))
    }))
}

fn segment_crosses_rect_interior(a: Point, b: Point, group: &GroupLayout) -> bool {
    crate::layout::routing::common::geom_obstacle::segment_pierces_group_interior(a, b, group)
}
pub(super) fn segment_intersects_node(a: Point, b: Point, node: &NodeLayout, pad: f64) -> bool {
    crate::layout::routing::common::geom_obstacle::segment_pierces_node(a, b, node, pad)
}

pub fn edge_overlap_penalty(path: &[Point], grid: &SegmentGrid) -> f64 {
    path.windows(2).map(|w| RoutedSegment { x1:w[0].x, y1:w[0].y, x2:w[1].x, y2:w[1].y, edge_index:usize::MAX })
        .map(|seg| grid.query_overlapping(&seg, BBOX_EXPAND).iter().filter(|other| segments_conflict(&seg, other)).count() as f64 * EDGE_OVERLAP_PENALTY).sum()
}
pub fn crossing_penalty(path: &[Point], grid: &SegmentGrid) -> f64 {
    path.windows(2).map(|w| RoutedSegment { x1:w[0].x, y1:w[0].y, x2:w[1].x, y2:w[1].y, edge_index:usize::MAX })
        .map(|seg| grid.query_overlapping(&seg, 0.0).iter().filter(|other| ortho_segments_cross(Point::new(seg.x1,seg.y1), Point::new(seg.x2,seg.y2), other)).count() as f64 * CROSSING_PENALTY).sum()
}
fn segments_conflict(a: &RoutedSegment, b: &RoutedSegment) -> bool {
    if a.edge_index == b.edge_index { return false; }
    classify_parallel_pair(a, b).is_some_and(|p| p.gap <= EDGE_PARALLEL_GAP && p.projection_overlaps)
        || segments_cross_perpendicular(a, b) || segments_cross_perpendicular(b, a)
}
fn ortho_segments_cross(a: Point, b: Point, other: &RoutedSegment) -> bool {
    segments_cross_perpendicular(&RoutedSegment { x1:a.x,y1:a.y,x2:b.x,y2:b.y,edge_index:usize::MAX }, other)
        || segments_cross_perpendicular(other, &RoutedSegment { x1:a.x,y1:a.y,x2:b.x,y2:b.y,edge_index:usize::MAX })
}
fn segments_cross_perpendicular(h: &RoutedSegment, v: &RoutedSegment) -> bool {
    if (h.y1-h.y2).abs() >= EPS || (v.x1-v.x2).abs() >= EPS { return false; }
    v.x1 > h.x1.min(h.x2)+EPS && v.x1 < h.x1.max(h.x2)-EPS
        && h.y1 > v.y1.min(v.y2)+EPS && h.y1 < v.y1.max(v.y2)-EPS
}
struct ParallelPairInfo { gap: f64, projection_overlaps: bool }
fn classify_parallel_pair(a: &RoutedSegment, b: &RoutedSegment) -> Option<ParallelPairInfo> {
    use crate::layout::routing::segment_pair::{measure_segment_pair, OrthoSegment};
    let convert = |s: &RoutedSegment| OrthoSegment { x1:s.x1,y1:s.y1,x2:s.x2,y2:s.y2,edge_index:s.edge_index };
    let pair = measure_segment_pair(&convert(a), &convert(b))?;
    Some(ParallelPairInfo { gap:pair.gap, projection_overlaps:pair.projection_overlaps })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpacingViolationKind { ExactOverlap, TightSpacing }
pub fn segments_violate_spacing(a: &RoutedSegment, b: &RoutedSegment, min_gap: f64) -> Option<(SpacingViolationKind, f64)> {
    if a.edge_index == b.edge_index { return None; }
    let info = classify_parallel_pair(a, b)?;
    if !info.projection_overlaps { return None; }
    if info.gap < EPS { Some((SpacingViolationKind::ExactOverlap, 0.0)) }
    else if info.gap < min_gap { Some((SpacingViolationKind::TightSpacing, info.gap)) } else { None }
}
fn non_stub_segments<'a>(path: &'a [Point], guard: f64) -> impl Iterator<Item=(usize, RoutedSegment)> + 'a {
    let count = path.len().saturating_sub(1);
    path.windows(2).enumerate().filter_map(move |(i,w)| {
        let seg = RoutedSegment{x1:w[0].x,y1:w[0].y,x2:w[1].x,y2:w[1].y,edge_index:usize::MAX};
        let len = (seg.x2-seg.x1).hypot(seg.y2-seg.y1);
        ((i != 0 && i + 1 != count) || len > guard + EPS).then_some((i,seg))
    })
}
pub fn path_edge_spacing_violations(path: &[Point], grid: &SegmentGrid, min_gap: f64) -> Vec<(usize, SpacingViolationKind, f64)> {
    let mut violations = Vec::new();
    for (i, seg) in non_stub_segments(path, STUB_GUARD_LENGTH) {
        for other in grid.query_overlapping(&seg, min_gap + 2.0) {
            if let Some((kind, gap)) = segments_violate_spacing(&seg, other, min_gap) {
                violations.push((i, kind, gap));
            }
        }
    }
    violations
}
pub fn path_is_clean_from_edges(path: &[Point], grid: &SegmentGrid, min_gap: f64, stub_guard_length: f64) -> bool {
    non_stub_segments(path, stub_guard_length).all(|(_,seg)| grid.query_overlapping(&seg,min_gap+2.0).iter().all(|other| segments_violate_spacing(&seg,other,min_gap).is_none()))
}
pub fn count_all_edge_spacing_violations(edges: &[EdgeLayout], grid: &SegmentGrid, min_gap: f64) -> (usize,usize) {
    let mut exact=0; let mut tight=0;
    for (edge_index,edge) in edges.iter().enumerate() {
        let points = edge.path_points();
        for (_,mut seg) in non_stub_segments(&points, STUB_GUARD_LENGTH) {
            seg.edge_index=edge_index;
            for other in grid.query_overlapping(&seg,min_gap+2.0) {
                if let Some((kind,_))=segments_violate_spacing(&seg,other,min_gap) { match kind { SpacingViolationKind::ExactOverlap=>exact+=1, SpacingViolationKind::TightSpacing=>tight+=1 } }
            }
        }
    }
    (exact/2,tight/2)
}
