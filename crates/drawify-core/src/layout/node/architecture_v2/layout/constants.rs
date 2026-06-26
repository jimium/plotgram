//! 架构图布局常量。

use crate::layout::constants;

pub(in super::super) const PADDING: f64 = constants::ARCH_V2_PADDING;
pub(in super::super) const GROUP_LABEL_HEIGHT: f64 = 20.0;
pub(in super::super) const LAYER_GAP: f64 = 80.0;
pub(in super::super) const MIN_GROUP_GAP: f64 = 8.0;
pub(in super::super) const NODE_GAP: f64 = 48.0;
pub(in super::super) const GROUP_GAP_X: f64 = 50.0;
pub(in super::super) const INTRA_LAYER_GAP: f64 = 56.0;
pub(in super::super) const CROSSING_SWEEPS_MAX: usize = 16;
pub(in super::super) const CROSSING_SWEEPS_MIN: usize = 4;

pub(in super::super) const COORDINATE_REFINE_ITERATIONS: usize = 8;
pub(in super::super) const COORDINATE_REFINE_EPSILON: f64 = 0.5;
pub(in super::super) const NEIGHBOR_PULL_FACTOR: f64 = 0.4;
pub(in super::super) const GROUP_CENTER_PULL_FACTOR: f64 = 0.25;
pub(in super::super) const TRANSPOSE_MAX_ROUNDS: usize = 10;
pub(in super::super) const NEIGHBOR_ALIGN_MAX_PASSES: usize = 4;
pub(in super::super) const LONG_EDGE_BARYCENTER_WEIGHT: f64 = 1.8;
