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
pub mod builder_common;
pub mod group_ir;
pub mod main_axis;
pub mod model;
pub mod optimizer;
pub mod projection;
pub mod session;

/// Phase 5 / D4-3：统一 CoordinateProblem 构造门面（共享层变量 + 相邻分离）。
///
/// 生产路径均经 `CoordinateProblem::build` + `builder_common` 辅助；
/// 内联 `CoordinateProblem {` 仅允许 kernel 测试辅助（见 `check-builder-entry.sh`）。
pub mod builder {
    pub use super::builder_common::{
        append_rank_layer_vars, build_adjacent_min_separations, RankNodeSpec,
    };
    pub use super::group_ir::{
        attach_group_ir, attach_group_ir_cross, draft_boost_h_g3_from_pair_gaps,
        draft_sibling_gap_from_load, pair_gaps_from_corridor_demands,
        pair_gaps_from_cross_group_edge_loads, materialize_group_cross_bounds,
        materialize_group_main_bounds, refine_group_frames, refine_group_frames_cross,
    };
    pub use super::main_axis::{layer_center_ys_from_tops, solve_main_axis_layer_tops};
    pub use super::model::CoordinateProblem;
    pub use super::session::{LayoutSession, LayoutSolution};
}

// Builder 和 objectives 包含图类型语义，暂留在 node 层
// Phase 10: 进一步拆分为 kernel adapter + recipe builder
