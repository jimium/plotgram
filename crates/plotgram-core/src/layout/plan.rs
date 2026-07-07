//! 布局计划：在管线入口一次性解析算法名与 option 值。

use std::collections::HashMap;

use crate::ast::{AttributeValue, Diagram, Span};
use crate::error::{DiagnosticError, ValidationResult};
use crate::profile::DiagramProfile;
use crate::types::standard_attr_keys::diagram;

use super::algorithm_config::{diagram_algorithm_config, AlgorithmOptionSpec, OptionsReader};
use super::edge::edge_bundling::BundlingConfig;
use super::registry::LAYOUT_ALGORITHM_NAMES;
use super::{edge_routing_option_specs, layout_option_specs};

/// §6: friendliness 模式（可插拔诊断信号）。
///
/// 控制友好性评估（V1）与调整（V2）的执行：
/// - `Off`：跳过 V1 和 V2，零开销。
/// - `Diagnose`：仅 V1 评估（写入 `hints.friendliness_report`），不调整布局。
/// - `Adjust`：V1 + V2，当前默认行为。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FriendlinessMode {
    /// 跳过 V1 评估和 V2 调整，零开销。
    Off,
    /// 仅 V1 评估（写入 hints.friendliness_report），不调整布局。
    Diagnose,
    /// V1 + V2，当前默认行为。
    #[default]
    Adjust,
}

impl FriendlinessMode {
    /// 从 DSL 字符串解析。
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "off" => Some(Self::Off),
            "diagnose" => Some(Self::Diagnose),
            "adjust" => Some(Self::Adjust),
            _ => None,
        }
    }

    /// V1 评估是否启用。
    pub fn v1_enabled(self) -> bool {
        !matches!(self, Self::Off)
    }

    /// V2 调整是否启用。
    pub fn v2_enabled(self) -> bool {
        matches!(self, Self::Adjust)
    }
}

/// 某算法的 option 已解析值（缺失项使用 spec / profile 默认值）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResolvedAlgoOptions {
    values: HashMap<String, f64>,
}

impl ResolvedAlgoOptions {
    /// 从 spec 默认值构建（无 DSL / profile 覆盖）。
    pub fn from_spec_defaults(specs: &'static [AlgorithmOptionSpec]) -> Self {
        let values = specs
            .iter()
            .map(|s| (s.key.to_string(), s.default))
            .collect();
        Self { values }
    }

    /// 从 diagram 属性块解析 option；`attr_key` 为 `layout_algo` 或 `edge_routing`。
    pub fn resolve(
        diagram: &Diagram,
        attr_key: &str,
        algo: &str,
        specs: &'static [AlgorithmOptionSpec],
        profile_defaults: &[(&str, f64)],
    ) -> Self {
        if specs.is_empty() && profile_defaults.is_empty() {
            return Self::default();
        }

        let span = attr_span(diagram, attr_key);
        let context = format!("{attr_key}/{algo}");
        let empty: HashMap<String, AttributeValue> = HashMap::new();
        let options = diagram_algorithm_config(diagram, attr_key)
            .map(|(_, opts)| opts)
            .unwrap_or(&empty);
        let reader = OptionsReader::new(options, span, &context);

        let mut values = HashMap::new();
        for spec in specs {
            values.insert(
                spec.key.to_string(),
                reader.read_spec_or_default(spec),
            );
        }
        for &(key, value) in profile_defaults {
            values.entry(key.to_string()).or_insert(value);
        }
        Self { values }
    }

    pub fn get_or_default(&self, spec: &AlgorithmOptionSpec) -> f64 {
        self.values
            .get(spec.key)
            .copied()
            .unwrap_or(spec.default)
    }

    pub fn get(&self, key: &str) -> Option<f64> {
        self.values.get(key).copied()
    }
}

/// 单次布局所需的算法选择与已解析 option。
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutPlan {
    pub layout_algo: String,
    pub layout_options: ResolvedAlgoOptions,
    pub edge_routing: String,
    pub edge_options: ResolvedAlgoOptions,
    /// §6: friendliness 模式（off | diagnose | adjust），默认 adjust。
    pub friendliness: FriendlinessMode,
    /// §7.3: Edge Bundling 配置（默认 disabled）。
    ///
    /// 通过 `edge_routing: orthogonal { bundling: true }` 启用。
    pub edge_bundling: BundlingConfig,
}

