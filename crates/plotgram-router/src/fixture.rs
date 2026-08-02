//! Scene fixture file formats for text-driven testing.
//!
//! Fixtures live as `tests/scenes/*.json`. Two formats are supported:
//!
//! - **[`BoardFixture`]** (preferred): a discrete-grid notation. Obstacles are
//!   rectangles in cell units `(r, c, w, h)`; edges reference node ids + side
//!   and the port anchor is auto-computed as the side midpoint. This is the
//!   compact, hand-editable, ASCII-art-friendly format.
//! - **[`SceneFixture`]**: the legacy explicit-coordinate format. Kept for
//!   back-compat with external scripts; new fixtures should use `BoardFixture`.
//!
//! ## `BoardFixture` shape
//!
//! ```json
//! {
//!   "id": "L03",
//!   "name": "blocker_center",
//!   "level": 1,
//!   "requires": "search",
//!   "description": "A blocker between source and target.",
//!   "obstacles": [
//!     {"id": "a", "r": 8, "c": 0, "w": 8, "h": 4}
//!   ],
//!   "edges": [
//!     {"id": "e0", "from": ["a", "east"], "to": ["b", "west"]}
//!   ]
//! }
//! ```
//!
//! Coordinates: `r` = row (y axis, grows down), `c` = col (x axis, grows right).
//! World coords = `(c * cell, r * cell)`. `cell` defaults to 10.
//! Port anchors auto-computed as the side midpoint of the referenced node rect
//! (matches [`plotgram_router::core::port_anchor`]).

use std::collections::BTreeMap;
use std::path::Path;

use plotgram_engine_api::{
    BoundaryCrossing, CrossingDirection, GroupBoundary, Obstacle, OrthogonalRouteParams,
    PortAnchor, RouteScene, TerminalPair,
};
use plotgram_model::geometry::{Point, Rect};
use plotgram_model::port::Side;

// ─── Capability enum (shared by both formats) ───────────────

/// Minimum algorithm capability required to pass this fixture.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum Requires {
    /// No search needed — direct elbow path is valid (no obstacles in the way).
    None,
    /// Obstacle-avoiding search (OVG + A*) required.
    Search,
    /// Corridor track separation / shared-corridor splitting required (M1).
    Track,
    /// Group boundary crossing support required.
    Group,
}

impl std::fmt::Display for Requires {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "none"),
            Self::Search => write!(f, "search"),
            Self::Track => write!(f, "track"),
            Self::Group => write!(f, "group"),
        }
    }
}

/// Expected routing outcome for a fixture (group-crossing.md §5.3).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum RouteExpect {
    /// Route succeeds and passes verify.
    #[default]
    Ok,
    /// Route hard-fails with no collision-free path.
    NoPath,
    /// Scene rejected as unsupported (`UnsupportedRouteScene`).
    Unsupported,
}

// ─── Legacy explicit-coordinate fixture ─────────────────────

/// A self-contained scene fixture with metadata (legacy format).
///
/// Prefer [`BoardFixture`] for new fixtures — it is far more compact and
/// auto-computes port anchors from node geometry.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SceneFixture {
    /// Stable fixture name (matches file stem).
    pub name: String,
    /// Human-readable description of the routing challenge.
    pub description: String,
    /// Minimum capability an algorithm must have to pass this fixture.
    pub requires: Requires,
    /// Expected routing outcome (default: success).
    #[serde(default)]
    pub expect: RouteExpect,
    /// The routing scene (algorithm input).
    pub scene: RouteScene,
}

// ─── Board fixture (preferred, discrete-grid notation) ──────

/// Default cell size: world units per grid cell.
const DEFAULT_CELL: f64 = 10.0;

/// A rectangular obstacle in grid-cell units.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BoardObstacle {
    pub id: String,
    /// Row (y axis, grows down).
    pub r: u32,
    /// Column (x axis, grows right).
    pub c: u32,
    /// Width in cells.
    pub w: u32,
    /// Height in cells.
    pub h: u32,
}

/// One edge endpoint. Serialized as a 2- or 3-element array:
/// - `["a", "east"]` — side midpoint (default)
/// - `["a", "east", -1]` — offset `slot` cells from midpoint along the side
///   (positive = down for East/West, right for North/South)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum BoardEnd {
    /// Side midpoint: `["node", "side"]`.
    Mid(String, Side),
    /// Offset from midpoint: `["node", "side", slot_cells]`.
    Slotted(String, Side, i32),
}

