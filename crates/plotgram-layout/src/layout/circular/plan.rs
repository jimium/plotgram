//! Circular IR written by Compose / Metric.

use std::collections::BTreeMap;

use plotgram_model::geometry::{Point, Rect};

use super::params::CircleOrder;

pub type PartitionId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeRole {
    Intra,
    Inter,
    Loop,
    Parallel,
}

#[derive(Debug, Clone)]
pub struct CircComponent {
    pub id: u32,
    pub nodes: Vec<String>,
    pub partitions: Vec<PartitionId>,
    pub root_partition: PartitionId,
}

#[derive(Debug, Clone)]
pub struct BackboneNode {
    pub parent: Option<PartitionId>,
    pub children: Vec<PartitionId>,
}

#[derive(Debug, Clone)]
pub struct CircPlan {
    pub nodes: Vec<String>,
    pub components: Vec<CircComponent>,
    pub partitions: BTreeMap<PartitionId, Vec<String>>,
    pub partition_of: BTreeMap<String, PartitionId>,
    pub backbone: BTreeMap<PartitionId, BackboneNode>,
    /// Parent partition → child partition → cut vertex (if the skeleton hop is a cut).
    pub cut_of: BTreeMap<(PartitionId, PartitionId), String>,
    pub edge_role: BTreeMap<String, EdgeRole>,
    pub order_method: CircleOrder,
}

impl CircPlan {
    pub fn members(&self, pid: PartitionId) -> &[String] {
        self.partitions
            .get(&pid)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    pub fn children_of(&self, pid: PartitionId) -> &[PartitionId] {
        self.backbone
            .get(&pid)
            .map(|n| n.children.as_slice())
            .unwrap_or(&[])
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CircleGeom {
    pub center: Point,
    pub radius: f64,
}

/// Edge skeleton written by Metric; Ink expands to `EdgePath`.
#[derive(Debug, Clone)]
pub enum CircRoute {
    Chord {
        start: Point,
        end: Point,
    },
    Spoke {
        start: Point,
        end: Point,
    },
    ExteriorArc {
        origin: Point,
        radius: f64,
        t0: f64,
        t1: f64,
        start: Point,
        end: Point,
    },
    /// Short outer arc on the far side of the node; does not occupy circle order.
    Loop {
        points: Vec<Point>,
    },
}

impl CircRoute {
    pub fn points(&self) -> Vec<Point> {
        match self {
            Self::Chord { start, end } | Self::Spoke { start, end } => vec![*start, *end],
            Self::ExteriorArc {
                origin,
                radius,
                t0,
                t1,
                start,
                end,
            } => {
                let mut pts = vec![*start];
                pts.extend(super::geom::arc_points_span(
                    *origin,
                    *radius,
                    *t0,
                    super::geom::clockwise_span(*t0, *t1),
                ));
                pts.push(*end);
                pts
            }
            Self::Loop { points } => points.clone(),
        }
    }

    pub fn start(&self) -> Point {
        match self {
            Self::Chord { start, .. }
            | Self::Spoke { start, .. }
            | Self::ExteriorArc { start, .. } => *start,
            Self::Loop { points } => points.first().copied().unwrap_or(Point { x: 0.0, y: 0.0 }),
        }
    }

    pub fn end(&self) -> Point {
        match self {
            Self::Chord { end, .. } | Self::Spoke { end, .. } | Self::ExteriorArc { end, .. } => {
                *end
            }
            Self::Loop { points } => points.last().copied().unwrap_or(Point { x: 0.0, y: 0.0 }),
        }
    }

    pub fn translate(&mut self, dx: f64, dy: f64) {
        let bump = |p: &mut Point| {
            p.x += dx;
            p.y += dy;
        };
        match self {
            Self::Chord { start, end } | Self::Spoke { start, end } => {
                bump(start);
                bump(end);
            }
            Self::ExteriorArc {
                origin, start, end, ..
            } => {
                bump(origin);
                bump(start);
                bump(end);
            }
            Self::Loop { points } => {
                for p in points {
                    bump(p);
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct CircMetric {
    pub frames: BTreeMap<String, Rect>,
    pub circles: BTreeMap<PartitionId, CircleGeom>,
    pub angles: BTreeMap<String, f64>,
    pub routes: BTreeMap<String, CircRoute>,
}

impl CircMetric {
    pub fn translate_partition(&mut self, pid: PartitionId, members: &[String], dx: f64, dy: f64) {
        if let Some(c) = self.circles.get_mut(&pid) {
            c.center.x += dx;
            c.center.y += dy;
        }
        for id in members {
            if let Some(f) = self.frames.get_mut(id) {
                f.x += dx;
                f.y += dy;
            }
        }
    }
}