impl LayoutPlan {
    /// 按 diagram 属性与 profile 默认值解析布局计划。
    pub fn resolve(diagram: &Diagram, profile: &DiagramProfile) -> Self {
        let layout_algo = resolve_algo_name(diagram, diagram::LAYOUT, profile.default_layout);
        let layout_specs = layout_option_specs(&layout_algo);
        let layout_options = ResolvedAlgoOptions::resolve(
            diagram,
            diagram::LAYOUT,
            &layout_algo,
            layout_specs,
            profile.default_layout_options,
        );

        let edge_routing =
            resolve_algo_name(diagram, diagram::EDGE_ROUTING, profile.default_edge_routing);
        let edge_specs = edge_routing_option_specs(&edge_routing);
        let edge_options = ResolvedAlgoOptions::resolve(
            diagram,
            diagram::EDGE_ROUTING,
            &edge_routing,
            edge_specs,
            &[],
        );

        // §6: 从 layout 配置块解析 friendliness 模式
        let friendliness = resolve_friendliness_mode(diagram);

        // §7.3: 从 edge_routing 配置块解析 bundling 配置
        let edge_bundling = resolve_edge_bundling_config(diagram, &edge_routing, &edge_options);

        Self {
            layout_algo,
            layout_options,
            edge_routing,
            edge_options,
            friendliness,
            edge_bundling,
        }
    }

    /// catalog 查询用的空 plan（layout option 使用 spec 默认值）。
    pub fn default_for_catalog() -> Self {
        Self {
            layout_algo: LAYOUT_ALGORITHM_NAMES[0].to_string(),
            layout_options: ResolvedAlgoOptions::default(),
            edge_routing: String::new(),
            edge_options: ResolvedAlgoOptions::default(),
            friendliness: FriendlinessMode::default(),
            edge_bundling: BundlingConfig::default(),
        }
    }

    /// catalog 查询某边路由算法的 plan（edge option 使用 spec 默认值，不查 strategy 实例）。
    pub fn catalog_edge_plan(algo: &str) -> Self {
        Self {
            layout_algo: String::new(),
            layout_options: ResolvedAlgoOptions::default(),
            edge_routing: algo.to_string(),
            edge_options: ResolvedAlgoOptions::default(),
            friendliness: FriendlinessMode::default(),
            edge_bundling: BundlingConfig::default(),
        }
    }
}

/// §6: 从 diagram 的 layout 配置块解析 friendliness 模式。
///
/// DSL 语法：
/// ```dfy
/// layout: flowchart {
///     friendliness: off    // off | diagnose | adjust
/// }
/// ```
///
/// 缺省时返回 `Adjust`（与 v2.0 前行为一致）。
fn resolve_friendliness_mode(diagram: &Diagram) -> FriendlinessMode {
    if let Some((_, options)) = diagram_algorithm_config(diagram, diagram::LAYOUT) {
        if let Some(AttributeValue::String(tv)) = options.get("friendliness") {
            if let Some(mode) = FriendlinessMode::from_str(tv.as_str()) {
                return mode;
            }
        }
    }
    FriendlinessMode::default()
}

/// §7.3: 从 edge_routing 配置块解析 Edge Bundling 配置。
///
/// DSL 语法：
/// ```dfy
/// edge_routing: orthogonal {
///     bundling: true    // true | false（1 | 0）
/// }
/// ```
///
/// 仅对 `orthogonal` 路由有效；其他路由算法忽略 bundling 配置。
/// 当前仅支持 `bundling: true/false` 开关，其余参数使用 `BundlingConfig::default()`。
fn resolve_edge_bundling_config(
    _diagram: &Diagram,
    edge_routing: &str,
    edge_options: &ResolvedAlgoOptions,
) -> BundlingConfig {
    if edge_routing != "orthogonal" {
        return BundlingConfig::default();
    }

    // 检查 `bundling` option（Number 0.0/1.0）
    let enabled = edge_options.get("bundling").map_or(false, |v| v > 0.5);
    if !enabled {
        return BundlingConfig::default();
    }

    BundlingConfig {
        enabled: true,
        ..BundlingConfig::default()
    }
}

/// 校验配置块中显式 option 值的类型/范围（非法值发警告，layout 阶段会回退默认值）。
pub fn validate_layout_plan_warnings(diagram: &Diagram, plan: &LayoutPlan, result: &mut ValidationResult) {
    validate_explicit_options(
        diagram,
        diagram::LAYOUT,
        &plan.layout_algo,
        layout_option_specs(&plan.layout_algo),
        result,
    );
    if !plan.edge_routing.is_empty() {
        validate_explicit_options(
            diagram,
            diagram::EDGE_ROUTING,
            &plan.edge_routing,
            edge_routing_option_specs(&plan.edge_routing),
            result,
        );
    }
}

