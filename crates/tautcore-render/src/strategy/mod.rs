//! Render strategy: standard (precise) vs sketch (hand-drawn).

pub mod sketch;
pub mod standard;

use tautcore_model::geometry::Point;

use self::sketch::SketchStrategy;
use self::standard::StandardStrategy;

/// How fills are rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillMode {
    /// Solid fill (standard).
    Solid,
    /// Cross-hatch lines (sketch).
    Hatch,
}

/// Render style strategy (value type; no heap allocation).
#[derive(Debug, Clone)]
pub enum Strategy {
    Standard(StandardStrategy),
    Sketch(SketchStrategy),
}

impl Strategy {
    /// Transform path points (standard: identity; sketch: add jitter).
    pub fn transform_path(&self, points: &[Point], seed: u64) -> Vec<Point> {
        match self {
            Self::Standard(s) => s.transform_path(points, seed),
            Self::Sketch(s) => s.transform_path(points, seed),
        }
    }

    /// Fill rendering mode.
    pub fn fill_mode(&self) -> FillMode {
        match self {
            Self::Standard(s) => s.fill_mode(),
            Self::Sketch(s) => s.fill_mode(),
        }
    }

    /// Whether shape outlines should be sampled into polylines.
    pub fn sample_outlines(&self) -> bool {
        match self {
            Self::Standard(s) => s.sample_outlines(),
            Self::Sketch(s) => s.sample_outlines(),
        }
    }

    /// Strategy name (for SVG comments / debugging).
    pub fn name(&self) -> &'static str {
        match self {
            Self::Standard(s) => s.name(),
            Self::Sketch(s) => s.name(),
        }
    }
}

/// Select a strategy from the DSL `render_style` atom.
pub fn from_atom(atom: Option<&str>) -> Strategy {
    match atom {
        Some("sketch") => Strategy::Sketch(SketchStrategy::new()),
        _ => Strategy::Standard(StandardStrategy),
    }
}
