//! Zero-span undirected edges routed as same-layer side-links
//! (`compose::properify::split_intra_layer` parks them in
//! `RealGraph::intra_layer` — composition.md's `Undirected` stage fact).
//! Like `selfloop.rs`, these edges never touch ordering/properify/channel:
//! geometry is expanded here directly into [`CanonicalEdge`]s.
//!
//! Routing: endpoints take facing East/West ports at mid-height.
//! - Nothing between them (no elem on the same rank whose cross center lies
//!   strictly between, virtuals included) → straight horizontal segment.
//! - Blocked → U-shape detour through the adjacent layer seam: the gap above
//!   when `rank > 0`, the gap below for the top rank. The seam clears every
//!   blocker's frame by `edge_gap / 2`.
//!
//! Known limitation: the U-shape seam does not consult group-shell forbidden
//! bands — the side-link may ride a group frame's pad (tracked separately as
//! the back-edge / side-link shell-awareness follow-up).

use plotgram_algo::orientation::Side;
use plotgram_model::geometry::{Point, Rect};
use plotgram_model::port::AlongSpec;

use crate::layout::hierarchical::compose::ports::ResolvedPort;
use crate::layout::hierarchical::ink::route::{CanonicalEdge, InkPath};
use crate::layout::hierarchical::model::{PlanGraph, RealEdge};

/// `frames` is the canonical element frame table (elem-indexed; real nodes
/// occupy the first `ids.len()` slots in declaration order — same convention
/// `selfloop.rs` relies on).
pub fn intra_layer_edges(
    edges: &[RealEdge],
    ids: &[String],
    plan: &PlanGraph,
    frames: &[Rect],
    edge_gap: f64,
) -> Vec<CanonicalEdge> {
    let mut out = Vec::with_capacity(edges.len());
    for e in edges {
        let s = e.original_source;
        let t = e.original_target;
        let fs = frames[s];
        let ft = frames[t];
        // Cross-axis order decides which endpoint exits East vs West.
        let (left, right) = if fs.x + fs.width * 0.5 <= ft.x + ft.width * 0.5 {
            (s, t)
        } else {
            (t, s)
        };
        let (fl, fr) = (frames[left], frames[right]);
        let cl = fl.x + fl.width * 0.5;
        let cr = fr.x + fr.width * 0.5;
        let rank = plan.elems[left].rank;

        let blockers: Vec<Rect> = plan
            .layers
            .get(rank as usize)
            .map(|layer| {
                layer
                    .iter()
                    .filter(|&&ei| {
                        if ei == s || ei == t {
                            return false;
                        }
                        let f = frames[ei];
                        let cx = f.x + f.width * 0.5;
                        cx > cl && cx < cr
                    })
                    .map(|&ei| frames[ei])
                    .collect()
            })
            .unwrap_or_default();

        let src_side = if s == left { Side::East } else { Side::West };
        let tgt_side = if t == left { Side::East } else { Side::West };
        let from_port = mid_port(src_side, fs);
        let to_port = mid_port(tgt_side, ft);
        let p0 = anchor(src_side, fs);
        let p5 = anchor(tgt_side, ft);

        let points = if blockers.is_empty() {
            vec![p0, p5]
        } else {
            let seam_y = if rank > 0 {
                let mut top = fl.y.min(fr.y);
                for b in &blockers {
                    top = top.min(b.y);
                }
                top - edge_gap * 0.5
            } else {
                let mut bottom = fl.bottom().max(fr.bottom());
                for b in &blockers {
                    bottom = bottom.max(b.bottom());
                }
                bottom + edge_gap * 0.5
            };
            // Small horizontal stubs keep the port-adjacent segments on the
            // East/West outward normal (ink verify `port_stubs_normal`).
            let gap = (fr.x - fl.right()).max(0.0);
            let stub = edge_gap.min(gap / 3.0);
            let d_src = if src_side == Side::East { stub } else { -stub };
            let d_tgt = if tgt_side == Side::East { stub } else { -stub };
            let p1 = Point {
                x: p0.x + d_src,
                y: p0.y,
            };
            let p4 = Point {
                x: p5.x + d_tgt,
                y: p5.y,
            };
            vec![
                p0,
                p1,
                Point { x: p1.x, y: seam_y },
                Point { x: p4.x, y: seam_y },
                p4,
                p5,
            ]
        };

        out.push(CanonicalEdge {
            id: e.edge_id.clone(),
            source: ids[s].clone(),
            target: ids[t].clone(),
            path: InkPath::Polyline(points),
            from_port,
            to_port,
        });
    }
    out
}