fn validate_explicit_options(
    diagram: &Diagram,
    attr_key: &str,
    algo: &str,
    specs: &'static [AlgorithmOptionSpec],
    result: &mut ValidationResult,
) {
    let Some(attr) = diagram.attributes.iter().find(|a| a.key == attr_key) else {
        return;
    };
    let AttributeValue::Config { options, .. } = &attr.value else {
        return;
    };
    if options.is_empty() {
        return;
    }
    let context = format!("{attr_key}/{algo}");
    let reader = OptionsReader::new(options, attr.span, &context);
    for spec in specs {
        if !options.contains_key(spec.key) {
            continue;
        }
        if reader.read_spec(spec).is_none() {
            result.add_warning(DiagnosticError::structure_violation(
                attr.span,
                format!("{} 选项 '{}' 值无效，将使用默认值 {}", context, spec.key, spec.default),
            ));
        }
    }
}

fn resolve_algo_name(diagram: &Diagram, key: &str, profile_default: &str) -> String {
    diagram_algorithm_name(diagram, key)
        .unwrap_or(profile_default)
        .to_string()
}

fn diagram_algorithm_name<'a>(diagram: &'a Diagram, key: &str) -> Option<&'a str> {
    diagram
        .attributes
        .iter()
        .find(|a| a.key == key)
        .and_then(|a| a.value.algorithm_name())
}

