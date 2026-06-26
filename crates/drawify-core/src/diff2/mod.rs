//! Intent Diff — 语义层（RawDiagram）差异比较、补丁应用与格式化
//!
//! 本模块在 parse 之后、prepare 之前的语义层上工作，表达作者真正写进 DSL 的意图。
//!
//! ## 三个核心能力
//!
//! - [`diff`] — 比较两份 RawDiagram 的语义差异，输出结构化 [`ChangeSet`]
//! - [`patch`] — 将 [`ChangeSet`] 应用到基础 RawDiagram，产出更新后的 RawDiagram
//! - [`format`] — 将 RawDiagram 还原为可再解析的 DSL 文本
//!
//! ## 闭环
//!
//! ```text
//! diff(A, B) → Δ
//! patch(A, Δ) → A'
//! format(A') → DSL 文本
//! parse(DSL 文本) → A''
//! A'' 应与 B 语义等价
//! ```
//!
//! ## 适用场景
//!
//! - PR 审阅：结构化展示两份 DSL 的语义差异
//! - Agent 增量改图：基于 ChangeSet 精确修改图结构
//!
//! 物化后的有效态（PreparedDiagram）比较不在本模块范围内。

pub mod diff;
pub mod format;
pub mod patch;
pub mod types;

pub use diff::diff;
pub use format::format;
pub use patch::patch;
pub use types::{
    Change, ChangeOp, ChangePath, ChangeSet, ChangeTarget, PatchResult,
};

#[cfg(test)]
mod tests;
