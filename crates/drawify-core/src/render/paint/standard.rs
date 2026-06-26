//! 标准图 SVG 绘制（消费 ExportScene）。

use crate::kinds::standard::StandardStyleConfig;
use crate::layout::edge::edge_bundling::EdgePathRoles;
use crate::render::paint::edge::{paint_arrowed_edge, paint_plain_edge, uses_arrows};
use crate::render::paint::node::paint_labeled_node;
use crate::render::paint::svg_utils::BundleRenderInfo;
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
    let bundle = extract_bundle_info(scene, edge.index);
    if uses_arrows(&config.edge_config.arrow_style) {
        paint_arrowed_edge(
            &config.diagram_type,
            edge.relation,
            &edge.layout,
            &edge.style,
            false, // 标签由 paint_export_edge_label 单独渲染（三图层）
            &scene.context,
            bundle.as_ref(),
            svg,
        );
    } else {
        paint_plain_edge(&edge.layout, &edge.style, &scene.context, bundle.as_ref(), svg);
    }
}

/// P6 §6: 从 `scene.layout.hints.edge_bundling` 提取当前边的 bundle 渲染信息。
///
/// 返回 `Some` 当且仅当：
/// 1. bundling 已启用且 hints 已填充
/// 2. 该边属于某个 bundle（`edge_to_bundle[edge_index]` 为 `Some`）
/// 3. 该边有非空的路径区段分解（`edge_roles[edge_index].spans` 非空）
fn extract_bundle_info<'a>(scene: &'a ExportScene<'a>, edge_index: usize) -> Option<BundleRenderInfo<'a>> {
    let hints = scene.layout.hints.edge_bundling.as_ref()?;
    let result = &hints.result;
    let bundle_id = result.edge_to_bundle.get(edge_index).copied().flatten()?;
    let bundle = result.bundles.iter().find(|b| b.id == bundle_id)?;
    let roles: &EdgePathRoles = result.edge_roles.get(edge_index)?;
    if roles.spans.is_empty() {
        return None;
    }
    Some(BundleRenderInfo {
        bundle_size: bundle.edges.len(),
        roles,
        arrow_suppressed: result.arrow_suppressed.contains(&edge_index),
    })
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
