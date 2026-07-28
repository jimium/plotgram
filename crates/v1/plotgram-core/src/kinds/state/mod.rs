//! State（状态图）图表类型模块。

pub mod validate;

use crate::ast::Diagram;
use crate::error::ValidationResult;

use super::standard;
use super::traits::StandardDiagramKind;

/// State 图表类型的零大小标记类型。
pub struct State;

impl StandardDiagramKind for State {
    const STYLE_CONFIG: &'static standard::StandardStyleConfig = &standard::STATE;

    fn validate_specific(diagram: &Diagram, result: &mut ValidationResult) {
        validate::validate(diagram, result);
    }
}
