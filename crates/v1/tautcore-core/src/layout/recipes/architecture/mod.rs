//! 架构图专用布局 v2：无组 flat Sugiyama；有组走 Dialect StrongMacro
//!
//! # Recipe
//!
//! 通过 [`LayoutRecipe`](crate::layout::kernel::recipe::LayoutRecipe) 编排布局生命周期。
//! Stage 5：独立 `two_phase/` 已删除；有组路径调用
//! [`crate::layout::atlas::dialect::contraction::strong_macro`]。

pub(crate) mod arch_builder;
pub(crate) mod group_layout_hint;
pub(crate) mod group_sizing;
pub(crate) mod intra_sugiyama;
pub(crate) mod layout;
pub(crate) mod post_layout;
pub(crate) mod route_flags;

pub(crate) use group_layout_hint::{is_valid_group_layout_atom, VALID_GROUP_LAYOUTS};
pub use layout::ArchitectureV2Layout;
pub(crate) use route_flags::ArchRouteFlags;
