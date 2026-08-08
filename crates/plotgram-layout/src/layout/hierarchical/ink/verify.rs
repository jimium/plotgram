//! InkVerifier: orthogonal path post-checks (ink-and-verification.md §7).
//!
//! - Segment-level collinear overlap > `edge_gap/2` between non-bundle edges
//!   is reported as a soft relaxation (P5-5); identical whole paths still
//!   hard-fail. Hard segment-overlap gate lands once Cross lane separation
//!   drives `overlap_len` to 0 across the corpus.
//! - An orthogonal path must not intersect the **open interior** of any
//!   non-endpoint node obstacle (ink-and-verification.md §7.3 item 4).
//! - Polyline endpoints must match `port_anchor` within 1e-9 (P5-5).

use std::collections::BTreeMap;

use plotgram_engine_api::LayoutError;
use plotgram_model::diagnostics::Relaxation;
use plotgram_model::geometry::{Point, Rect};

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
                         (ink-and-verification.md §7.3)",
                        e.id, nid
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::hierarchical::compose::bundle::{BundleKind, BundlePlan};
    use crate::layout::hierarchical::compose::ports::ResolvedPort;
    use plotgram_algo::orientation::Side;
    use plotgram_model::port::AlongSpec;

    fn edge(id: &str, source: &str, target: &str, pts: Vec<Point>) -> CanonicalEdge {
        let port = ResolvedPort {
            side: Side::South,
            along: AlongSpec::Ordered { order: 0, count: 1 },
        };
        CanonicalEdge {
            id: id.into(),
            source: source.into(),
            target: target.into(),
            path: InkPath::Polyline(pts),
            from_port: port,
            to_port: port,
        }
    }

    #[test]
    fn identical_paths_without_bundle_fail() {
        let pts = vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 0.0, y: 10.0 },
        ];
        let edges = vec![
            edge("e0", "a", "b", pts.clone()),
            edge("e1", "a", "c", pts),
        ];
        let err = verify_no_illegal_overlap(&edges, &[], 16.0).unwrap_err();
        assert!(err.to_string().contains("BundlePlan"));
    }

    #[test]
    fn identical_paths_with_bundle_ok() {
        let pts = vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 0.0, y: 10.0 },
        ];
        let edges = vec![
            edge("e0", "a", "b", pts.clone()),
            edge("e1", "a", "c", pts),
        ];
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
            vec![
                Point { x: 5.0, y: 0.0 },
                Point { x: 5.0, y: 50.0 },
            ],
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
            vec![
                Point { x: 10.0, y: 0.0 },
                Point { x: 10.0, y: 40.0 },
            ],
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
            vec![
                Point { x: 0.0, y: 0.0 },
                Point { x: 10.0, y: 10.0 },
            ],
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
            (
                Point { x: 5.0, y: -5.0 },
                Point { x: 5.0, y: 15.0 },
                true,
            ),
            (
                Point { x: -5.0, y: 5.0 },
                Point { x: 15.0, y: 5.0 },
                true,
            ),
            (
                Point { x: 0.0, y: -5.0 },
                Point { x: 0.0, y: 15.0 },
                false,
            ),
            (
                Point { x: 20.0, y: 0.0 },
                Point { x: 20.0, y: 10.0 },
                false,
            ),
        ];
        for (a, b, want) in cases {
            assert_eq!(
                segment_hits_rect_interior(a, b, frame),
                want,
                "seg {a:?}->{b:?}"
            );
        }
    }
}
