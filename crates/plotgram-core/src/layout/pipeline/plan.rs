//! 布局计划：在管线入口一次性解析算法名与 option 值。

use std::collections::HashMap;

use crate::ast::{AttributeValue, Diagram, Span};
use crate::error::{DiagnosticError, ValidationResult};
use crate::profile::DiagramProfile;
use crate::types::standard_attr_keys::diagram;

use crate::layout::algorithm_config::{diagram_algorithm_config, AlgorithmOptionSpec, OptionsReader};
use super::registry::LAYOUT_ALGORITHM_NAMES;
use super::entry::{edge_routing_option_specs, layout_option_specs};

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
    /// 当 `layout_algo == "auto"` 时，由 profile 解析出的实际算法名。
    /// 非 auto 时为 `None`。
    pub resolved_auto_algo: Option<String>,
    pub layout_options: ResolvedAlgoOptions,
    pub edge_routing: String,
    pub edge_options: ResolvedAlgoOptions,
}

impl LayoutPlan {
    /// 按 diagram 属性与 profile 默认值解析布局计划。
    pub fn resolve(diagram: &Diagram, profile: &DiagramProfile) -> Self {
        let layout_algo = resolve_algo_name(diagram, diagram::LAYOUT, profile.default_layout);

        // "auto" 解析为 profile 默认算法
        let resolved_auto_algo = if layout_algo == "auto" {
            Some(profile.default_layout.to_string())
        } else {
            None
        };
        // option specs 按实际算法查询
        let effective_algo = resolved_auto_algo.as_deref().unwrap_or(&layout_algo);
        let layout_specs = layout_option_specs(effective_algo);
        let layout_options = ResolvedAlgoOptions::resolve(
            diagram,
            diagram::LAYOUT,
            effective_algo,
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

        Self {
            layout_algo,
            resolved_auto_algo,
            layout_options,
            edge_routing,
            edge_options,
        }
    }

    /// 解析有效边路由算法：用户显式配置优先，否则采用布局 hints 推荐。
    pub fn resolve_effective_edge_routing(
        diagram: &Diagram,
        plan: &LayoutPlan,
        hints: &crate::layout::LayoutHints,
    ) -> String {
        if diagram_algorithm_name(diagram, diagram::EDGE_ROUTING).is_some() {
            return plan.edge_routing.clone();
        }
        match hints.edge_routing_style {
            crate::layout::EdgeRoutingStyle::Orthogonal => "orthogonal".to_string(),
            crate::layout::EdgeRoutingStyle::Curved => {
                if diagram.diagram_type == crate::types::DiagramType::Mindmap {
                    "organic".to_string()
                } else {
                    "circular".to_string()
                }
            }
            crate::layout::EdgeRoutingStyle::Straight => "straight".to_string(),
            crate::layout::EdgeRoutingStyle::Spline => "spline".to_string(),
            crate::layout::EdgeRoutingStyle::SelfLoop | crate::layout::EdgeRoutingStyle::Unspecified => {
                plan.edge_routing.clone()
            }
        }
    }

    /// catalog 查询用的空 plan（layout option 使用 spec 默认值）。
    pub fn default_for_catalog() -> Self {
        Self {
            layout_algo: LAYOUT_ALGORITHM_NAMES[0].to_string(),
            resolved_auto_algo: None,
            layout_options: ResolvedAlgoOptions::default(),
            edge_routing: String::new(),
            edge_options: ResolvedAlgoOptions::default(),
        }
    }

    /// catalog 查询某边路由算法的 plan（edge option 使用 spec 默认值，不查 strategy 实例）。
    pub fn catalog_edge_plan(algo: &str) -> Self {
        Self {
            layout_algo: String::new(),
            resolved_auto_algo: None,
            layout_options: ResolvedAlgoOptions::default(),
            edge_routing: algo.to_string(),
            edge_options: ResolvedAlgoOptions::default(),
        }
    }
}

/// 校验配置块中显式 option 值的类型/范围（非法值发警告，layout 阶段会回退默认值）。
pub fn validate_layout_plan_warnings(diagram: &Diagram, plan: &LayoutPlan, result: &mut ValidationResult) {
    let effective_algo = plan.resolved_auto_algo.as_deref().unwrap_or(&plan.layout_algo);
    validate_explicit_options(
        diagram,
        diagram::LAYOUT,
        effective_algo,
        layout_option_specs(effective_algo),
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

/// 读取 diagram 属性块中的算法名（未配置时返回 `None`）。
pub fn diagram_algorithm_name<'a>(diagram: &'a Diagram, key: &str) -> Option<&'a str> {
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
