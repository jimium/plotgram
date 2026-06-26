//! Sequence（时序图）图表类型模块。

pub mod validate;

use crate::ast::{Diagram, Entity, Relation};
use crate::error::ValidationResult;
use crate::render::paint::sequence as sequence_paint;
use crate::render::visual::{EdgeStyle, NodeStyle};
use crate::render::{ExportEdge, ExportNode, ExportScene, CompiledRenderContext};

use super::traits::DiagramKind;

/// Sequence 图表类型的零大小标记类型。
pub struct Sequence;

impl DiagramKind for Sequence {
    fn validate(diagram: &Diagram, result: &mut ValidationResult) {
        validate::validate(diagram, result);
    }

    fn materialize_node_style(entity: &Entity, ctx: &CompiledRenderContext) -> NodeStyle {
        sequence_paint::materialize_node_style(entity, ctx)
    }

    fn materialize_edge_style(relation: &Relation, ctx: &CompiledRenderContext) -> EdgeStyle {
        sequence_paint::materialize_edge_style(relation, ctx)
    }

    fn paint_export_node(node: &ExportNode<'_>, scene: &ExportScene<'_>, svg: &mut String) {
        sequence_paint::paint_export_node(node, scene, svg);
    }

    fn paint_export_edge(edge: &ExportEdge<'_>, scene: &ExportScene<'_>, svg: &mut String) {
        sequence_paint::paint_export_edge(edge, scene, svg);
    }

    fn paint_export_edge_label(edge: &ExportEdge<'_>, scene: &ExportScene<'_>, svg: &mut String) {
        sequence_paint::paint_export_edge_label(edge, scene, svg);
    }

    fn paint_svg_defs(ctx: &CompiledRenderContext) -> Option<String> {
        sequence_paint::paint_svg_defs(ctx)
    }
}
