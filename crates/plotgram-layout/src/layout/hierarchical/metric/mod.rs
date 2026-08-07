//! Metric phase: main-axis stacking + cross-axis coordinate assignment
//! (BK ideal + cross-layer VPSC) + port point expansion + D1.0 track
//! coordinates. Canonical (TB) space only.

pub mod anchor;
pub mod bk;
pub mod bus;
pub mod cross_axis;
pub mod main_axis;
pub mod port_lane;
pub mod symmetry;
pub mod track;
