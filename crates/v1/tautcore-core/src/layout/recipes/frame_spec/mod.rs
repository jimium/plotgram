//! Group Frame Spec — 仅保留 DSL 解析类型（几何整形生产路径已删除）。
//!
//! G-pre 后不再消费 `group_frame` 几何遥控器；本模块只提供算法默认 [`GroupFrameSpec`]
//! 与 padding 选择，供 architecture / flowchart 编译与 runner 标志使用。

mod padding;
mod spec;

pub use padding::group_padding_for_algo;
pub use spec::*;

/// 历史 `apply_group_frame` 报告类型（生产不再填充；hints 字段保留兼容）。
#[derive(Debug, Clone, Default)]
pub struct GroupFrameReport {
    pub top_group_count: usize,
    pub nested_frames_applied: usize,
    pub matrix_applied: bool,
    pub equalized: bool,
    pub cross_aligned: bool,
    pub borders_aligned: usize,
    pub groups_quantized: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{AttributeValue, Diagram, DiagramAttribute, Span, TextValue};
    use std::collections::HashMap as StdHashMap;

    fn config_attr(algo: &str, options: &[(&str, AttributeValue)]) -> DiagramAttribute {
        let mut map = StdHashMap::new();
        for (k, v) in options {
            map.insert(k.to_string(), v.clone());
        }
        DiagramAttribute {
            key: "group_frame".to_string(),
            value: AttributeValue::Config {
                algo: algo.to_string(),
                options: map,
            },
            span: Span::dummy(),
        }
    }

    fn str_val(s: &str) -> AttributeValue {
        AttributeValue::String(TextValue::unquoted(s))
    }

    fn num_val(n: f64) -> AttributeValue {
        AttributeValue::Number(n)
    }

    #[test]
    fn group_frame_config_ignored_keeps_algorithm_defaults() {
        let diagram = Diagram {
            attributes: vec![config_attr(
                "stack",
                &[
                    ("axis", str_val("horizontal")),
                    ("gap", num_val(48.0)),
                    ("track", str_val("equal")),
                    ("cross", str_val("start")),
                    ("border", str_val("shared")),
                    ("snap", num_val(16.0)),
                ],
            )],
            ..Default::default()
        };
        let spec = resolve_group_frame_spec(&diagram, "flowchart");
        assert_eq!(
            spec.arrangement,
            GroupArrangement::Stack {
                axis: Axis::Vertical
            }
        );
        assert_eq!(spec.track_sizing, TrackSizing::Fit);
        assert_eq!(spec.border_align, BorderAlign::None);
    }

    #[test]
    fn architecture_defaults_to_fit_container() {
        let diagram = Diagram::default();
        let spec = resolve_group_frame_spec(&diagram, "architecture");
        assert_eq!(spec.track_sizing, TrackSizing::Fit);
        assert_eq!(spec.border_align, BorderAlign::None);
        assert!(spec.architecture_recipe);
    }
}
