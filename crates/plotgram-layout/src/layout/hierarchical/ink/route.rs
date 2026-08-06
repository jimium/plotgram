//! P5 Ink: pure expansion of Plan + Metric into edge paths. Reads the
//! dummy-chain waypoints, finalized ports, and D1.0 TrackOrder /
//! TrackCoords; never invents a port side or a horizontal-track Y
//! (ink-and-verification.md §1; channel-d1.md §5.1).
//!
//! Port anchors come exclusively from `metric::anchor::port_anchor` — Ink
//! has no default-side fallback and never re-derives an anchor (M2: 无 Ink
//! fallback / Ink 零猜测).
//!
//! Routing styles (edge-parameters.md §2.1):
//! - **orthogonal** — TrackOrder rails + `normalize_orthogonal`; bus-grouped
//!   ends join Compose [`BundlePlan`] + Metric bus levels into
//!   SharedPort → Trunk → Bus → Stub (yFiles AutomaticEdgeGrouping);
//! - **polyline** — waypoints connected by straight segments, no bend logic;
//! - **curved** — one cubic Bézier between the two anchors; only main-axis
//!   (North/South) port pairs are supported (G3 cross-axis ports wait for
//!   Channel — roadmap D₁).

use std::collections::BTreeMap;

use plotgram_algo::path_ortho::{normalize_orthogonal, NormalizeOptions};
use plotgram_algo::orientation::Side;
use plotgram_engine_api::LayoutError;
use plotgram_model::geometry::{Point, Rect};

use crate::layout::hierarchical::channel::{
    ChannelPath, ChannelRoutePlan, RouteTopology, Substrate, TrackId, TrackOrient,
};
use crate::layout::hierarchical::compose::ports::{EdgePorts, ResolvedPort};
use crate::layout::hierarchical::compose::track_order::TrackOrderPlan;
use crate::layout::hierarchical::metric::anchor::port_anchor;
use crate::layout::hierarchical::metric::bus::BusLevels;
use crate::layout::hierarchical::metric::track::TrackCoords;
use crate::layout::hierarchical::model::{PlanGraph, RealGraph, Segment};
use crate::layout::hierarchical::orient::{from_algo_point, to_algo_point};
use crate::layout::hierarchical::params::RoutingStyle;

/// Ink-internal path carrier (canonical TB space). Maps to
/// `plotgram_model::result::EdgePath` in `mod.rs`'s orientation-out pass.
#[derive(Debug, Clone, PartialEq)]
pub enum InkPath {
    Polyline(Vec<Point>),
    Cubic {
        start: Point,
        end: Point,
        controls: [Point; 2],
    },
}

impl InkPath {
    /// Polyline approximation (debug / verify); cubic is sampled via the
    /// model's deterministic sampler.
    pub fn samples(&self) -> Vec<Point> {
        match self {
            InkPath::Polyline(points) => points.clone(),
            InkPath::Cubic {
                start,
                end,
                controls,
            } => plotgram_model::result::EdgePath::cubic(*start, *end, *controls).samples(),
        }
    }

    /// Polyline vertices; panics for cubic (test helper).
    #[cfg(test)]
    fn polyline_points(&self) -> &[Point] {
        match self {
            InkPath::Polyline(points) => points,
            InkPath::Cubic { .. } => panic!("expected polyline ink path"),
        }
    }
}

/// One routed edge, still in canonical (TB) space.
#[derive(Debug)]
pub struct CanonicalEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub path: InkPath,
    pub from_port: ResolvedPort,
    pub to_port: ResolvedPort,
}

/// Elem indices along `edge_id`'s dummy chain in **original** source->target
/// order (properify stores segments in *working* direction; this un-swaps
/// them when the edge was reversed by FAS — composition.md's "FAS 方向不变量").
fn chain_in_original_order(plan: &PlanGraph, edge_id: &str, reversed: bool) -> Vec<usize> {
    let mut segs: Vec<&Segment> = plan
        .segments
        .iter()
        .filter(|s| s.edge_id == edge_id)
        .collect();
    segs.sort_by_key(|s| s.ordinal);
    let mut chain = Vec::with_capacity(segs.len() + 1);
    chain.push(segs[0].from);
    chain.extend(segs.iter().map(|s| s.to));
    if reversed {
        chain.reverse();
    }
    chain
}

