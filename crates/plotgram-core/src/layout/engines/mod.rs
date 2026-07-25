//! 遗留引擎目录 —— 仅保留 `coordinate` builder。
//!
//! - `common` / `layered` 真源：[`crate::layout::kernel`]
//! - [`coordinate`]：遗留 builder，仍驻本目录并 re-export kernel 求解面

pub mod coordinate;
