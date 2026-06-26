//! State 专属验证规则。

use crate::ast::Diagram;
use crate::error::ValidationResult;
use crate::validation::common::validate_unique_canonical_type;

pub fn validate(diagram: &Diagram, result: &mut ValidationResult) {
    validate_unique_canonical_type(
        diagram,
        "initial",
        1,
        "state 图中最多只能有一个 initial 节点",
        result,
    );
}
