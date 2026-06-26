//! Style context resolution: determine which StyleSheet to use.

use crate::ast::{AttributeValue, Diagram};
use crate::error::{DrawifyError, Result};
use crate::theme::{compiled_builtin_theme, CompiledTheme, ThemeIdResolver, is_internal_base};

/// 样式解析请求：指定如何选择 StyleSheet。
#[derive(Debug, Clone)]
pub struct StyleRequest {
    /// 显式指定的 theme_id（最高优先级）。
    pub theme_id: Option<String>,
    /// 是否暗色模式。
    pub dark_mode: bool,
}

impl Default for StyleRequest {
    fn default() -> Self {
        Self {
            theme_id: None,
            dark_mode: false,
        }
    }
}

/// 从 diagram 属性中读取 `theme`。
pub fn theme_from_diagram_attrs(diagram: &Diagram) -> Option<&str> {
    for attr in &diagram.attributes {
        if attr.key == "theme" {
            match &attr.value {
                AttributeValue::String(s) => return Some(s.as_str()),
                _ => continue,
            }
        }
    }
    None
}

/// 解析出 `CompiledTheme`。
///
/// theme_id 优先级由 [`crate::theme::resolve_theme_id`] 统一管理：
/// 1. `request.theme_id`（显式指定）
/// 2. diagram 属性中的 `theme`
/// 3. `profile.dark_theme_id`（当 dark_mode=true）
/// 4. `profile.default_theme_id`（兜底）
pub fn resolve_compiled_theme(
    diagram: &Diagram,
    request: &StyleRequest,
) -> Result<CompiledTheme> {
    let scene_theme_id = theme_from_diagram_attrs(diagram);
    let theme_id = ThemeIdResolver::new(&diagram.diagram_type)
        .explicit(request.theme_id.as_deref())
        .scene(scene_theme_id)
        .dark_mode(request.dark_mode)
        .resolve();

    // 拒绝内部基座（§10.2/§13.9）：mindmap.base 等仅作 extends 用，不可被用户显式指定
    if is_internal_base(theme_id) {
        return Err(DrawifyError::Style(format!(
            "theme '{theme_id}' is an internal base and cannot be used directly"
        )));
    }

    compiled_builtin_theme(theme_id)
        .ok_or_else(|| DrawifyError::Style(format!("unknown builtin theme '{theme_id}'")))
}