impl BoardEnd {
    fn node(&self) -> &str {
        match self {
            Self::Mid(n, _) | Self::Slotted(n, _, _) => n,
        }
    }
    fn side(&self) -> Side {
        match self {
            Self::Mid(_, s) | Self::Slotted(_, s, _) => *s,
        }
    }
    fn slot(&self) -> f64 {
        match self {
            Self::Mid(..) => 0.0,
            Self::Slotted(_, _, slot) => *slot as f64,
        }
    }
}

/// An edge defined by node ids + sides; port anchors auto-computed.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BoardEdge {
    pub id: String,
    pub from: BoardEnd,
    pub to: BoardEnd,
    /// Permitted group crossings (M2). Default empty = no permission.
    #[serde(default)]
    pub crossings: Vec<BoardCrossing>,
}

/// One permitted group-boundary crossing in board-cell units.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BoardCrossing {
    pub group: String,
    /// `"enter"` | `"leave"`.
    pub dir: CrossingDirection,
    /// Gate region in cell units (required for M2).
    pub gate: BoardRect,
}

/// Axis-aligned rectangle in cell units (no id).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BoardRect {
    pub r: u32,
    pub c: u32,
    pub w: u32,
    pub h: u32,
}

impl BoardRect {
    fn to_world(&self, cell: f64) -> Rect {
        Rect::new(
            self.c as f64 * cell,
            self.r as f64 * cell,
            self.w as f64 * cell,
            self.h as f64 * cell,
        )
    }
}

/// Compact discrete-grid fixture format.
///
/// Obstacles are rectangles in cell units; edges reference node ids + side and
/// the port anchor is auto-computed as the side midpoint. Converts to
/// [`SceneFixture`] (then [`RouteScene`]) via [`BoardFixture::to_scene_fixture`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BoardFixture {
    /// Level id, e.g. `"L03"`. Used for ordering and benchmark reports.
    pub id: String,
    /// Stable fixture name (matches file stem). Used for baseline comparison.
    pub name: String,
    /// Difficulty level (1 = easiest). Used for test-matrix ordering.
    pub level: u8,
    /// Minimum algorithm capability required.
    pub requires: Requires,
    /// Expected routing outcome. Default [`RouteExpect::Ok`].
    #[serde(default)]
    pub expect: RouteExpect,
    /// Human-readable description.
    pub description: String,
    /// World units per grid cell. Defaults to 10 if absent.
    #[serde(default = "default_cell")]
    pub cell: f64,
    /// Obstacle rectangles in cell units.
    pub obstacles: Vec<BoardObstacle>,
    /// Group boundary rectangles in cell units (M2). Default empty.
    #[serde(default)]
    pub groups: Vec<BoardObstacle>,
    /// Edges (port anchors auto-computed from node rect + side midpoint).
    pub edges: Vec<BoardEdge>,
    /// Optional params override. Defaults if absent.
    #[serde(default)]
    pub params: Option<OrthogonalRouteParams>,
}

fn default_cell() -> f64 {
    DEFAULT_CELL
}

