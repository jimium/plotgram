//! 架构图专用布局 v2：全局 Sugiyama 与两阶段（组内→组间）布局
//!
//! # Recipe（Phase 10）
//!
//! [`recipe::ArchitectureRecipe`] 显式编排布局生命周期，复用 kernel solver。

pub(crate) mod arch_builder;
mod group_layout_hint;
mod group_sizing;
mod intra_sugiyama;
mod layout;
mod pipeline;
pub(crate) mod post_layout;
pub mod recipe;
mod two_phase;

pub(crate) use group_layout_hint::{is_valid_group_layout_atom, VALID_GROUP_LAYOUTS};
pub use layout::ArchitectureV2Layout;