/// Jog between `from` and `to` via a Metric track Y when they don't share
/// an axis. Missing track when a jog is required is `InternalInvariant`.
fn append_tracked(
    path: &mut Vec<Point>,
    to: Point,
    track_y: Option<f64>,
    edge_id: &str,
) -> Result<(), LayoutError> {
    let from = *path.last().unwrap();
    if (from.x - to.x).abs() > 1e-9 && (from.y - to.y).abs() > 1e-9 {
        let mid_y = track_y.ok_or_else(|| {
            LayoutError::message(format!(
                "hierarchical: edge `{edge_id}` needs a horizontal track but TrackOrder \
                 has no assignment (InternalInvariant; channel-d1.md)"
            ))
        })?;
        path.push(Point {
            x: from.x,
            y: mid_y,
        });
        path.push(Point { x: to.x, y: mid_y });
    }
    path.push(to);
    Ok(())
}

fn lane_coord(
    edge_id: &str,
    tid: TrackId,
    track_order: &TrackOrderPlan,
    track_coords: &TrackCoords,
) -> Result<f64, LayoutError> {
    let hop = track_order.assignments.get(&(edge_id.to_string(), tid));
    let idx = hop.map(|h| h.track_index).unwrap_or(0);
    track_coords.lane(tid, idx).ok_or_else(|| {
        LayoutError::message(format!(
            "hierarchical: edge `{edge_id}` missing Metric coord for track {:?}",
            tid
        ))
    })
}

/// Own-node frame whose East or West face sits at `port_x` (within eps).
fn face_frame_at_x<'a>(
    plan: &PlanGraph,
    frames: &'a [Rect],
    rank: usize,
    port_x: f64,
) -> Option<&'a Rect> {
    let layer = plan.layers.get(rank)?;
    for &ei in layer {
        let r = frames.get(ei)?;
        let east = r.x + r.width;
        let west = r.x;
        if (east - port_x).abs() < 1e-6 || (west - port_x).abs() < 1e-6 {
            return Some(r);
        }
    }
    None
}

/// Stub from a side port to a Main rail must leave the node outward
/// (East port → rail further East; West → further West).
fn stub_outward_to_rail(port_x: f64, rail_x: f64, frame: &Rect) -> bool {
    let east = frame.x + frame.width;
    let west = frame.x;
    if (east - port_x).abs() < 1e-6 {
        rail_x >= port_x - 1e-9
    } else if (west - port_x).abs() < 1e-6 {
        rail_x <= port_x + 1e-9
    } else {
        false
    }
}

/// True when the open horizontal `(from_x, to_x)` at `y` does not enter any
/// real-node body other than the outward stub's own face. Scans all frames
/// (not only `rank`) so a side stub cannot skim a neighbor on another band
/// that still overlaps `y`.
fn horizontal_clear_at_y(
    plan: &PlanGraph,
    frames: &[Rect],
    _rank: usize,
    from_x: f64,
    to_x: f64,
    y: f64,
) -> bool {
    let lo = from_x.min(to_x);
    let hi = from_x.max(to_x);
    for (ei, r) in frames.iter().enumerate() {
        if plan.elems.get(ei).is_some_and(|e| e.key.is_virtual()) {
            continue;
        }
        if y + 1e-9 < r.y || y > r.y + r.height + 1e-9 {
            continue;
        }
        let nl = r.x;
        let nr = r.x + r.width;
        if nr <= lo + 1e-9 || nl >= hi - 1e-9 {
            continue;
        }
        if (nr - from_x).abs() < 1e-6 || (nl - from_x).abs() < 1e-6 {
            continue;
        }
        return false;
    }
    true
}

fn is_cross_axis_side(side: Side) -> bool {
    matches!(side, Side::East | Side::West)
}

/// Clear outward horizontal stub from a cross-axis port to a Main rail.
fn cross_axis_stub_clear(
    plan: &PlanGraph,
    frames: &[Rect],
    rank: usize,
    port: Point,
    rail_x: f64,
) -> bool {
    let Some(frame) = face_frame_at_x(plan, frames, rank, port.x) else {
        return false;
    };
    stub_outward_to_rail(port.x, rail_x, frame)
        && horizontal_clear_at_y(plan, frames, rank, port.x, rail_x, port.y)
}

