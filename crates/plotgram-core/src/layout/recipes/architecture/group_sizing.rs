//! 顶层分组宽度策略：fit（内容贴合）与 uniform（等宽阶段条带）
//!
//! DSL 唯一入口：`group_frame: stack { track: fit | equal | uniform }`。

use crate::ast::{AttributeValue, Diagram};
use crate::types::standard_attr_keys::diagram;

/// 图级分组宽度策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupSizingPolicy {
    /// 组宽 = 组内内容 + padding
    Fit,
    /// 所有顶层 group 拉齐到最宽者，组内内容水平居中（默认）
    Uniform,
}

/// 从 `group_frame` 的 `track` 选项读取策略。
///
/// 默认 `Uniform`（同级条带，由 L1 GroupFramePass 拉齐）；
/// 显式 `track: fit` 才退回内容贴合。two_phase 本身不再执行 Equal。
pub fn parse_group_sizing(diagram: &Diagram) -> GroupSizingPolicy {
    match diagram_group_frame_track(diagram) {
        Some(t) => match t.trim().to_ascii_lowercase().as_str() {
            "fit" => GroupSizingPolicy::Fit,
            "equal" | "uniform" => GroupSizingPolicy::Uniform,
            _ => GroupSizingPolicy::Uniform,
        },
        None => GroupSizingPolicy::Uniform,
    }
}

/// 读取 `group_frame { track: ... }` 的原始字符串（若有）。
pub(crate) fn diagram_group_frame_track(diagram: &Diagram) -> Option<&str> {
    for attr in &diagram.attributes {
        if attr.key != diagram::GROUP_FRAME {
            continue;
        }
        if let AttributeValue::Config { options, .. } = &attr.value {
            return options.get("track").and_then(|v| v.as_str());
        }
    }
    None
}

/// 组块 trait：供宏观间距等按块 id / 是否 group 过滤
pub trait GroupWidthBlock {
    fn block_id(&self) -> &str;
    fn is_group_block(&self) -> bool;
}
