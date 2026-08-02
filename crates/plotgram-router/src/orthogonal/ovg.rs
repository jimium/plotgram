//! Orthogonal visibility graph over reduced interesting lines — the search
//! graph of the main routing path (M0+; see architecture.md §5).
//!
//! Line set: every obstacle edge offset outward by `line_offset`
//! (`spacing + ε`, so routed segments keep strictly positive clearance from
//! `spacing`-inflated obstacles — touching counts as intersecting), plus the
//! anchor / stub coordinates of every terminal. Vertices are all line
//! intersections; two adjacent intersections are connected when the segment
//! between them is collision-free (checked lazily by the search).
//!
//! See `docs/design/layout/routing/architecture.md` §5 (搜索图主选).

use plotgram_engine_api::Obstacle;
use plotgram_model::geometry::Point;
use plotgram_model::port::Side;

use crate::core::{padding_rect, segment_intersects_rect};

// ─── Directions ─────────────────────────────────────────────

/// Cardinal direction of travel on the grid.
///
/// Discriminant order matches [`Dir4::ALL`] (used for dense state indexing
/// and fixed neighbour expansion order — both deterministic, R3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum Dir4 {
    East = 0,
    North = 1,
    West = 2,
    South = 3,
}

impl Dir4 {
    /// Fixed expansion order (deterministic tie-breaking input).
    pub const ALL: [Dir4; 4] = [Dir4::East, Dir4::North, Dir4::West, Dir4::South];

    pub fn from_index(i: usize) -> Dir4 {
        Dir4::ALL[i]
    }

    pub fn dx(self) -> f64 {
        match self {
            Dir4::East => 1.0,
            Dir4::West => -1.0,
            _ => 0.0,
        }
    }

    pub fn dy(self) -> f64 {
        match self {
            Dir4::South => 1.0,
            Dir4::North => -1.0,
            _ => 0.0,
        }
    }

    pub fn opposite(self) -> Dir4 {
        match self {
            Dir4::East => Dir4::West,
            Dir4::West => Dir4::East,
            Dir4::North => Dir4::South,
            Dir4::South => Dir4::North,
        }
    }
}

/// Outward normal of a node side: the port-stub departure direction.
pub fn outward(side: Side) -> Dir4 {
    match side {
        Side::North => Dir4::North,
        Side::South => Dir4::South,
        Side::East => Dir4::East,
        Side::West => Dir4::West,
    }
}

/// Stub endpoint: anchor pushed along the side's outward normal (§5.1 出针).
pub fn stub_point(anchor: Point, side: Side, stub_len: f64) -> Point {
    let d = outward(side);
    Point {
        x: anchor.x + d.dx() * stub_len,
        y: anchor.y + d.dy() * stub_len,
    }
}

// ─── Grid ───────────────────────────────────────────────────

/// Reduced-interesting-line coordinate grid: sorted unique lines per axis.
///
/// Coordinates are computed once from identical float expressions, so exact
/// `f64` binary search / equality is sound (no approximate matching).
pub struct Grid {
    xs: Vec<f64>,
    ys: Vec<f64>,
}

impl Grid {
    /// Build the line set: obstacle edges ± `line_offset`, plus every extra
    /// point's coordinates (terminal anchors and stub ends).
    pub fn build(obstacles: &[Obstacle], extra_points: &[Point], line_offset: f64) -> Grid {
        let mut xs = Vec::with_capacity(obstacles.len() * 2 + extra_points.len());
        let mut ys = Vec::with_capacity(obstacles.len() * 2 + extra_points.len());
        for o in obstacles {
            xs.push(o.rect.x - line_offset);
            xs.push(o.rect.right() + line_offset);
            ys.push(o.rect.y - line_offset);
            ys.push(o.rect.bottom() + line_offset);
        }
        for p in extra_points {
            xs.push(p.x);
            ys.push(p.y);
        }
        xs.sort_by(f64::total_cmp);
        xs.dedup();
        ys.sort_by(f64::total_cmp);
        ys.dedup();
        Grid { xs, ys }
    }

