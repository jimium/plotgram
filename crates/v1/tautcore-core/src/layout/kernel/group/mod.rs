//! Kernel group：包围盒物化 + 层级。

pub mod bounds;
pub mod hierarchy;

pub use bounds::{
    compute_group_bounds, compute_group_bounds_unrecorded, compute_group_bounds_with_side_gutters,
    container_padding_for_leaf, GroupPadding, GutterSide, SideGutter,
};
pub use hierarchy::{build_group_hierarchy, GroupHierarchy};
