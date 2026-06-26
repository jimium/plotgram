//! LayoutLint 配置：按规则开关与严重级别覆盖。

use super::violation::{LintRuleId, LintSeverity};

/// 单条规则配置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleConfig {
    pub enabled: bool,
    /// `None` 表示使用 [`LintRuleId::default_severity`]
    pub severity: Option<LintSeverity>,
}

impl RuleConfig {
    pub const fn on() -> Self {
        Self {
            enabled: true,
            severity: None,
        }
    }

    pub const fn off() -> Self {
        Self {
            enabled: false,
            severity: None,
        }
    }

    pub const fn on_with_severity(severity: LintSeverity) -> Self {
        Self {
            enabled: true,
            severity: Some(severity),
        }
    }
}

/// Lint 预设档位。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LintProfile {
    /// 日常：硬约束 error；交叉 warning；贴 group 边框默认关闭
    #[default]
    Default,
    /// CI 门禁：仅硬约束，其余关闭
    Strict,
    /// 调试：全部规则按默认级别开启
    Verbose,
}

/// LayoutLint 全局配置。
#[derive(Debug, Clone)]
pub struct LintConfig {
    pub(crate) rules: [RuleConfig; LintRuleId::COUNT],
    /// 为 true 时 warning 也视为失败（`LintReport::is_acceptable`）
    pub fail_on_warning: bool,
}

impl Default for LintConfig {
    fn default() -> Self {
        Self::profile(LintProfile::Default)
    }
}

impl LintConfig {
    pub fn profile(profile: LintProfile) -> Self {
        match profile {
            LintProfile::Default => Self::default_preset(),
            LintProfile::Strict => Self::strict(),
            LintProfile::Verbose => Self::verbose(),
        }
    }

    /// 日常预设。
    pub fn default_preset() -> Self {
        let mut rules = Self::all_enabled_rules();
        rules[LintRuleId::EdgeOnGroupBorder.index()] = RuleConfig::off();
        Self {
            rules,
            fail_on_warning: false,
        }
    }

    /// CI 门禁：仅重叠 / 越界 / 穿节点 / 无关 group 穿透。
    pub fn strict() -> Self {
        let mut rules = [RuleConfig::off(); LintRuleId::COUNT];
        rules[LintRuleId::NodeOverlap.index()] = RuleConfig::on();
        rules[LintRuleId::GroupOverlap.index()] = RuleConfig::on();
        rules[LintRuleId::NodeOutsideGroup.index()] = RuleConfig::on();
        rules[LintRuleId::ChildGroupOutsideParent.index()] = RuleConfig::on();
        rules[LintRuleId::EdgeThroughNode.index()] = RuleConfig::on();
        rules[LintRuleId::EdgeCrossesGroupInterior.index()] = RuleConfig::on();
        Self {
            rules,
            fail_on_warning: false,
        }
    }

    /// 调试：全部规则开启。
    pub fn verbose() -> Self {
        Self {
            rules: Self::all_enabled_rules(),
            fail_on_warning: false,
        }
    }

    fn all_enabled_rules() -> [RuleConfig; LintRuleId::COUNT] {
        [RuleConfig::on(); LintRuleId::COUNT]
    }

    pub fn rule(&self, id: LintRuleId) -> RuleConfig {
        self.rules[id.index()]
    }

    pub fn is_enabled(&self, id: LintRuleId) -> bool {
        self.rules[id.index()].enabled
    }

    pub fn severity_for(&self, id: LintRuleId) -> LintSeverity {
        self.rules[id.index()]
            .severity
            .unwrap_or_else(|| id.default_severity())
    }

    /// 关闭指定规则（链式）。
    pub fn without(mut self, ids: &[LintRuleId]) -> Self {
        for id in ids {
            self.rules[id.index()] = RuleConfig::off();
        }
        self
    }

    /// 仅启用指定规则，其余关闭。
    pub fn only(mut self, ids: &[LintRuleId]) -> Self {
        self.rules = [RuleConfig::off(); LintRuleId::COUNT];
        for id in ids {
            self.rules[id.index()] = RuleConfig::on();
        }
        self
    }

    pub fn with_fail_on_warning(mut self, fail: bool) -> Self {
        self.fail_on_warning = fail;
        self
    }
}

pub fn parse_lint_profile(s: &str) -> Option<LintProfile> {
    match s {
        "default" => Some(LintProfile::Default),
        "strict" | "ci" => Some(LintProfile::Strict),
        "verbose" | "all" => Some(LintProfile::Verbose),
        _ => None,
    }
}

pub fn parse_lint_rule(s: &str) -> Option<LintRuleId> {
    LintRuleId::ALL.iter().copied().find(|id| id.as_str() == s)
}

pub fn parse_lint_rules_list(s: &str) -> Vec<LintRuleId> {
    s.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .filter_map(parse_lint_rule)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_preset_disables_group_border() {
        let config = LintConfig::default();
        assert!(!config.is_enabled(LintRuleId::EdgeOnGroupBorder));
        assert!(config.is_enabled(LintRuleId::NodeOverlap));
        assert!(config.is_enabled(LintRuleId::EdgeCrossing));
    }

    #[test]
    fn strict_preset_only_hard_rules() {
        let config = LintConfig::strict();
        assert!(config.is_enabled(LintRuleId::NodeOverlap));
        assert!(!config.is_enabled(LintRuleId::EdgeCrossing));
        assert!(!config.is_enabled(LintRuleId::EdgeOnGroupBorder));
    }

    #[test]
    fn verbose_enables_all_rules() {
        let config = LintConfig::verbose();
        for rule in LintRuleId::ALL {
            assert!(config.is_enabled(rule), "{rule:?} should be enabled");
        }
    }

    #[test]
    fn severity_override() {
        let mut config = LintConfig::strict();
        config.rules[LintRuleId::EdgeThroughNode.index()] =
            RuleConfig::on_with_severity(LintSeverity::Warning);
        assert_eq!(
            config.severity_for(LintRuleId::EdgeThroughNode),
            LintSeverity::Warning
        );
    }

    #[test]
    fn parse_profile_aliases() {
        assert_eq!(parse_lint_profile("ci"), Some(LintProfile::Strict));
        assert_eq!(parse_lint_profile("all"), Some(LintProfile::Verbose));
    }
}
