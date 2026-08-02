//! Quality metrics for routed edge paths.
//!
//! Library code — usable from tests, CLI, and benchmark binaries.
//! All metrics are deterministic and derived purely from geometry.

use plotgram_engine_api::RouteScene;
use plotgram_model::geometry::Point;
use plotgram_model::result::EdgePlacement;

// ─── Score types ────────────────────────────────────────────

/// Aggregate quality metrics for a routed scene.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SceneScore {
    pub scene_name: String,
    pub edge_count: usize,
    pub total_bends: usize,
    pub total_length: f64,
    pub crossings: usize,
    pub shared_segments: usize,
    pub max_bends_single_edge: usize,
    pub bbox: [f64; 4],
}

/// Quality metrics for a single edge path.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EdgeScore {
    pub edge_id: String,
    pub bends: usize,
    pub length: f64,
    pub segments: usize,
}

// ─── Public API ─────────────────────────────────────────────

/// Compute aggregate score for a routed scene.
pub fn score_scene(name: &str, _scene: &RouteScene, placements: &[EdgePlacement]) -> SceneScore {
    let edge_scores: Vec<EdgeScore> = placements
        .iter()
        .map(|p| score_edge(&p.id, &p.path.points))
        .collect();

    let total_bends: usize = edge_scores.iter().map(|s| s.bends).sum();
    let total_length: f64 = edge_scores.iter().map(|s| s.length).sum();
    let max_bends_single_edge = edge_scores.iter().map(|s| s.bends).max().unwrap_or(0);

    let crossings = count_crossings(placements);
    let shared_segments = count_shared_segments(placements);
    let bbox = compute_bbox(placements);

    SceneScore {
        scene_name: name.to_string(),
        edge_count: placements.len(),
        total_bends,
        total_length,
        crossings,
        shared_segments,
        max_bends_single_edge,
        bbox,
    }
}

/// Compute score for a single edge path.
pub fn score_edge(edge_id: &str, path: &[Point]) -> EdgeScore {
    let segments = path.len().saturating_sub(1);
    let bends = segments.saturating_sub(1); // Interior points are bends.
    let length = path_length(path);

    EdgeScore {
        edge_id: edge_id.to_string(),
        bends,
        length,
        segments,
    }
}

// ─── Metric computations ────────────────────────────────────

fn path_length(path: &[Point]) -> f64 {
    path.windows(2)
        .map(|w| {
            let dx = w[1].x - w[0].x;
            let dy = w[1].y - w[0].y;
            (dx * dx + dy * dy).sqrt()
        })
        .sum()
}

/// Count crossings between orthogonal segments of different edges.
///
/// A crossing occurs when a horizontal segment of one edge intersects
/// a vertical segment of another edge (or vice versa) at an interior point.
fn count_crossings(placements: &[EdgePlacement]) -> usize {
    let mut count = 0;
    for i in 0..placements.len() {
        for j in (i + 1)..placements.len() {
            count += crossings_between(&placements[i].path.points, &placements[j].path.points);
        }
    }
    count
}

fn crossings_between(path_a: &[Point], path_b: &[Point]) -> usize {
    let mut count = 0;
    for wa in path_a.windows(2) {
        for wb in path_b.windows(2) {
            if segments_cross(wa[0], wa[1], wb[0], wb[1]) {
                count += 1;
            }
        }
    }
    count
}

/// Test if two orthogonal segments cross at an interior point.
///
/// Only counts true crossings (horizontal × vertical); collinear overlaps
/// are handled by `count_shared_segments`.
fn segments_cross(a0: Point, a1: Point, b0: Point, b1: Point) -> bool {
    let a_horiz = (a1.y - a0.y).abs() < 1e-9;
    let b_horiz = (b1.y - b0.y).abs() < 1e-9;

    // Need one horizontal and one vertical.
    if a_horiz == b_horiz {
        return false;
    }

    let (h0, h1, v0, v1) = if a_horiz {
        (a0, a1, b0, b1)
    } else {
        (b0, b1, a0, a1)
    };

    let hx_min = h0.x.min(h1.x);
    let hx_max = h0.x.max(h1.x);
    let hy = h0.y;

    let vx = v0.x;
    let vy_min = v0.y.min(v1.y);
    let vy_max = v0.y.max(v1.y);

    // Strict interior intersection (not at endpoints).
    let eps = 1e-9;
    vx > hx_min + eps && vx < hx_max - eps && hy > vy_min + eps && hy < vy_max - eps
}

/// Count pairs of collinear overlapping segments between different edges.
fn count_shared_segments(placements: &[EdgePlacement]) -> usize {
    let mut count = 0;
    for i in 0..placements.len() {
        for j in (i + 1)..placements.len() {
            count += shared_between(&placements[i].path.points, &placements[j].path.points);
        }
    }
    count
}

fn shared_between(path_a: &[Point], path_b: &[Point]) -> usize {
    let mut count = 0;
    for wa in path_a.windows(2) {
        for wb in path_b.windows(2) {
            if segments_overlap(wa[0], wa[1], wb[0], wb[1]) {
                count += 1;
            }
        }
    }
    count
}

/// Test if two collinear segments overlap (share a sub-segment of positive length).
fn segments_overlap(a0: Point, a1: Point, b0: Point, b1: Point) -> bool {
    let eps = 1e-9;
    let a_horiz = (a1.y - a0.y).abs() < eps;
    let b_horiz = (b1.y - b0.y).abs() < eps;

    if a_horiz != b_horiz {
        return false; // Not collinear.
    }

    if a_horiz {
        // Both horizontal: same y?
        if (a0.y - b0.y).abs() > eps {
            return false;
        }
        let a_min = a0.x.min(a1.x);
        let a_max = a0.x.max(a1.x);
        let b_min = b0.x.min(b1.x);
        let b_max = b0.x.max(b1.x);
        a_min < b_max - eps && b_min < a_max - eps
    } else {
        // Both vertical: same x?
        if (a0.x - b0.x).abs() > eps {
            return false;
        }
        let a_min = a0.y.min(a1.y);
        let a_max = a0.y.max(a1.y);
        let b_min = b0.y.min(b1.y);
        let b_max = b0.y.max(b1.y);
        a_min < b_max - eps && b_min < a_max - eps
    }
}

fn compute_bbox(placements: &[EdgePlacement]) -> [f64; 4] {
    let mut x_min = f64::MAX;
    let mut y_min = f64::MAX;
    let mut x_max = f64::MIN;
    let mut y_max = f64::MIN;

    for p in placements {
        for pt in &p.path.points {
            x_min = x_min.min(pt.x);
            y_min = y_min.min(pt.y);
            x_max = x_max.max(pt.x);
            y_max = y_max.max(pt.y);
        }
    }

    if x_min > x_max {
        [0.0, 0.0, 0.0, 0.0]
    } else {
        [x_min, y_min, x_max, y_max]
    }
}