impl BoardFixture {
    /// Convert to the algorithm-facing [`RouteScene`].
    pub fn to_route_scene(&self) -> RouteScene {
        let cell = self.cell;
        let board_to_obstacle = |o: &BoardObstacle| Obstacle {
            id: o.id.clone(),
            rect: Rect::new(
                o.c as f64 * cell,
                o.r as f64 * cell,
                o.w as f64 * cell,
                o.h as f64 * cell,
            ),
        };
        let obstacles: Vec<Obstacle> = self.obstacles.iter().map(board_to_obstacle).collect();

        let group_boundaries: Vec<GroupBoundary> = self
            .groups
            .iter()
            .map(|g| GroupBoundary {
                group_id: g.id.clone(),
                rect: Rect::new(
                    g.c as f64 * cell,
                    g.r as f64 * cell,
                    g.w as f64 * cell,
                    g.h as f64 * cell,
                ),
            })
            .collect();

        // Index node_id → rect for port-anchor computation.
        let node_rect: BTreeMap<&str, Rect> = self
            .obstacles
            .iter()
            .map(|o| {
                (
                    o.id.as_str(),
                    Rect::new(
                        o.c as f64 * cell,
                        o.r as f64 * cell,
                        o.w as f64 * cell,
                        o.h as f64 * cell,
                    ),
                )
            })
            .collect();

        let mut terminals: BTreeMap<String, TerminalPair> = BTreeMap::new();
        let mut boundary_permissions: BTreeMap<String, Vec<BoundaryCrossing>> = BTreeMap::new();
        for e in &self.edges {
            let src_rect = *node_rect.get(e.from.node()).unwrap_or_else(|| {
                panic!(
                    "BoardFixture `{}`: edge `{}` references unknown node `{}`",
                    self.id,
                    e.id,
                    e.from.node()
                )
            });
            let tgt_rect = *node_rect.get(e.to.node()).unwrap_or_else(|| {
                panic!(
                    "BoardFixture `{}`: edge `{}` references unknown node `{}`",
                    self.id,
                    e.id,
                    e.to.node()
                )
            });
            terminals.insert(
                e.id.clone(),
                TerminalPair {
                    source: PortAnchor {
                        point: side_midpoint(src_rect, e.from.side(), e.from.slot() * cell),
                        side: e.from.side(),
                        node_id: e.from.node().to_string(),
                    },
                    target: PortAnchor {
                        point: side_midpoint(tgt_rect, e.to.side(), e.to.slot() * cell),
                        side: e.to.side(),
                        node_id: e.to.node().to_string(),
                    },
                },
            );
            if !e.crossings.is_empty() {
                let crossings: Vec<BoundaryCrossing> = e
                    .crossings
                    .iter()
                    .map(|c| BoundaryCrossing {
                        group_id: c.group.clone(),
                        direction: c.dir,
                        gate_region: Some(c.gate.to_world(cell)),
                    })
                    .collect();
                boundary_permissions.insert(e.id.clone(), crossings);
            }
        }

        let edge_order: Vec<String> = self.edges.iter().map(|e| e.id.clone()).collect();

        RouteScene {
            obstacles,
            terminals,
            edge_order,
            group_boundaries,
            boundary_permissions,
            params: self.params.unwrap_or_default(),
        }
    }

    /// Convert to the legacy [`SceneFixture`] wrapper (for consumers that
    /// read `name` / `description` / `requires` / `scene`).
    pub fn to_scene_fixture(&self) -> SceneFixture {
        SceneFixture {
            name: self.name.clone(),
            description: self.description.clone(),
            requires: self.requires,
            expect: self.expect,
            scene: self.to_route_scene(),
        }
    }
}

/// Point on a rect's side: midpoint + `offset` (world units) along the side.
/// Positive offset = down for East/West, right for North/South.
/// Matches `core::port_anchor` when `offset == 0.0`.
fn side_midpoint(rect: Rect, side: Side, offset: f64) -> Point {
    let cx = rect.x + rect.width / 2.0;
    let cy = rect.y + rect.height / 2.0;
    match side {
        Side::North => Point {
            x: cx + offset,
            y: rect.y,
        },
        Side::South => Point {
            x: cx + offset,
            y: rect.y + rect.height,
        },
        Side::West => Point {
            x: rect.x,
            y: cy + offset,
        },
        Side::East => Point {
            x: rect.x + rect.width,
            y: cy + offset,
        },
    }
}

// ─── Loading ────────────────────────────────────────────────

/// Load all `*.json` fixtures from `dir`, sorted by filename.
///
/// Each file is tried as [`BoardFixture`] first, then as legacy
/// [`SceneFixture`]. Returns the unified [`SceneFixture`] view.
pub fn load_dir(dir: &Path) -> Vec<SceneFixture> {
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    paths.sort();

    paths
        .iter()
        .map(|p| {
            let text =
                std::fs::read_to_string(p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()));
            // BoardFixture requires `obstacles` + `edges`; legacy SceneFixture
            // requires `scene`. Try the compact format first, then fall back —
            // serde's required fields do the discrimination, not text sniffing.
            match serde_json::from_str::<BoardFixture>(&text) {
                Ok(board) => board.to_scene_fixture(),
                Err(_) => serde_json::from_str(&text)
                    .unwrap_or_else(|e| panic!("parse {}: {e}", p.display())),
            }
        })
        .collect()
}

/// Load all `*.json` fixtures as [`BoardFixture`] (preferred format), sorted by
/// filename. Preserves the fixture `id` for display/sorting. Panics if any file
/// is not a valid `BoardFixture`.
pub fn load_dir_board(dir: &Path) -> Vec<BoardFixture> {
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    paths.sort();

    paths
        .iter()
        .map(|p| {
            let text =
                std::fs::read_to_string(p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()));
            serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", p.display()))
        })
        .collect()
}
