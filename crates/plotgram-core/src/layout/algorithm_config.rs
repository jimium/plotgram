//! 布局/边路由算法配置块读写与选项校验。

use std::collections::HashMap;

use crate::ast::{AttributeValue, Diagram, Span};
use crate::error::{DiagnosticError, ValidationResult};

use super::constants;

/// 算法 option 值的类型约束。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OptionKind {
    /// 非负浮点数（>= 0）
    NonNegativeNumber,
    /// 正浮点数（> 0）
    PositiveNumber,
    /// 有上下界的浮点数；`exclude_min` 为 true 时不含下界。
    Number {
        min: f64,
        max: f64,
        exclude_min: bool,
    },
}

/// 单个算法 option 的元数据（登记在 Strategy trait 上）。
#[derive(Debug, Clone, Copy)]
pub struct AlgorithmOptionSpec {
    pub key: &'static str,
    pub kind: OptionKind,
    pub default: f64,
    pub description: &'static str,
}

/// 生成布局算法 Config struct + OPTIONS 常量表 + Default + from_options。
///
/// 用法：
/// ```ignore
/// define_layout_config! {
///     /// 文档注释
///     pub struct FooConfig {
///         options = FOO_LAYOUT_OPTIONS;
///         padding: "padding" ; OptionKind::NonNegativeNumber ; constants::FOO_PADDING ; "画布内边距",
///         gap: "gap" ; OptionKind::PositiveNumber ; constants::FOO_GAP ; "间距",
///     }
/// }
/// ```
macro_rules! define_layout_config {
    (
        $(#[$meta:meta])*
        pub struct $name:ident {
            options = $options_const:ident;
            $(
                $(#[$field_meta:meta])*
                $field:ident: $opt_key:literal ; $opt_kind:expr ; $opt_default:expr ; $opt_desc:literal
            ),+ $(,)?
        }
    ) => {
        pub const $options_const: &[AlgorithmOptionSpec] = &[
            $( AlgorithmOptionSpec {
                key: $opt_key,
                kind: $opt_kind,
                default: $opt_default,
                description: $opt_desc,
            } ),+
        ];

        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq)]
        pub struct $name {
            $( $(#[$field_meta])* pub $field: f64, )+
        }

        impl Default for $name {
            fn default() -> Self {
                let opts: &[AlgorithmOptionSpec] = $options_const;
                let mut iter = opts.iter();
                $(
                    let $field = iter.next().unwrap().default;
                )+
                Self {
                    $( $field, )+
                }
            }
        }

        impl $name {
            pub fn from_options(options: &super::plan::ResolvedAlgoOptions) -> Self {
                let opts: &[AlgorithmOptionSpec] = $options_const;
                let mut iter = opts.iter();
                $(
                    let $field = options.get_or_default(iter.next().unwrap());
                )+
                Self {
                    $( $field, )+
                }
            }
        }
    };
}

define_layout_config! {
    /// Sugiyama 系布局已解析的运行时配置。
    pub struct SugiyamaLayoutConfig {
        options = SUGIYAMA_LAYOUT_OPTIONS;
        group_padding: "group_padding" ; OptionKind::NonNegativeNumber ; constants::SUGIYAMA_GROUP_PADDING ; "分组包围框内边距",
    }
}

define_layout_config! {
    /// 圆形布局已解析的运行时配置。
    pub struct CircularLayoutConfig {
        options = CIRCULAR_LAYOUT_OPTIONS;
        group_padding: "group_padding" ; OptionKind::NonNegativeNumber ; constants::DEFAULT_GROUP_PADDING ; "分组包围框内边距",
        padding: "padding" ; OptionKind::NonNegativeNumber ; constants::CIRCULAR_PADDING ; "画布内边距",
        component_gap: "component_gap" ; OptionKind::NonNegativeNumber ; constants::CIRCULAR_COMPONENT_GAP ; "多连通分量圆环之间的水平间距",
    }
}

define_layout_config! {
    /// 思维导图布局已解析的运行时配置。
    pub struct MindmapLayoutConfig {
        options = MINDMAP_LAYOUT_OPTIONS;
        padding: "padding" ; OptionKind::NonNegativeNumber ; constants::MINDMAP_PADDING ; "画布内边距",
        level_gap: "level_gap" ; OptionKind::PositiveNumber ; constants::MINDMAP_LEVEL_GAP ; "层级之间的主轴间距",
        branch_gap: "branch_gap" ; OptionKind::NonNegativeNumber ; constants::MINDMAP_BRANCH_GAP ; "同层兄弟节点间距",
        node_gap: "node_gap" ; OptionKind::NonNegativeNumber ; constants::MINDMAP_NODE_GAP ; "径向模式下子树垂直间距",
        center_gap: "center_gap" ; OptionKind::NonNegativeNumber ; constants::MINDMAP_CENTER_GAP ; "根节点到一级分支的水平间距",
    }
}

define_layout_config! {
    /// 时序图布局已解析的运行时配置。
    pub struct SequenceLayoutConfig {
        options = SEQUENCE_LAYOUT_OPTIONS;
        group_padding: "group_padding" ; OptionKind::NonNegativeNumber ; constants::DEFAULT_GROUP_PADDING ; "分组包围框内边距",
        node_spacing: "node_spacing" ; OptionKind::NonNegativeNumber ; constants::SEQUENCE_NODE_SPACING ; "参与者节点水平间距",
        message_spacing: "message_spacing" ; OptionKind::PositiveNumber ; constants::SEQUENCE_MESSAGE_SPACING ; "相邻消息行垂直间距",
    }
}

define_layout_config! {
    /// architecture 布局已解析的运行时配置。
    pub struct ArchitectureV2LayoutConfig {
        options = ARCHITECTURE_V2_LAYOUT_OPTIONS;
        group_padding: "group_padding" ; OptionKind::NonNegativeNumber ; constants::ARCH_V2_GROUP_PADDING ; "分组包围框内边距",
        padding: "padding" ; OptionKind::NonNegativeNumber ; constants::ARCH_V2_PADDING ; "画布内边距",
    }
}

/// 读取算法配置块内的选项。
pub struct OptionsReader<'a> {
    options: &'a HashMap<String, AttributeValue>,
    span: Span,
    context: &'a str,
}

impl<'a> OptionsReader<'a> {
    pub fn new(
        options: &'a HashMap<String, AttributeValue>,
        span: Span,
        context: &'a str,
    ) -> Self {
        Self {
            options,
            span,
            context,
        }
    }

    pub fn number(&self, key: &str, range: Option<(f64, f64)>) -> Option<f64> {
        let value = self.options.get(key)?;
        let n = match value {
            AttributeValue::Number(n) => *n,
            AttributeValue::String(s) => {
                s.trim().parse().ok()?
            }
            _ => return None,
        };
        if let Some((min, max)) = range {
            if n < min || n > max {
                return None;
            }
        }
        Some(n)
    }

    pub fn positive_number(&self, key: &str) -> Option<f64> {
        self.number(key, None).filter(|v| *v > 0.0)
    }

    pub fn non_negative_number(&self, key: &str) -> Option<f64> {
        self.number(key, None).filter(|v| *v >= 0.0)
    }

    /// 按 spec 约束读取 option；键不存在或值不合法时返回 `None`。
    pub fn read_spec(&self, spec: &AlgorithmOptionSpec) -> Option<f64> {
        if !self.options.contains_key(spec.key) {
            return None;
        }
        match spec.kind {
            OptionKind::NonNegativeNumber => self.non_negative_number(spec.key),
            OptionKind::PositiveNumber => self.positive_number(spec.key),
            OptionKind::Number {
                min,
                max,
                exclude_min,
            } => self.number(spec.key, Some((min, max))).and_then(|v| {
                if exclude_min && v <= min {
                    None
                } else {
                    Some(v)
                }
            }),
        }
    }

    /// 按 spec 读取 option，缺失或非法时回退到 spec 默认值。
    pub fn read_spec_or_default(&self, spec: &AlgorithmOptionSpec) -> f64 {
        self.read_spec(spec).unwrap_or(spec.default)
    }

    /// 对不在 `known` 列表中的 option key 发出警告。
    pub fn warn_unknown_keys(&self, known: &[&str], result: &mut ValidationResult) {
        for key in self.options.keys() {
            if known.contains(&key.as_str()) {
                continue;
            }
            result.add_warning(DiagnosticError::structure_violation(
                self.span,
                format!("{} 包含未知选项 '{key}'", self.context),
            ));
        }
    }
}

/// 在 spec 列表中按 key 查找。
pub fn find_option_spec<'a>(
    specs: &'static [AlgorithmOptionSpec],
    key: &str,
) -> Option<&'a AlgorithmOptionSpec> {
    specs.iter().find(|s| s.key == key)
}

