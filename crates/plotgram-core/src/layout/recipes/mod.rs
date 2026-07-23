//! 布局配方：每个图类型对应一个专属配方实现。

pub mod architecture;
pub mod circular;
pub mod er;
pub mod flowchart;
pub mod mindmap;
pub mod sequence;
pub mod state;

// 过渡期 re-export：保持旧路径可用
pub use architecture as architecture_v2;
pub use crate::layout::engines::layered as sugiyama_v2;
pub use crate::layout::engines::common;
pub use crate::layout::engines::coordinate as coordinate_solver;
