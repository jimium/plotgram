//! 锯齿消毒收尾
//!
//! 从 `run.rs` 原样搬迁（A4 结构重构，行为不变）。

use super::super::*;
use std::collections::HashMap;

/// Phase 4g + X-0：锯齿消毒（端点反向 stub + 微折折叠）+ 边间距违规统计
pub(crate) fn phase_sanitize(
    edges: &mut [EdgeLayout],
    relations: &[crate::ast::Relation],
    from_side: &[Port],
    to_side: &[Port],
    grid: &SegmentGrid,
    parallel_gap: f64,
    ortho_stats: &mut crate::layout::OrthoDebugStats,
    annotations: Option<&crate::layout::routing::RouteAnnotationSet>,
    nodes: Option<&HashMap<String, NodeLayout>>,
    sorted_node_ids: Option<&[String]>,
) {
    // Slice D3：canonicalize 归 materializer（C 末保守版，merge_overshoot=false）。
    crate::layout::routing::model::GeometryMaterializer::canonicalize_orthogonal_edges(
        edges,
        relations,
        from_side,
        to_side,
        false,
        annotations,
        nodes,
        sorted_node_ids,
    );
    // P3.1：正反向 gap 写权 = C 预修（phase_lane 末）+ D 一次审计（pipeline）。
    // sanitize 后再 enforce 无独立证据支撑（与 lane 后重复），此处不再调用。

    // ── X-0: 统计边间距违规（排除 stub 段） ──
    let (exact_overlap_pairs, tight_spacing_pairs) =
        count_all_edge_spacing_violations(edges, grid, parallel_gap);
    ortho_stats.edge_exact_overlap_pairs = exact_overlap_pairs;
    ortho_stats.edge_tight_spacing_pairs = tight_spacing_pairs;
}
