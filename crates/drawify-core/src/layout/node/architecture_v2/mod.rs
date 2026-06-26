//! 架构图专用布局 v2：全局 Sugiyama 与两阶段（组内→组间）布局

mod group_layout_hint;
mod group_sizing;
mod layout;
mod pipeline;
pub(crate) mod post_layout;
mod two_phase;

pub(crate) use group_layout_hint::{is_valid_group_layout_atom, VALID_GROUP_LAYOUTS};
pub(crate) use group_sizing::{
    is_valid_group_sizing_atom,
    GroupPaddingLike, VALID_GROUP_SIZING,
};
pub use layout::ArchitectureV2Layout;
