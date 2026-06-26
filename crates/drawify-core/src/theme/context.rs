//! context：`ThemeContext` re-export。
//!
//! `ThemeContext` 定义在 `schema.rs`，本模块提供便捷构造函数。

pub use super::schema::ThemeContext;

use crate::ast::Identifier;
use super::schema::CompiledTheme;

impl<'a> ThemeContext<'a> {
    /// 构造 prepare 侧的 ThemeContext。
    pub fn new(
        compiled: &'a CompiledTheme,
        diagram_type: &'a str,
        root_id: Option<&'a Identifier>,
    ) -> Self {
        Self {
            compiled,
            diagram_type,
            root_id,
        }
    }
}
