//! Standard render strategy: precise geometry, solid fills.

use plotgram_model::geometry::Point;
use super::FillMode;

/// Precise line rendering (default).
#[derive(Debug, Clone, Copy, Default)]
pub struct StandardStrategy;

impl StandardStrategy {
    pub fn transform_path(&self, points: &[Point], _seed: u64) -> Vec<Point> {
        points.to_vec()
    }

    pub fn fill_mode(&self) -> FillMode {
        FillMode::Solid
    }

    pub fn sample_outlines(&self) -> bool {
        false
    }

    pub fn name(&self) -> &'static str {
        "standard"
    }
}
