//! InkVerifier: orthogonal path post-checks (ink-and-verification.md §7).
//!
//! - Segment-level collinear overlap > `edge_gap/2` between non-bundle edges
//!   is reported as a soft relaxation (P5-5); identical whole paths still
//!   hard-fail. Hard segment-overlap gate lands once Cross lane separation
//!   drives `overlap_len` to 0 across the corpus.
//! - An orthogonal path must not intersect the **open interior** of any
//!   non-endpoint node obstacle (ink-and-verification.md §7.3 item 4).
//! - Polyline endpoints must match `port_anchor` within 1e-9 (P5-5).
//! - The segment adjacent to each port must run along the port's outward
//!   normal (v1 audit E1 direction clause; ink-and-verification.md §4).

use std::collections::{BTreeMap, BTreeSet};

use plotgram_algo::orientation::Side;
use plotgram_engine_api::LayoutError;
use plotgram_model::diagnostics::Relaxation;
use plotgram_model::geometry::{Point, Rect};
use plotgram_model::result::{EdgePlacement, GroupPlacement};

use crate::layout::hierarchical::compose::bundle::{edges_share_bundle, BundlePlan};
use crate::layout::hierarchical::ink::route::{CanonicalEdge, InkPath};
use crate::layout::hierarchical::metric::anchor::port_anchor;

const EPS: f64 = 1e-6;
const ENDPOINT_EPS: f64 = 1e-9;
/// Shrink obstacle frames slightly so port-boundary grazing and float noise
/// do not count as penetration.
const OBSTACLE_INSET: f64 = 1e-3;

fn polyline_samples(path: &InkPath) -> Option<Vec<Point>> {
    match path {
        InkPath::Polyline(pts) => Some(pts.clone()),
        InkPath::Cubic { .. } => None,
    }
}