fn attr_span(diagram: &Diagram, key: &str) -> Span {
    diagram
        .attributes
        .iter()
        .find(|a| a.key == key)
        .map(|a| a.span)
        .unwrap_or_else(Span::dummy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{AttributeValue, Diagram, DiagramAttribute, Position, SourceInfo, TextValue};
    use crate::layout::algorithm_config::SUGIYAMA_LAYOUT_OPTIONS;
    use crate::layout::edge::edge_routing_bezier::BEZIER_OPTIONS;
    use crate::layout::edge::edge_routing_orthogonal::ORTHOGONAL_OPTIONS;
    use crate::profile::profile_for;
    use crate::types::DiagramType;

    fn sample_diagram() -> Diagram {
        Diagram::new(
            DiagramType::Flowchart,
            SourceInfo {
                file: None,
                line_count: 1,
            },
        )
    }

    fn config_attr(key: &str, algo: &str, options: &[(&str, AttributeValue)]) -> DiagramAttribute {
        DiagramAttribute {
            key: key.to_string(),
            value: AttributeValue::Config {
                algo: algo.to_string(),
                options: options
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.clone()))
                    .collect(),
            },
            span: Span::new(Position::new(1, 1), Position::new(1, 1)),
        }
    }

    #[test]
    fn resolve_uses_profile_defaults_when_attrs_missing() {
        let diagram = sample_diagram();
        let profile = profile_for(&diagram.diagram_type);
        let plan = LayoutPlan::resolve(&diagram, profile);

        assert_eq!(plan.layout_algo, "flowchart");
        assert_eq!(plan.edge_routing, "orthogonal");
    }

    #[test]
    fn resolve_edge_options_from_config_block() {
        let mut diagram = sample_diagram();
        diagram.attributes.push(config_attr(
            "edge_routing",
            "orthogonal",
            &[
                ("slot_pitch", AttributeValue::Number(55.0)),
                ("channel_margin", AttributeValue::Number(22.0)),
            ],
        ));
        let profile = profile_for(&diagram.diagram_type);
        let plan = LayoutPlan::resolve(&diagram, profile);

        assert_eq!(
            plan.edge_options.get_or_default(&ORTHOGONAL_OPTIONS[0]),
            55.0
        );
        assert_eq!(
            plan.edge_options.get_or_default(&ORTHOGONAL_OPTIONS[1]),
            22.0
        );
    }

    #[test]
    fn resolve_bezier_tension_from_config_block() {
        let mut diagram = sample_diagram();
        diagram.attributes.push(config_attr(
            "edge_routing",
            "bezier",
            &[("tension", AttributeValue::Number(1.2))],
        ));
        let profile = profile_for(&diagram.diagram_type);
        let plan = LayoutPlan::resolve(&diagram, profile);

        assert_eq!(plan.edge_routing, "bezier");
        assert_eq!(plan.edge_options.get_or_default(&BEZIER_OPTIONS[0]), 1.2);
    }

    #[test]
    fn resolve_layout_group_padding_from_config_block() {
        let mut diagram = sample_diagram();
        diagram.attributes.push(config_attr(
            "layout",
            "flowchart",
            &[("group_padding", AttributeValue::Number(40.0))],
        ));
        let profile = profile_for(&diagram.diagram_type);
        let plan = LayoutPlan::resolve(&diagram, profile);

        assert_eq!(
            plan.layout_options.get_or_default(&SUGIYAMA_LAYOUT_OPTIONS[0]),
            40.0
        );
    }

    #[test]
    fn invalid_layout_option_emits_warning() {
        let mut diagram = sample_diagram();
        diagram.attributes.push(config_attr(
            "layout",
            "flowchart",
            &[("group_padding", AttributeValue::Number(-5.0))],
        ));
        let profile = profile_for(&diagram.diagram_type);
        let plan = LayoutPlan::resolve(&diagram, profile);
        let mut result = ValidationResult::new();
        validate_layout_plan_warnings(&diagram, &plan, &mut result);
        assert!(!result.warnings.is_empty());
    }

    // ── §6: friendliness 解耦测试 ──

    #[test]
    fn friendliness_mode_parses_valid_strings() {
        assert_eq!(FriendlinessMode::from_str("off"), Some(FriendlinessMode::Off));
        assert_eq!(FriendlinessMode::from_str("diagnose"), Some(FriendlinessMode::Diagnose));
        assert_eq!(FriendlinessMode::from_str("adjust"), Some(FriendlinessMode::Adjust));
    }

    #[test]
    fn friendliness_mode_rejects_invalid_strings() {
        assert_eq!(FriendlinessMode::from_str("on"), None);
        assert_eq!(FriendlinessMode::from_str("enabled"), None);
        assert_eq!(FriendlinessMode::from_str(""), None);
    }

    #[test]
    fn friendliness_mode_v1_v2_flags() {
        assert!(!FriendlinessMode::Off.v1_enabled());
        assert!(!FriendlinessMode::Off.v2_enabled());

        assert!(FriendlinessMode::Diagnose.v1_enabled());
        assert!(!FriendlinessMode::Diagnose.v2_enabled());

        assert!(FriendlinessMode::Adjust.v1_enabled());
        assert!(FriendlinessMode::Adjust.v2_enabled());
    }

    #[test]
    fn friendliness_defaults_to_adjust() {
        // 无 friendliness 配置时，默认为 Adjust（向后兼容）
        let diagram = sample_diagram();
        let profile = profile_for(&diagram.diagram_type);
        let plan = LayoutPlan::resolve(&diagram, profile);
        assert_eq!(plan.friendliness, FriendlinessMode::Adjust);
    }

    #[test]
    fn friendliness_off_parsed_from_layout_config() {
        let mut diagram = sample_diagram();
        diagram.attributes.push(config_attr(
            "layout",
            "flowchart",
            &[("friendliness", AttributeValue::String(TextValue::unquoted("off".to_string())))],
        ));
        let profile = profile_for(&diagram.diagram_type);
        let plan = LayoutPlan::resolve(&diagram, profile);
        assert_eq!(plan.friendliness, FriendlinessMode::Off);
    }

    #[test]
    fn friendliness_diagnose_parsed_from_layout_config() {
        let mut diagram = sample_diagram();
        diagram.attributes.push(config_attr(
            "layout",
            "flowchart",
            &[("friendliness", AttributeValue::String(TextValue::unquoted("diagnose".to_string())))],
        ));
        let profile = profile_for(&diagram.diagram_type);
        let plan = LayoutPlan::resolve(&diagram, profile);
        assert_eq!(plan.friendliness, FriendlinessMode::Diagnose);
    }

    #[test]
    fn friendliness_invalid_value_falls_back_to_default() {
        let mut diagram = sample_diagram();
        diagram.attributes.push(config_attr(
            "layout",
            "flowchart",
            &[("friendliness", AttributeValue::String(TextValue::unquoted("yes".to_string())))],
        ));
        let profile = profile_for(&diagram.diagram_type);
        let plan = LayoutPlan::resolve(&diagram, profile);
        // 非法值回退到默认（Adjust）
        assert_eq!(plan.friendliness, FriendlinessMode::Adjust);
    }
}