/// Leave a node onto a Main rail. Cross-axis ports prefer a horizontal stub
/// at port Y when clear; otherwise drop to `src_gap` first (sibling escape).
fn leave_to_main(
    path: &mut Vec<Point>,
    start: Point,
    src_side: Side,
    src_rank: usize,
    rail_x: f64,
    src_gap: f64,
    plan: &PlanGraph,
    frames: &[Rect],
) {
    let at_port_y = is_cross_axis_side(src_side)
        && cross_axis_stub_clear(plan, frames, src_rank, start, rail_x);
    if at_port_y {
        if (start.x - rail_x).abs() > 1e-9 {
            path.push(Point {
                x: rail_x,
                y: start.y,
            });
        }
    } else {
        if (start.y - src_gap).abs() > 1e-9 {
            path.push(Point {
                x: start.x,
                y: src_gap,
            });
        }
        if (path.last().unwrap().x - rail_x).abs() > 1e-9 {
            path.push(Point {
                x: rail_x,
                y: path.last().unwrap().y,
            });
        }
    }
}

/// Arrive from a Main rail into a port. Final segment follows the port
/// normal when the cross-axis stub is clear (E/W → horizontal at `end.y`).
/// Otherwise fall back to a layer-gap join (may end with a vertical onto
/// the face) so we never pierce siblings just to keep the normal.
fn arrive_from_main(
    path: &mut Vec<Point>,
    end: Point,
    tgt_side: Side,
    tgt_rank: usize,
    rail_x: f64,
    tgt_gap: f64,
    plan: &PlanGraph,
    frames: &[Rect],
) {
    // Ensure we are on the rail first.
    let cur = *path.last().unwrap();
    if (cur.x - rail_x).abs() > 1e-9 {
        path.push(Point {
            x: rail_x,
            y: cur.y,
        });
    }

    if is_cross_axis_side(tgt_side)
        && cross_axis_stub_clear(plan, frames, tgt_rank, end, rail_x)
    {
        if (path.last().unwrap().y - end.y).abs() > 1e-9 {
            path.push(Point {
                x: rail_x,
                y: end.y,
            });
        }
        if (path.last().unwrap().x - end.x).abs() > 1e-9
            || (path.last().unwrap().y - end.y).abs() > 1e-9
        {
            path.push(end);
        }
    } else {
        // N/S, or E/W stub blocked: gap join then into the port.
        if (path.last().unwrap().y - tgt_gap).abs() > 1e-9 {
            path.push(Point {
                x: rail_x,
                y: tgt_gap,
            });
        }
        if (path.last().unwrap().x - end.x).abs() > 1e-9 {
            path.push(Point {
                x: end.x,
                y: tgt_gap,
            });
        }
        if path.last() != Some(&end) {
            path.push(end);
        }
    }
}

