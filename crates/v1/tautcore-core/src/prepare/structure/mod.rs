//! 结构展开层：从 relation 树派生语义属性（branch_slot / tree_depth）。
//!
//! 在 `apply_profile_defaults` 之后、`materialize_styles` 之前执行。
//! 仅 mindmap 需要结构展开；其他图表类型为 no-op。

pub mod mindmap;

use crate::ast::Diagram;
use crate::types::DiagramType;

/// 按图表类型分派结构展开。
pub fn expand_structure(diagram: &mut Diagram) {
    match diagram.diagram_type {
        DiagramType::Mindmap => mindmap::expand(diagram),
        _ => {}
    }
}
