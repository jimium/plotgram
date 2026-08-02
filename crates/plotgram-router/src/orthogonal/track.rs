//! L3-M1 track separation: spread overlapping collinear segments across a
//! shared corridor by uniform track offsets (architecture.md §5 "Nudging:
//! 均匀 track 间距（M1）").
//!
//! Pure post-processing on routed paths:
//! 1. detect corridors — groups of edges whose segments overlap on one
//!    backbone line (horizontal or vertical);
//! 2. order each corridor's edges by `edge_order` position (edge id tie);
//! 3. shift edge k ≥ 1 by `k × spacing` perpendicular to the backbone;
//!    a shifted path is accepted only when it stays collision-free
//!    (own nodes exempt), otherwise the edge keeps its path.
//!
//! Shared-segment penalties (round 2) pull edges into *different* corridors;
//! this pass handles the *same* corridor, so both mechanisms are
//! complementary and deterministic (no `HashMap` iteration).

use plotgram_engine_api::RouteScene;
use plotgram_model::geometry::Point;
use plotgram_model::result::EdgePlacement;

use crate::core::{normalize_polyline, overlap_len};

use super::ovg::segment_blocked;

/// A corridor: one backbone line + the edges (indices into the placements
/// slice, in processing order) that have overlapping segments on it.
struct Corridor {
    horizontal: bool,
    coord: f64,
    edges: Vec<usize>,
}

/// Shift edges sharing a corridor onto distinct tracks (§5 L3-M1).
pub fn spread_tracks(scene: &RouteScene, placements: &mut [EdgePlacement]) {
    if placements.len() < 2 {
        return;
    }
    let segments: Vec<Vec<(Point, Point)>> = placements
        .iter()
        .map(|p| p.path.points.windows(2).map(|w| (w[0], w[1])).collect())
        .collect();

    // Detect corridors from pairwise segment overlap.
    let mut corridors: Vec<Corridor> = Vec::new();
    for i in 0..placements.len() {
        for j in (i + 1)..placements.len() {
            for &(a, b) in &segments[i] {
                for &(c, d) in &segments[j] {
                    if overlap_len(a, b, c, d) <= 1e-9 {
                        continue;
                    }
                    let (horizontal, coord) = if (a.y - b.y).abs() < 1e-9 {
                        (true, a.y)
                    } else {
                        (false, a.x)
                    };
                    let mut found = None;
                    for (ci, cor) in corridors.iter().enumerate() {
                        if cor.horizontal == horizontal && (cor.coord - coord).abs() < 1e-9 {
                            found = Some(ci);
                            break;
                        }
                    }
                    match found {
                        Some(ci) => {
                            for &e in &[i, j] {
                                if !corridors[ci].edges.contains(&e) {
                                    corridors[ci].edges.push(e);
                                }
                            }
                        }
                        None => corridors.push(Corridor {
                            horizontal,
                            coord,
                            edges: vec![i, j],
                        }),
                    }
                }
            }
        }
    }

    for mut cor in corridors {
        cor.edges.sort_unstable();
        for (k, &e) in cor.edges.iter().enumerate() {
            if k == 0 {
                continue; // first edge keeps its path (track 0)
            }
            let offset = k as f64 * scene.params.spacing;
            let exempt = [placements[e].source.as_str(), placements[e].target.as_str()];
            // Try both perpendicular directions; first collision-free wins.
            for off in [offset, -offset] {
                let shifted =
                    shift_on_line(&placements[e].path.points, cor.horizontal, cor.coord, off);
                if path_clear(&shifted, scene, &exempt) {
                    placements[e].path.points = shifted;
                    break;
                }
            }
        }
    }
}

/// Translate every segment of `points` that lies on the corridor backbone by
/// `offset` perpendicular to it, reconnecting through the original endpoints.
fn shift_on_line(points: &[Point], horizontal: bool, coord: f64, offset: f64) -> Vec<Point> {
    let shift = |p: Point| {
        if horizontal {
            Point {
                x: p.x,
                y: p.y + offset,
            }
        } else {
            Point {
                x: p.x + offset,
                y: p.y,
            }
        }
    };
    let on_line = |a: Point, b: Point| {
        if horizontal {
            (a.y - coord).abs() < 1e-9 && (b.y - coord).abs() < 1e-9
        } else {
            (a.x - coord).abs() < 1e-9 && (b.x - coord).abs() < 1e-9
        }
    };
    let mut out: Vec<Point> = Vec::with_capacity(points.len() * 2);
    let mut i = 0;
    while i + 1 < points.len() {
        let (a, b) = (points[i], points[i + 1]);
        out.push(a);
        if on_line(a, b) {
            out.push(shift(a));
            if i + 2 < points.len() && along_offset(b, points[i + 2], horizontal, offset) {
                // The next original segment runs in the offset direction:
                // merge the return connector into it (skip the un-shifted
                // bend `b`) instead of dipping back to the backbone line.
                out.push(shift(b));
                i += 2; // consume [a,b]; the next iteration pushes `c`
                if i + 1 >= points.len() {
                    // `c` is the last point: emit it and stop.
                    out.push(points[i]);
                    break;
                }
                continue;
            }
            out.push(shift(b));
        }
        out.push(b);
        i += 1;
    }
    normalize_polyline(&out)
}

