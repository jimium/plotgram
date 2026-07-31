//! Layout algorithms (in-tree; extract to `plotgram-layout-*` when large enough).

pub mod hierarchical;

pub use hierarchical::{
    GroupAlign, GroupPolicy, GroupSizing, HierarchicalLayout, HierarchicalParams,
    HierarchicalPreset, Orientation, RoutingStyle,
};
