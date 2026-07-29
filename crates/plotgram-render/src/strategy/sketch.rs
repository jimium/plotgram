//! Sketch render strategy: hand-drawn jitter + hatch fills.

use plotgram_model::geometry::Point;
use super::FillMode;

/// Hand-drawn style rendering.
#[derive(Debug, Clone)]
pub struct SketchStrategy {
    /// Jitter amplitude in px.
    amplitude: f64,
}

impl SketchStrategy {
    pub fn new() -> Self {
        Self { amplitude: 1.5 }
    }

    pub fn transform_path(&self, points: &[Point], seed: u64) -> Vec<Point> {
        points
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let hash = simple_hash(seed, i as u64);
                let jx = (hash % 1000) as f64 / 1000.0 - 0.5;
                let jy = ((hash / 1000) % 1000) as f64 / 1000.0 - 0.5;
                Point {
                    x: p.x + jx * self.amplitude,
                    y: p.y + jy * self.amplitude,
                }
            })
            .collect()
    }

    pub fn fill_mode(&self) -> FillMode {
        FillMode::Hatch
    }

    pub fn sample_outlines(&self) -> bool {
        true
    }

    pub fn name(&self) -> &'static str {
        "sketch"
    }
}

impl Default for SketchStrategy {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple deterministic hash for jitter.
fn simple_hash(seed: u64, index: u64) -> u64 {
    let mut h = seed.wrapping_mul(6364136223846793005).wrapping_add(index);
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51afd7ed558ccd);
    h ^= h >> 33;
    h
}
