//! L3 corridor track assignment + L4 VPSC nudging.
//!
//! Pipeline (architecture.md §4–5; yFiles 03):
//! 1. detect corridors — edges with overlapping collinear segments on one
//!    backbone line;
//! 2. **L3a** — [`color_intervals`] assigns colour labels so overlapping
//!    intervals never share a track;
//! 3. **L3b** — remap colour → spatial order by enter/leave perpendicular
//!    preference (crossing-aware; Wybrow / yFiles §3.2);
//! 4. **L4** — [`nudge_track_coords`] (VPSC) places ordered tracks with gap ≥
//!    `spacing`, minimising displacement from the backbone;
//! 5. shift each edge onto its track; accept only if collision-free.
//!
//! L4 never reorders L3 tracks. Round-2 shared-segment penalties remain
//! complementary (they pull edges into *different* corridors).

use plotgram_algo::interval_color::{color_intervals, Interval};
use plotgram_engine_api::RouteScene;
use plotgram_model::geometry::Point;
use plotgram_model::result::EdgePlacement;

use crate::core::{normalize_polyline, overlap_len};

use super::nudge::nudge_track_coords;
use super::ovg::step_blocked;

const EPS: f64 = 1e-9;

/// A corridor: one backbone line + the edges (indices into the placements
/// slice) that have overlapping segments on it.
struct Corridor {
    horizontal: bool,
    coord: f64,
    edges: Vec<usize>,
}

