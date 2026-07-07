//! Flowchart 专属验证规则。

use crate::ast::Diagram;
use crate::error::ValidationResult;
use crate::validation::common::validate_self_loop;

/// decision 类型允许自环，其他类型发出警告。
const SELF_LOOP_EXEMPT_TYPES: &[&str] = &["decision"];

pub fn validate(diagram: &Diagram, result: &mut ValidationResult) {
    validate_self_loop(
        diagram,
        SELF_LOOP_EXEMPT_TYPES,
        "仅 type: decision 允许自环",
        result,
    );
}
