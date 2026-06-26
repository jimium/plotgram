//! 图表类型行为契约：trait 定义与标准图默认实现。
//!
//! - [`DiagramKind`]：每种图表类型的完整行为接口
//! - [`StandardDiagramKind`]：标准图（flowchart / state / architecture / mindmap）共享默认实现

use crate::ast::{Diagram, Entity, Relation};
use crate::error::ValidationResult;
use crate::render::paint::standard as standard_paint;
use crate::render::visual::{EdgeStyle, NodeStyle};
use crate::render::{ExportEdge, ExportNode, ExportScene, CompiledRenderContext};

use super::standard::{self, StandardStyleConfig};

/// 每种图表类型在 scene 管线中的完整行为入口。
///
/// 三图层渲染契约：
/// - `paint_export_edge`：只渲染边路径（不含标签）
/// - `paint_export_node`：渲染节点
/// - `paint_export_edge_label`：渲染边标签（在节点之上）
pub trait DiagramKind {
    fn validate(diagram: &Diagram, result: &mut ValidationResult);
    fn materialize_node_style(entity: &Entity, ctx: &CompiledRenderContext) -> NodeStyle;
    fn materialize_edge_style(relation: &Relation, ctx: &CompiledRenderContext) -> EdgeStyle;
    fn paint_export_node(node: &ExportNode<'_>, scene: &ExportScene<'_>, svg: &mut String);
    fn paint_export_edge(edge: &ExportEdge<'_>, scene: &ExportScene<'_>, svg: &mut String);
    fn paint_export_edge_label(edge: &ExportEdge<'_>, scene: &ExportScene<'_>, svg: &mut String);
    fn paint_svg_defs(ctx: &CompiledRenderContext) -> Option<String>;
}

/// 标准图（flowchart / state / architecture / mindmap）共享配置与默认实现。
///
/// 实现 `StandardDiagramKind` 即自动获得 `DiagramKind` 的默认实现，
/// 只需提供 `STYLE_CONFIG` 常量与 `validate` 方法。
pub trait StandardDiagramKind: DiagramKind {
    /// 该图表类型使用的标准样式配置。
    const STYLE_CONFIG: &'static StandardStyleConfig;

    /// 该图表类型专属的语义校验规则。
    fn validate_specific(diagram: &Diagram, result: &mut ValidationResult);
}

/// 为所有 `StandardDiagramKind` 提供 `DiagramKind` 的默认实现。
impl<T: StandardDiagramKind> DiagramKind for T {
    fn validate(diagram: &Diagram, result: &mut ValidationResult) {
        T::validate_specific(diagram, result);
    }

    fn materialize_node_style(entity: &Entity, ctx: &CompiledRenderContext) -> NodeStyle {
        standard::materialize_node_style(T::STYLE_CONFIG, entity, ctx)
    }

    fn materialize_edge_style(relation: &Relation, ctx: &CompiledRenderContext) -> EdgeStyle {
        standard::materialize_edge_style(T::STYLE_CONFIG, relation, ctx)
    }

    fn paint_export_node(node: &ExportNode<'_>, scene: &ExportScene<'_>, svg: &mut String) {
        standard_paint::paint_export_node(T::STYLE_CONFIG, node, scene, svg);
    }

    fn paint_export_edge(edge: &ExportEdge<'_>, scene: &ExportScene<'_>, svg: &mut String) {
        standard_paint::paint_export_edge(T::STYLE_CONFIG, edge, scene, svg);
    }

    fn paint_export_edge_label(edge: &ExportEdge<'_>, scene: &ExportScene<'_>, svg: &mut String) {
        standard_paint::paint_export_edge_label(T::STYLE_CONFIG, edge, scene, svg);
    }

    fn paint_svg_defs(ctx: &CompiledRenderContext) -> Option<String> {
        standard_paint::paint_svg_defs(T::STYLE_CONFIG, ctx)
    }
}