fn mid_port(side: Side, frame: Rect) -> ResolvedPort {
    let x = match side {
        Side::East => frame.width,
        Side::West => 0.0,
        _ => unreachable!("side-link ports are East/West only"),
    };
    ResolvedPort {
        side,
        along: AlongSpec::LocalOffset(Point {
            x,
            y: frame.height * 0.5,
        }),
    }
}

fn anchor(side: Side, frame: Rect) -> Point {
    match side {
        Side::East => Point {
            x: frame.right(),
            y: frame.y + frame.height * 0.5,
        },
        Side::West => Point {
            x: frame.x,
            y: frame.y + frame.height * 0.5,
        },
        _ => unreachable!("side-link ports are East/West only"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::hierarchical::model::{Elem, ElemKey};
    use std::collections::BTreeMap;

    /// Two real nodes on rank 0, optional blocker between them.
    fn fixture(blocked: bool) -> (Vec<RealEdge>, Vec<String>, PlanGraph, Vec<Rect>) {
        let ids = vec!["a".to_string(), "b".to_string()];
        let mut elems = vec![
            Elem {
                key: ElemKey::Real("a".into()),
                group_path: Vec::new(),
                rank: 0,
            },
            Elem {
                key: ElemKey::Real("b".into()),
                group_path: Vec::new(),
                rank: 0,
            },
        ];
        let mut frames = vec![
            Rect::new(0.0, 0.0, 40.0, 20.0),   // a
            Rect::new(160.0, 0.0, 40.0, 20.0), // b
        ];
        let mut layer = vec![0usize, 1];
        if blocked {
            elems.push(Elem {
                key: ElemKey::Real("m".into()),
                group_path: Vec::new(),
                rank: 0,
            });
            frames.push(Rect::new(80.0, 0.0, 40.0, 20.0));
            layer = vec![0, 2, 1];
        }
        let index_of: BTreeMap<ElemKey, usize> = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let decl_index = (0..elems.len()).collect();
        let plan = PlanGraph {
            elems,
            index_of,
            decl_index,
            segments: Vec::new(),
            layers: vec![layer],
        };
        let edges = vec![RealEdge {
            edge_id: "e0".into(),
            original_source: 0,
            original_target: 1,
            working_source: 0,
            working_target: 1,
            ..Default::default()
        }];
        (edges, ids, plan, frames)
    }

    #[test]
    fn clear_lane_routes_straight_horizontal() {
        let (edges, ids, plan, frames) = fixture(false);
        let out = intra_layer_edges(&edges, &ids, &plan, &frames, 8.0);
        assert_eq!(out.len(), 1);
        let e = &out[0];
        assert_eq!(e.source, "a");
        assert_eq!(e.target, "b");
        let pts = e.path.samples();
        assert_eq!(pts.len(), 2);
        // Endpoints exactly at the facing mid-height ports.
        assert_eq!(pts[0], Point { x: 40.0, y: 10.0 });
        assert_eq!(pts[1], Point { x: 160.0, y: 10.0 });
        assert!(pts.iter().all(|p| (p.y - 10.0).abs() < 1e-9));
        assert_eq!(e.from_port.side, Side::East);
        assert_eq!(e.to_port.side, Side::West);
    }

    #[test]
    fn blocked_lane_detours_around_and_skips_the_blocker() {
        let (edges, ids, plan, frames) = fixture(true);
        let out = intra_layer_edges(&edges, &ids, &plan, &frames, 8.0);
        let e = &out[0];
        let pts = e.path.samples();
        assert_eq!(pts.len(), 6);
        // Rank 0 → seam below; clears every frame (bottom = 20) by gap/2.
        let seam = pts[2].y;
        assert!((seam - 24.0).abs() < 1e-9);
        // No sample may sit inside the blocker's frame.
        let b = frames[2];
        let inside = |p: &Point| {
            p.x > b.x && p.x < b.right() && p.y > b.y && p.y < b.bottom()
        };
        assert!(!pts.iter().any(inside));
        // Stubs leave the ports along their outward normal (horizontal).
        assert!((pts[1].y - pts[0].y).abs() < 1e-9);
        assert!((pts[4].y - pts[5].y).abs() < 1e-9);
    }
}
