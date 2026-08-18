//! 将 [`ExportScene`] 编码为 SVG 字符串。

use crate::kinds;
use crate::types::DiagramType;
use crate::render::paint::{svg_debug, svg_utils};
use crate::render::ExportScene;

/// 从 scene 中间层生成完整 SVG 文档。
///
/// 三图层渲染顺序：edges（底层）→ nodes（中层）→ labels（顶层）。
/// 标签始终在最上层，避免被边路径或节点遮挡。
pub fn encode(scene: &ExportScene<'_>) -> String {
    let diagram = scene.diagram();
    let entry = kinds::entry_for(&diagram.diagram_type);
    let mut svg = String::with_capacity(8192);

    let extra_defs = (entry.paint_svg_defs)(&scene.context);
    let title_offset = svg_utils::write_svg_preamble(scene, &mut svg, extra_defs.as_deref());
    let skipped_nodes = diagram.entities.len().saturating_sub(scene.nodes.len());
    let mut skipped_edges = 0;
    let edges_under_nodes = matches!(diagram.diagram_type, DiagramType::Mindmap);

    // ── 底层：边路径（不含标签）──
    if edges_under_nodes {
        for edge in &scene.edges {
            if edge.layout.path_len() >= 2 {
                svg_debug::open_edge_g(edge.index, edge.relation, &mut svg);
                (entry.paint_export_edge)(edge, scene, &mut svg);
                svg_debug::close_g(&mut svg);
            } else {
                skipped_edges += 1;
            }
        }
    }

    // ── 中层：节点 ──
    for node in &scene.nodes {
        svg_debug::open_node_g(node.entity, &mut svg);
        (entry.paint_export_node)(node, scene, &mut svg);
        svg_debug::close_g(&mut svg);
    }

    if !edges_under_nodes {
        for edge in &scene.edges {
            if edge.layout.path_len() >= 2 {
                svg_debug::open_edge_g(edge.index, edge.relation, &mut svg);
                (entry.paint_export_edge)(edge, scene, &mut svg);
                svg_debug::close_g(&mut svg);
            } else {
                skipped_edges += 1;
            }
        }
    }

    // ── 顶层：边标签 ──
    for edge in &scene.edges {
        if edge.layout.path_len() >= 2 {
            svg_debug::open_edge_label_g(edge.index, edge.relation, &mut svg);
            (entry.paint_export_edge_label)(edge, scene, &mut svg);
            svg_debug::close_g(&mut svg);
        }
    }

    svg_utils::write_svg_postamble(
        &mut svg,
        scene,
        title_offset,
        skipped_nodes,
        skipped_edges,
    );
    svg
}
