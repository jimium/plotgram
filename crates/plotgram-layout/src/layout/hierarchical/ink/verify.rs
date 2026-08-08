//! InkVerifier: orthogonal path post-checks (ink-and-verification.md §7).
//!
//! - Complete geometric coincidence between two edges is allowed only when
//!   both belong to the same [`BundlePlan`] (sole intentional-collinearity
//!   exemption).
//! - An orthogonal path must not intersect the **open interior** of any
//!   non-endpoint node obstacle (ink-and-verification.md §7.3 item 4).

use plotgram_engine_api::LayoutError;
use plotgram_model::geometry::{Point, Rect};

use crate::layout::hierarchical::compose::bundle::{edges_share_bundle, BundlePlan};
use crate::layout::hierarchical::ink::route::{CanonicalEdge, InkPath};

const EPS: f64 = 1e-6;
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
    a.iter().zip(b.iter()).all(|(p, q)| {
        (p.x - q.x).abs() < EPS && (p.y - q.y).abs() < EPS
    })
}

/// Hard-fail when two non-bundle edges have identical orthogonal polylines.
pub fn verify_no_illegal_overlap(
    edges: &[CanonicalEdge],
    bundles: &[BundlePlan],
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

/// Hard-fail when an orthogonal polyline intersects the open interior of a
/// node that is neither the edge source nor the edge target.
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
        // Vertical segment at x = vx.
        let vx = a.x;
        if vx <= left + EPS || vx >= right - EPS {
            return false;
        }
        let y0 = a.y.min(b.y);
        let y1 = a.y.max(b.y);
        y0 < bottom - EPS && y1 > top + EPS
    } else if (a.y - b.y).abs() <= EPS {
        // Horizontal segment at y = hy.
        let hy = a.y;
        if hy <= top + EPS || hy >= bottom - EPS {
            return false;
        }
        let x0 = a.x.min(b.x);
        let x1 = a.x.max(b.x);
        x0 < right - EPS && x1 > left + EPS
    } else {
        false // non-orthogonal — caller may hard-fail separately
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
        let err = verify_no_illegal_overlap(&edges, &[]).unwrap_err();
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
        assert!(verify_no_illegal_overlap(&edges, &bundles).is_ok());
    }

    #[test]
    fn path_through_foreign_node_fails() {
        // Vertical corridor at x=50 runs straight through obstacle [40,40]–[60,60].
        let edges = vec![edge(
            "e0",
            "a",
            "b",
            vec![
                Point { x: 50.0, y: 0.0 },
                Point { x: 50.0, y: 100.0 },
            ],
        )];
        let frames = vec![
            ("a".into(), Rect::new(40.0, -10.0, 20.0, 10.0)),
            ("b".into(), Rect::new(40.0, 100.0, 20.0, 10.0)),
            ("mid".into(), Rect::new(40.0, 40.0, 20.0, 20.0)),
        ];
        let err = verify_no_node_penetration(&edges, &frames, true).unwrap_err();
        assert!(err.to_string().contains("penetrates node `mid`"), "{err}");
    }

    #[test]
    fn path_grazing_endpoint_and_clear_corridor_ok() {
        // a at top, b at bottom; mid is to the right — vertical at x=10 misses it.
        let edges = vec![edge(
            "e0",
            "a",
            "b",
            vec![
                Point { x: 10.0, y: 10.0 },
                Point { x: 10.0, y: 90.0 },
            ],
        )];
        let frames = vec![
            ("a".into(), Rect::new(0.0, 0.0, 20.0, 10.0)),
            ("b".into(), Rect::new(0.0, 90.0, 20.0, 10.0)),
            ("mid".into(), Rect::new(40.0, 40.0, 20.0, 20.0)),
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
        let frames = vec![
            ("a".into(), Rect::new(-5.0, -5.0, 10.0, 10.0)),
            ("b".into(), Rect::new(5.0, 5.0, 10.0, 10.0)),
        ];
        let err = verify_no_node_penetration(&edges, &frames, true).unwrap_err();
        assert!(err.to_string().contains("non-orthogonal"), "{err}");
        assert!(verify_no_node_penetration(&edges, &frames, false).is_ok());
    }

    #[test]
    fn table_driven_segment_vs_rect() {
        let r = Rect::new(10.0, 10.0, 20.0, 20.0); // [10,10]–[30,30]
        let cases: &[(&str, Point, Point, bool)] = &[
            (
                "vertical through",
                Point { x: 20.0, y: 0.0 },
                Point { x: 20.0, y: 40.0 },
                true,
            ),
            (
                "horizontal through",
                Point { x: 0.0, y: 20.0 },
                Point { x: 40.0, y: 20.0 },
                true,
            ),
            (
                "miss left",
                Point { x: 5.0, y: 0.0 },
                Point { x: 5.0, y: 40.0 },
                false,
            ),
            (
                "graze left boundary",
                Point { x: 10.0, y: 0.0 },
                Point { x: 10.0, y: 40.0 },
                false,
            ),
            (
                "above only",
                Point { x: 20.0, y: 0.0 },
                Point { x: 20.0, y: 10.0 },
                false,
            ),
        ];
        for (name, a, b, want) in cases {
            assert_eq!(
                segment_hits_rect_interior(*a, *b, r),
                *want,
                "case `{name}`"
            );
        }
    }
}
