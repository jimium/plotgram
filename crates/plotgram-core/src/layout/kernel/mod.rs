//! 布局内核：共享的坐标求解与布局基础设施。
//!
//! 本模块包含多图类型共享的核心算法，与具体图类型语义解耦。
//! Kernel API 不接受 `DiagramType`，只消费 IR 数据结构。
//!
//! ## 模块结构
//!
//! - [`recipe`]: 统一布局配方 trait（`LayoutRecipe`）——整图节点布局的生命周期
//! - [`coordinate`]: 统一坐标约束求解器（PAVA + Projected Gradient）
//! - [`coordinator`]: Coordinate Kernel 调用封装（solve + P0 audit）
//! - [`frozen`]: 节点布局冻结（布局完成后不可再修改坐标）
//!
//! ## 概念边界
//!
//! | 概念 | 职责 | 输入 → 输出 |
//! |------|------|------------|
//! | `LayoutRecipe` | 整图节点布局 | Diagram → LayoutResult |
//! | `CoordinateSolveStep` | 坐标求解步骤 | CoordinateProblem → 坐标 |
//! | `coordinate::optimizer` | 纯求解器 | CoordinateProblem → SolverResult |
//!
//! ## 设计原则
//!
//! 1. **无图类型语义**：Kernel 不知道 flowchart/architecture/mindmap 的区别
//! 2. **IR 驱动**：所有布局语义在外部编译为 IR，Kernel 只消费 IR
//! 3. **确定性**：相同输入必须产生相同输出

pub mod coordinate;
pub mod coordinator;
pub mod frozen;
pub mod recipe;
