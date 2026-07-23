//! 统一坐标约束求解器（Node 层适配器）。
//!
//! Phase 9: 核心求解器已移至 `layout::kernel::coordinate`。
//! 本模块保留图类型相关的 builder 和 objectives，并从 kernel 重导出核心类型。
//!
//! ## 模块结构
//!
//! - `builder`: 从 Sugiyama layers/sizes 构建 CoordinateProblem
//! - `objectives`: 基础目标构建器
//! - `structure_objectives`: 结构目标构建器
//!
//! ## 重导出（来自 kernel）
//!
//! - `model`: IR 数据结构
//! - `projection`: PAVA 硬约束投影器
//! - `optimizer`: projected gradient optimizer
//! - `auditor`: P0 约束审计器

// 从 kernel 重导出核心模块
pub use crate::layout::kernel::coordinate::{auditor, model, optimizer, projection};

// 图类型相关的 builder 和 objectives 保留在 node 层
pub mod builder;
pub mod objectives;
pub mod structure_objectives;