/// Does `to` lie from `from` in the offset direction (same axis, same sign)?
/// Only called for a segment that follows an on-line segment, so it is
/// perpendicular to the backbone; sign match means the connector can be
/// merged without reversing travel.
fn along_offset(from: Point, to: Point, horizontal: bool, offset: f64) -> bool {
    let d = if horizontal {
        to.y - from.y
    } else {
        to.x - from.x
    };
    d.signum() == offset.signum()
}

/// Is every segment of `points` clear of all obstacles except the edge's own
/// nodes? Mirrors the acceptance gate in `verify`.
fn path_clear(points: &[Point], scene: &RouteScene, exempt: &[&str; 2]) -> bool {
    points
        .windows(2)
        .all(|w| !segment_blocked(w[0], w[1], &scene.obstacles, exempt, scene.params.spacing))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fanout_e2_merges_return_connector() {
        use plotgram_engine_api::{EdgeRouter, Obstacle, PortAnchor, TerminalPair};
        use plotgram_model::geometry::Rect;
        use plotgram_model::port::Side;
        use std::collections::BTreeMap;
        let obs = |id: &str, x: f64, y: f64, w: f64, h: f64| Obstacle {
            id: id.to_string(),
            rect: Rect::new(x, y, w, h),
        };
        let a = |x: f64, y: f64, side: Side, n: &str| PortAnchor {
            point: Point { x, y },
            side,
            node_id: n.to_string(),
        };
        let mut terminals = BTreeMap::new();
        terminals.insert(
            "e0".to_string(),
            TerminalPair {
                source: a(80.0, 110.0, Side::East, "a"),
                target: a(200.0, 20.0, Side::West, "b"),
            },
        );
        terminals.insert(
            "e1".to_string(),
            TerminalPair {
                source: a(80.0, 110.0, Side::East, "a"),
                target: a(300.0, 100.0, Side::West, "c"),
            },
        );
        terminals.insert(
            "e2".to_string(),
            TerminalPair {
                source: a(80.0, 110.0, Side::East, "a"),
                target: a(200.0, 220.0, Side::West, "d"),
            },
        );
        let scene = plotgram_engine_api::RouteScene {
            obstacles: vec![
                obs("a", 0.0, 80.0, 80.0, 60.0),
                obs("b", 200.0, 0.0, 80.0, 40.0),
                obs("c", 300.0, 80.0, 80.0, 40.0),
                obs("d", 200.0, 200.0, 80.0, 40.0),
            ],
            terminals,
            edge_order: vec!["e0".to_string(), "e1".to_string(), "e2".to_string()],
            group_boundaries: vec![],
            boundary_permissions: BTreeMap::new(),
            params: Default::default(),
        };
        let p = crate::OrthogonalEdgeRouter.route(&scene).unwrap();
        let pts: Vec<Vec<Point>> = p.iter().map(|pl| pl.path.points.clone()).collect();
        eprintln!("[dbg] paths = {pts:?}");
        // e2's shifted corridor run must not dip back to the backbone:
        // its run (y=150) connects straight into the (190,220) descent.
        assert_eq!(pts[2].len(), 5, "e2 should have 5 points, got {:?}", pts[2]);
        assert_eq!(
            pts[2],
            vec![
                Point { x: 80.0, y: 110.0 },
                Point { x: 80.0, y: 150.0 },
                Point { x: 190.0, y: 150.0 },
                Point { x: 190.0, y: 220.0 },
                Point { x: 200.0, y: 220.0 },
            ]
        );
    }

    #[test]
    fn shift_line_keeps_orthogonal_and_endpoints() {
        let p = |x: f64, y: f64| Point { x, y };
        // Straight corridor segment between two anchors.
        let pts = vec![p(80.0, 70.0), p(200.0, 70.0)];
        let out = shift_on_line(&pts, true, 70.0, 20.0);
        assert_eq!(
            out,
            vec![p(80.0, 70.0), p(80.0, 90.0), p(200.0, 90.0), p(200.0, 70.0)]
        );
        // L path whose next segment runs in the offset direction: the return
        // connector merges into it (no dip back to the backbone).
        let l = vec![p(80.0, 70.0), p(120.0, 70.0), p(120.0, 100.0)];
        let out = shift_on_line(&l, true, 70.0, 20.0);
        assert_eq!(
            out,
            vec![
                p(80.0, 70.0),
                p(80.0, 90.0),
                p(120.0, 90.0),
                p(120.0, 100.0)
            ]
        );
        // L path whose next segment runs *against* the offset: the connector
        // dips back and the monotone three-point run collapses via normalize
        // (still no dangling bend).
        let against = vec![p(80.0, 70.0), p(120.0, 70.0), p(120.0, 40.0)];
        let out = shift_on_line(&against, true, 70.0, 20.0);
        assert_eq!(
            out,
            vec![p(80.0, 70.0), p(80.0, 90.0), p(120.0, 90.0), p(120.0, 40.0)]
        );
        // Vertical corridor with a following segment along the offset.
        let v = vec![p(70.0, 0.0), p(70.0, 120.0), p(150.0, 120.0)];
        let out = shift_on_line(&v, false, 70.0, 20.0);
        assert_eq!(
            out,
            vec![p(70.0, 0.0), p(90.0, 0.0), p(90.0, 120.0), p(150.0, 120.0)]
        );
    }
}
