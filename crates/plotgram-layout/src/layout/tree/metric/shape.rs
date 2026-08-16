//! SubtreeShape: Metric / placer IR. Absolute origin is arbitrary; the
//! dispatcher translates after merge.

use std::collections::BTreeMap;

use plotgram_model::geometry::Rect;

use super::geom::translate_route;
use crate::layout::tree::plan::TreeRoute;

#[derive(Debug, Clone)]
pub struct SubtreeShape {
    /// Local root id (empty for the forest accumulator).
    pub root: String,
    pub frames: BTreeMap<String, Rect>,
    pub routes: BTreeMap<String, TreeRoute>,
}

impl SubtreeShape {
    pub fn forest() -> Self {
        Self {
            root: String::new(),
            frames: BTreeMap::new(),
            routes: BTreeMap::new(),
        }
    }

    pub fn from_parts(
        root: impl Into<String>,
        frames: BTreeMap<String, Rect>,
        routes: BTreeMap<String, TreeRoute>,
    ) -> Self {
        Self {
            root: root.into(),
            frames,
            routes,
        }
    }

    pub fn frame(&self) -> Option<Rect> {
        self.frames.get(&self.root).copied()
    }

    pub fn translate(&mut self, dx: f64, dy: f64) {
        for f in self.frames.values_mut() {
            f.x += dx;
            f.y += dy;
        }
        for r in self.routes.values_mut() {
            translate_route(r, dx, dy);
        }
    }

    pub fn bounds(&self) -> Option<Rect> {
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for f in self.frames.values() {
            min_x = min_x.min(f.x);
            min_y = min_y.min(f.y);
            max_x = max_x.max(f.right());
            max_y = max_y.max(f.bottom());
        }
        if !min_x.is_finite() {
            return None;
        }
        Some(Rect::new(min_x, min_y, max_x - min_x, max_y - min_y))
    }

    pub fn merge(&mut self, other: SubtreeShape) {
        self.frames.extend(other.frames);
        self.routes.extend(other.routes);
    }
}
