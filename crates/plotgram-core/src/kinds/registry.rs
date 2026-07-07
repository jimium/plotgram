//! 图表类型注册表：validate + scene 物化 + SVG 绘制分派。
//!
//! 通过 [`DiagramKind`] trait 定义行为，通过 [`DiagramTypeEntry`] 函数指针结构体
//! 实现运行时分派，兼顾 trait 的可读性与动态分派的灵活性。

use crate::ast::{Diagram, Entity, Relation};
use crate::error::ValidationResult;
use crate::render::visual::{EdgeStyle, NodeStyle};
use crate::render::{ExportEdge, ExportNode, ExportScene, CompiledRenderContext};
use crate::types::DiagramType;

use super::architecture::Architecture;
use super::er::Er;
use super::flowchart::Flowchart;
use super::mindmap::Mindmap;
use super::sequence::Sequence;
use super::state::State;
use super::traits::DiagramKind;

/// 一种图表类型在 scene 管线中的完整行为入口。
///
/// 字段为函数指针，由 [`DiagramTypeEntry::from_kind`] 从 trait 实现中自动生成。
///
/// 三图层渲染：`paint_export_edge` 只渲染边路径（底层），
/// `paint_export_edge_label` 渲染边标签（顶层，在节点之上）。
pub struct DiagramTypeEntry {
    pub validate: fn(&Diagram, &mut ValidationResult),
    pub materialize_node_style: fn(&Entity, &CompiledRenderContext) -> NodeStyle,
    pub materialize_edge_style: fn(&Relation, &CompiledRenderContext) -> EdgeStyle,
    pub paint_export_node: fn(&ExportNode<'_>, &ExportScene<'_>, &mut String),
    /// 渲染边路径（不含标签）
    pub paint_export_edge: fn(&ExportEdge<'_>, &ExportScene<'_>, &mut String),
    /// 渲染边标签（顶层图层）
    pub paint_export_edge_label: fn(&ExportEdge<'_>, &ExportScene<'_>, &mut String),
    pub paint_svg_defs: fn(&CompiledRenderContext) -> Option<String>,
}

impl DiagramTypeEntry {
    /// 从 `DiagramKind` trait 实现自动生成注册表项。
    pub const fn from_kind<K: DiagramKind>() -> Self {
        Self {
            validate: K::validate,
            materialize_node_style: K::materialize_node_style,
            materialize_edge_style: K::materialize_edge_style,
            paint_export_node: K::paint_export_node,
            paint_export_edge: K::paint_export_edge,
            paint_export_edge_label: K::paint_export_edge_label,
            paint_svg_defs: K::paint_svg_defs,
        }
    }
}

static FLOWCHART: DiagramTypeEntry = DiagramTypeEntry::from_kind::<Flowchart>();
static SEQUENCE: DiagramTypeEntry = DiagramTypeEntry::from_kind::<Sequence>();
static STATE: DiagramTypeEntry = DiagramTypeEntry::from_kind::<State>();
static ER: DiagramTypeEntry = DiagramTypeEntry::from_kind::<Er>();
static MINDMAP: DiagramTypeEntry = DiagramTypeEntry::from_kind::<Mindmap>();
static ARCHITECTURE: DiagramTypeEntry = DiagramTypeEntry::from_kind::<Architecture>();

/// 根据图表类型查找注册表项。
pub fn entry_for(diagram_type: &DiagramType) -> &'static DiagramTypeEntry {
    match diagram_type {
        DiagramType::Flowchart => &FLOWCHART,
        DiagramType::Sequence => &SEQUENCE,
        DiagramType::State => &STATE,
        DiagramType::Er => &ER,
        DiagramType::Mindmap => &MINDMAP,
        DiagramType::Architecture => &ARCHITECTURE,
        DiagramType::Custom(_) => &FLOWCHART,
    }
}
