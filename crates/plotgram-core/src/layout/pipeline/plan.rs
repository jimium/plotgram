//! 布局计划：在管线入口一次性解析算法名与 option 值。

use std::collections::HashMap;

use crate::ast::{AttributeValue, Diagram, Span};
use crate::error::{DiagnosticError, ValidationResult};
use crate::profile::DiagramProfile;
use crate::types::attr_constants::pipeline as pipeline_atoms;
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

/// 布局管线选择（Atlas Stage 7+：仅 Atlas；Shadow 对拍已退役，见 doc 30 R2）。
///
/// 解析优先级：diagram attr `pipeline:` > 环境变量 `PLOTGRAM_PIPELINE` >
/// 默认 Atlas。非法值（含已删的 `shadow` / `legacy`）静默回落默认；
/// WASM 上 `env::var` 返回 Err，天然回落。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PipelineChoice {
    /// Atlas 三相管线（默认生产路径）。
    #[default]
    Atlas,
}

impl PipelineChoice {
    /// 从字符串解析（attr 与环境变量共用）。
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            pipeline_atoms::ATLAS => Some(Self::Atlas),
            _ => None,
        }
    }

    fn resolve(diagram: &Diagram) -> Self {
        // 1. 显式属性优先
        if let Some(v) = diagram
            .attributes
            .iter()
            .find(|a| a.key == diagram::PIPELINE)
            .and_then(|a| a.value.as_str())
        {
            if let Some(choice) = Self::parse(v) {
                return choice;
            }
            crate::perf_log!(
                "[pipeline] unknown pipeline={v:?} (shadow retired); falling back to atlas"
            );
        }
        // 2. 环境变量次之
        if let Ok(v) = std::env::var("PLOTGRAM_PIPELINE") {
            if let Some(choice) = Self::parse(&v) {
                return choice;
            }
            crate::perf_log!(
                "[pipeline] unknown PLOTGRAM_PIPELINE={v:?} (shadow retired); falling back to atlas"
            );
        }
        // 3. 默认 Atlas
        Self::Atlas
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
    /// 管线选择（仅 atlas）。
    pub pipeline: PipelineChoice,
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
            pipeline: PipelineChoice::resolve(diagram),
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
            crate::layout::EdgeRoutingStyle::Orthogonal => {
                // R1：OVG 已删；Hier 走 Atlas Ink。LayoutPipeline 若见此 hint，回落 plan 默认。
                plan.edge_routing.clone()
            }
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
            pipeline: PipelineChoice::default(),
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
            pipeline: PipelineChoice::default(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{DiagramAttribute, TextValue};

    fn diagram_with_pipeline_attr(value: &str) -> Diagram {
        let mut d = Diagram::default();
        d.attributes.push(DiagramAttribute {
            key: diagram::PIPELINE.to_string(),
            value: AttributeValue::String(TextValue::unquoted(value)),
            span: Span::dummy(),
        });
        d
    }

    #[test]
    fn pipeline_choice_parses_known_atoms_only() {
        assert_eq!(PipelineChoice::parse("legacy"), None, "legacy 已删除");
        assert_eq!(PipelineChoice::parse("atlas"), Some(PipelineChoice::Atlas));
        assert_eq!(PipelineChoice::parse("shadow"), None, "shadow 已退役");
        assert_eq!(PipelineChoice::parse("bogus"), None);
        assert_eq!(PipelineChoice::parse(""), None);
    }

    /// 解析优先级：attr > 环境变量 > 默认；非法 env 值静默回落。
    /// Stage 7 默认：全部图种 → Atlas；无 legacy / shadow。
    #[test]
    fn pipeline_choice_resolution_priority() {
        std::env::remove_var("PLOTGRAM_PIPELINE");

        // 无组 flowchart → Atlas
        assert_eq!(
            PipelineChoice::resolve(&Diagram::default()),
            PipelineChoice::Atlas,
            "无 attr 无 env，无组 flowchart → Atlas"
        );

        // 有组 flowchart → Atlas
        let mut with_group = Diagram::default();
        with_group.groups = vec![crate::ast::Group {
            id: crate::ast::Identifier::new_unchecked("g1"),
            label: "G1".into(),
            attributes: Default::default(),
            parent_id: None,
            depth: 0,
            entity_ids: vec![],
            child_group_ids: vec![],
            span: crate::ast::Span::dummy(),
        }];
        assert_eq!(
            PipelineChoice::resolve(&with_group),
            PipelineChoice::Atlas,
            "有组 flowchart → Atlas"
        );

        // architecture → Atlas
        let mut arch = Diagram::default();
        arch.diagram_type = crate::types::DiagramType::Architecture;
        assert_eq!(
            PipelineChoice::resolve(&arch),
            PipelineChoice::Atlas,
            "architecture → Atlas"
        );

        // er → Atlas
        let mut er = Diagram::default();
        er.diagram_type = crate::types::DiagramType::Er;
        assert_eq!(
            PipelineChoice::resolve(&er),
            PipelineChoice::Atlas,
            "er → Atlas"
        );

        // mindmap → Atlas
        let mut mm = Diagram::default();
        mm.diagram_type = crate::types::DiagramType::Mindmap;
        assert_eq!(
            PipelineChoice::resolve(&mm),
            PipelineChoice::Atlas,
            "mindmap → Atlas"
        );

        // 已退役 shadow env → 回落 Atlas
        std::env::set_var("PLOTGRAM_PIPELINE", "shadow");
        assert_eq!(
            PipelineChoice::resolve(&Diagram::default()),
            PipelineChoice::Atlas,
            "shadow env 不再识别 → 回落 Atlas"
        );

        // attr atlas
        assert_eq!(
            PipelineChoice::resolve(&diagram_with_pipeline_attr("atlas")),
            PipelineChoice::Atlas,
            "attr atlas"
        );

        // 非法 env（含已删 legacy）静默回落默认值
        std::env::set_var("PLOTGRAM_PIPELINE", "legacy");
        assert_eq!(
            PipelineChoice::resolve(&Diagram::default()),
            PipelineChoice::Atlas,
            "legacy env 不再识别 → 回落 Atlas"
        );

        std::env::set_var("PLOTGRAM_PIPELINE", "bogus");
        assert_eq!(
            PipelineChoice::resolve(&Diagram::default()),
            PipelineChoice::Atlas,
            "非法 env 值回落默认值（flowchart → Atlas）"
        );

        std::env::remove_var("PLOTGRAM_PIPELINE");
    }

    /// DSL 全链：parser 接受 `pipeline:` atom，validation 枚举校验，
    /// LayoutPlan 解析到位；非法值（含 shadow）被 validation 拦截。
    #[test]
    fn pipeline_attr_flows_through_dsl_chain() {
        let ok = crate::pipeline::parse_prepare_validate(
            "diagram flowchart {\n    config {\n        pipeline: atlas\n    }\n    entity a \"A\"\n}",
            &crate::prepare::StyleRequest::default(),
        );
        assert!(ok.is_valid(), "{:?}", ok.errors);
        assert_eq!(
            ok.diagram.unwrap().layout_plan().pipeline,
            PipelineChoice::Atlas
        );

        let retired = crate::pipeline::parse_prepare_validate(
            "diagram flowchart {\n    config {\n        pipeline: shadow\n    }\n    entity a \"A\"\n}",
            &crate::prepare::StyleRequest::default(),
        );
        assert!(
            !retired.is_valid(),
            "shadow 应被 validation 拦截: {:?}",
            retired.errors
        );

        let bad = crate::pipeline::parse_prepare_validate(
            "diagram flowchart {\n    config {\n        pipeline: bogus\n    }\n    entity a \"A\"\n}",
            &crate::prepare::StyleRequest::default(),
        );
        assert!(!bad.is_valid(), "非法 pipeline 值应被 validation 拦截");
    }
}
