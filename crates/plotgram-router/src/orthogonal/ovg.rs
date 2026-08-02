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
//! See `docs/design/routing/orthogonal/architecture.md` §5 (搜索图主选).

use plotgram_engine_api::{BoundaryCrossing, GroupBoundary, Obstacle, RouteScene};
use plotgram_model::geometry::{Point, Rect};
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
    /// Build the line set: obstacle + group edges ± `line_offset`, gate rect
    /// edges (raw + padded so the seam is searchable), plus every extra
    /// point's coordinates (terminal anchors and stub ends).
    pub fn build(
        obstacles: &[Obstacle],
        groups: &[GroupBoundary],
        gate_rects: &[Rect],
        extra_points: &[Point],
        line_offset: f64,
    ) -> Grid {
        let mut xs = Vec::with_capacity(
            (obstacles.len() + groups.len()) * 2 + gate_rects.len() * 4 + extra_points.len(),
        );
        let mut ys = Vec::with_capacity(xs.capacity());
        let push_rect_lines = |xs: &mut Vec<f64>, ys: &mut Vec<f64>, r: Rect, offset: f64| {
            xs.push(r.x - offset);
            xs.push(r.right() + offset);
            ys.push(r.y - offset);
            ys.push(r.bottom() + offset);
        };
        for o in obstacles {
            push_rect_lines(&mut xs, &mut ys, o.rect, line_offset);
        }
        for g in groups {
            push_rect_lines(&mut xs, &mut ys, g.rect, line_offset);
        }
        for gate in gate_rects {
            push_rect_lines(&mut xs, &mut ys, *gate, 0.0);
            push_rect_lines(&mut xs, &mut ys, *gate, line_offset);
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

// ─── Spatial index ─────────────────────────────────────────

/// Uniform-grid spatial hash over inflated obstacle rects.
///
/// Built once per scene; answers axis-aligned segment queries in O(k)
/// where k = nearby obstacles, instead of O(N) full scan.
pub struct ObstacleIndex {
    /// Cell size (world units).
    cell: f64,
    /// World-space origin (min x, min y) of the grid.
    ox: f64,
    oy: f64,
    /// Grid dimensions.
    cols: usize,
    rows: usize,
    /// Per-cell list of obstacle indices (into the original slice).
    cells: Vec<Vec<usize>>,
    /// Pre-inflated rects (inflated by `spacing`) for fast intersection.
    inflated: Vec<Rect>,
}

impl ObstacleIndex {
    /// Build from obstacles inflated by `spacing`.
    pub fn build(obstacles: &[Obstacle], spacing: f64) -> Self {
        if obstacles.is_empty() {
            return Self {
                cell: 1.0,
                ox: 0.0,
                oy: 0.0,
                cols: 1,
                rows: 1,
                cells: vec![vec![]],
                inflated: vec![],
            };
        }
        let inflated: Vec<Rect> = obstacles
            .iter()
            .map(|o| padding_rect(o.rect, spacing))
            .collect();

        // Determine world bounds.
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for r in &inflated {
            min_x = min_x.min(r.x);
            min_y = min_y.min(r.y);
            max_x = max_x.max(r.right());
            max_y = max_y.max(r.bottom());
        }

        // Cell size: aim for ~4-16 obstacles per cell on average.
        let total_area = (max_x - min_x).max(1.0) * (max_y - min_y).max(1.0);
        let avg_obs_area = total_area / obstacles.len() as f64;
        let cell = avg_obs_area.sqrt().max(spacing).max(1.0) * 2.0;

        let cols = ((max_x - min_x) / cell).ceil().max(1.0) as usize + 1;
        let rows = ((max_y - min_y) / cell).ceil().max(1.0) as usize + 1;

        let mut cells = vec![Vec::new(); cols * rows];
        for (i, r) in inflated.iter().enumerate() {
            let c0 = ((r.x - min_x) / cell).floor().max(0.0) as usize;
            let c1 = ((r.right() - min_x) / cell).floor().max(0.0) as usize;
            let r0 = ((r.y - min_y) / cell).floor().max(0.0) as usize;
            let r1 = ((r.bottom() - min_y) / cell).floor().max(0.0) as usize;
            let c0 = c0.min(cols - 1);
            let c1 = c1.min(cols - 1);
            let r0 = r0.min(rows - 1);
            let r1 = r1.min(rows - 1);
            for ri in r0..=r1 {
                for ci in c0..=c1 {
                    cells[ri * cols + ci].push(i);
                }
            }
        }

        Self {
            cell,
            ox: min_x,
            oy: min_y,
            cols,
            rows,
            cells,
            inflated,
        }
    }

    /// Query: does segment `a→b` (axis-aligned) intersect any non-exempt
    /// inflated obstacle? Returns `true` if blocked.
    ///
    /// `exempt_indices` are obstacle indices to skip (own-node exemption).
    #[inline]
    pub fn segment_blocked(
        &self,
        a: Point,
        b: Point,
        exempt_i: usize,
        exempt_j: usize,
    ) -> bool {
        // Segment bounding box.
        let sx0 = a.x.min(b.x);
        let sx1 = a.x.max(b.x);
        let sy0 = a.y.min(b.y);
        let sy1 = a.y.max(b.y);

        // Cell range overlapping the segment bbox.
        let c0 = ((sx0 - self.ox) / self.cell).floor().max(0.0) as usize;
        let c1 = ((sx1 - self.ox) / self.cell).floor().max(0.0) as usize;
        let r0 = ((sy0 - self.oy) / self.cell).floor().max(0.0) as usize;
        let r1 = ((sy1 - self.oy) / self.cell).floor().max(0.0) as usize;
        let c0 = c0.min(self.cols - 1);
        let c1 = c1.min(self.cols - 1);
        let r0 = r0.min(self.rows - 1);
        let r1 = r1.min(self.rows - 1);

        for ri in r0..=r1 {
            for ci in c0..=c1 {
                for &idx in &self.cells[ri * self.cols + ci] {
                    if idx == exempt_i || idx == exempt_j {
                        continue;
                    }
                    if segment_intersects_rect(a, b, self.inflated[idx]) {
                        return true;
                    }
                }
            }
        }
        false
    }
}

// ─── Collision model ────────────────────────────────────────

const EPS: f64 = 1e-9;

/// Full step collision for one edge: node obstacles + group boundaries
/// (group-crossing.md §3.2). Shared by the router and `verify`.
pub fn step_blocked(a: Point, b: Point, scene: &RouteScene, edge_id: &str) -> bool {
    let exempt = match scene.terminals.get(edge_id) {
        Some(pair) => [pair.source.node_id.as_str(), pair.target.node_id.as_str()],
        None => return true,
    };
    if segment_blocked(a, b, &scene.obstacles, &exempt, scene.params.spacing) {
        return true;
    }
    let crossings = scene
        .boundary_permissions
        .get(edge_id)
        .map(|v| v.as_slice())
        .unwrap_or(&[]);
    group_blocks_segment(
        a,
        b,
        &scene.group_boundaries,
        crossings,
        scene.params.spacing,
    )
}

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

/// Group-boundary collision (group-crossing.md §3.2).
///
/// Without permission: any touch of `inflate(group)` is blocked.
/// With permission: allowed through the padded gate corridor, or when both
/// endpoints lie inside the raw group rect (interior travel after entry).
pub(crate) fn group_blocks_segment(
    a: Point,
    b: Point,
    groups: &[GroupBoundary],
    crossings: &[BoundaryCrossing],
    inflate: f64,
) -> bool {
    for g in groups {
        let inflated = padding_rect(g.rect, inflate);
        if !segment_intersects_rect(a, b, inflated) {
            continue;
        }
        let gates: Vec<Rect> = crossings
            .iter()
            .filter(|c| c.group_id == g.group_id)
            .filter_map(|c| c.gate_region)
            .collect();
        if gates.is_empty() {
            return true; // no permission for this group
        }
        // Through gate corridor?
        let via_gate = gates
            .iter()
            .any(|gate| segment_intersects_rect(a, b, padding_rect(*gate, inflate)));
        if via_gate {
            continue;
        }
        // Interior travel: both endpoints inside the raw group rect.
        if point_in_rect(a, g.rect) && point_in_rect(b, g.rect) {
            continue;
        }
        return true;
    }
    false
}

fn point_in_rect(p: Point, r: Rect) -> bool {
    p.x >= r.x - EPS
        && p.x <= r.right() + EPS
        && p.y >= r.y - EPS
        && p.y <= r.bottom() + EPS
}

/// Collect every gate rect from the scene (stable edge_order then crossing order).
pub fn all_gate_rects(scene: &RouteScene) -> Vec<Rect> {
    let mut out = Vec::new();
    for eid in &scene.edge_order {
        if let Some(crossings) = scene.boundary_permissions.get(eid) {
            for c in crossings {
                if let Some(g) = c.gate_region {
                    out.push(g);
                }
            }
        }
    }
    out
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
        let g = Grid::build(&obstacles, &[], &[], &extra, 20.0);
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
