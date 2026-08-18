//! Layout audit metrics (showcase-redesign-2026-08.md §5.2).
//!
//! Pure geometry over [`LayoutResult`]. No I/O, no timing, no diagram-type or
//! layout-name branching (AGENTS.md §1/§3). WASM-safe — safe to call from a
//! WASM-bound build. Pair iteration is index-based (i < j) so determinism does
//! not depend on any collection key order (AGENTS.md §2).
//!
//! `det` (double-render byte equality) and `elapsed_ms` are NOT computed here:
//! they need the SVG string / wall clock and belong at the CLI layer.

use tautcore_model::geometry::{Point, Rect};
use tautcore_model::result::LayoutResult;

/// Geometry-derived metrics (all three §5.2 tracks merged; the CLI splits
/// them into correctness / quality / observation for JSON output).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GeometryMetrics {
    pub node_overlap_count: u32,
    pub edge_crosses_group_interior: u32,
    pub label_overlap_count: u32,
    pub edge_crossing_count: u32,
    pub total_edge_length: f64,
    pub canvas_area: f64,
    pub aspect_ratio: f64,
    pub node_count: u32,
    pub edge_count: u32,
}

/// Compute all geometry metrics for a laid-out graph.
pub fn compute(layout: &LayoutResult) -> GeometryMetrics {
    GeometryMetrics {
        node_overlap_count: count_node_overlaps(&layout.nodes),
        edge_crosses_group_interior: count_edge_group_interior(layout),
        label_overlap_count: count_label_overlaps(&layout.labels),
        edge_crossing_count: count_edge_crossings(&layout.edges),
        total_edge_length: total_edge_length(&layout.edges),
        canvas_area: layout.canvas_width * layout.canvas_height,
        aspect_ratio: aspect_ratio(layout.canvas_width, layout.canvas_height),
        node_count: layout.nodes.len() as u32,
        edge_count: layout.edges.len() as u32,
    }
}

// ── geometry helpers ──────────────────────────────────────────

fn rects_overlap(a: Rect, b: Rect) -> bool {
    // Strict (shared edge does not count as overlap).
    a.x < b.right() && a.right() > b.x && a.y < b.bottom() && a.bottom() > b.y
}

fn point_strictly_inside(p: Point, r: Rect) -> bool {
    p.x > r.x && p.x < r.right() && p.y > r.y && p.y < r.bottom()
}

/// 2D cross product of vectors (o→a) × (o→b). Sign indicates which side of
/// segment o→a the point b lies on.
fn cross(o: Point, a: Point, b: Point) -> f64 {
    (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
}

/// Proper (interior) segment intersection — excludes collinear / touching.
fn segments_properly_intersect(p1: Point, p2: Point, p3: Point, p4: Point) -> bool {
    let d1 = cross(p3, p4, p1);
    let d2 = cross(p3, p4, p2);
    let d3 = cross(p1, p2, p3);
    let d4 = cross(p1, p2, p4);
    // Strict opposite signs on both tests ⇔ proper crossing.
    d1 * d2 < 0.0 && d3 * d4 < 0.0
}

// ── node_overlap_count ─────────────────────────────────────────

fn count_node_overlaps(nodes: &[tautcore_model::result::NodePlacement]) -> u32 {
    let mut count = 0u32;
    for i in 0..nodes.len() {
        for j in (i + 1)..nodes.len() {
            if rects_overlap(nodes[i].frame, nodes[j].frame) {
                count += 1;
            }
        }
    }
    count
}

// ── label_overlap_count ────────────────────────────────────────

fn count_label_overlaps(labels: &[tautcore_model::result::LabelSlot]) -> u32 {
    let mut count = 0u32;
    for i in 0..labels.len() {
        for j in (i + 1)..labels.len() {
            if rects_overlap(labels[i].frame, labels[j].frame) {
                count += 1;
            }
        }
    }
    count
}

// ── edge_crosses_group_interior ────────────────────────────────
// An edge "cuts through" a group if neither endpoint node lies inside the
// group, yet some interior point of the edge path is strictly inside the
// group's frame. Endpoint anchors are excluded so a legitimate entry/exit
// at an endpoint is not counted.

fn count_edge_group_interior(layout: &LayoutResult) -> u32 {
    use std::collections::HashMap;
    // Node id -> frame center. (Read-only lookup; key order irrelevant.)
    let centers: HashMap<&str, Point> = layout
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.frame.center()))
        .collect();

    let mut count = 0u32;
    for edge in &layout.edges {
        let pts = edge.path.samples();
        if pts.len() < 3 {
            continue;
        }
        let src_center = centers.get(edge.source.as_str()).copied();
        let tgt_center = centers.get(edge.target.as_str()).copied();
        for group in &layout.groups {
            let src_in = src_center.map_or(false, |c| point_strictly_inside(c, group.frame));
            let tgt_in = tgt_center.map_or(false, |c| point_strictly_inside(c, group.frame));
            if src_in || tgt_in {
                continue; // edge legitimately touches this group
            }
            // Interior points = all except first & last.
            for p in &pts[1..pts.len() - 1] {
                if point_strictly_inside(*p, group.frame) {
                    count += 1;
                    break; // one interior hit = one (edge, group) defect
                }
            }
        }
    }
    count
}