/// Spread overlapping corridor edges onto VPSC-nudged tracks (L3 + L4).
pub fn spread_tracks(scene: &RouteScene, placements: &mut [EdgePlacement]) {
    if placements.len() < 2 {
        return;
    }
    let segments: Vec<Vec<(Point, Point)>> = placements
        .iter()
        .map(|p| p.path.points.windows(2).map(|w| (w[0], w[1])).collect())
        .collect();

    let mut corridors: Vec<Corridor> = Vec::new();
    for i in 0..placements.len() {
        for j in (i + 1)..placements.len() {
            for &(a, b) in &segments[i] {
                for &(c, d) in &segments[j] {
                    if overlap_len(a, b, c, d) <= EPS {
                        continue;
                    }
                    let (horizontal, coord) = if (a.y - b.y).abs() < EPS {
                        (true, a.y)
                    } else {
                        (false, a.x)
                    };
                    let mut found = None;
                    for (ci, cor) in corridors.iter().enumerate() {
                        if cor.horizontal == horizontal && (cor.coord - coord).abs() < EPS {
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

    let gap = scene.params.spacing;
    for mut cor in corridors {
        // Stable edge order within the corridor (placement index = edge_order).
        cor.edges.sort_unstable();

        // L3a: interval colouring — overlapping spans get distinct track labels.
        let intervals: Vec<Interval> = cor
            .edges
            .iter()
            .map(|&e| backbone_interval(&segments[e], cor.horizontal, cor.coord))
            .collect();
        let color = color_intervals(&intervals, 0.0);
        let track_count = color.iter().copied().max().map_or(0, |t| t + 1);
        if track_count < 2 {
            continue; // everyone shares track 0 → nothing to nudge
        }

        // L3b: crossing-aware spatial order — remap colour labels so tracks
        // whose members prefer the "low" perpendicular side come first.
        // (yFiles 03 §3.2 / Wybrow: enter/leave preference → track order.)
        let preferred: Vec<f64> = cor
            .edges
            .iter()
            .map(|&e| {
                preferred_perp(
                    &placements[e].path.points,
                    cor.horizontal,
                    cor.coord,
                )
            })
            .collect();
        let rank = order_tracks_by_preference(&color, &preferred, track_count);

        // L4: VPSC exact coordinates for spatial ranks 0..track_count.
        let coords = nudge_track_coords(cor.coord, track_count, gap);

        for (local, &e) in cor.edges.iter().enumerate() {
            let spatial = rank[color[local]];
            let offset = coords[spatial] - cor.coord;
            if offset.abs() < EPS {
                continue;
            }
            let edge_id = placements[e].id.as_str();
            let shifted =
                shift_on_line(&placements[e].path.points, cor.horizontal, cor.coord, offset);
            if path_clear(&shifted, scene, edge_id) {
                placements[e].path.points = shifted;
            }
        }
    }
}

/// Preferred perpendicular coordinate for an edge on a corridor backbone.
///
/// Average of the off-backbone neighbours abutting the longest on-line run.
/// Falls back to `coord` when the run has no off-line neighbour
/// (pure stub-to-stub collinear).
fn preferred_perp(points: &[Point], horizontal: bool, coord: f64) -> f64 {
    let on = |p: Point| {
        if horizontal {
            (p.y - coord).abs() < EPS
        } else {
            (p.x - coord).abs() < EPS
        }
    };
    let perp = |p: Point| if horizontal { p.y } else { p.x };

    // Longest contiguous run of on-backbone points (≥ 2 ⇒ at least one segment).
    let mut best: Option<(usize, usize)> = None;
    let mut i = 0;
    while i < points.len() {
        if !on(points[i]) {
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        while i < points.len() && on(points[i]) {
            i += 1;
        }
        let end = i - 1; // inclusive
        if end > start {
            let len = end - start;
            if best.map_or(true, |(s, e)| len > e - s) {
                best = Some((start, end));
            }
        }
    }
    let Some((s, e)) = best else {
        return coord;
    };
    let mut samples = Vec::new();
    if s > 0 {
        samples.push(perp(points[s - 1]));
    }
    if e + 1 < points.len() {
        samples.push(perp(points[e + 1]));
    }
    if samples.is_empty() {
        coord
    } else {
        samples.iter().sum::<f64>() / samples.len() as f64
    }
}

/// Map colour-track id → spatial rank (0 = lowest perp coord).
///
/// Each colour track gets the mean preferred coordinate of its members;
/// tracks are sorted by that mean (tie → colour id).
fn order_tracks_by_preference(
    color: &[usize],
    preferred: &[f64],
    track_count: usize,
) -> Vec<usize> {
    let mut sum = vec![0.0; track_count];
    let mut cnt = vec![0usize; track_count];
    for (i, &t) in color.iter().enumerate() {
        sum[t] += preferred[i];
        cnt[t] += 1;
    }
    let mut order: Vec<usize> = (0..track_count).collect();
    order.sort_by(|&a, &b| {
        let pa = if cnt[a] == 0 {
            f64::INFINITY
        } else {
            sum[a] / cnt[a] as f64
        };
        let pb = if cnt[b] == 0 {
            f64::INFINITY
        } else {
            sum[b] / cnt[b] as f64
        };
        pa.total_cmp(&pb).then(a.cmp(&b))
    });
    let mut rank = vec![0usize; track_count];
    for (spatial, &colour) in order.iter().enumerate() {
        rank[colour] = spatial;
    }
    rank
}

/// Bounding interval of segments that lie on the corridor backbone.
fn backbone_interval(segs: &[(Point, Point)], horizontal: bool, coord: f64) -> Interval {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for &(a, b) in segs {
        let on = if horizontal {
            (a.y - coord).abs() < EPS && (b.y - coord).abs() < EPS
        } else {
            (a.x - coord).abs() < EPS && (b.x - coord).abs() < EPS
        };
        if !on {
            continue;
        }
        if horizontal {
            lo = lo.min(a.x).min(b.x);
            hi = hi.max(a.x).max(b.x);
        } else {
            lo = lo.min(a.y).min(b.y);
            hi = hi.max(a.y).max(b.y);
        }
    }
    if !lo.is_finite() {
        // No on-line segment (should not happen for corridor members); empty.
        Interval::new(0.0, 0.0)
    } else {
        Interval::new(lo, hi)
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
            (a.y - coord).abs() < EPS && (b.y - coord).abs() < EPS
        } else {
            (a.x - coord).abs() < EPS && (b.x - coord).abs() < EPS
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
                i += 2;
                if i + 1 >= points.len() {
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
fn along_offset(from: Point, to: Point, horizontal: bool, offset: f64) -> bool {
    let d = if horizontal {
        to.y - from.y
    } else {
        to.x - from.x
    };
    d.signum() == offset.signum()
}

fn path_clear(points: &[Point], scene: &RouteScene, edge_id: &str) -> bool {
    points
        .windows(2)
        .all(|w| !step_blocked(w[0], w[1], scene, edge_id))
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
        let report = crate::verify::verify_all(&scene, &p);
        assert!(report.all_pass, "failures: {:?}", report.failures());
        // e0/e1/e2 must not fully coincide after L3+L4 separation.
        assert_ne!(p[0].path.points, p[1].path.points);
        assert_ne!(p[0].path.points, p[2].path.points);
        // e2's shifted corridor run must not dip back to the shared backbone:
        // after leaving the stub, the long horizontal run stays on one y.
        let pts = &p[2].path.points;
        assert!(pts.len() >= 4, "e2 too short: {pts:?}");
        // Find the longest horizontal segment — it is the corridor ride.
        let mut best: Option<(usize, f64)> = None;
        for (i, w) in pts.windows(2).enumerate() {
            if (w[0].y - w[1].y).abs() < EPS {
                let len = (w[1].x - w[0].x).abs();
                if best.map_or(true, |(_, l)| len > l) {
                    best = Some((i, len));
                }
            }
        }
        let (hi, _) = best.expect("e2 must have a horizontal corridor run");
        let run_y = pts[hi].y;
        assert!(
            (run_y - 110.0).abs() > 1.0,
            "e2 corridor run must leave backbone y=110: {pts:?}"
        );
        // No dip: the run's two endpoints share run_y; neighbours connect
        // without an intermediate return to 110 on that span.
        assert_eq!(pts[hi].y, pts[hi + 1].y);
    }

    #[test]
    fn shift_line_keeps_orthogonal_and_endpoints() {
        let p = |x: f64, y: f64| Point { x, y };
        let pts = vec![p(80.0, 70.0), p(200.0, 70.0)];
        let out = shift_on_line(&pts, true, 70.0, 20.0);
        assert_eq!(
            out,
            vec![p(80.0, 70.0), p(80.0, 90.0), p(200.0, 90.0), p(200.0, 70.0)]
        );
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
        let against = vec![p(80.0, 70.0), p(120.0, 70.0), p(120.0, 40.0)];
        let out = shift_on_line(&against, true, 70.0, 20.0);
        assert_eq!(
            out,
            vec![p(80.0, 70.0), p(80.0, 90.0), p(120.0, 90.0), p(120.0, 40.0)]
        );
        let v = vec![p(70.0, 0.0), p(70.0, 120.0), p(150.0, 120.0)];
        let out = shift_on_line(&v, false, 70.0, 20.0);
        assert_eq!(
            out,
            vec![p(70.0, 0.0), p(90.0, 0.0), p(90.0, 120.0), p(150.0, 120.0)]
        );
    }

    #[test]
    fn backbone_interval_spans_on_line_segments() {
        let segs = vec![
            (Point { x: 0.0, y: 10.0 }, Point { x: 5.0, y: 10.0 }),
            (Point { x: 5.0, y: 10.0 }, Point { x: 5.0, y: 20.0 }),
            (Point { x: 5.0, y: 10.0 }, Point { x: 15.0, y: 10.0 }),
        ];
        let iv = backbone_interval(&segs, true, 10.0);
        assert!((iv.lo - 0.0).abs() < EPS);
        assert!((iv.hi - 15.0).abs() < EPS);
    }

    #[test]
    fn preferred_perp_reads_off_line_neighbours() {
        let p = |x: f64, y: f64| Point { x, y };
        // From below: neighbours at y=150 around a y=100 run.
        let from_below = vec![
            p(50.0, 150.0),
            p(50.0, 100.0),
            p(250.0, 100.0),
            p(250.0, 150.0),
        ];
        assert!((preferred_perp(&from_below, true, 100.0) - 150.0).abs() < EPS);
        // From above.
        let from_above = vec![
            p(50.0, 50.0),
            p(50.0, 100.0),
            p(250.0, 100.0),
            p(250.0, 50.0),
        ];
        assert!((preferred_perp(&from_above, true, 100.0) - 50.0).abs() < EPS);
    }

    #[test]
    fn preference_order_inverts_colour_when_needed() {
        // Colour: e0→0, e1→1. Preferences: e0 wants high y, e1 wants low y.
        // Spatial rank must put colour 1 first (low), colour 0 second.
        let color = vec![0, 1];
        let preferred = vec![150.0, 50.0];
        let rank = order_tracks_by_preference(&color, &preferred, 2);
        assert_eq!(rank[1], 0, "colour 1 (prefers low) → spatial 0");
        assert_eq!(rank[0], 1, "colour 0 (prefers high) → spatial 1");
    }

    #[test]
    fn invert_paths_separate_without_crossing() {
        use plotgram_engine_api::{OrthogonalRouteParams, PortAnchor, TerminalPair};
        use plotgram_model::port::Side;
        use plotgram_model::result::EdgePath;
        use std::collections::BTreeMap;

        let p = |x: f64, y: f64| Point { x, y };
        // Pre-L3 paths sharing y=100; e0 from below, e1 from above.
        let mut placements = vec![
            EdgePlacement {
                id: "e0".into(),
                source: "bl".into(),
                target: "br".into(),
                path: EdgePath {
                    points: vec![
                        p(50.0, 150.0),
                        p(50.0, 100.0),
                        p(250.0, 100.0),
                        p(250.0, 150.0),
                    ],
                },
                from_port: None,
                to_port: None,
            },
            EdgePlacement {
                id: "e1".into(),
                source: "tl".into(),
                target: "tr".into(),
                path: EdgePath {
                    points: vec![
                        p(50.0, 50.0),
                        p(50.0, 100.0),
                        p(250.0, 100.0),
                        p(250.0, 50.0),
                    ],
                },
                from_port: None,
                to_port: None,
            },
        ];
        let mut terminals = BTreeMap::new();
        terminals.insert(
            "e0".into(),
            TerminalPair {
                source: PortAnchor {
                    point: p(50.0, 150.0),
                    side: Side::North,
                    node_id: "bl".into(),
                },
                target: PortAnchor {
                    point: p(250.0, 150.0),
                    side: Side::North,
                    node_id: "br".into(),
                },
            },
        );
        terminals.insert(
            "e1".into(),
            TerminalPair {
                source: PortAnchor {
                    point: p(50.0, 50.0),
                    side: Side::South,
                    node_id: "tl".into(),
                },
                target: PortAnchor {
                    point: p(250.0, 50.0),
                    side: Side::South,
                    node_id: "tr".into(),
                },
            },
        );
        let scene = RouteScene {
            obstacles: vec![],
            terminals,
            edge_order: vec!["e0".into(), "e1".into()],
            group_boundaries: vec![],
            boundary_permissions: BTreeMap::new(),
            params: OrthogonalRouteParams {
                spacing: 20.0,
                ..Default::default()
            },
        };

        // Before: paths share y=100 → 0 geometric crossings yet, but wrong
        // track order after naive colour would cross stubs. After spread:
        let before = crate::score::score_scene("before", &scene, &placements);
        spread_tracks(&scene, &mut placements);
        let after = crate::score::score_scene("after", &scene, &placements);
        assert_eq!(after.crossings, 0, "preference order must avoid stub crosses");
        // Corridor runs must sit on opposite sides of y=100.
        let run_y = |pts: &[Point]| {
            pts.windows(2)
                .find(|w| (w[0].y - w[1].y).abs() < EPS && (w[1].x - w[0].x).abs() > 50.0)
                .map(|w| w[0].y)
                .unwrap()
        };
        let y0 = run_y(&placements[0].path.points);
        let y1 = run_y(&placements[1].path.points);
        assert!(y1 < 100.0, "e1 (from above) must ride north of backbone: {y1}");
        assert!(y0 > 100.0, "e0 (from below) must ride south of backbone: {y0}");
        assert!(
            (y0 - y1).abs() >= scene.params.spacing - 1e-6,
            "gap: y0={y0} y1={y1}"
        );
        let _ = before; // used for local debugging when comparing
    }
}