fn polylines_equal(a: &[Point], b: &[Point]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .all(|(p, q)| (p.x - q.x).abs() < EPS && (p.y - q.y).abs() < EPS)
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum SegDir {
    H,
    V,
}

struct BucketSeg {
    edge_id: String,
    lo: f64,
    hi: f64,
}

/// Hard-fail on identical whole polylines (non-bundle). Segment-level
/// overlaps become [`segment_overlap_relaxations`].
pub fn verify_no_illegal_overlap(
    edges: &[CanonicalEdge],
    bundles: &[BundlePlan],
    _edge_gap: f64,
) -> Result<(), LayoutError> {
    let samples: Vec<(String, Vec<Point>)> = edges
        .iter()
        .filter_map(|e| polyline_samples(&e.path).map(|pts| (e.id.clone(), pts)))
        .collect();

    for i in 0..samples.len() {
        for j in (i + 1)..samples.len() {
            let (a_id, a_pts) = &samples[i];
            let (b_id, b_pts) = &samples[j];
            if !polylines_equal(a_pts, b_pts) {
                continue;
            }
            if edges_share_bundle(bundles, a_id, b_id) {
                continue;
            }
            return Err(LayoutError::message(format!(
                "hierarchical: edges `{a_id}` and `{b_id}` have identical paths \
                 but are not co-members of any BundlePlan (illegal collinearity; \
                 ink-and-verification.md §5)"
            )));
        }
    }
    Ok(())
}

/// Soft P5-5 segment overlaps: length > `edge_gap/2` on a shared axis line.
pub fn segment_overlap_relaxations(
    edges: &[CanonicalEdge],
    bundles: &[BundlePlan],
    edge_gap: f64,
) -> Vec<Relaxation> {
    let thresh = (edge_gap * 0.5).max(0.0);
    let mut buckets: BTreeMap<(SegDir, i64), Vec<BucketSeg>> = BTreeMap::new();
    let mut out = Vec::new();

    for e in edges {
        let Some(pts) = polyline_samples(&e.path) else {
            continue;
        };
        for w in pts.windows(2) {
            let (a, b) = (w[0], w[1]);
            if (a.x - b.x).abs() <= EPS && (a.y - b.y).abs() <= EPS {
                continue;
            }
            if (a.y - b.y).abs() <= EPS {
                let key = (SegDir::H, quantize(a.y));
                let (lo, hi) = (a.x.min(b.x), a.x.max(b.x));
                buckets.entry(key).or_default().push(BucketSeg {
                    edge_id: e.id.clone(),
                    lo,
                    hi,
                });
            } else if (a.x - b.x).abs() <= EPS {
                let key = (SegDir::V, quantize(a.x));
                let (lo, hi) = (a.y.min(b.y), a.y.max(b.y));
                buckets.entry(key).or_default().push(BucketSeg {
                    edge_id: e.id.clone(),
                    lo,
                    hi,
                });
            }
        }
    }

    let mut seen: BTreeMap<(String, String), f64> = BTreeMap::new();
    for segs in buckets.values() {
        for i in 0..segs.len() {
            for j in (i + 1)..segs.len() {
                let a = &segs[i];
                let b = &segs[j];
                if a.edge_id == b.edge_id {
                    continue;
                }
                let overlap = a.hi.min(b.hi) - a.lo.max(b.lo);
                if overlap <= thresh + EPS {
                    continue;
                }
                if edges_share_bundle(bundles, &a.edge_id, &b.edge_id) {
                    continue;
                }
                let key = if a.edge_id <= b.edge_id {
                    (a.edge_id.clone(), b.edge_id.clone())
                } else {
                    (b.edge_id.clone(), a.edge_id.clone())
                };
                let e = seen.entry(key).or_insert(0.0);
                *e = e.max(overlap);
            }
        }
    }
    for ((a, b), overlap) in seen {
        out.push(Relaxation {
            rule: "ink-segment-overlap".into(),
            detail: format!(
                "edges `{a}` and `{b}` collinear overlap {overlap:.3} > edge_gap/2={thresh:.3}"
            ),
        });
    }
    out
}

fn quantize(v: f64) -> i64 {
    (v / EPS).round() as i64
}

/// Hard-fail when an orthogonal polyline intersects the open interior of a
/// non-endpoint node. When `require_orthogonal` is true (orthogonal routing
/// style), non-axis-aligned segments are also an InternalInvariant failure.
pub fn verify_no_node_penetration(
    edges: &[CanonicalEdge],
    node_frames: &[(String, Rect)],
    require_orthogonal: bool,
) -> Result<(), LayoutError> {
    for e in edges {
        let Some(pts) = polyline_samples(&e.path) else {
            continue; // curved paths are not subject to this orthogonal check
        };
        for w in pts.windows(2) {
            let (a, b) = (w[0], w[1]);
            let non_ortho = (a.x - b.x).abs() > EPS && (a.y - b.y).abs() > EPS;
            if non_ortho {
                if require_orthogonal {
                    return Err(LayoutError::message(format!(
                        "hierarchical: InternalInvariant — edge `{}` has non-orthogonal \
                         segment under orthogonal routing_style",
                        e.id
                    )));
                }
                continue;
            }
            for (nid, frame) in node_frames {
                if nid == &e.source || nid == &e.target {
                    continue;
                }
                if segment_hits_rect_interior(a, b, *frame) {
                    return Err(LayoutError::message(format!(
                        "hierarchical: edge `{}` penetrates node `{}` \
                         (ink-and-verification.md §7.3) \
                         seg=({:.3},{:.3})->({:.3},{:.3}) \
                         frame=[{:.3},{:.3}]x[{:.3},{:.3}]",
                        e.id,
                        nid,
                        a.x,
                        a.y,
                        b.x,
                        b.y,
                        frame.x,
                        frame.x + frame.width,
                        frame.y,
                        frame.y + frame.height
                    )));
                }
            }
        }
    }
    Ok(())
}

/// Endpoints of each polyline must equal `port_anchor` within 1e-9 (P5-5).
pub fn verify_endpoints_exact(
    edges: &[CanonicalEdge],
    node_frames: &[(String, Rect)],
) -> Result<(), LayoutError> {
    let frames: BTreeMap<&str, Rect> = node_frames
        .iter()
        .map(|(id, r)| (id.as_str(), *r))
        .collect();
    for e in edges {
        let Some(pts) = polyline_samples(&e.path) else {
            continue;
        };
        let Some(first) = pts.first() else {
            return Err(LayoutError::message(format!(
                "hierarchical: edge `{}` has empty polyline",
                e.id
            )));
        };
        let Some(last) = pts.last() else {
            continue;
        };
        let Some(&src_frame) = frames.get(e.source.as_str()) else {
            continue;
        };
        let Some(&tgt_frame) = frames.get(e.target.as_str()) else {
            continue;
        };
        let want_s = port_anchor(src_frame, e.from_port);
        let want_t = port_anchor(tgt_frame, e.to_port);
        if (first.x - want_s.x).abs() > ENDPOINT_EPS || (first.y - want_s.y).abs() > ENDPOINT_EPS {
            return Err(LayoutError::message(format!(
                "hierarchical: edge `{}` source endpoint ({}, {}) ≠ port_anchor ({}, {})",
                e.id, first.x, first.y, want_s.x, want_s.y
            )));
        }
        if (last.x - want_t.x).abs() > ENDPOINT_EPS || (last.y - want_t.y).abs() > ENDPOINT_EPS {
            return Err(LayoutError::message(format!(
                "hierarchical: edge `{}` target endpoint ({}, {}) ≠ port_anchor ({}, {})",
                e.id, last.x, last.y, want_t.x, want_t.y
            )));
        }
    }
    Ok(())
}

/// Count orthogonal bends in a polyline (`n` points → `n.saturating_sub(2)`).
pub fn polyline_bend_count(path: &InkPath) -> Option<usize> {
    let pts = polyline_samples(path)?;
    Some(pts.len().saturating_sub(2))
}

/// Hard-fail when the segment adjacent to a port does not run along the
/// port's outward normal (v1 audit E1 direction clause; a tangential stub
/// rides the node face). Zero-length segments at either end are skipped —
/// collinear merging does not change direction. Length is deliberately not
/// guarded here; only direction. When `require_orthogonal` is set, a
/// non-axis-aligned stub is an InternalInvariant.
pub fn verify_port_stubs_normal(
    edges: &[CanonicalEdge],
    require_orthogonal: bool,
) -> Result<(), LayoutError> {
    for e in edges {
        let Some(pts) = polyline_samples(&e.path) else {
            continue; // curved paths have no orthogonal stubs
        };
        if pts.len() < 2 {
            continue;
        }
        check_stub_dir(&e.id, &pts, e.from_port.side, true, require_orthogonal)?;
        check_stub_dir(&e.id, &pts, e.to_port.side, false, require_orthogonal)?;
    }
    Ok(())
}

fn check_stub_dir(
    edge_id: &str,
    pts: &[Point],
    side: Side,
    at_source: bool,
    require_orthogonal: bool,
) -> Result<(), LayoutError> {
    // First non-zero-length segment at the relevant end.
    let (a, b) = if at_source {
        let mut i = 0;
        while i + 1 < pts.len() && seg_len(pts[i], pts[i + 1]) <= EPS {
            i += 1;
        }
        if i + 1 >= pts.len() {
            return Ok(());
        }
        (pts[i], pts[i + 1])
    } else {
        let mut i = pts.len();
        while i >= 2 && seg_len(pts[i - 2], pts[i - 1]) <= EPS {
            i -= 1;
        }
        if i < 2 {
            return Ok(());
        }
        // Outward from the target port: last → second-last.
        (pts[i - 1], pts[i - 2])
    };
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let non_ortho = dx.abs() > EPS && dy.abs() > EPS;
    if non_ortho {
        if require_orthogonal {
            return Err(LayoutError::message(format!(
                "hierarchical: InternalInvariant — edge `{edge_id}` has a \
                 non-orthogonal port stub under orthogonal routing_style"
            )));
        }
        return Ok(());
    }
    if !dir_matches_normal(dx, dy, side) {
        let end = if at_source { "source" } else { "target" };
        return Err(LayoutError::message(format!(
            "hierarchical: edge `{edge_id}` {end} stub ({dx:.3}, {dy:.3}) is not \
             along the {side:?} port normal — path rides the node face \
             (ink-and-verification.md §4)"
        )));
    }
    Ok(())
}

fn seg_len(a: Point, b: Point) -> f64 {
    (b.x - a.x).abs().max((b.y - a.y).abs())
}

fn dir_matches_normal(dx: f64, dy: f64, side: Side) -> bool {
    match side {
        Side::North => dy < -EPS && dx.abs() <= EPS,
        Side::South => dy > EPS && dx.abs() <= EPS,
        Side::West => dx < -EPS && dy.abs() <= EPS,
        Side::East => dx > EPS && dy.abs() <= EPS,
    }
}

/// True when the closed axis-aligned segment intersects the **open** interior
/// of `frame` (after a tiny inset for float / boundary grazing).
pub fn segment_hits_rect_interior(a: Point, b: Point, frame: Rect) -> bool {
    let left = frame.x + OBSTACLE_INSET;
    let right = frame.right() - OBSTACLE_INSET;
    let top = frame.y + OBSTACLE_INSET;
    let bottom = frame.bottom() - OBSTACLE_INSET;
    if left >= right - EPS || top >= bottom - EPS {
        return false;
    }

    if (a.x - b.x).abs() <= EPS {
        let vx = a.x;
        if vx <= left + EPS || vx >= right - EPS {
            return false;
        }
        let y0 = a.y.min(b.y);
        let y1 = a.y.max(b.y);
        y0 < bottom - EPS && y1 > top + EPS
    } else if (a.y - b.y).abs() <= EPS {
        let hy = a.y;
        if hy <= top + EPS || hy >= bottom - EPS {
            return false;
        }
        let x0 = a.x.min(b.x);
        let x1 = a.x.max(b.x);
        x0 < right - EPS && x1 > left + EPS
    } else {
        false
    }
}

/// One group-penetration violation found by [`group_penetration_violations`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct GroupPenetrationViolation {
    pub edge: String,
    pub group: String,
}

