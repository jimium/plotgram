//! Ink: straight organic edges. Zero new decisions.
//!
//! yFiles OrganicLayout default edge style is the straight line; curved
//! variants belong to the router stage (`EdgeGeometryMode::DeferToRouter`).

use std::collections::BTreeMap;

use plotgram_engine_api::EdgeGeometryMode;
use plotgram_model::geometry::{Point, Rect};
use plotgram_model::graph::Graph;
use plotgram_model::port::{AlongSpec, PortRef};
use plotgram_model::result::{EdgePath, EdgePlacement};

use super::geom::{offset_segment, rect_boundary_toward, side_of};
use super::plan::OrganicMetric;

/// Parallel-edge fanout pitch (px).
const PARALLEL_PITCH: f64 = 6.0;
/// Self-loop outward flick length (px).
const LOOP_FLICK: f64 = 12.0;

pub fn expand(
    graph: &Graph,
    metric: &OrganicMetric,
    mode: EdgeGeometryMode,
) -> Vec<EdgePlacement> {
    // Declaration-order index within each node pair, for parallel offsets.
    let mut seen: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut counts: BTreeMap<(String, String), usize> = BTreeMap::new();
    for edge in graph.edges_in_declaration_order() {
        if edge.source == edge.target {
            continue;
        }
        let key = pair_key(&edge.source, &edge.target);
        *counts.entry(key).or_insert(0) += 1;
    }

    let mut out = Vec::new();
    for edge in graph.edges_in_declaration_order() {
        let Some(src) = metric.frames.get(&edge.source) else {
            continue;
        };
        let Some(tgt) = metric.frames.get(&edge.target) else {
            continue;
        };

        let (path, from_side, to_side) = if edge.source == edge.target {
            let pts = self_loop_points(src);
            let side = side_of(pts[1], src.center());
            (EdgePath::polyline(pts), side, side)
        } else {
            let key = pair_key(&edge.source, &edge.target);
            let n = counts[&key];
            let entry = seen.entry(key).or_insert(0);
            let idx = *entry;
            *entry += 1;

            let (mut start, mut end) = (
                rect_boundary_toward(src, tgt.center()),
                rect_boundary_toward(tgt, src.center()),
            );
            if n > 1 {
                let amount = (idx as f64 - (n as f64 - 1.0) / 2.0) * PARALLEL_PITCH;
                (start, end) = offset_segment(start, end, amount);
            }
            let path = if mode == EdgeGeometryMode::DeferToRouter {
                EdgePath::polyline(Vec::new())
            } else {
                EdgePath::polyline(vec![start, end])
            };
            (
                path,
                side_of(start, src.center()),
                side_of(end, tgt.center()),
            )
        };

        out.push(EdgePlacement {
            id: edge.id.clone(),
            source: edge.source.clone(),
            target: edge.target.clone(),
            path,
            from_port: Some(port(from_side)),
            to_port: Some(port(to_side)),
        });
    }
    out
}

fn pair_key(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

/// Self-loop: small deterministic flick to the right of the node.
fn self_loop_points(src: &Rect) -> Vec<Point> {
    let c = src.center();
    vec![
        Point {
            x: src.right(),
            y: c.y - src.height / 4.0,
        },
        Point {
            x: src.right() + LOOP_FLICK,
            y: c.y,
        },
        Point {
            x: src.right(),
            y: c.y + src.height / 4.0,
        },
    ]
}

fn port(side: plotgram_model::port::Side) -> PortRef {
    PortRef {
        side,
        along: AlongSpec::Ordered { order: 0, count: 1 },
    }
}