/// Expand a ChannelPath into an orthogonal polyline (D1.1).
///
/// Leaving onto Main may drop to a layer-gap Y first so horizontals do not
/// pierce same-layer siblings. Arriving from Main always ends along the
/// port normal (E/W horizontal, N/S vertical) — ink-and-verification §4.
fn expand_channel_path(
    start: Point,
    end: Point,
    channel: &ChannelPath,
    substrate: &Substrate,
    track_order: &TrackOrderPlan,
    track_coords: &TrackCoords,
    edge_id: &str,
    plan: &PlanGraph,
    frames: &[Rect],
    src_rank: usize,
    tgt_rank: usize,
    src_side: Side,
    tgt_side: Side,
    layer_gap: f64,
) -> Result<Vec<Point>, LayoutError> {
    let tracks = &channel.tracks;
    if tracks.is_empty() {
        return Err(LayoutError::message(format!(
            "hierarchical: edge `{edge_id}` has empty ChannelPath"
        )));
    }

    let gap_y = |rank: usize, toward_end: Point, from: Point| -> f64 {
        let line = if toward_end.y + 1e-9 < from.y {
            rank
        } else {
            rank + 1
        };
        layer_gap_y(plan, frames, line, layer_gap)
    };

    if tracks.len() == 1 {
        let tid = tracks[0];
        let orient = substrate
            .track(tid)
            .map(|t| t.orient)
            .ok_or_else(|| LayoutError::message("channel: unknown track"))?;
        let mut path = vec![start];
        match orient {
            TrackOrient::Cross => {
                let y = lane_coord(edge_id, tid, track_order, track_coords)?;
                append_tracked(&mut path, end, Some(y), edge_id)?;
            }
            TrackOrient::Main => {
                let x = lane_coord(edge_id, tid, track_order, track_coords)?;
                let src_gap = gap_y(src_rank, end, start);
                let tgt_gap = gap_y(tgt_rank, start, end);
                leave_to_main(
                    &mut path, start, src_side, src_rank, x, src_gap, plan, frames,
                );
                arrive_from_main(
                    &mut path, end, tgt_side, tgt_rank, x, tgt_gap, plan, frames,
                );
            }
        }
        if path.last() != Some(&end) {
            path.push(end);
        }
        return Ok(path);
    }

    let mut path = vec![start];
    let first = tracks[0];
    let first_orient = substrate.track(first).unwrap().orient;
    let first_c = lane_coord(edge_id, first, track_order, track_coords)?;
    match first_orient {
        TrackOrient::Cross => {
            if (start.y - first_c).abs() > 1e-9 {
                path.push(Point {
                    x: start.x,
                    y: first_c,
                });
            }
        }
        TrackOrient::Main => {
            let src_gap = gap_y(src_rank, end, start);
            leave_to_main(
                &mut path, start, src_side, src_rank, first_c, src_gap, plan, frames,
            );
        }
    }

    for w in tracks.windows(2) {
        let (a, b) = (w[0], w[1]);
        let ta = substrate.track(a).unwrap();
        let tb = substrate.track(b).unwrap();
        let ca = lane_coord(edge_id, a, track_order, track_coords)?;
        let cb = lane_coord(edge_id, b, track_order, track_coords)?;
        let prev = *path.last().unwrap();
        match (ta.orient, tb.orient) {
            (TrackOrient::Cross, TrackOrient::Main) => {
                let junction = Point { x: cb, y: ca };
                if (prev.x - junction.x).abs() > 1e-9 || (prev.y - junction.y).abs() > 1e-9 {
                    if (prev.y - ca).abs() > 1e-9 {
                        path.push(Point { x: prev.x, y: ca });
                    }
                    path.push(junction);
                }
            }
            (TrackOrient::Main, TrackOrient::Cross) => {
                let junction = Point { x: ca, y: cb };
                if (prev.x - junction.x).abs() > 1e-9 || (prev.y - junction.y).abs() > 1e-9 {
                    if (prev.x - ca).abs() > 1e-9 {
                        path.push(Point { x: ca, y: prev.y });
                    }
                    path.push(junction);
                }
            }
            (TrackOrient::Cross, TrackOrient::Cross) => {
                if (prev.y - ca).abs() > 1e-9 {
                    path.push(Point { x: prev.x, y: ca });
                }
                if (ca - cb).abs() > 1e-9 {
                    let cur = *path.last().unwrap();
                    path.push(Point { x: cur.x, y: cb });
                }
            }
            (TrackOrient::Main, TrackOrient::Main) => {
                if (prev.x - ca).abs() > 1e-9 {
                    path.push(Point { x: ca, y: prev.y });
                }
                if (ca - cb).abs() > 1e-9 {
                    let cur = *path.last().unwrap();
                    path.push(Point { x: cb, y: cur.y });
                }
            }
        }
    }

    let last = *tracks.last().unwrap();
    let last_orient = substrate.track(last).unwrap().orient;
    let last_c = lane_coord(edge_id, last, track_order, track_coords)?;
    match last_orient {
        TrackOrient::Cross => {
            let prev = *path.last().unwrap();
            if (prev.x - end.x).abs() > 1e-9 {
                path.push(Point {
                    x: end.x,
                    y: last_c,
                });
            }
        }
        TrackOrient::Main => {
            let tgt_gap = gap_y(tgt_rank, start, end);
            arrive_from_main(
                &mut path, end, tgt_side, tgt_rank, last_c, tgt_gap, plan, frames,
            );
        }
    }
    if path.last() != Some(&end) {
        path.push(end);
    }
    Ok(path)
}

/// Mid-Y of the layer gap at `gap_line` (0 = above first layer), expanded
/// from Metric node frames.
fn layer_gap_y(plan: &PlanGraph, frames: &[Rect], gap_line: usize, layer_gap: f64) -> f64 {
    let n = plan.layers.len();
    if n == 0 {
        return 0.0;
    }
    if gap_line == 0 {
        let top = plan.layers[0]
            .iter()
            .map(|&e| frames[e].y)
            .fold(f64::INFINITY, f64::min);
        return top - layer_gap.max(1.0) * 0.5;
    }
    if gap_line >= n {
        let bot = plan.layers[n - 1]
            .iter()
            .map(|&e| frames[e].bottom())
            .fold(f64::NEG_INFINITY, f64::max);
        return bot + layer_gap.max(1.0) * 0.5;
    }
    let above = plan.layers[gap_line - 1]
        .iter()
        .map(|&e| frames[e].bottom())
        .fold(f64::NEG_INFINITY, f64::max);
    let below = plan.layers[gap_line]
        .iter()
        .map(|&e| frames[e].y)
        .fold(f64::INFINITY, f64::min);
    0.5 * (above + below)
}