/// 校验 diagram 上 layout_algo / edge_routing 配置块中的未知 option key。
pub fn validate_algorithm_config_warnings(diagram: &Diagram, result: &mut ValidationResult) {
    for attr in &diagram.attributes {
        let AttributeValue::Config { algo, options } = &attr.value else {
            continue;
        };
        if options.is_empty() {
            continue;
        }
        match attr.key.as_str() {
            "layout" => {
                let context = format!("layout/{algo}");
                let reader = OptionsReader::new(options, attr.span, &context);
                let known: Vec<&str> = super::layout_option_specs(algo)
                    .iter()
                    .map(|s| s.key)
                    .collect();
                reader.warn_unknown_keys(&known, result);
            }
            "edge_routing" => {
                let context = format!("edge_routing/{algo}");
                let reader = OptionsReader::new(options, attr.span, &context);
                let known: Vec<&str> = super::edge_routing_option_specs(algo)
                    .iter()
                    .map(|s| s.key)
                    .collect();
                reader.warn_unknown_keys(&known, result);
            }
            _ => {}
        }
    }
}

/// 读取 diagram 级算法属性：算法名 + option map（简写 atom 时 options 为空）。
pub fn diagram_algorithm_config<'a>(
    diagram: &'a Diagram,
    key: &str,
) -> Option<(&'a str, &'a HashMap<String, AttributeValue>)> {
    static EMPTY: std::sync::OnceLock<HashMap<String, AttributeValue>> =
        std::sync::OnceLock::new();
    let empty = EMPTY.get_or_init(HashMap::new);

    diagram.attributes.iter().find(|a| a.key == key).map(|a| {
        (
            a.value.algorithm_name().unwrap_or_default(),
            a.value.algorithm_options().unwrap_or(empty),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::DiagramType;
    use crate::ast::{DiagramAttribute, SourceInfo};

    fn diagram_with_edge_routing(value: AttributeValue) -> Diagram {
        Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: vec![DiagramAttribute {
                key: "edge_routing".to_string(),
                value,
                span: Span::dummy(),
            }],
            entities: vec![],
            relations: vec![],
            groups: vec![],
            constraints: vec![],
            style_decls: vec![],
            source_info: SourceInfo {
                file: None,
                line_count: 1,
            },
            ..Default::default()
        }
    }

    #[test]
    fn unknown_edge_routing_option_emits_warning() {
        let mut options = HashMap::new();
        options.insert("tension".to_string(), AttributeValue::Number(0.5));
        options.insert("typo".to_string(), AttributeValue::Number(1.0));
        let diagram = diagram_with_edge_routing(AttributeValue::Config {
            algo: "bezier".to_string(),
            options,
        });

        let mut result = ValidationResult::new();
        validate_algorithm_config_warnings(&diagram, &mut result);
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0]
            .message
            .contains("未知选项 'typo'"));
    }

    #[test]
    fn read_spec_respects_exclude_min() {
        let mut options = HashMap::new();
        options.insert("tension".to_string(), AttributeValue::Number(0.0));
        let spec = AlgorithmOptionSpec {
            key: "tension",
            kind: OptionKind::Number {
                min: 0.0,
                max: 2.0,
                exclude_min: true,
            },
            default: 0.55,
            description: "test",
        };
        let reader = OptionsReader::new(&options, Span::dummy(), "test");
        assert!(reader.read_spec(&spec).is_none());
        assert_eq!(reader.read_spec_or_default(&spec), 0.55);
    }
}
