//! Tree IR written by Compose.

use std::collections::{BTreeMap, BTreeSet};

use plotgram_model::geometry::Point;
use plotgram_model::port::Side;

use super::params::{BusSlot, PlacerId, SplitSide, SubtreeTransform};

#[derive(Debug, Clone)]
pub struct TreePlan {
    pub roots: Vec<String>,
    pub nodes: Vec<String>,
    pub children: BTreeMap<String, Vec<String>>,
    pub parent: BTreeMap<String, String>,
    pub depth: BTreeMap<String, u32>,
    pub tree_edge_ids: Vec<String>,
    /// Child id → tree-edge id (unique parent link).
    pub edge_of_child: BTreeMap<String, String>,
    pub extra_edge_ids: Vec<String>,
    /// Local-root placer. Covers every node.
    pub placer_of: BTreeMap<String, PlacerId>,
    /// Local subtree transform (mindmap left/right). Covers every node.
    pub transform_of: BTreeMap<String, SubtreeTransform>,
    /// Child → side of the child where the parent edge attaches.
    pub child_connectors: BTreeMap<String, Side>,
    pub split_side: BTreeMap<String, SplitSide>,
    pub bus_slot: BTreeMap<String, BusSlot>,
    /// Nodes whose `subtree_placer` was authored (Processor must not overwrite).
    pub explicit_placer: BTreeMap<String, PlacerId>,
    /// Children marked `assistant: true` (consumed by `assistant` / `compact`).
    pub assistants: BTreeSet<String>,
}

impl TreePlan {
    pub fn children_of(&self, id: &str) -> &[String] {
        self.children.get(id).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn placer(&self, id: &str) -> PlacerId {
        self.placer_of
            .get(id)
            .copied()
            .unwrap_or(PlacerId::SingleLayer)
    }

    pub fn transform(&self, id: &str) -> SubtreeTransform {
        self.transform_of
            .get(id)
            .copied()
            .unwrap_or(SubtreeTransform::None)
    }

    pub fn is_assistant(&self, id: &str) -> bool {
        self.assistants.contains(id)
    }
}

/// Edge skeleton written by Metric; Ink expands to `EdgePath`.
#[derive(Debug, Clone)]
pub enum TreeRoute {
    OrthoThreeSeg {
        start: Point,
        mid_y: f64,
        end: Point,
    },
    Polyline {
        points: Vec<Point>,
    },
    Straight {
        start: Point,
        end: Point,
    },
    /// Parent drops to a shared horizontal rail, then to the child.
    HorizontalBus {
        start: Point,
        bus_y: f64,
        end: Point,
    },
}

impl TreeRoute {
    pub fn points(&self) -> Vec<Point> {
        match self {
            Self::OrthoThreeSeg { start, mid_y, end } => vec![
                *start,
                Point {
                    x: start.x,
                    y: *mid_y,
                },
                Point {
                    x: end.x,
                    y: *mid_y,
                },
                *end,
            ],
            Self::Polyline { points } => points.clone(),
            Self::Straight { start, end } => vec![*start, *end],
            Self::HorizontalBus { start, bus_y, end } => vec![
                *start,
                Point {
                    x: start.x,
                    y: *bus_y,
                },
                Point {
                    x: end.x,
                    y: *bus_y,
                },
                *end,
            ],
        }
    }

    pub fn start(&self) -> Point {
        match self {
            Self::OrthoThreeSeg { start, .. }
            | Self::Straight { start, .. }
            | Self::HorizontalBus { start, .. } => *start,
            Self::Polyline { points } => {
                points.first().copied().unwrap_or(Point { x: 0.0, y: 0.0 })
            }
        }
    }

    pub fn end(&self) -> Point {
        match self {
            Self::OrthoThreeSeg { end, .. }
            | Self::Straight { end, .. }
            | Self::HorizontalBus { end, .. } => *end,
            Self::Polyline { points } => points.last().copied().unwrap_or(Point { x: 0.0, y: 0.0 }),
        }
    }
}
