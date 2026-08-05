//! InkVerifier: orthogonal path post-checks (ink-and-verification.md §7).
//!
//! Complete geometric coincidence between two edges is allowed only when both
//! belong to the same [`BundlePlan`] (sole intentional-collinearity exemption).

use plotgram_engine_api::LayoutError;
use plotgram_model::geometry::Point;

use crate::layout::hierarchical::compose::bundle::{edges_share_bundle, BundlePlan};
use crate::layout::hierarchical::compose::ports::ResolvedPort;
use crate::layout::hierarchical::ink::route::{CanonicalEdge, InkPath};
use plotgram_algo::orientation::Side;
use plotgram_model::port::AlongSpec;

const EPS: f64 = 1e-6;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::hierarchical::compose::bundle::{BundleKind, BundlePlan};

    fn edge(id: &str, pts: Vec<Point>) -> CanonicalEdge {
        let port = ResolvedPort {
            side: Side::South,
            along: AlongSpec::Ordered { order: 0, count: 1 },
        };
        CanonicalEdge {
            id: id.into(),
            source: "a".into(),
            target: "b".into(),
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
        let edges = vec![edge("e0", pts.clone()), edge("e1", pts)];
        let err = verify_no_illegal_overlap(&edges, &[]).unwrap_err();
        assert!(err.to_string().contains("BundlePlan"));
    }

    #[test]
    fn identical_paths_with_bundle_ok() {
        let pts = vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 0.0, y: 10.0 },
        ];
        let edges = vec![edge("e0", pts.clone()), edge("e1", pts)];
        let bundles = vec![BundlePlan {
            id: "b".into(),
            kind: BundleKind::SourcePrefix,
            member_edges: vec!["e0".into(), "e1".into()],
        }];
        assert!(verify_no_illegal_overlap(&edges, &bundles).is_ok());
    }
}
