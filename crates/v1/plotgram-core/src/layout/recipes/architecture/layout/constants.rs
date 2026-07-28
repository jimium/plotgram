//! 架构图布局常量。

use crate::layout::constants;

pub(crate) const PADDING: f64 = constants::ARCH_V2_PADDING;
pub(crate) const GROUP_LABEL_HEIGHT: f64 = 20.0;
pub(crate) const LAYER_GAP: f64 = 72.0;
pub(crate) const NODE_GAP: f64 = 32.0;
pub(crate) const GROUP_GAP_X: f64 = 40.0;
pub(crate) const INTRA_LAYER_GAP: f64 = 40.0;
pub(crate) const CROSSING_SWEEPS_MAX: usize = 16;
pub(crate) const CROSSING_SWEEPS_MIN: usize = 4;

pub(crate) const TRANSPOSE_MAX_ROUNDS: usize = 10;
pub(crate) const LONG_EDGE_BARYCENTER_WEIGHT: f64 = 1.8;
