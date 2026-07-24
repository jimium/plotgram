//! 统一坐标约束求解器（Kernel 层）。
//!
//! 本模块是坐标求解的核心实现，与具体图类型语义解耦。
//! 所有对齐、对称、间距逻辑通过 IR 表达，由 solver 统一求解。
//!
//! ## 模块结构
//!
//! - `model`: IR 数据结构（变量、硬约束、目标、问题对象）
//! - `projection`: PAVA 硬约束投影器
//! - `optimizer`: projected gradient optimizer
//! - `auditor`: P0 约束审计器
//!
//! ## 设计原则
//!
//! 1. **无图类型语义**：不接受 `DiagramType`，只消费 IR
//! 2. **确定性**：相同输入必须产生相同输出
//! 3. **可审计**：所有约束违反都可追溯

pub mod analysis;
pub mod auditor;
pub mod model;
pub mod optimizer;
pub mod projection;

// Builder 和 objectives 包含图类型语义，暂留在 node 层
// Phase 10: 进一步拆分为 kernel adapter + recipe builder
