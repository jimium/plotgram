//! Atlas 下一代布局与路由架构（22 号文）的孵化子树。
//!
//! Stage 7：全图种默认 Atlas；Plan 边级 Provenance 覆盖 + 不变量门禁。

pub mod adapter;
pub mod channel;
pub mod channel_metric;
pub mod coord_descent;
pub mod dialect;
pub mod gate_mcf;
pub mod ink;
pub mod pipeline;
pub mod plan;
pub mod probe;
pub mod provenance;
pub mod provenance_check;
pub mod relaxation;
pub mod shadow;
pub mod solve;
pub mod space;
