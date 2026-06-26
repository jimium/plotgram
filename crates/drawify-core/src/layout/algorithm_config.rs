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

/// Sugiyama 系布局算法共用的 option 列表。
pub const SUGIYAMA_LAYOUT_OPTIONS: &[AlgorithmOptionSpec] = &[AlgorithmOptionSpec {
    key: "group_padding",
    kind: OptionKind::NonNegativeNumber,
    default: constants::SUGIYAMA_GROUP_PADDING,
    description: "分组包围框内边距",
}];

/// Sugiyama 系布局已解析的运行时配置。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SugiyamaLayoutConfig {
    pub group_padding: f64,
}

impl Default for SugiyamaLayoutConfig {
    fn default() -> Self {
        Self {
            group_padding: SUGIYAMA_LAYOUT_OPTIONS[0].default,
        }
    }
}

impl SugiyamaLayoutConfig {
    pub fn from_options(options: &super::plan::ResolvedAlgoOptions) -> Self {
        Self {
            group_padding: options.get_or_default(&SUGIYAMA_LAYOUT_OPTIONS[0]),
        }
    }
}

/// 圆形布局算法 option 列表。
pub const CIRCULAR_LAYOUT_OPTIONS: &[AlgorithmOptionSpec] = &[
    AlgorithmOptionSpec {
        key: "group_padding",
        kind: OptionKind::NonNegativeNumber,
        default: constants::DEFAULT_GROUP_PADDING,
        description: "分组包围框内边距",
    },
    AlgorithmOptionSpec {
        key: "padding",
        kind: OptionKind::NonNegativeNumber,
        default: constants::CIRCULAR_PADDING,
        description: "画布内边距",
    },
    AlgorithmOptionSpec {
        key: "component_gap",
        kind: OptionKind::NonNegativeNumber,
        default: constants::CIRCULAR_COMPONENT_GAP,
        description: "多连通分量圆环之间的水平间距",
    },
];

/// 圆形布局已解析的运行时配置。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CircularLayoutConfig {
    pub group_padding: f64,
    pub padding: f64,
    pub component_gap: f64,
}

impl Default for CircularLayoutConfig {
    fn default() -> Self {
        Self {
            group_padding: CIRCULAR_LAYOUT_OPTIONS[0].default,
            padding: CIRCULAR_LAYOUT_OPTIONS[1].default,
            component_gap: CIRCULAR_LAYOUT_OPTIONS[2].default,
        }
    }
}

impl CircularLayoutConfig {
    pub fn from_options(options: &super::plan::ResolvedAlgoOptions) -> Self {
        Self {
            group_padding: options.get_or_default(&CIRCULAR_LAYOUT_OPTIONS[0]),
            padding: options.get_or_default(&CIRCULAR_LAYOUT_OPTIONS[1]),
            component_gap: options.get_or_default(&CIRCULAR_LAYOUT_OPTIONS[2]),
        }
    }
}