// ── edge_crossing_count ────────────────────────────────────────
// Count edge pairs whose paths have a proper (interior) segment intersection.
// Shared-endpoint junctions are naturally excluded: the touching segments
// yield d == 0 (collinear), failing the strict sign test.

fn count_edge_crossings(edges: &[tautcore_model::result::EdgePlacement]) -> u32 {
    edge_crossing_pairs(edges).len() as u32
}

/// Edge id pairs (index order, i < j) whose paths properly cross at least
/// once. Consumed by the layout-facts channel (`explain`) to name the
/// crossing participants, not just the count.
pub(crate) fn edge_crossing_pairs(
    edges: &[tautcore_model::result::EdgePlacement],
) -> Vec<(&str, &str)> {
    let polylines: Vec<Vec<Point>> = edges.iter().map(|e| e.path.samples()).collect();
    let mut pairs = Vec::new();
    for i in 0..polylines.len() {
        let pi = &polylines[i];
        if pi.len() < 2 {
            continue;
        }
        for j in (i + 1)..polylines.len() {
            let pj = &polylines[j];
            if pj.len() < 2 {
                continue;
            }
            let mut crossed = false;
            'seg_i: for a in 0..pi.len() - 1 {
                for b in 0..pj.len() - 1 {
                    if segments_properly_intersect(pi[a], pi[a + 1], pj[b], pj[b + 1]) {
                        crossed = true;
                        break 'seg_i;
                    }
                }
            }
            if crossed {
                pairs.push((edges[i].id.as_str(), edges[j].id.as_str()));
            }
        }
    }
    pairs
}

// ── total_edge_length ──────────────────────────────────────────

fn total_edge_length(edges: &[tautcore_model::result::EdgePlacement]) -> f64 {
    let mut total = 0.0f64;
    for edge in edges {
        let pts = edge.path.samples();
        for w in pts.windows(2) {
            let dx = w[1].x - w[0].x;
            let dy = w[1].y - w[0].y;
            total += (dx * dx + dy * dy).sqrt();
        }
    }
    total
}

// ── aspect_ratio ───────────────────────────────────────────────

