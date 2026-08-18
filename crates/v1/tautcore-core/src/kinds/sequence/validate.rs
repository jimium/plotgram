//! Sequence 专属验证规则。

use crate::ast::Diagram;
use crate::error::ValidationResult;

pub fn validate(_diagram: &Diagram, _result: &mut ValidationResult) {
    // 时序图暂无额外验证规则
}