/// 思维导图布局 option 列表。
pub const MINDMAP_LAYOUT_OPTIONS: &[AlgorithmOptionSpec] = &[
    AlgorithmOptionSpec {
        key: "padding",
        kind: OptionKind::NonNegativeNumber,
        default: constants::MINDMAP_PADDING,
        description: "画布内边距",
    },
    AlgorithmOptionSpec {
        key: "level_gap",
        kind: OptionKind::PositiveNumber,
        default: constants::MINDMAP_LEVEL_GAP,
        description: "层级之间的主轴间距",
    },
    AlgorithmOptionSpec {
        key: "branch_gap",
        kind: OptionKind::NonNegativeNumber,
        default: constants::MINDMAP_BRANCH_GAP,
        description: "同层兄弟节点间距",
    },
    AlgorithmOptionSpec {
        key: "node_gap",
        kind: OptionKind::NonNegativeNumber,
        default: constants::MINDMAP_NODE_GAP,
        description: "径向模式下子树垂直间距",
    },
    AlgorithmOptionSpec {
        key: "center_gap",
        kind: OptionKind::NonNegativeNumber,
        default: constants::MINDMAP_CENTER_GAP,
        description: "根节点到一级分支的水平间距",
    },
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MindmapLayoutConfig {
    pub padding: f64,
    pub level_gap: f64,
    pub branch_gap: f64,
    pub node_gap: f64,
    pub center_gap: f64,
}

impl Default for MindmapLayoutConfig {
    fn default() -> Self {
        Self {
            padding: MINDMAP_LAYOUT_OPTIONS[0].default,
            level_gap: MINDMAP_LAYOUT_OPTIONS[1].default,
            branch_gap: MINDMAP_LAYOUT_OPTIONS[2].default,
            node_gap: MINDMAP_LAYOUT_OPTIONS[3].default,
            center_gap: MINDMAP_LAYOUT_OPTIONS[4].default,
        }
    }
}

impl MindmapLayoutConfig {
    pub fn from_options(options: &super::plan::ResolvedAlgoOptions) -> Self {
        Self {
            padding: options.get_or_default(&MINDMAP_LAYOUT_OPTIONS[0]),
            level_gap: options.get_or_default(&MINDMAP_LAYOUT_OPTIONS[1]),
            branch_gap: options.get_or_default(&MINDMAP_LAYOUT_OPTIONS[2]),
            node_gap: options.get_or_default(&MINDMAP_LAYOUT_OPTIONS[3]),
            center_gap: options.get_or_default(&MINDMAP_LAYOUT_OPTIONS[4]),
        }
    }
}

/// 时序图布局 option 列表。
pub const SEQUENCE_LAYOUT_OPTIONS: &[AlgorithmOptionSpec] = &[
    AlgorithmOptionSpec {
        key: "group_padding",
        kind: OptionKind::NonNegativeNumber,
        default: constants::DEFAULT_GROUP_PADDING,
        description: "分组包围框内边距",
    },
    AlgorithmOptionSpec {
        key: "node_spacing",
        kind: OptionKind::NonNegativeNumber,
        default: constants::SEQUENCE_NODE_SPACING,
        description: "参与者节点水平间距",
    },
    AlgorithmOptionSpec {
        key: "message_spacing",
        kind: OptionKind::PositiveNumber,
        default: constants::SEQUENCE_MESSAGE_SPACING,
        description: "相邻消息行垂直间距",
    },
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SequenceLayoutConfig {
    pub group_padding: f64,
    pub node_spacing: f64,
    pub message_spacing: f64,
}

impl Default for SequenceLayoutConfig {
    fn default() -> Self {
        Self {
            group_padding: SEQUENCE_LAYOUT_OPTIONS[0].default,
            node_spacing: SEQUENCE_LAYOUT_OPTIONS[1].default,
            message_spacing: SEQUENCE_LAYOUT_OPTIONS[2].default,
        }
    }
}

impl SequenceLayoutConfig {
    pub fn from_options(options: &super::plan::ResolvedAlgoOptions) -> Self {
        Self {
            group_padding: options.get_or_default(&SEQUENCE_LAYOUT_OPTIONS[0]),
            node_spacing: options.get_or_default(&SEQUENCE_LAYOUT_OPTIONS[1]),
            message_spacing: options.get_or_default(&SEQUENCE_LAYOUT_OPTIONS[2]),
        }
    }
}

/// 力导向布局 option 列表。
pub const FORCE_DIRECTED_LAYOUT_OPTIONS: &[AlgorithmOptionSpec] = &[
    AlgorithmOptionSpec {
        key: "group_padding",
        kind: OptionKind::NonNegativeNumber,
        default: constants::FORCE_DIRECTED_GROUP_PADDING,
        description: "分组包围框内边距",
    },
    AlgorithmOptionSpec {
        key: "padding",
        kind: OptionKind::NonNegativeNumber,
        default: constants::WIDE_PADDING,
        description: "画布内边距",
    },
    AlgorithmOptionSpec {
        key: "component_gap",
        kind: OptionKind::NonNegativeNumber,
        default: constants::FORCE_DIRECTED_COMPONENT_GAP,
        description: "连通分量之间的水平间距",
    },
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ForceDirectedLayoutConfig {
    pub group_padding: f64,
    pub padding: f64,
    pub component_gap: f64,
}

impl Default for ForceDirectedLayoutConfig {
    fn default() -> Self {
        Self {
            group_padding: FORCE_DIRECTED_LAYOUT_OPTIONS[0].default,
            padding: FORCE_DIRECTED_LAYOUT_OPTIONS[1].default,
            component_gap: FORCE_DIRECTED_LAYOUT_OPTIONS[2].default,
        }
    }
}

impl ForceDirectedLayoutConfig {
    pub fn from_options(options: &super::plan::ResolvedAlgoOptions) -> Self {
        Self {
            group_padding: options.get_or_default(&FORCE_DIRECTED_LAYOUT_OPTIONS[0]),
            padding: options.get_or_default(&FORCE_DIRECTED_LAYOUT_OPTIONS[1]),
            component_gap: options.get_or_default(&FORCE_DIRECTED_LAYOUT_OPTIONS[2]),
        }
    }
}

/// architecture 布局 option 列表。
pub const ARCHITECTURE_V2_LAYOUT_OPTIONS: &[AlgorithmOptionSpec] = &[
    AlgorithmOptionSpec {
        key: "group_padding",
        kind: OptionKind::NonNegativeNumber,
        default: constants::ARCH_V2_GROUP_PADDING,
        description: "分组包围框内边距",
    },
    AlgorithmOptionSpec {
        key: "padding",
        kind: OptionKind::NonNegativeNumber,
        default: constants::ARCH_V2_PADDING,
        description: "画布内边距",
    },
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArchitectureV2LayoutConfig {
    pub group_padding: f64,
    pub padding: f64,
}

impl Default for ArchitectureV2LayoutConfig {
    fn default() -> Self {
        Self {
            group_padding: ARCHITECTURE_V2_LAYOUT_OPTIONS[0].default,
            padding: ARCHITECTURE_V2_LAYOUT_OPTIONS[1].default,
        }
    }
}

impl ArchitectureV2LayoutConfig {
    pub fn from_options(options: &super::plan::ResolvedAlgoOptions) -> Self {
        Self {
            group_padding: options.get_or_default(&ARCHITECTURE_V2_LAYOUT_OPTIONS[0]),
            padding: options.get_or_default(&ARCHITECTURE_V2_LAYOUT_OPTIONS[1]),
        }
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
