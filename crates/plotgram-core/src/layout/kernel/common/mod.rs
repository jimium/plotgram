//! Kernel 共享工具（原 `engines::common`）。

pub mod acyclic;
pub mod canvas_bounds;
pub mod crossings;
pub mod divide_and_conquer;
pub mod edge_gutter;
pub mod graph_index;
pub mod group_map;
pub mod node_sizing;
pub mod overlap;
pub mod pack;
pub mod stats;

/// 过渡别名：包围盒真源在 `kernel::group::bounds`。
pub use crate::layout::kernel::group::bounds as group_bounds;
