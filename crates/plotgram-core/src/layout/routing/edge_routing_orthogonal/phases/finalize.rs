//! C 末收尾：边间距违规统计。
//!
//! Phase 0（策略 B）：C 段 canonicalize 已删；正交几何规范化只在
//! `RoutingCoordinator` D 段（`merge_overshoot=true`）执行一次。

use super::super::*;

/// Phase 4g + X-0：边间距违规统计（canonicalize 已迁至 D 段唯一写点）。
pub(crate) fn phase_sanitize(
    edges: &[EdgeLayout],
    grid: &SegmentGrid,
    parallel_gap: f64,
    ortho_stats: &mut crate::layout::OrthoDebugStats,
) {
    let (exact_overlap_pairs, tight_spacing_pairs) =
        count_all_edge_spacing_violations(edges, grid, parallel_gap);
    ortho_stats.edge_exact_overlap_pairs = exact_overlap_pairs;
    ortho_stats.edge_tight_spacing_pairs = tight_spacing_pairs;
}
