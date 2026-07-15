//! Plotgram 布局模块
//!
//! 提供可插拔的布局算法框架。每种布局算法实现 `LayoutStrategy` trait，
//! 通过 `compute_layout` 统一调度。
//!
//! ## 布局管线层级（Group Frame 三层模型）
//!
//! 详见 `docs/architecture/布局优化/group-frame-spec.md`（v0.2）。
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────┐
//! │ 拓扑主布局（Sugiyama / two_phase / group_divide）              │
//! │   L2 Intra Frame：组内节点排列（group_layout_hint）            │
//! ├──────────────────────────────────────────────────────────────┤
//! │ L3 Node Frame（grid_snap::align_nodes）：rank/layer 轴独立对齐       │
//! │   rank 轴：同层中心线对齐；layer 轴：重叠消除（保持层重心）          │
//! │   仅节点坐标调整；由 `align` 属性控制（rank/layer 分级）        │
//! ├──────────────────────────────────────────────────────────────┤
//! │ recompute group bounds from nodes（L3→L1 数据流桥梁）         │
//! ├──────────────────────────────────────────────────────────────┤
//! │ L1 Group Frame（group_frame）：组间排列/尺寸/对齐/间距/量化    │
//! │   apply_group_frame：Equal / border_align / quantize groups   │
//! ├──────────────────────────────────────────────────────────────┤
//! │ route + refine（LayoutRouteFeedback）                       │
//! ├──────────────────────────────────────────────────────────────┤
//! │ L1 Group Frame（幂等恢复）                                    │
//! ├──────────────────────────────────────────────────────────────┤
//! │ Edge Pixel Snap（grid_snap::snap_edge_waypoints）             │
//! │   边 waypoint 量化 + 边框排斥；管道末尾仅执行一次，由 `snap`   │
//! │   属性控制；grid_step 按节点密度自适应（P5）                   │
//! └──────────────────────────────────────────────────────────────┘
//! ```

pub mod algorithm_config;
pub mod catalog;
pub mod canvas_finalize;
pub mod constants;
pub mod decl_order;
pub mod edge;
pub mod edge_band_demand;
pub mod entry;
pub mod geometry;
pub mod geometry_helpers;
pub mod grid_snap;
pub mod group;
pub mod group_frame;
pub mod lint;
pub mod metrics;
pub mod node;
pub mod plan;
pub mod perf;
pub mod pipeline;
pub mod post_route;
pub mod refine;
pub mod registry;
pub mod route_feedback;
pub mod space_budget;
pub mod space_budget_guard;
pub mod traits;
pub mod types;

pub use algorithm_config::{
    AlgorithmOptionSpec, ArchitectureV2LayoutConfig, CircularLayoutConfig, ForceDirectedLayoutConfig,
    MindmapLayoutConfig, OptionKind, SequenceLayoutConfig, SugiyamaLayoutConfig,
};
pub use catalog::{
    layout_catalog, AlgorithmOptionInfo, DiagramTypeCatalog, EdgeRoutingAlgoInfo, LayoutAlgoInfo,
    LayoutCatalog,
};
pub use plan::{validate_layout_plan_warnings, LayoutPlan, ResolvedAlgoOptions};
pub use lint::{
    compute_lint_metrics, count_unrelated_parallel_overlaps, lint_layout, parse_lint_profile,
    parse_lint_rule, parse_lint_rules_list, AdviceConfidence, LayoutKnob, LintAdvice,
    LayoutLinter, LayoutViolation, LintConfig, LintMetricsSummary, LintProfile, LintReport,
    LintRuleId, LintSeverity, RuleConfig,
};
pub use edge_band_demand::{
    demand_extra_over_base, edge_band_demand, layer_gaps_from_demand, EdgeBandDemandBreakdown,
    EdgeBandDemandProfile,
};
pub use metrics::{
    compute_collinear_sample_metrics, compute_congestion_sample_metrics, node_fingerprint,
    CollinearBaselineSnapshot, CollinearOrthoStats, CollinearSampleMetrics,
    CongestionBaselineSnapshot, CongestionSampleMetrics,
};
pub use registry::{EDGE_ROUTING_NAMES, LAYOUT_ALGORITHM_NAMES};
pub use grid_snap::{DiagramAlignOverride, EdgeSnapConfig, LayerAxisAlign, NodeAlignConfig};
pub use edge::segment_pair::{
    classify_segment_pair, find_needs_separation_edge_pairs, measure_segment_pair,
    ClassifyPairContext, ClassifyResult, ConflictDisposition, OrthoSegment, SegmentPairMeasure,
    SeparationReason, SpacingClass,
};

// 向后兼容：保持 `crate::layout::sugiyama` 等路径可用
pub use edge::{
    edge_routing, edge_routing_bezier, edge_routing_circular,
    edge_routing_organic, edge_routing_orthogonal, edge_routing_spline, visibility,
};
pub use node::{
    architecture_v2, circular, er, flowchart, force_directed, mindmap, sequence,
    sugiyama_v2,
};

// Re-exports from split modules (preserve external API)
pub use types::{
    NodeLayout, GroupLayout, Port, PathGeometry, EdgeLabelLayout, EdgeLayout,
    EdgeRoutingStyle, LayoutHints, GutterBudgetDebug, RefineDebugStats,
    OrthoDebugStats, GroupLayoutWarning, GroupLayoutWarningKind,
    LayoutResult, GroupContainmentViolation, ContainmentViolationKind,
};
pub use traits::{LayoutStrategy, EdgeRoutingStrategy};
pub use geometry_helpers::{styled_node_size, edge_point, ellipse_edge_point};
pub use entry::{
    resolve_effective_direction, compute_layout, compute_layout_with_plan,
    layout_option_specs, edge_routing_option_specs, applicable_layouts_for_type,
    applicable_routings_for_type, diagram_types_for_layout, diagram_types_for_routing,
    BUILTIN_DIAGRAM_TYPES,
};
// `pub(crate)` items can't be re-exported via `pub use` (would widen visibility),
// so re-export them at the same visibility.
pub(crate) use entry::{
    known_layout_algo_names, known_edge_routing_names, layout_config_error,
};
// `pub(super)` items from `entry` are visible to `layout`; bind them privately here
// so siblings like `catalog` can still call `super::all_layout_strategies()`.
// A private `use` is accessible to `layout` and its descendants, matching the
// original effective visibility for in-`layout` callers.
use entry::{all_layout_strategies, all_routing_strategies};

#[cfg(test)]
mod tests;
