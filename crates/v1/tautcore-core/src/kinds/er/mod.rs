//! ER（实体关系图）图表类型模块。

pub mod semantics;
pub mod validate;

use crate::ast::{Diagram, Entity, Relation};
use crate::error::ValidationResult;
use crate::render::paint::er as er_paint;
use crate::render::visual::{EdgeStyle, NodeStyle};
use crate::render::{ExportEdge, ExportNode, ExportScene, CompiledRenderContext};

use super::traits::DiagramKind;

/// ER 图表类型的零大小标记类型。
pub struct Er;

impl DiagramKind for Er {
    fn validate(diagram: &Diagram, result: &mut ValidationResult) {
        validate::validate(diagram, result);
    }

    fn materialize_node_style(entity: &Entity, ctx: &CompiledRenderContext) -> NodeStyle {
        er_paint::materialize_node_style(entity, ctx)
    }

    fn materialize_edge_style(relation: &Relation, ctx: &CompiledRenderContext) -> EdgeStyle {
        er_paint::materialize_edge_style(relation, ctx)
    }

    fn paint_export_node(node: &ExportNode<'_>, scene: &ExportScene<'_>, svg: &mut String) {
        er_paint::paint_export_node(node, scene, svg);
    }

    fn paint_export_edge(edge: &ExportEdge<'_>, scene: &ExportScene<'_>, svg: &mut String) {
        er_paint::paint_export_edge(edge, scene, svg);
    }

    fn paint_export_edge_label(_edge: &ExportEdge<'_>, _scene: &ExportScene<'_>, _svg: &mut String) {
        // ER 图的标签（基数）已在 paint_export_edge 中作为菱形渲染，无需单独标签层
    }

    fn paint_svg_defs(ctx: &CompiledRenderContext) -> Option<String> {
        er_paint::paint_svg_defs(ctx)
    }
}
