//! 主题选择策略：从多来源解析出最终 `theme_id`。
//!
//! 与 [`super::builtin`] 分工：
//! - 本模块决定 **用哪套主题**（引用 StyleSheet ID）
//! - 其余 `theme` 子模块处理 **主题里有什么**（加载、compile、物化）

use crate::types::DiagramType;
use crate::profile::profile_for;

/// 内部基座 ID（不在 `all_theme_ids()` 中，仅作 `extends` 用）。
///
/// 用户显式指定这些 ID 时应被拒绝（§10.2/§13.9）。
const INTERNAL_BASE_IDS: &[&str] = &["mindmap.base"];

/// 判断 theme_id 是否为内部基座（不可被用户显式指定）。
pub fn is_internal_base(id: &str) -> bool {
    INTERNAL_BASE_IDS.contains(&id)
}

/// 统一的 theme_id 解析优先级链。
///
/// 供 `prepare::resolve_compiled_theme` 和 `render::request::RenderRequest` 共用，
/// 消除两处独立实现的漂移风险。
///
/// 优先级（从高到低）：
/// 1. `explicit`（显式指定，如 API 参数 / request.theme_id）
/// 2. `scene`（场景级，如 diagram 属性中的 `theme`）
/// 3. `profile.dark_theme_id`（当 `dark_mode=true`）
/// 4. `profile.default_theme_id`（兜底）
pub fn resolve_theme_id<'a>(
    diagram_type: &DiagramType,
    explicit: Option<&'a str>,
    scene: Option<&'a str>,
    dark_mode: bool,
) -> &'a str {
    let profile = profile_for(diagram_type);
    explicit
        .or(scene)
        .or_else(|| {
            if dark_mode {
                profile.dark_theme_id
            } else {
                None
            }
        })
        .unwrap_or(profile.default_theme_id)
}

/// 主题 ID 解析器：封装 theme_id 解析的输入参数与解析逻辑。
///
/// 供 `prepare::resolve_compiled_theme` 和 `render::request::RenderRequest` 共用，
/// 消除两处独立组装解析链的漂移风险。
///
/// # 用法
///
/// ```ignore
/// let resolver = ThemeIdResolver::new(&diagram.diagram_type)
///     .explicit(request.theme_id.as_deref())
///     .scene(scene_theme_id)
///     .dark_mode(request.dark_mode);
/// let theme_id = resolver.resolve();
/// ```
#[derive(Debug, Clone)]
pub struct ThemeIdResolver<'a> {
    diagram_type: &'a DiagramType,
    explicit: Option<&'a str>,
    scene: Option<&'a str>,
    dark_mode: bool,
}

impl<'a> ThemeIdResolver<'a> {
    /// 创建解析器，指定图表类型。
    pub fn new(diagram_type: &'a DiagramType) -> Self {
        Self {
            diagram_type,
            explicit: None,
            scene: None,
            dark_mode: false,
        }
    }

    /// 设置显式指定的 theme_id（最高优先级）。
    pub fn explicit(mut self, id: Option<&'a str>) -> Self {
        self.explicit = id;
        self
    }

    /// 设置场景级 theme_id（如 diagram 属性中的 `theme`）。
    pub fn scene(mut self, id: Option<&'a str>) -> Self {
        self.scene = id;
        self
    }

    /// 设置是否暗色模式。
    pub fn dark_mode(mut self, dark: bool) -> Self {
        self.dark_mode = dark;
        self
    }

    /// 解析出最终的 theme_id。
    ///
    /// 优先级与 [`resolve_theme_id`] 一致。
    pub fn resolve(&self) -> &'a str {
        resolve_theme_id(self.diagram_type, self.explicit, self.scene, self.dark_mode)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::DiagramType;

    #[test]
    fn explicit_beats_profile_default() {
        let id = resolve_theme_id(
            &DiagramType::Flowchart,
            Some("common.presentation"),
            Some("common.blueprint"),
            true,
        );
        assert_eq!(id, "common.presentation");
    }

    #[test]
    fn dark_mode_uses_profile_dark_theme() {
        let id = resolve_theme_id(&DiagramType::Flowchart, None, None, true);
        assert_eq!(id, "common.clean-dark");
    }

    #[test]
    fn architecture_defaults_to_blueprint() {
        let id = resolve_theme_id(&DiagramType::Architecture, None, None, false);
        assert_eq!(id, "common.blueprint");
    }
}
