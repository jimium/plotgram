//! Self-loop stub geometry (`source == target`, extracted before rank/order
//! by `graph_index::build_real_graph` — composition.md's `SelfLoop` Stage
//! fact). Not modeled in the channel/dummy-chain machinery at all: a small
//! fixed loop bump off the node's canonical East side, matching v1's
//! documented simplification (no multi-loop collision proof — see
//! `docs/design/layout/hierarchical/notes/2026-08-02-mvp-scope.md` §2.5).

use std::collections::BTreeMap;

use plotgram_algo::orientation::Side;
use plotgram_model::geometry::{Point, Rect};
use plotgram_model::port::AlongSpec;

use crate::layout::hierarchical::compose::ports::ResolvedPort;
use crate::layout::hierarchical::ink::route::CanonicalEdge;

const BASE_OUT: f64 = 24.0;
const OUT_STEP: f64 = 10.0;

pub fn self_loop_edges(
    self_loops: &[(String, usize)],
    ids: &[String],
    frames: &[Rect],
    node_gap: f64,
) -> Vec<CanonicalEdge> {
    let mut per_node_count: BTreeMap<usize, u32> = BTreeMap::new();
    let mut out = Vec::with_capacity(self_loops.len());

    for (edge_id, node_idx) in self_loops {
        let idx = *per_node_count.entry(*node_idx).or_insert(0);
        *per_node_count.get_mut(node_idx).unwrap() += 1;

        let frame = frames[*node_idx];
        let out_dist = node_gap.max(BASE_OUT) + idx as f64 * OUT_STEP;
        let top = Point {
            x: frame.right(),
            y: frame.y + frame.height * 0.3,
        };
        let bottom = Point {
            x: frame.right(),
            y: frame.y + frame.height * 0.7,
        };
        let path = vec![
            top,
            Point {
                x: top.x + out_dist,
                y: top.y,
            },
            Point {
                x: bottom.x + out_dist,
                y: bottom.y,
            },
            bottom,
        ];

        let id = ids[*node_idx].clone();
        out.push(CanonicalEdge {
            id: edge_id.clone(),
            source: id.clone(),
            target: id,
            path,
            // Self-loop stubs are fixed-geometry: record the exact path
            // endpoints as LocalOffset pins so the emitted PortRef is
            // truthful (the path is not derived from port anchors here).
            from_port: ResolvedPort {
                side: Side::East,
                along: AlongSpec::LocalOffset(Point {
                    x: frame.width,
                    y: frame.height * 0.3,
                }),
            },
            to_port: ResolvedPort {
                side: Side::East,
                along: AlongSpec::LocalOffset(Point {
                    x: frame.width,
                    y: frame.height * 0.7,
                }),
            },
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loop_stays_off_to_the_east_and_returns_to_the_node() {
        let ids = vec!["a".to_string()];
        let frames = vec![Rect::new(0.0, 0.0, 20.0, 10.0)];
        let loops = vec![("e0".to_string(), 0usize)];
        let edges = self_loop_edges(&loops, &ids, &frames, 24.0);
        assert_eq!(edges.len(), 1);
        let e = &edges[0];
        assert_eq!(e.source, "a");
        assert_eq!(e.target, "a");
        assert!(e.path.iter().all(|p| p.x >= frames[0].right()));
    }

    #[test]
    fn multiple_loops_on_same_node_offset_outward() {
        let ids = vec!["a".to_string()];
        let frames = vec![Rect::new(0.0, 0.0, 20.0, 10.0)];
        let loops = vec![("e0".to_string(), 0usize), ("e1".to_string(), 0usize)];
        let edges = self_loop_edges(&loops, &ids, &frames, 24.0);
        let max_x0 = edges[0].path.iter().map(|p| p.x).fold(0.0_f64, f64::max);
        let max_x1 = edges[1].path.iter().map(|p| p.x).fold(0.0_f64, f64::max);
        assert!(max_x1 > max_x0, "second loop should extend further out");
    }
}
