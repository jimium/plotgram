//! Interchange 模块：语义型格式的导出与导入。
//!
//! 与视觉型导出（SVG/PNG/JSON/Draw.io）不同，interchange 格式
//! 从 `PreparedDiagram` 的语义树直接编码，不经过 layout 和 scene 物化。

pub mod mindmap;