    /// `(nx, ny)` line counts.
    pub fn dims(&self) -> (usize, usize) {
        (self.xs.len(), self.ys.len())
    }

    /// Coordinate of the intersection vertex `(xi, yi)`.
    pub fn point(&self, xi: u32, yi: u32) -> Point {
        Point {
            x: self.xs[xi as usize],
            y: self.ys[yi as usize],
        }
    }

    /// Vertex indices of a point known to be on the line set (exact match).
    pub fn find(&self, p: Point) -> Option<(u32, u32)> {
        let xi = self.xs.binary_search_by(|v| v.total_cmp(&p.x)).ok()?;
        let yi = self.ys.binary_search_by(|v| v.total_cmp(&p.y)).ok()?;
        Some((xi as u32, yi as u32))
    }

    /// Vertex one line-step away in `dir`, if within grid bounds.
    pub fn step(&self, xi: u32, yi: u32, dir: Dir4) -> Option<(u32, u32)> {
        let (nx, ny) = (self.xs.len() as u32, self.ys.len() as u32);
        match dir {
            Dir4::East if xi + 1 < nx => Some((xi + 1, yi)),
            Dir4::West if xi > 0 => Some((xi - 1, yi)),
            Dir4::South if yi + 1 < ny => Some((xi, yi + 1)),
            Dir4::North if yi > 0 => Some((xi, yi - 1)),
            _ => None,
        }
    }
}

// ─── Collision model ────────────────────────────────────────

