//! Mindmap 专属验证规则。

use crate::ast::Diagram;
use crate::error::ValidationResult;
use crate::validation::common::validate_unique_canonical_type;

pub fn validate(diagram: &Diagram, result: &mut ValidationResult) {
    validate_unique_canonical_type(
        diagram,
        "root",
        1,
        "mindmap 图中最多只能有一个 root 节点",
        result,
    );
}
