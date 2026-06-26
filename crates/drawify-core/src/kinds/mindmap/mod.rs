//! Mindmap（思维导图）图表类型模块。

pub mod validate;

use crate::ast::Diagram;
use crate::error::ValidationResult;

use super::standard;
use super::traits::StandardDiagramKind;

/// Mindmap 图表类型的零大小标记类型。
pub struct Mindmap;

impl StandardDiagramKind for Mindmap {
    const STYLE_CONFIG: &'static standard::StandardStyleConfig = &standard::MINDMAP;

    fn validate_specific(diagram: &Diagram, result: &mut ValidationResult) {
        validate::validate(diagram, result);
    }
}
