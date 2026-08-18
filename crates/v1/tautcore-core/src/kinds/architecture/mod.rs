//! Architecture（架构图）图表类型模块。

pub mod validate;

use crate::ast::Diagram;
use crate::error::ValidationResult;

use super::standard;
use super::traits::StandardDiagramKind;

/// Architecture 图表类型的零大小标记类型。
pub struct Architecture;

impl StandardDiagramKind for Architecture {
    const STYLE_CONFIG: &'static standard::StandardStyleConfig = &standard::ARCHITECTURE;

    fn validate_specific(diagram: &Diagram, result: &mut ValidationResult) {
        validate::validate(diagram, result);
    }
}
