//! 对齐与量化：节点/边 waypoint 的 grid snap 和画布最终化。

pub mod canvas_finalize;
pub mod grid_snap;

pub use grid_snap::{DiagramAlignOverride, EdgeSnapConfig, LayerAxisAlign, NodeAlignConfig};
