//! 标准节点尺寸计算
//!
//! 多个布局算法（architecture_v2、force_directed、flowchart/sugiyama Standard）
//! 共享同一套基于标签宽度的节点尺寸估算逻辑。
//!
//! [`NodeSizing`] 枚举统一各算法的节点尺寸策略分派：
//! - sugiyama_v2 通过 `SugiyamaPreset.node_sizing` 选择策略
//! - 其他算法可直接使用 [`standard_node_size`] 或自定义策略

use std::collections::HashMap;

use crate::ast::{Diagram, Entity};
use crate::layout;

/// 节点标签渲染字号（与 Clean Light `typography.label_size` 对齐）。
pub const NODE_LABEL_FONT_SIZE: f64 = 17.0;

/// 边标签估宽所用的参考字号（`DEFAULT_*_CHAR_WIDTH` 的标定基准）。
const LABEL_WIDTH_REF_FONT_SIZE: f64 = 14.0;

/// 左右 padding + border 总宽度（约 `2 × node_padding_x(14) + stroke`）。
pub const LABEL_WIDTH_OFFSET: f64 = 32.0;

/// 最小节点宽度（Phase B：短标签不再撑到固定 160）。
pub const MIN_NODE_WIDTH: f64 = 112.0;

/// 最大节点宽度（Phase B：避免无谓顶到 240）。
pub const MAX_NODE_WIDTH: f64 = 200.0;

/// 默认节点高度
pub const DEFAULT_NODE_HEIGHT: f64 = layout::constants::DEFAULT_NODE_HEIGHT;

/// 节点尺寸策略
///
/// 由各算法的 preset 携带，驱动节点宽高估算的分派。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeSizing {
    /// 流程图等标准矩形节点：按标签估宽。
    Standard,
    /// ER 实体：按属性数量估算宽高。
    Er,
    /// 状态图节点（initial / final / choice 等）。
    State,
    /// 通用算法：仍按 diagram 类型推断（向后兼容显式声明）。
    InferFromDiagram,
}

/// 按标签内容估算标准节点宽度（不含 style/icon 覆盖）。
///
/// 区分 ASCII / CJK，并按 [`NODE_LABEL_FONT_SIZE`] 相对边标签参考字号缩放，
/// 再加 padding 偏移后 clamp。
pub fn estimate_standard_node_width(label: &str) -> f64 {
    let scale = NODE_LABEL_FONT_SIZE / LABEL_WIDTH_REF_FONT_SIZE;
    let mut text_w = 0.0;
    for ch in label.chars() {
        text_w += if ch.is_ascii() {
            layout::constants::DEFAULT_ASCII_CHAR_WIDTH
        } else {
            layout::constants::DEFAULT_CJK_CHAR_WIDTH
        };
    }
    (text_w * scale + LABEL_WIDTH_OFFSET).clamp(MIN_NODE_WIDTH, MAX_NODE_WIDTH)
}

/// 按标签宽度估算单个节点的尺寸（标准策略）。
///
/// 宽度见 [`estimate_standard_node_width`]，再经 [`layout::styled_node_size`] 应用实体样式覆盖。
pub fn standard_node_size(entity: &Entity) -> (f64, f64) {
    layout::styled_node_size(
        entity,
        estimate_standard_node_width(entity.label.as_str()),
        DEFAULT_NODE_HEIGHT,
    )
}

/// 批量计算图中所有实体的标准尺寸。
pub fn standard_node_sizes(diagram: &Diagram) -> HashMap<String, (f64, f64)> {
    diagram
        .entities
        .iter()
        .map(|entity| {
            (
                entity.id.as_str().to_string(),
                standard_node_size(entity),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_ascii_label_uses_min_width() {
        let w = estimate_standard_node_width("A");
        assert!((w - MIN_NODE_WIDTH).abs() < f64::EPSILON);
    }

    #[test]
    fn medium_ascii_label_between_min_and_legacy_fixed() {
        // 14 个 ASCII → 应落在 min~160 之间（短于旧固定宽 160）
        let w = estimate_standard_node_width("abcdefghijklmn");
        assert!(w > MIN_NODE_WIDTH);
        assert!(w < 160.0, "expected compact width, got {w}");
    }

    #[test]
    fn long_label_caps_at_max() {
        let w = estimate_standard_node_width("这是一个非常非常非常非常非常长的中文节点标签用于测试上限");
        assert!((w - MAX_NODE_WIDTH).abs() < f64::EPSILON);
    }

    #[test]
    fn cjk_wider_than_same_count_ascii() {
        let ascii = estimate_standard_node_width("ABCDEFGH");
        let cjk = estimate_standard_node_width("中文标签测试字");
        assert!(cjk > ascii);
    }
}
