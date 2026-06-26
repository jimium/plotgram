//! Drawify DSL 前端：词法分析与语法分析。
//!
//! 将源文本解析为 [`crate::ast::Diagram`]。

pub mod lexer;
pub mod parser;

pub use parser::{parse, parse_with_diagnostics};
