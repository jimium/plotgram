//! 标准节点尺寸计算
//!
//! 多个布局算法（architecture_v2、force_directed）共享同一套基于标签宽度的节点尺寸估算逻辑。
//! 本模块抽取该公共逻辑，消除重复实现。
//!
//! [`NodeSizing`] 枚举统一各算法的节点尺寸策略分派：
//! - sugiyama_v2 通过 `SugiyamaPreset.node_sizing` 选择策略
//! - 其他算法可直接使用 [`standard_node_size`] 或自定义策略

use std::collections::HashMap;

use crate::ast::{Diagram, Entity};
use crate::layout;

/// 标签字符宽度（像素）
pub const LABEL_CHAR_WIDTH: f64 = 11.0;
/// 标签宽度偏移量（padding + border 等）
pub const LABEL_WIDTH_OFFSET: f64 = 44.0;
/// 最小节点宽度
pub const MIN_NODE_WIDTH: f64 = 96.0;
/// 最大节点宽度
pub const MAX_NODE_WIDTH: f64 = 240.0;
/// 默认节点高度
pub const DEFAULT_NODE_HEIGHT: f64 = layout::constants::DEFAULT_NODE_HEIGHT;

/// 节点尺寸策略
///
/// 由各算法的 preset 携带，驱动节点宽高估算的分派。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeSizing {
    /// 流程图等标准矩形节点。
    Standard,
    /// ER 实体：按属性数量估算宽高。
    Er,
    /// 状态图节点（initial / final / choice 等）。
    State,
    /// 通用算法：仍按 diagram 类型推断（向后兼容显式声明）。
    InferFromDiagram,
}

/// 按标签宽度估算单个节点的尺寸（标准策略）。
///
/// 宽度 = `clamp(label_chars * LABEL_CHAR_WIDTH + LABEL_WIDTH_OFFSET, MIN, MAX)`，
/// 再经 [`layout::styled_node_size`] 应用实体样式覆盖。
pub fn standard_node_size(entity: &Entity) -> (f64, f64) {
    let width = (unicode_width::UnicodeWidthStr::width(entity.label.as_str()) as f64
        * LABEL_CHAR_WIDTH
        + LABEL_WIDTH_OFFSET)
        .clamp(MIN_NODE_WIDTH, MAX_NODE_WIDTH);
    layout::styled_node_size(entity, width, DEFAULT_NODE_HEIGHT)
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
