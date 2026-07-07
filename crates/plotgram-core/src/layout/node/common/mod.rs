//! 节点布局共享工具

pub mod acyclic;
pub mod barnes_hut;
pub mod crossings;
pub mod divide_and_conquer;
pub mod group_bounds;
pub mod group_map;
pub mod graph_index;
pub mod node_sizing;
pub mod overlap;
pub mod pack;
pub mod preset;

// Re-export: label_placement 已下沉到 edge::common，此处保留兼容入口
pub use crate::layout::edge::common::label_placement;