/// Bus-internal jog: horizontal rail is the bus Y already on the path, so a
/// mid-gap track is not required. Still no invented mid_y — uses the last
/// point's Y when already on a horizontal, otherwise requires axis share.
fn append_bus_bend(path: &mut Vec<Point>, to: Point) {
    let from = *path.last().unwrap();
    if (from.x - to.x).abs() > 1e-9 && (from.y - to.y).abs() > 1e-9 {
        // Prefer staying on current Y (bus rail) then vertical into `to`.
        path.push(Point {
            x: to.x,
            y: from.y,
        });
    }
    path.push(to);
}

/// Outward unit normal (canonical TB) of a port side.
fn side_normal(side: Side) -> Point {
    match side {
        Side::North => Point { x: 0.0, y: -1.0 },
        Side::South => Point { x: 0.0, y: 1.0 },
        Side::West => Point { x: -1.0, y: 0.0 },
        Side::East => Point { x: 1.0, y: 0.0 },
    }
}

/// Orthogonal bus join: SharedPort → Trunk → Bus → Stub using Metric bus
/// levels. Same PortPoint for all prefix members (no pitch fan).
fn orthogonal_bus_path(
    start: Point,
    end: Point,
    mid_waypoints: &[Point],
    src_bus_y: Option<f64>,
    tgt_bus_y: Option<f64>,
) -> Vec<Point> {
    let mut path = vec![start];
    if let Some(by) = src_bus_y {
        // Shared vertical trunk to the bus level.
        path.push(Point {
            x: start.x,
            y: by,
        });
    }
    for &wp in mid_waypoints {
        if let Some(by) = src_bus_y {
            // Branch horizontally on the bus toward the next column once.
            if (wp.x - path.last().unwrap().x).abs() > 1e-9
                && (path.last().unwrap().y - by).abs() < 1e-9
            {
                path.push(Point { x: wp.x, y: by });
            }
        }
        append_bus_bend(&mut path, wp);
    }
    match tgt_bus_y {
        Some(by) => {
            let prev = *path.last().unwrap();
            if (prev.y - by).abs() > 1e-9 {
                path.push(Point { x: prev.x, y: by });
            }
            if (end.x - path.last().unwrap().x).abs() > 1e-9 {
                path.push(Point { x: end.x, y: by });
            }
            path.push(end);
        }
        None => {
            if let Some(by) = src_bus_y {
                // Adjacent-layer fan: horizontal bus toward target column,
                // then stub into the target port.
                if (end.x - path.last().unwrap().x).abs() > 1e-9 {
                    path.push(Point { x: end.x, y: by });
                }
            }
            append_bus_bend(&mut path, end);
        }
    }
    path
}