/// Closed-collision test: does segment `a → b` touch any non-exempt obstacle
/// inflated by `inflate`?
///
/// `exempt` holds the routed edge's own terminal node ids (source + target) —
/// the only own-obstacle exemption allowed (§5.1); all other obstacles block.
/// Mirrors `verify::check_obstacle_clearance` exactly (same `core` geometry).
pub fn segment_blocked(
    a: Point,
    b: Point,
    obstacles: &[Obstacle],
    exempt: &[&str; 2],
    inflate: f64,
) -> bool {
    obstacles
        .iter()
        .filter(|o| o.id != exempt[0] && o.id != exempt[1])
        .any(|o| segment_intersects_rect(a, b, padding_rect(o.rect, inflate)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use plotgram_model::geometry::Rect;

    fn obs(id: &str, x: f64, y: f64, w: f64, h: f64) -> Obstacle {
        Obstacle {
            id: id.to_string(),
            rect: Rect::new(x, y, w, h),
        }
    }

    #[test]
    fn dir4_ops() {
        assert_eq!(outward(Side::East).opposite(), Dir4::West);
        assert_eq!(outward(Side::North), Dir4::North);
        let p = stub_point(Point { x: 5.0, y: 5.0 }, Side::South, 10.0);
        assert_eq!(p, Point { x: 5.0, y: 15.0 });
    }

    #[test]
    fn grid_lines_sorted_deduped_and_findable() {
        let obstacles = [
            obs("a", 0.0, 0.0, 80.0, 40.0),
            obs("b", 200.0, 0.0, 80.0, 40.0),
        ];
        let extra = [Point { x: 90.0, y: 20.0 }, Point { x: 190.0, y: 20.0 }];
        let g = Grid::build(&obstacles, &extra, 20.0);
        // x lines: -20, 100, 180, 300 + extras 90, 190
        let (nx, ny) = g.dims();
        assert_eq!((nx, ny), (6, 3));
        let p = g.point(2, 1);
        assert_eq!(g.find(p), Some((2, 1)));
        assert_eq!(g.find(Point { x: 1.5, y: 1.5 }), None);
    }

    #[test]
    fn segment_blocked_matches_verify_gate() {
        // The router's collision model and the verify acceptance gate are two
        // implementations of the same clearance rule; a test locks them to the
        // same geometry truth (ovg.rs doc: "Mirrors verify exactly").
        use crate::verify::verify_edge;
        use plotgram_engine_api::{PortAnchor, RouteScene, TerminalPair};
        use plotgram_model::port::Side;
        use std::collections::BTreeMap;

        let obstacles = vec![
            obs("a", 0.0, 0.0, 80.0, 40.0),
            obs("b", 200.0, 0.0, 80.0, 40.0),
            obs("blk", 100.0, -20.0, 40.0, 60.0),
        ];
        let mut terminals = BTreeMap::new();
        terminals.insert(
            "e0".to_string(),
            TerminalPair {
                source: PortAnchor {
                    point: Point { x: 80.0, y: 20.0 },
                    side: Side::East,
                    node_id: "a".to_string(),
                },
                target: PortAnchor {
                    point: Point { x: 200.0, y: 20.0 },
                    side: Side::West,
                    node_id: "b".to_string(),
                },
            },
        );
        let scene = RouteScene {
            obstacles,
            terminals,
            edge_order: vec!["e0".to_string()],
            group_boundaries: vec![],
            boundary_permissions: BTreeMap::new(),
            params: Default::default(),
        };
        let spacing = scene.params.spacing;

        // Crosses the inflated blocker: both sides must flag it.
        let blocked_line = [Point { x: 80.0, y: 20.0 }, Point { x: 200.0, y: 20.0 }];
        let report = verify_edge(&scene, "e0", &blocked_line);
        let clearance = report
            .checks
            .iter()
            .find(|c| c.name == "obstacle_clearance")
            .expect("obstacle_clearance check exists");
        assert!(!clearance.pass, "verify must flag the blocked line");
        assert!(
            segment_blocked(
                blocked_line[0],
                blocked_line[1],
                &scene.obstacles,
                &["a", "b"],
                spacing
            ),
            "collision model must agree"
        );

        // Far above every inflate: both sides must pass.
        let clear_line = [Point { x: 0.0, y: -60.0 }, Point { x: 300.0, y: -60.0 }];
        let report = verify_edge(&scene, "e0", &clear_line);
        let clearance = report
            .checks
            .iter()
            .find(|c| c.name == "obstacle_clearance")
            .expect("obstacle_clearance check exists");
        assert!(clearance.pass, "verify must pass the clear line");
        assert!(
            !segment_blocked(
                clear_line[0],
                clear_line[1],
                &scene.obstacles,
                &["a", "b"],
                spacing
            ),
            "collision model must agree"
        );
    }

    #[test]
    fn segment_blocked_cases() {
        let obstacles = [
            obs("a", 0.0, 0.0, 80.0, 40.0),
            obs("blk", 150.0, 60.0, 80.0, 80.0),
        ];
        let inflate = 20.0;
        let cases: &[(Point, Point, [&str; 2], bool)] = &[
            // grazing "a" — exempt because "a" is an own node
            (
                Point { x: 40.0, y: 0.0 },
                Point { x: 40.0, y: -50.0 },
                ["a", "b"],
                false,
            ),
            // same segment without exemption → blocked
            (
                Point { x: 40.0, y: 0.0 },
                Point { x: 40.0, y: -50.0 },
                ["x", "y"],
                true,
            ),
            // clear corridor far from "blk"
            (
                Point { x: 0.0, y: -30.0 },
                Point { x: 300.0, y: -30.0 },
                ["a", "b"],
                false,
            ),
            // crosses inflated blocker (y=100 within [40,160], x span covers [130,250])
            (
                Point { x: 0.0, y: 100.0 },
                Point { x: 300.0, y: 100.0 },
                ["a", "b"],
                true,
            ),
        ];
        for (i, (a, b, exempt, want)) in cases.iter().enumerate() {
            assert_eq!(
                segment_blocked(*a, *b, &obstacles, exempt, inflate),
                *want,
                "case {i}"
            );
        }
    }
}
