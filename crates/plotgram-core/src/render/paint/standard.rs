//! 标准图 SVG 绘制（消费 ExportScene）。

use crate::kinds::standard::StandardStyleConfig;
use crate::render::paint::edge::{paint_arrowed_edge, paint_plain_edge, uses_arrows};
use crate::render::paint::node::paint_labeled_node;
use crate::render::{ExportEdge, ExportNode, ExportScene};

pub fn paint_export_node(
    config: &StandardStyleConfig,
    node: &ExportNode<'_>,
    scene: &ExportScene<'_>,
    svg: &mut String,
) {
    paint_labeled_node(
        &config.diagram_type,
        node.entity,
        &node.layout,
        &node.style,
        config.label_weight,
        &scene.context,
        svg,
    );
}

pub fn paint_export_edge(
    config: &StandardStyleConfig,
    edge: &ExportEdge<'_>,
    scene: &ExportScene<'_>,
    svg: &mut String,
) {
    if edge.layout.path_len() < 2 {
        return;
    }
    if uses_arrows(&config.edge_config.arrow_style) {
        paint_arrowed_edge(
            &config.diagram_type,
            edge.relation,
            &edge.layout,
            &edge.style,
            false, // 标签由 paint_export_edge_label 单独渲染（三图层）
            &scene.context,
            svg,
        );
    } else {
        paint_plain_edge(&edge.layout, &edge.style, &scene.context, svg);
    }
}

/// 渲染边标签（三图层顶层）。
///
/// 在所有边路径与节点渲染完成后调用，确保标签在最上层。
/// 遍历 `edge.layout.labels` 渲染所有标签（中段/头部/尾部）。
pub fn paint_export_edge_label(
    config: &StandardStyleConfig,
    edge: &ExportEdge<'_>,
    scene: &ExportScene<'_>,
    svg: &mut String,
) {
    if !config.edge_config.render_labels {
        return;
    }
    crate::render::paint::svg_utils::render_edge_labels(
        &edge.layout,
        &edge.style.label_style,
        &scene.context,
        &config.diagram_type,
        svg,
    );
}

pub fn paint_svg_defs(_config: &StandardStyleConfig, _context: &crate::render::CompiledRenderContext) -> Option<String> {
    None
}
