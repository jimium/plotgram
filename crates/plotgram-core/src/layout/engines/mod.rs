//! 共享布局引擎：与具体图类型语义解耦的算法构建块。
//!
//! - [`layered`]: Sugiyama 分层布局引擎（graph → rank → order → coordinate → postprocess）
//! - [`coordinate`]: 坐标约束问题构建器（消费 LayeredGraph → CoordinateProblem）
//! - [`common`]: 通用工具（node_sizing、acyclic、overlap、group_bounds、pack 等）

pub mod common;
pub mod coordinate;
pub mod layered;