/// List form of [`verify_no_group_penetration`] so gates can choose
/// hard-fail vs observation (strong-macro.md §6 SM-4).
///
/// Semantics mirror the v1 substrate L6 checker: an edge segment entering
/// the **open interior** of a group frame is a violation unless the group
/// is on either endpoint's group-ancestor chain (`allowed`, computed by the
/// caller from the parsed graph — descendants of a foreign group are never
/// whitelisted here). Boundary grazing does not count (OBSTACLE_INSET).
pub fn group_penetration_violations(
    edges: &[EdgePlacement],
    groups: &[GroupPlacement],
    allowed: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<GroupPenetrationViolation> {
    let mut out = Vec::new();
    for e in edges {
        let pts = e.path.samples();
        if pts.len() < 2 {
            continue;
        }
        for g in groups {
            if let Some(chain) = allowed.get(&e.id) {
                if chain.contains(&g.id) {
                    continue;
                }
            }
            let hit = pts
                .windows(2)
                .any(|w| segment_hits_open_rect(w[0], w[1], g.frame));
            if hit {
                out.push(GroupPenetrationViolation {
                    edge: e.id.clone(),
                    group: g.id.clone(),
                });
            }
        }
    }
    out.sort();
    out
}

/// Hard gate over [`group_penetration_violations`]: any violation fails the
/// run with the deterministic violation list.
pub fn verify_no_group_penetration(
    edges: &[EdgePlacement],
    groups: &[GroupPlacement],
    allowed: &BTreeMap<String, BTreeSet<String>>,
) -> Result<(), LayoutError> {
    let violations = group_penetration_violations(edges, groups, allowed);
    if violations.is_empty() {
        return Ok(());
    }
    let head: Vec<String> = violations
        .iter()
        .take(5)
        .map(|v| format!("edge `{}` into group `{}`", v.edge, v.group))
        .collect();
    let more = violations.len().saturating_sub(head.len());
    let suffix = if more > 0 {
        format!(" (+{more} more)")
    } else {
        String::new()
    };
    Err(LayoutError::message(format!(
        "hierarchical: {} edge/group penetration violation(s): {}{suffix} \
         (strong-macro.md §6 SM-4)",
        violations.len(),
        head.join("; ")
    )))
}

/// General-segment variant of [`segment_hits_rect_interior`]: true when the
/// closed segment `a → b` (any orientation) meets the open interior of
/// `frame`. Parametric t-interval intersection with the inset rectangle.
fn segment_hits_open_rect(a: Point, b: Point, frame: Rect) -> bool {
    let left = frame.x + OBSTACLE_INSET;
    let right = frame.right() - OBSTACLE_INSET;
    let top = frame.y + OBSTACLE_INSET;
    let bottom = frame.bottom() - OBSTACLE_INSET;
    if left >= right || top >= bottom {
        return false;
    }
    let (Some(x), Some(y)) = (
        open_axis_t_interval(a.x, b.x, left, right),
        open_axis_t_interval(a.y, b.y, top, bottom),
    ) else {
        return false;
    };
    x.0 <= y.1 && y.0 <= x.1
}

/// t ∈ [0,1] where the axis projection `p0 → p1` lies strictly inside
/// `(lo, hi)`; `None` = never inside.
fn open_axis_t_interval(p0: f64, p1: f64, lo: f64, hi: f64) -> Option<(f64, f64)> {
    let d = p1 - p0;
    if d.abs() <= f64::EPSILON {
        return (p0 > lo && p0 < hi).then_some((0.0, 1.0));
    }
    let mut t_lo = (lo - p0) / d;
    let mut t_hi = (hi - p0) / d;
    if t_lo > t_hi {
        std::mem::swap(&mut t_lo, &mut t_hi);
    }
    t_lo = t_lo.max(0.0);
    t_hi = t_hi.min(1.0);
    (t_lo <= t_hi).then_some((t_lo, t_hi))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::hierarchical::compose::bundle::{BundleKind, BundlePlan};
    use crate::layout::hierarchical::compose::ports::ResolvedPort;
    use plotgram_algo::orientation::Side;
    use plotgram_model::port::AlongSpec;
    use plotgram_model::result::EdgePath;

    fn edge(id: &str, source: &str, target: &str, pts: Vec<Point>) -> CanonicalEdge {
        edge_sides(id, source, target, Side::South, Side::South, pts)
    }

    fn edge_sides(
        id: &str,
        source: &str,
        target: &str,
        from_side: Side,
        to_side: Side,
        pts: Vec<Point>,
    ) -> CanonicalEdge {
        CanonicalEdge {
            id: id.into(),
            source: source.into(),
            target: target.into(),
            path: InkPath::Polyline(pts),
            from_port: ResolvedPort {
                side: from_side,
                along: AlongSpec::Ordered { order: 0, count: 1 },
            },
            to_port: ResolvedPort {
                side: to_side,
                along: AlongSpec::Ordered { order: 0, count: 1 },
            },
        }
    }

    #[test]
    fn identical_paths_without_bundle_fail() {
        let pts = vec![Point { x: 0.0, y: 0.0 }, Point { x: 0.0, y: 10.0 }];
        let edges = vec![edge("e0", "a", "b", pts.clone()), edge("e1", "a", "c", pts)];
        let err = verify_no_illegal_overlap(&edges, &[], 16.0).unwrap_err();
        assert!(err.to_string().contains("BundlePlan"));
    }

    #[test]
    fn identical_paths_with_bundle_ok() {
        let pts = vec![Point { x: 0.0, y: 0.0 }, Point { x: 0.0, y: 10.0 }];
        let edges = vec![edge("e0", "a", "b", pts.clone()), edge("e1", "a", "c", pts)];
        let bundles = vec![BundlePlan {
            id: "b".into(),
            kind: BundleKind::SourcePrefix,
            member_edges: vec!["e0".into(), "e1".into()],
        }];
        assert!(verify_no_illegal_overlap(&edges, &bundles, 16.0).is_ok());
    }

    #[test]
    fn segment_overlap_emits_relaxation() {
        let edges = vec![
            edge(
                "e0",
                "a",
                "b",
                vec![Point { x: 0.0, y: 5.0 }, Point { x: 20.0, y: 5.0 }],
            ),
            edge(
                "e1",
                "a",
                "c",
                vec![Point { x: 5.0, y: 5.0 }, Point { x: 25.0, y: 5.0 }],
            ),
        ];
        let relax = segment_overlap_relaxations(&edges, &[], 16.0);
        assert_eq!(relax.len(), 1);
        assert!(relax[0].rule.contains("segment-overlap"));
    }

    #[test]
    fn path_through_foreign_node_fails() {
        let edges = vec![edge(
            "e0",
            "a",
            "c",
            vec![Point { x: 5.0, y: 0.0 }, Point { x: 5.0, y: 50.0 }],
        )];
        let frames = vec![(
            "b".into(),
            Rect {
                x: 0.0,
                y: 10.0,
                width: 20.0,
                height: 20.0,
            },
        )];
        let err = verify_no_node_penetration(&edges, &frames, true).unwrap_err();
        assert!(err.to_string().contains("penetrates"));
    }

    #[test]
    fn path_grazing_endpoint_and_clear_corridor_ok() {
        let edges = vec![edge(
            "e0",
            "a",
            "b",
            vec![Point { x: 10.0, y: 0.0 }, Point { x: 10.0, y: 40.0 }],
        )];
        let frames = vec![
            (
                "a".into(),
                Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 20.0,
                    height: 10.0,
                },
            ),
            (
                "b".into(),
                Rect {
                    x: 0.0,
                    y: 40.0,
                    width: 20.0,
                    height: 10.0,
                },
            ),
        ];
        assert!(verify_no_node_penetration(&edges, &frames, true).is_ok());
    }

    #[test]
    fn non_orthogonal_segment_fails_when_required() {
        let edges = vec![edge(
            "e0",
            "a",
            "b",
            vec![Point { x: 0.0, y: 0.0 }, Point { x: 10.0, y: 10.0 }],
        )];
        let err = verify_no_node_penetration(&edges, &[], true).unwrap_err();
        assert!(err.to_string().contains("non-orthogonal"));
        assert!(verify_no_node_penetration(&edges, &[], false).is_ok());
    }

    #[test]
    fn table_driven_segment_vs_rect() {
        let frame = Rect {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        };
        let cases = [
            (Point { x: 5.0, y: -5.0 }, Point { x: 5.0, y: 15.0 }, true),
            (Point { x: -5.0, y: 5.0 }, Point { x: 15.0, y: 5.0 }, true),
            (Point { x: 0.0, y: -5.0 }, Point { x: 0.0, y: 15.0 }, false),
            (Point { x: 20.0, y: 0.0 }, Point { x: 20.0, y: 10.0 }, false),
        ];
        for (a, b, want) in cases {
            assert_eq!(
                segment_hits_rect_interior(a, b, frame),
                want,
                "seg {a:?}->{b:?}"
            );
        }
    }

    /// Port stub direction audit: tangential first/last segments hard-fail,
    /// normal ones pass (both ends checked).
    #[test]
    fn port_stub_direction_audit() {
        struct Case {
            label: &'static str,
            from_side: Side,
            to_side: Side,
            pts: Vec<Point>,
            ok: bool,
        }
        let cases = [
            Case {
                label: "S-port tangential first segment",
                from_side: Side::South,
                to_side: Side::North,
                pts: vec![
                    Point { x: 10.0, y: 10.0 },
                    Point { x: 30.0, y: 10.0 },
                    Point { x: 30.0, y: 40.0 },
                ],
                ok: false,
            },
            Case {
                label: "S-port normal stub",
                from_side: Side::South,
                to_side: Side::North,
                pts: vec![
                    Point { x: 10.0, y: 10.0 },
                    Point { x: 10.0, y: 22.0 },
                    Point { x: 30.0, y: 22.0 },
                    Point { x: 30.0, y: 40.0 },
                ],
                ok: true,
            },
            Case {
                label: "E-port normal stub",
                from_side: Side::East,
                to_side: Side::West,
                pts: vec![
                    Point { x: 20.0, y: 5.0 },
                    Point { x: 32.0, y: 5.0 },
                    Point { x: 32.0, y: 45.0 },
                    Point { x: 40.0, y: 45.0 },
                ],
                ok: true,
            },
            Case {
                label: "N-port normal last segment",
                from_side: Side::South,
                to_side: Side::North,
                pts: vec![
                    Point { x: 10.0, y: 10.0 },
                    Point { x: 10.0, y: 30.0 },
                    Point { x: 30.0, y: 30.0 },
                    Point { x: 30.0, y: 40.0 },
                ],
                ok: true,
            },
            Case {
                label: "S-port entering from below tangentially",
                from_side: Side::South,
                to_side: Side::South,
                pts: vec![
                    Point { x: 10.0, y: 10.0 },
                    Point { x: 10.0, y: 55.0 },
                    Point { x: 30.0, y: 55.0 },
                ],
                ok: false,
            },
        ];
        for case in &cases {
            let edges = vec![edge_sides(
                "e0",
                "a",
                "b",
                case.from_side,
                case.to_side,
                case.pts.clone(),
            )];
            let res = verify_port_stubs_normal(&edges, true);
            assert_eq!(res.is_ok(), case.ok, "{}: {res:?}", case.label);
            if !case.ok {
                assert!(
                    res.unwrap_err().to_string().contains("port normal"),
                    "{}: error must name the violated normal",
                    case.label
                );
            }
        }
    }

    fn placed_edge(id: &str, pts: Vec<Point>) -> EdgePlacement {
        EdgePlacement {
            id: id.into(),
            source: "a".into(),
            target: "b".into(),
            path: EdgePath::polyline(pts),
            from_port: None,
            to_port: None,
        }
    }

    fn group_at(id: &str, frame: Rect) -> GroupPlacement {
        GroupPlacement {
            id: id.into(),
            frame,
        }
    }

    #[test]
    fn group_penetration_detects_crossing_and_respects_allowed() {
        let groups = vec![group_at("g1", Rect::new(0.0, 0.0, 100.0, 100.0))];
        // Straight through g1's interior.
        let cross = vec![placed_edge(
            "e0",
            vec![Point { x: 50.0, y: -20.0 }, Point { x: 50.0, y: 120.0 }],
        )];
        let v = group_penetration_violations(&cross, &groups, &BTreeMap::new());
        assert_eq!(
            v,
            vec![GroupPenetrationViolation {
                edge: "e0".into(),
                group: "g1".into()
            }]
        );
        assert!(verify_no_group_penetration(&cross, &groups, &BTreeMap::new()).is_err());

        // Same edge with g1 on an endpoint's ancestor chain → legal.
        let mut allowed = BTreeMap::new();
        allowed.insert("e0".to_string(), BTreeSet::from(["g1".to_string()]));
        assert!(group_penetration_violations(&cross, &groups, &allowed).is_empty());

        // Grazing along the frame boundary is not penetration.
        let graze = vec![placed_edge(
            "e1",
            vec![Point { x: 0.0, y: -20.0 }, Point { x: 0.0, y: 120.0 }],
        )];
        assert!(group_penetration_violations(&graze, &groups, &BTreeMap::new()).is_empty());

        // Segment ending exactly on the boundary (port entry) is fine.
        let into = vec![placed_edge(
            "e2",
            vec![Point { x: 50.0, y: -20.0 }, Point { x: 50.0, y: 0.0 }],
        )];
        assert!(group_penetration_violations(&into, &groups, &BTreeMap::new()).is_empty());
    }
}