fn aspect_ratio(w: f64, h: f64) -> f64 {
    if w <= 0.0 || h <= 0.0 {
        return 0.0;
    }
    // Normalize to ≥ 1 (max / min) so a portrait canvas is comparable to landscape.
    if w >= h {
        w / h
    } else {
        h / w
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tautcore_model::geometry::{Point, Rect};
    use tautcore_model::result::{
        EdgePath, EdgePlacement, GroupPlacement, LabelOwner, LabelSlot, LayoutResult, NodePlacement,
    };

    fn p(x: f64, y: f64) -> Point {
        Point { x, y }
    }
    fn r(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect::new(x, y, w, h)
    }

    fn empty_layout() -> LayoutResult {
        LayoutResult {
            nodes: vec![],
            edges: vec![],
            groups: vec![],
            labels: vec![],
            canvas_width: 100.0,
            canvas_height: 100.0,
            diagnostics: Default::default(),
            decorations: vec![],
        }
    }

    #[test]
    fn empty_graph_has_zero_metrics() {
        let m = compute(&empty_layout());
        assert_eq!(m.node_overlap_count, 0);
        assert_eq!(m.edge_crossing_count, 0);
        assert_eq!(m.node_count, 0);
        assert_eq!(m.canvas_area, 10000.0);
        assert_eq!(m.aspect_ratio, 1.0);
    }

    #[test]
    fn node_overlaps_detected() {
        let mut lay = empty_layout();
        lay.nodes = vec![
            NodePlacement {
                id: "a".into(),
                frame: r(0.0, 0.0, 10.0, 10.0),
            },
            NodePlacement {
                id: "b".into(),
                frame: r(5.0, 5.0, 10.0, 10.0),
            }, // overlaps a
            NodePlacement {
                id: "c".into(),
                frame: r(50.0, 50.0, 5.0, 5.0),
            }, // isolated
        ];
        assert_eq!(compute(&lay).node_overlap_count, 1);
    }

    #[test]
    fn touching_nodes_do_not_count_as_overlap() {
        let mut lay = empty_layout();
        lay.nodes = vec![
            NodePlacement {
                id: "a".into(),
                frame: r(0.0, 0.0, 10.0, 10.0),
            },
            NodePlacement {
                id: "b".into(),
                frame: r(10.0, 0.0, 10.0, 10.0),
            }, // shares edge
        ];
        assert_eq!(compute(&lay).node_overlap_count, 0);
    }

    #[test]
    fn edge_crossing_detected() {
        let mut lay = empty_layout();
        // Two edges forming an X.
        lay.edges = vec![
            EdgePlacement {
                id: "e1".into(),
                source: "a".into(),
                target: "b".into(),
                path: EdgePath::Polyline {
                    points: vec![p(0.0, 0.0), p(10.0, 10.0)],
                },
                from_port: None,
                to_port: None,
            },
            EdgePlacement {
                id: "e2".into(),
                source: "c".into(),
                target: "d".into(),
                path: EdgePath::Polyline {
                    points: vec![p(0.0, 10.0), p(10.0, 0.0)],
                },
                from_port: None,
                to_port: None,
            },
        ];
        assert_eq!(compute(&lay).edge_crossing_count, 1);
    }

    #[test]
    fn shared_endpoint_not_a_crossing() {
        let mut lay = empty_layout();
        // Two edges meeting at (0,0) — a junction, not a crossing.
        lay.edges = vec![
            EdgePlacement {
                id: "e1".into(),
                source: "a".into(),
                target: "b".into(),
                path: EdgePath::Polyline {
                    points: vec![p(0.0, 0.0), p(10.0, 0.0)],
                },
                from_port: None,
                to_port: None,
            },
            EdgePlacement {
                id: "e2".into(),
                source: "c".into(),
                target: "b".into(),
                path: EdgePath::Polyline {
                    points: vec![p(0.0, 10.0), p(0.0, 0.0), p(10.0, 0.0)],
                },
                from_port: None,
                to_port: None,
            },
        ];
        // e1 (0,0)->(10,0); e2 (0,10)->(0,0)->(10,0). They share the (0,0)->(10,0) tail collinearly.
        // No proper interior crossing expected.
        assert_eq!(compute(&lay).edge_crossing_count, 0);
    }

    #[test]
    fn edge_through_unrelated_group_is_counted() {
        let mut lay = empty_layout();
        lay.nodes = vec![
            NodePlacement {
                id: "a".into(),
                frame: r(0.0, 0.0, 4.0, 4.0),
            }, // center (2,2)
            NodePlacement {
                id: "b".into(),
                frame: r(20.0, 20.0, 4.0, 4.0),
            }, // center (22,22)
        ];
        // Group box in the middle, neither endpoint inside.
        lay.groups = vec![GroupPlacement {
            id: "g".into(),
            frame: r(8.0, 8.0, 6.0, 6.0),
        }];
        // Edge passes straight through the group.
        lay.edges = vec![EdgePlacement {
            id: "e".into(),
            source: "a".into(),
            target: "b".into(),
            path: EdgePath::Polyline {
                points: vec![p(4.0, 4.0), p(11.0, 11.0), p(20.0, 20.0)],
            },
            from_port: None,
            to_port: None,
        }];
        assert_eq!(compute(&lay).edge_crosses_group_interior, 1);
    }

    #[test]
    fn edge_to_node_inside_group_not_counted() {
        let mut lay = empty_layout();
        lay.nodes = vec![
            NodePlacement {
                id: "a".into(),
                frame: r(0.0, 0.0, 4.0, 4.0),
            }, // outside
            NodePlacement {
                id: "b".into(),
                frame: r(9.0, 9.0, 4.0, 4.0),
            }, // center (11,11) inside group
        ];
        lay.groups = vec![GroupPlacement {
            id: "g".into(),
            frame: r(8.0, 8.0, 6.0, 6.0),
        }];
        lay.edges = vec![EdgePlacement {
            id: "e".into(),
            source: "a".into(),
            target: "b".into(),
            path: EdgePath::Polyline {
                points: vec![p(4.0, 4.0), p(11.0, 11.0)],
            },
            from_port: None,
            to_port: None,
        }];
        // Target is inside the group → legitimate entry, not a defect.
        assert_eq!(compute(&lay).edge_crosses_group_interior, 0);
    }

    #[test]
    fn label_overlap_detected() {
        let mut lay = empty_layout();
        lay.labels = vec![
            LabelSlot {
                owner: LabelOwner::Node("a".into()),
                role: None,
                text: "A".into(),
                frame: r(0.0, 0.0, 8.0, 4.0),
            },
            LabelSlot {
                owner: LabelOwner::Node("b".into()),
                role: None,
                text: "B".into(),
                frame: r(4.0, 0.0, 8.0, 4.0),
            }, // overlaps
        ];
        assert_eq!(compute(&lay).label_overlap_count, 1);
    }

    #[test]
    fn total_edge_length_sums_segments() {
        let mut lay = empty_layout();
        lay.edges = vec![EdgePlacement {
            id: "e".into(),
            source: "a".into(),
            target: "b".into(),
            path: EdgePath::Polyline {
                points: vec![p(0.0, 0.0), p(3.0, 0.0), p(3.0, 4.0)],
            },
            from_port: None,
            to_port: None,
        }];
        let m = compute(&lay);
        assert!((m.total_edge_length - 7.0).abs() < 1e-9);
    }
}
