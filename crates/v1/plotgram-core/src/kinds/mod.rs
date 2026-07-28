//! 图表种类（kind）模块统一入口。
//!
//! 每种 [`DiagramType`] 的专属语义校验、结构语义与 scene 管线分派。
//!
//! - [`traits::DiagramKind`]：行为契约 trait
//! - [`traits::StandardDiagramKind`]：标准图共享默认实现
//! - [`validate_diagram_type`]：图表专属语义校验
//! - [`registry::DiagramTypeEntry`]：scene 物化 + SVG 绘制分派
//!
//! 静态契约见 [`crate::profile`]；视觉样式类型见 [`crate::render::visual`]；
//! scene 构建见 [`crate::render::export_scene`]；
//! SVG 编码见 [`crate::render::paint::scene_svg`]。

pub mod registry;
pub mod standard;
pub mod traits;
pub mod flowchart;
pub mod sequence;
pub mod state;
pub mod er;
pub mod mindmap;
pub mod architecture;

pub use registry::{entry_for, DiagramTypeEntry};
pub use standard::{StandardEdgeConfig, StandardStyleConfig};
pub use traits::{DiagramKind, StandardDiagramKind};
pub use crate::render::visual::{ArrowStyle, EdgeLabelStyle, EdgeStyle, LabelAnchor, NodeShape, NodeStyle};

use crate::ast::Diagram;
use crate::error::ValidationResult;
use crate::types::DiagramType;

/// 分派图表类型专属语义校验。
pub fn validate_diagram_type(
    diagram_type: &DiagramType,
    diagram: &Diagram,
    result: &mut ValidationResult,
) {
    let entry = entry_for(diagram_type);
    (entry.validate)(diagram, result);
}