#[allow(clippy::too_many_arguments)]
pub fn route_edges(
    graph: &RealGraph,
    plan: &PlanGraph,
    ports: &BTreeMap<String, EdgePorts>,
    frames: &[Rect],
    bus_levels: &BusLevels,
    route_plan: &ChannelRoutePlan,
    track_order: &TrackOrderPlan,
    track_coords: &TrackCoords,
    routing_style: RoutingStyle,
    layer_gap: f64,
    node_gap: f64,
) -> Result<Vec<CanonicalEdge>, LayoutError> {
    let mut out = Vec::with_capacity(graph.edges.len());
    for e in &graph.edges {
        let chain = chain_in_original_order(plan, &e.edge_id, e.reversed);
        let rp = &ports[&e.edge_id];

        let last_i = chain.len() - 1;
        let waypoints: Vec<Point> = chain
            .iter()
            .enumerate()
            .map(|(i, &elem)| {
                if i == 0 {
                    port_anchor(frames[elem], rp.source)
                } else if i == last_i {
                    port_anchor(frames[elem], rp.target)
                } else {
                    frames[elem].center()
                }
            })
            .collect();

        let path = match routing_style {
            RoutingStyle::Polyline => InkPath::Polyline(waypoints),
            RoutingStyle::Curved => {
                for (side, end) in [(rp.source.side, "source"), (rp.target.side, "target")] {
                    if !matches!(side, Side::North | Side::South) {
                        return Err(LayoutError::message(format!(
                            "hierarchical: edge `{}` routing_style `curved` requires \
                             North/South ports, but its {end} port is on {side:?} \
                             (cross-axis ports need Channel routing — roadmap D₁)",
                            e.edge_id
                        )));
                    }
                }
                let start = waypoints[0];
                let end = *waypoints.last().unwrap();
                let d = ((end.y - start.y).abs() / 3.0).clamp(24.0, layer_gap + node_gap);
                let ns = side_normal(rp.source.side);
                let nt = side_normal(rp.target.side);
                let controls = [
                    Point {
                        x: start.x + ns.x * d,
                        y: start.y + ns.y * d,
                    },
                    Point {
                        x: end.x + nt.x * d,
                        y: end.y + nt.y * d,
                    },
                ];
                InkPath::Cubic {
                    start,
                    end,
                    controls,
                }
            }
            _ => {
                let start = waypoints[0];
                let end = *waypoints.last().unwrap();
                let mid = &waypoints[1..waypoints.len() - 1];
                let src_bus = bus_levels.source.get(&e.edge_id).copied();
                let tgt_bus = bus_levels.target.get(&e.edge_id).copied();
                let path = if src_bus.is_some() || tgt_bus.is_some() {
                    orthogonal_bus_path(start, end, mid, src_bus, tgt_bus)
                } else {
                    let topo = route_plan.routes.get(&e.edge_id).ok_or_else(|| {
                        LayoutError::message(format!(
                            "hierarchical: edge `{}` missing Channel RouteTopology \
                             (InternalInvariant; channel-d1.md D1.1)",
                            e.edge_id
                        ))
                    })?;
                    let RouteTopology::Orthogonal(ref channel) = topo;
                    expand_channel_path(
                        start,
                        end,
                        channel,
                        &route_plan.substrate,
                        track_order,
                        track_coords,
                        &e.edge_id,
                        plan,
                        frames,
                        plan.elems[chain[0]].rank as usize,
                        plan.elems[*chain.last().unwrap()].rank as usize,
                        rp.source.side,
                        rp.target.side,
                        layer_gap,
                    )?
                };

                let algo_pts: Vec<_> = path.iter().map(|&p| to_algo_point(p)).collect();
                let normalized = normalize_orthogonal(&algo_pts, &NormalizeOptions::default());
                InkPath::Polyline(normalized.into_iter().map(from_algo_point).collect())
            }
        };

        out.push(CanonicalEdge {
            id: e.edge_id.clone(),
            source: graph.ids[e.original_source].clone(),
            target: graph.ids[e.original_target].clone(),
            path,
            from_port: rp.source,
            to_port: rp.target,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::hierarchical::channel::{
        derive_root_substrate, ChannelPath, ChannelRoutePlan, RouteTopology,
    };
    use crate::layout::hierarchical::compose::track_order::HopTrack;
    use crate::layout::hierarchical::model::{Elem, ElemKey, RealEdge};
    use plotgram_algo::orientation::Side;
    use plotgram_model::port::AlongSpec;

    fn empty_bus() -> BusLevels {
        BusLevels::default()
    }

    /// Minimal Channel plan: single Cross track for `e0` (adjacent-layer).
    fn single_cross_route(plan: &PlanGraph, track_y: f64) -> (ChannelRoutePlan, TrackOrderPlan, TrackCoords) {
        let (substrate, index) = derive_root_substrate(plan);
        let cross_id = index
            .cross_lines
            .get(&1)
            .and_then(|v| v.first().copied())
            .map(|s| s.id)
            .expect("cross line 1");
        let mut routes = BTreeMap::new();
        routes.insert(
            "e0".into(),
            RouteTopology::Orthogonal(ChannelPath {
                tracks: vec![cross_id],
                gates: Vec::new(),
            }),
        );
        let route_plan = ChannelRoutePlan {
            substrate,
            index,
            routes,
            bundles: Vec::new(),
            relaxations: Vec::new(),
            used_gates: false,
            route_order: Vec::new(),
        };
        let mut track_order = TrackOrderPlan::default();
        track_order.assignments.insert(
            ("e0".into(), cross_id),
            HopTrack {
                track: cross_id,
                track_index: 0,
            },
        );
        track_order.track_counts.insert(cross_id, 1);
        track_order.rank_gap_track_counts.insert(0, 1);
        let mut track_coords = TrackCoords::default();
        track_coords.coords.insert(cross_id, vec![track_y]);
        (route_plan, track_order, track_coords)
    }

    fn empty_channel(plan: &PlanGraph) -> ChannelRoutePlan {
        let (substrate, index) = derive_root_substrate(plan);
        ChannelRoutePlan {
            substrate,
            index,
            routes: BTreeMap::new(),
            bundles: Vec::new(),
            relaxations: Vec::new(),
            used_gates: false,
            route_order: Vec::new(),
        }
    }

    fn route(
        graph: &RealGraph,
        plan: &PlanGraph,
        ports: &BTreeMap<String, EdgePorts>,
        frames: &[Rect],
        style: RoutingStyle,
        bus: &BusLevels,
    ) -> Vec<CanonicalEdge> {
        let (rp, to, tc) = if style == RoutingStyle::Orthogonal && bus.source.is_empty() && bus.target.is_empty() {
            // Aligned / bus tests: provide a Cross route when orthogonal.
            single_cross_route(plan, 25.0)
        } else {
            (
                empty_channel(plan),
                TrackOrderPlan::default(),
                TrackCoords::default(),
            )
        };
        route_edges(
            graph, plan, ports, frames, bus, &rp, &to, &tc, style, 60.0, 24.0,
        )
        .unwrap()
    }

    fn route_with(
        graph: &RealGraph,
        plan: &PlanGraph,
        ports: &BTreeMap<String, EdgePorts>,
        frames: &[Rect],
        style: RoutingStyle,
        bus: &BusLevels,
        track_y: f64,
    ) -> Vec<CanonicalEdge> {
        let (rp, to, tc) = single_cross_route(plan, track_y);
        route_edges(
            graph, plan, ports, frames, bus, &rp, &to, &tc, style, 60.0, 24.0,
        )
        .unwrap()
    }

    fn simple_setup() -> (RealGraph, PlanGraph, BTreeMap<String, EdgePorts>, Vec<Rect>) {
        let ids = vec!["a".to_string(), "b".to_string()];
        let index_of = ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), i))
            .collect();
        let edges = vec![RealEdge {
            edge_id: "e0".into(),
            original_source: 0,
            original_target: 1,
            working_source: 0,
            working_target: 1,
            reversed: false,
            from_port: None,
            to_port: None,
            critical: false,
        }];
        let graph = RealGraph {
            ids,
            index_of,
            group_path: vec![Vec::new(); 2],
            shapes: vec![plotgram_model::NodeShape::DEFAULT; 2],
            edges,
            self_loops: Vec::new(),
        };

        let elems = vec![
            Elem {
                key: ElemKey::Real("a".into()),
                group_path: Vec::new(),
                rank: 0,
            },
            Elem {
                key: ElemKey::Real("b".into()),
                group_path: Vec::new(),
                rank: 1,
            },
        ];
        let plan_index = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let segments = vec![Segment {
            edge_id: "e0".into(),
            ordinal: 0,
            from: 0,
            to: 1,
        }];
        let layers = vec![vec![0], vec![1]];
        let plan = PlanGraph {
            elems,
            index_of: plan_index,
            decl_index: vec![0, 1],
            segments,
            layers,
        };

        let mut ports = BTreeMap::new();
        ports.insert(
            "e0".to_string(),
            EdgePorts {
                source: ResolvedPort {
                    side: Side::South,
                    along: AlongSpec::Ordered { order: 0, count: 1 },
                },
                target: ResolvedPort {
                    side: Side::North,
                    along: AlongSpec::Ordered { order: 0, count: 1 },
                },
                source_cluster: None,
                target_cluster: None,
            },
        );

        let frames = vec![
            Rect::new(0.0, 0.0, 20.0, 10.0),
            Rect::new(30.0, 40.0, 20.0, 10.0),
        ];
        (graph, plan, ports, frames)
    }

    #[test]
    fn straight_when_ports_share_x() {
        let (graph, plan, ports, mut frames) = simple_setup();
        frames[1] = Rect::new(0.0, 40.0, 20.0, 10.0);
        let routed = route(
            &graph,
            &plan,
            &ports,
            &frames,
            RoutingStyle::Orthogonal,
            &empty_bus(),
        );
        let pts = routed[0].path.polyline_points();
        assert_eq!(pts.first().unwrap().x, pts.last().unwrap().x);
    }

    #[test]
    fn elbow_inserted_when_axes_differ() {
        let (graph, plan, ports, frames) = simple_setup();
        let routed = route_with(
            &graph,
            &plan,
            &ports,
            &frames,
            RoutingStyle::Orthogonal,
            &empty_bus(),
            25.0,
        );
        let pts = routed[0].path.polyline_points();
        assert!(pts.len() >= 3, "expected an elbow bend, got {:?}", pts);
        assert_eq!(pts.first().copied().unwrap().y, 10.0);
        assert_eq!(pts.last().copied().unwrap().y, 40.0);
        // Horizontal rail at Metric track Y (not mid_y invention).
        assert!(
            pts.iter().any(|p| (p.y - 25.0).abs() < 1e-9),
            "expected track y=25 in {:?}",
            pts
        );
    }

    #[test]
    fn path_endpoints_are_exact_port_anchors() {
        let (graph, plan, ports, frames) = simple_setup();
        let routed = route_with(
            &graph,
            &plan,
            &ports,
            &frames,
            RoutingStyle::Orthogonal,
            &empty_bus(),
            25.0,
        );
        let pts = routed[0].path.polyline_points();
        assert_eq!(
            *pts.first().unwrap(),
            port_anchor(frames[0], ports["e0"].source)
        );
        assert_eq!(
            *pts.last().unwrap(),
            port_anchor(frames[1], ports["e0"].target)
        );
    }

    #[test]
    fn polyline_connects_waypoints_directly() {
        let (graph, plan, ports, frames) = simple_setup();
        let routed = route(
            &graph,
            &plan,
            &ports,
            &frames,
            RoutingStyle::Polyline,
            &empty_bus(),
        );
        let pts = routed[0].path.polyline_points();
        assert_eq!(pts.len(), 2);
        assert_eq!(pts[0], port_anchor(frames[0], ports["e0"].source));
        assert_eq!(pts[1], port_anchor(frames[1], ports["e0"].target));
    }

    #[test]
    fn curved_emits_one_cubic_with_normal_controls() {
        let (graph, plan, ports, frames) = simple_setup();
        let routed = route(
            &graph,
            &plan,
            &ports,
            &frames,
            RoutingStyle::Curved,
            &empty_bus(),
        );
        let start = port_anchor(frames[0], ports["e0"].source);
        let end = port_anchor(frames[1], ports["e0"].target);
        let d = ((end.y - start.y).abs() / 3.0).clamp(24.0, 60.0 + 24.0);
        match &routed[0].path {
            InkPath::Cubic {
                start: s,
                end: e,
                controls,
            } => {
                assert_eq!(*s, start);
                assert_eq!(*e, end);
                assert_eq!(controls[0], Point { x: start.x, y: start.y + d });
                assert_eq!(controls[1], Point { x: end.x, y: end.y - d });
            }
            other => panic!("expected cubic, got {other:?}"),
        }
    }

    #[test]
    fn curved_hard_fails_on_cross_axis_ports() {
        let (graph, plan, mut ports, frames) = simple_setup();
        ports.get_mut("e0").unwrap().source = ResolvedPort {
            side: Side::West,
            along: AlongSpec::Ordered { order: 0, count: 1 },
        };
        let (rp, to, tc) = (
            empty_channel(&plan),
            TrackOrderPlan::default(),
            TrackCoords::default(),
        );
        let err = route_edges(
            &graph,
            &plan,
            &ports,
            &frames,
            &empty_bus(),
            &rp,
            &to,
            &tc,
            RoutingStyle::Curved,
            60.0,
            24.0,
        )
        .unwrap_err();
        assert!(format!("{err}").contains("curved"), "unexpected: {err}");
    }

    /// Bus-grouped source: shared port + trunk to Metric bus_y, then bus
    /// toward the target column.
    #[test]
    fn bus_source_shares_single_trunk() {
        let (graph, plan, ports, frames) = simple_setup();
        let anchor = port_anchor(frames[0], ports["e0"].source);
        let bus_y = anchor.y + 20.0;
        let mut bus = BusLevels::default();
        bus.source.insert("e0".into(), bus_y);
        let routed = route(
            &graph,
            &plan,
            &ports,
            &frames,
            RoutingStyle::Orthogonal,
            &bus,
        );
        let pts = routed[0].path.polyline_points();
        assert_eq!(*pts.first().unwrap(), anchor);
        assert!(
            pts.iter().any(|p| (p.x - anchor.x).abs() < 1e-9 && (p.y - bus_y).abs() < 1e-9),
            "expected trunk tip at ({}, {}), got {:?}",
            anchor.x,
            bus_y,
            pts
        );
    }
}
