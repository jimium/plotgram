//! Atlas 下一代布局与路由架构（22 号文）的孵化子树。
//!
//! Stage 7：全图种默认 Atlas；Plan 边级 Provenance 覆盖 + 不变量门禁。

pub mod channel;
pub mod channel_metric;
pub mod dialect;
pub mod group_invariant;
pub mod ink;
pub mod ink_verify;
pub mod pipeline;
pub mod plan;
pub mod probe;
pub mod provenance;
pub mod provenance_check;
pub mod relaxation;
pub mod solve;
pub mod space;
