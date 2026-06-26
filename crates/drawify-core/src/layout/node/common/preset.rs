//! 布局参数预设 trait
//!
//! 各算法可定义自己的 preset 类型（如 `SugiyamaPreset`），实现 [`LayoutPreset`]
//! 暴露统一接口：名称、节点尺寸策略、可选的布局后调整回调。
//!
//! 当前仅 sugiyama_v2 实现此 trait；其他算法可按需实现，以便 catalog/CLI
//! 查询可用 preset 或在运行时切换参数集。

use crate::layout::LayoutResult;
use crate::layout::node::common::node_sizing::NodeSizing;

/// 布局参数预设 trait
///
/// 实现方通常是 `const` 结构体（如 `SugiyamaPreset`），携带：
/// - 布局参数（padding、gap 等）
/// - 节点尺寸策略
/// - 可选的布局后调整回调
pub trait LayoutPreset: std::fmt::Debug + Copy {
    /// preset 名称（如 `"flowchart"`、`"er"`、`"generic"`）
    fn name(&self) -> &'static str;

    /// 节点尺寸策略
    fn node_sizing(&self) -> NodeSizing;

    /// 布局完成后可选调整（如 ER 整体平移对齐边距）
    ///
    /// 默认实现为 no-op；需要后调整的 preset 覆写此方法。
    fn finish_layout(&self, _result: &mut LayoutResult) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::node::sugiyama_v2::preset::{
        FLOWCHART_PRESET, ER_PRESET, GENERIC_PRESET, SugiyamaPreset,
    };

    impl LayoutPreset for SugiyamaPreset {
        fn name(&self) -> &'static str {
            // 通过节点尺寸策略反推 preset 名（const 结构体无 name 字段）
            match self.node_sizing {
                NodeSizing::Standard => "flowchart",
                NodeSizing::Er => "er",
                NodeSizing::InferFromDiagram => "generic",
                NodeSizing::State => "state",
            }
        }

        fn node_sizing(&self) -> NodeSizing {
            self.node_sizing
        }

        fn finish_layout(&self, result: &mut LayoutResult) {
            if let Some(callback) = self.finish_layout {
                callback(result, self);
            }
        }
    }

    #[test]
    fn sugiyama_presets_implement_layout_preset() {
        assert_eq!(FLOWCHART_PRESET.name(), "flowchart");
        assert_eq!(FLOWCHART_PRESET.node_sizing(), NodeSizing::Standard);

        assert_eq!(ER_PRESET.name(), "er");
        assert_eq!(ER_PRESET.node_sizing(), NodeSizing::Er);

        assert_eq!(GENERIC_PRESET.name(), "generic");
        assert_eq!(GENERIC_PRESET.node_sizing(), NodeSizing::InferFromDiagram);
    }
}
